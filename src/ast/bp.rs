//! Blueprint graphs (K2 event/function/macro graphs, anim graphs, state machines) to AST.

use std::collections::{HashMap, HashSet, VecDeque};

use super::{Block, Expr, Function, Program, Stmt, ident, tidy_numbers};
use crate::graph::Pin;
use crate::object::{Extra, Object};
use crate::property::Value;
use crate::reader::Guid;
use crate::render::Ctx;
use crate::render::blueprint::node_title;

/// (node export index, pin id)
type PinRef = (i32, Guid);

/// Engine function libraries whose class name is omitted in calls.
const IMPLICIT_LIBRARIES: &[&str] = &[
    "KismetSystemLibrary", "KismetMathLibrary", "KismetStringLibrary", "KismetTextLibrary", "KismetArrayLibrary",
    "KismetStringTableLibrary", "BlueprintMapLibrary", "BlueprintSetLibrary", "GameplayStatics", "KismetRenderingLibrary",
    "KismetInputLibrary", "KismetNodeHelperLibrary", "BlueprintGameplayTagLibrary", "KismetMaterialLibrary",
    "BlueprintPathsLibrary", "WidgetBlueprintLibrary", "KismetGuidLibrary", "KismetInternationalizationLibrary",
];

const BINARY_OPS: &[(&str, &str)] = &[
    ("Add_", "+"), ("Subtract_", "-"), ("Multiply_", "*"), ("Divide_", "/"), ("Percent_", "%"), ("LessEqual_", "<="),
    ("GreaterEqual_", ">="), ("Less_", "<"), ("Greater_", ">"), ("EqualEqual_", "=="), ("NotEqual_", "!="),
    ("EqualExactly_", "=="), ("NotEqualExactly_", "!="), ("BooleanAND", "&&"), ("BooleanOR", "||"), ("BooleanXOR", "^"),
    ("BooleanNAND", "nand"), ("And_", "&"), ("Or_", "|"), ("Xor_", "^"),
];

fn is_exec(p: &Pin) -> bool {
    p.is_exec()
}

fn member_name(v: Option<&Value>) -> (String, i32, bool) {
    let Some(s) = v.and_then(|v| v.as_struct()) else { return (String::new(), 0, false) };
    let name = s.get("MemberName").and_then(|x| x.as_str()).unwrap_or("").to_string();
    let parent = s.get("MemberParent").and_then(|x| x.as_object()).unwrap_or(0);
    let self_ctx = s.get("bSelfContext").and_then(|x| x.as_bool()).unwrap_or(false);
    (name, parent, self_ctx)
}

pub struct GraphBuilder<'a, 'c> {
    c: &'a Ctx<'c>,
    nodes: Vec<&'a Object>,
    set: HashSet<i32>,
    /// Resolved exec predecessors count (through reroute nodes)
    incoming: HashMap<i32, usize>,
    /// Data output pins of impure nodes that were captured into variables
    temps: HashMap<PinRef, String>,
    used_names: HashSet<String>,
    labels: HashMap<i32, String>,
    pending: Vec<i32>,
    emitted: HashSet<i32>,
    on_path: Vec<i32>,
    loops: Vec<i32>,
    /// Exports covered by this builder (graphs and nodes)
    pub covered: Vec<i32>,
    depth: u32,
}

impl<'a, 'c> GraphBuilder<'a, 'c> {
    pub fn new(c: &'a Ctx<'c>, graph: &'a Object) -> Self {
        let node_ids: Vec<i32> = graph.prop("Nodes").and_then(|v| v.as_array()).unwrap_or(&[]).iter().filter_map(|x| x.as_object()).collect();
        let nodes: Vec<&Object> = node_ids.iter().filter_map(|i| c.obj(*i)).collect();
        let set: HashSet<i32> = nodes.iter().map(|n| n.index).collect();
        let mut b = GraphBuilder {
            c,
            nodes,
            set,
            incoming: HashMap::new(),
            temps: HashMap::new(),
            used_names: HashSet::new(),
            labels: HashMap::new(),
            pending: vec![],
            emitted: HashSet::new(),
            on_path: vec![],
            loops: vec![],
            covered: vec![graph.index],
            depth: 0,
        };
        b.covered.extend(b.set.iter().copied());
        for n in b.nodes.clone() {
            if n.class == "K2Node_Knot" {
                continue;
            }
            for p in n.pins().iter().filter(|p| is_exec(p) && p.is_output()) {
                for l in &p.linked_to {
                    if let Some((t, _)) = b.resolve_exec((l.node, l.pin)) {
                        *b.incoming.entry(t).or_default() += 1;
                    }
                }
            }
        }
        b
    }

    fn obj(&self, i: i32) -> Option<&'a Object> {
        self.c.obj(i)
    }

    fn pin(&self, r: PinRef) -> Option<&'a Pin> {
        self.obj(r.0)?.pins().iter().find(|p| p.id == r.1)
    }

    fn pin_named(n: &'a Object, name: &str, output: bool) -> Option<&'a Pin> {
        n.pins().iter().find(|p| p.name == name && p.is_output() == output)
    }

    /// Follows exec links through reroute nodes.
    fn resolve_exec(&self, mut r: PinRef) -> Option<PinRef> {
        for _ in 0..64 {
            let n = self.obj(r.0)?;
            if n.class != "K2Node_Knot" {
                return Some(r);
            }
            let out = n.pins().iter().find(|p| p.is_output())?;
            let l = out.linked_to.first()?;
            r = (l.node, l.pin);
        }
        None
    }

    fn exec_target(&self, n: &Object, pin: &Pin) -> Option<PinRef> {
        let _ = n;
        let l = pin.linked_to.first()?;
        self.resolve_exec((l.node, l.pin))
    }

    fn exec_outputs(n: &'a Object) -> Vec<&'a Pin> {
        n.pins().iter().filter(|p| is_exec(p) && p.is_output()).collect()
    }

    fn exec_inputs(n: &'a Object) -> Vec<&'a Pin> {
        n.pins().iter().filter(|p| is_exec(p) && !p.is_output()).collect()
    }

    fn fresh(&mut self, base: &str) -> String {
        let base = ident(base);
        let mut name = base.clone();
        let mut i = 2;
        while self.used_names.contains(&name) {
            name = format!("{base}{i}");
            i += 1;
        }
        self.used_names.insert(name.clone());
        name
    }

    fn label_for(&mut self, n: i32) -> String {
        if let Some(l) = self.labels.get(&n) {
            return l.clone();
        }
        let title = self.obj(n).map(|o| node_title(self.c, o)).unwrap_or_default();
        let l = format!("L{}_{}", self.labels.len() + 1, ident(&title));
        self.labels.insert(n, l.clone());
        self.pending.push(n);
        l
    }

    // ------------------------------------------------------------------------------------------
    // Expressions

    fn short_object(&self, path: &str) -> String {
        // "/Script/Engine.Actor" -> Actor ; asset paths are kept
        if let Some(rest) = path.strip_prefix("/Script/") {
            return rest.rsplit('.').next().unwrap_or(rest).to_string();
        }
        path.to_string()
    }

    fn literal(&self, p: &Pin) -> Option<Expr> {
        let cat = p.pin_type.category.as_str();
        if p.default_object != 0 {
            return Some(Expr::lit(self.short_object(&self.c.obj_ref(p.default_object))));
        }
        if let Some(t) = &p.default_text {
            return Some(Expr::lit(crate::render::json::quote(&t.display())));
        }
        let d = p.default_value.as_str();
        if d.is_empty() {
            return None;
        }
        Some(Expr::lit(match cat {
            "string" | "name" => crate::render::json::quote(d),
            "text" => crate::render::json::quote(d),
            "object" | "class" | "softobject" | "softclass" | "interface" => self.short_object(d),
            "struct" => {
                // GameplayTag defaults: (TagName="A.B")
                if let Some(rest) = d.strip_prefix("(TagName=\"").and_then(|r| r.strip_suffix("\")")) {
                    crate::render::json::quote(rest)
                } else if d.starts_with('(') {
                    tidy_numbers(d)
                } else {
                    format!("({})", tidy_numbers(d))
                }
            }
            _ => tidy_numbers(d),
        }))
    }

    /// Expression for an input pin (None when unconnected and left at its default).
    fn input(&mut self, p: &'a Pin) -> Option<Expr> {
        if let Some(l) = p.linked_to.first() {
            return Some(self.output((l.node, l.pin)));
        }
        if p.default_value == p.autogenerated_default && p.default_object == 0 && p.default_text.is_none() && !self.c.opts.verbose {
            return None;
        }
        self.literal(p)
    }

    fn input_named(&mut self, n: &'a Object, name: &str) -> Option<Expr> {
        let p = Self::pin_named(n, name, false)?;
        self.input(p)
    }

    /// Data arguments of a node (all visible non-exec inputs).
    fn args(&mut self, n: &'a Object, skip: &[&str]) -> Vec<(String, Expr)> {
        let mut v = vec![];
        for p in n.pins() {
            if p.is_output() || is_exec(p) || p.name == "self" || skip.contains(&p.name.as_str()) {
                continue;
            }
            if p.hidden && p.linked_to.is_empty() {
                continue;
            }
            if p.parent_pin.is_some() && p.linked_to.is_empty() && p.default_value == p.autogenerated_default {
                continue;
            }
            if let Some(e) = self.input(p) {
                v.push((crate::property::clean_field_name(&p.name), e));
            }
        }
        v
    }

    fn data_outputs(n: &'a Object) -> Vec<&'a Pin> {
        n.pins().iter().filter(|p| p.is_output() && !is_exec(p) && !p.hidden).collect()
    }

    fn function_parts(&self, n: &Object) -> (String, i32, bool) {
        for k in ["FunctionReference", "DelegateReference"] {
            let (name, parent, self_ctx) = member_name(n.prop(k));
            if !name.is_empty() {
                return (name, parent, self_ctx);
            }
        }
        (String::new(), 0, false)
    }

    /// `target.Func(args)` for call-like nodes.
    fn call_expr(&mut self, n: &'a Object) -> Expr {
        let (func, parent, _self_ctx) = self.function_parts(n);
        if func.is_empty() {
            let title = node_title(self.c, n);
            let args = self.args(n, &[]);
            let target = Self::pin_named(n, "self", false).and_then(|p| if p.linked_to.is_empty() { None } else { self.input(p) });
            return Expr::Call { target: target.map(Box::new), func: ident(&title), args };
        }
        let func_short = ident(func.strip_prefix("K2_").unwrap_or(&func));
        let parent_name = if parent != 0 { self.c.name(parent) } else { String::new() };
        // operators
        if IMPLICIT_LIBRARIES.contains(&parent_name.as_str()) || n.class == "K2Node_CommutativeAssociativeBinaryOperator" || n.class == "K2Node_PromotableOperator" {
            if let Some(e) = self.operator(n, &func) {
                return e;
            }
        }
        let mut target: Option<Expr> = None;
        if n.class == "K2Node_CallParentFunction" {
            target = Some(Expr::var("super"));
        } else if let Some(sp) = Self::pin_named(n, "self", false) {
            if !sp.linked_to.is_empty() {
                target = self.input(sp);
            }
        } else if !parent_name.is_empty() && !IMPLICIT_LIBRARIES.contains(&parent_name.as_str()) {
            target = Some(Expr::var(parent_name.clone()));
        }
        let args = self.args(n, &[]);
        Expr::Call { target: target.map(Box::new), func: func_short, args }
    }

    fn operator(&mut self, n: &'a Object, func: &str) -> Option<Expr> {
        if func == "Not_PreBool" {
            let a = self.input_named(n, "A").unwrap_or(Expr::lit("false"));
            return Some(Expr::Unary("!".into(), Box::new(a)));
        }
        if let Some(rest) = func.strip_prefix("Conv_") {
            // implicit conversions: keep the value
            let _ = rest;
            let inputs: Vec<&Pin> = n.pins().iter().filter(|p| !p.is_output() && !is_exec(p) && p.name != "self").collect();
            if inputs.len() == 1 {
                return self.input(inputs[0]).or(Some(Expr::lit("?")));
            }
        }
        if func.starts_with("MakeLiteral") {
            return self.input_named(n, "Value").or(Some(Expr::lit("\"\"")));
        }
        let op = BINARY_OPS.iter().find(|(p, _)| func.starts_with(p)).map(|(_, o)| *o)?;
        let inputs: Vec<&'a Pin> = n.pins().iter().filter(|p| !p.is_output() && !is_exec(p) && p.name != "self" && !p.hidden).collect();
        let operands: Vec<&'a Pin> = inputs.iter().copied().filter(|p| p.name.len() <= 2 || p.name.starts_with('A') || p.name.starts_with('B')).collect();
        if operands.len() < 2 {
            return None;
        }
        let mut it = operands.into_iter();
        let first = it.next()?;
        let mut e = self.input(first).or_else(|| self.literal(first)).unwrap_or(Expr::lit("0"));
        for p in it {
            let rhs = self.input(p).or_else(|| self.literal(p)).unwrap_or(Expr::lit("0"));
            e = Expr::Binary(op.into(), Box::new(e), Box::new(rhs));
        }
        Some(e)
    }

    /// Expression for an output pin of a node.
    fn output(&mut self, r: PinRef) -> Expr {
        self.depth += 1;
        let e = if self.depth > 64 { Expr::lit("<...>") } else { self.output_inner(r) };
        self.depth -= 1;
        e
    }

    fn output_inner(&mut self, r: PinRef) -> Expr {
        if let Some(t) = self.temps.get(&r) {
            return Expr::var(t.clone());
        }
        let Some(n) = self.obj(r.0) else { return Expr::lit("?") };
        let Some(p) = self.pin(r) else { return Expr::lit("?") };
        // Split struct pins ("Health_CurrentValue") are members of their parent pin
        if let Some(parent) = p.parent_pin {
            if let Some(pp) = self.pin((parent.node, parent.pin)) {
                let field = p.name.strip_prefix(&format!("{}_", pp.name)).unwrap_or(&p.name).to_string();
                let base = self.output((parent.node, parent.pin));
                return Expr::Member(Box::new(base), crate::property::clean_field_name(&field));
            }
        }
        let pin_name = crate::property::clean_field_name(&p.name);
        let outs = Self::data_outputs(n);
        let single = outs.len() <= 1;
        let with_member = |e: Expr, single: bool| if single || pin_name == "ReturnValue" { e } else { Expr::Member(Box::new(e), pin_name.clone()) };
        match n.class.as_str() {
            "K2Node_Knot" => {
                let inp = n.pins().iter().find(|p| !p.is_output());
                return inp.and_then(|i| self.input(i)).unwrap_or(Expr::lit("?"));
            }
            "K2Node_Self" => return Expr::SelfRef,
            "K2Node_VariableGet" | "K2Node_VariableSet" => {
                let (name, _, _) = member_name(n.prop("VariableReference"));
                let name = if name.is_empty() { pin_name.clone() } else { name };
                if let Some(sp) = Self::pin_named(n, "self", false) {
                    if !sp.linked_to.is_empty() {
                        let t = self.input(sp).unwrap_or(Expr::SelfRef);
                        return Expr::Member(Box::new(t), name);
                    }
                }
                return Expr::var(ident(&name));
            }
            "K2Node_Event" | "K2Node_CustomEvent" | "K2Node_ComponentBoundEvent" | "K2Node_ActorBoundEvent" | "K2Node_FunctionEntry"
            | "K2Node_InputAction" | "K2Node_InputKey" | "K2Node_InputAxisEvent" | "K2Node_EnhancedInputAction" | "K2Node_Tunnel" => {
                if pin_name == "OutputDelegate" {
                    return Expr::var(format!("&{}", self.event_name(n)));
                }
                return Expr::var(ident(&pin_name));
            }
            "K2Node_CreateDelegate" => {
                let f = n.prop("SelectedFunctionName").and_then(|v| v.as_str()).unwrap_or("?");
                return Expr::var(format!("&{f}"));
            }
            "K2Node_DynamicCast" | "K2Node_ClassDynamicCast" => {
                let t = n.prop("TargetType").and_then(|v| v.as_object()).map(|i| self.c.name(i)).unwrap_or("?".into());
                let obj = self.input_named(n, "Object").or_else(|| self.input_named(n, "Class")).unwrap_or(Expr::lit("?"));
                if pin_name == "bSuccess" {
                    return Expr::call(format!("isa<{t}>"), vec![(String::new(), obj)]);
                }
                return Expr::Cast(t, Box::new(obj));
            }
            "K2Node_MakeStruct" | "K2Node_MakeStructure" => {
                let t = n.prop("StructType").and_then(|v| v.as_object()).map(|i| self.c.name(i)).unwrap_or("Struct".into());
                let args = self.args(n, &[]);
                return Expr::Struct(t, args);
            }
            "K2Node_BreakStruct" => {
                let inp = n.pins().iter().find(|p| !p.is_output() && !is_exec(p));
                let base = inp.and_then(|i| self.input(i)).unwrap_or(Expr::lit("?"));
                return Expr::Member(Box::new(base), pin_name);
            }
            "K2Node_MakeArray" | "K2Node_MakeSet" => {
                let args = self.args(n, &[]);
                return Expr::List(args.into_iter().map(|(_, e)| e).collect());
            }
            "K2Node_GetArrayItem" => {
                let ins: Vec<&Pin> = n.pins().iter().filter(|p| !p.is_output() && !is_exec(p)).collect();
                let a = ins.first().and_then(|p| self.input(p)).unwrap_or(Expr::lit("?"));
                let i = ins.get(1).and_then(|p| self.input(p).or_else(|| self.literal(p))).unwrap_or(Expr::lit("0"));
                return Expr::Index(Box::new(a), Box::new(i));
            }
            "K2Node_Literal" => {
                let v = n.prop("ObjectRef").map(|v| self.c.inline(v, 200).unwrap_or_default()).unwrap_or_default();
                return Expr::lit(self.short_object(&v));
            }
            "K2Node_EnumLiteral" => return self.input_named(n, "Enum").unwrap_or(Expr::lit("?")),
            "K2Node_FormatText" => {
                let fmt = self.input_named(n, "Format").or_else(|| Self::pin_named(n, "Format", false).and_then(|p| self.literal(p))).unwrap_or(Expr::lit("\"\""));
                let mut args = vec![(String::new(), fmt)];
                args.extend(self.args(n, &["Format"]));
                return Expr::call("Format", args);
            }
            "K2Node_Select" => {
                let mut args = vec![];
                for p in n.pins().iter().filter(|p| !p.is_output() && !is_exec(p) && !p.hidden) {
                    let e = self.input(p).or_else(|| self.literal(p)).unwrap_or(Expr::lit("default"));
                    args.push((p.name.clone(), e));
                }
                return Expr::call("select", args);
            }
            "K2Node_EnumEquality" | "K2Node_EnumInequality" => {
                let ins: Vec<&Pin> = n.pins().iter().filter(|p| !p.is_output() && !is_exec(p)).collect();
                let a = ins.first().and_then(|p| self.input(p).or_else(|| self.literal(p))).unwrap_or(Expr::lit("?"));
                let b = ins.get(1).and_then(|p| self.input(p).or_else(|| self.literal(p))).unwrap_or(Expr::lit("?"));
                let op = if n.class == "K2Node_EnumEquality" { "==" } else { "!=" };
                return Expr::Binary(op.into(), Box::new(a), Box::new(b));
            }
            "K2Node_PropertyAccess" => {
                let path = match n.prop("TextPath") {
                    Some(Value::Text(t)) if !t.display().is_empty() => t.display(),
                    _ => n.prop("Path").and_then(|v| v.as_array()).unwrap_or(&[]).iter().filter_map(|x| x.as_str()).collect::<Vec<_>>().join("."),
                };
                return Expr::var(if path.is_empty() { "PropertyAccess".to_string() } else { path });
            }
            "K2Node_GetSubsystem" | "K2Node_GetSubsystemFromPC" | "K2Node_GetEngineSubsystem" | "K2Node_GetEditorSubsystem" | "K2Node_GetWorldSubsystem" => {
                let cls = n.prop("CustomClass").and_then(|v| v.as_object()).map(|i| self.c.name(i)).unwrap_or("?".into());
                let args = self.args(n, &[]);
                return Expr::call(format!("GetSubsystem<{cls}>"), args);
            }
            "K2Node_MacroInstance" => {
                let m = macro_name(self.c, n);
                let args = self.args(n, &[]);
                let e = Expr::call(ident(&m), args);
                return with_member(e, single);
            }
            "K2Node_Composite" => {
                let g = n.prop("BoundGraph").and_then(|v| v.as_object()).map(|i| self.c.name(i)).unwrap_or("Collapsed".into());
                let args = self.args(n, &[]);
                let e = Expr::call(ident(&g), args);
                return with_member(e, single);
            }
            _ => {}
        }
        if n.class.starts_with("AnimGraphNode_") {
            let e = self.anim_node_expr(n);
            return with_member(e, single || p.pin_type.sub_category_object != 0 && pin_name == "Pose");
        }
        let e = self.call_expr(n);
        with_member(e, single)
    }

    fn event_name(&self, n: &Object) -> String {
        let custom = n.prop("CustomFunctionName").and_then(|v| v.as_str()).unwrap_or("");
        let bound = matches!(n.class.as_str(), "K2Node_ComponentBoundEvent" | "K2Node_ActorBoundEvent");
        if !custom.is_empty() && custom != "None" && !bound {
            return custom.to_string();
        }
        if let Some(ia) = n.prop("InputAction").and_then(|v| v.as_object()) {
            return self.c.name(ia);
        }
        for k in ["InputActionName", "InputAxisName"] {
            if let Some(a) = n.prop(k).and_then(|v| v.as_str()) {
                return a.to_string();
            }
        }
        match n.class.as_str() {
            "K2Node_ComponentBoundEvent" | "K2Node_ActorBoundEvent" => {
                let d = n.prop("DelegatePropertyName").and_then(|v| v.as_str()).unwrap_or("?");
                let comp = n.prop("ComponentPropertyName").and_then(|v| v.as_str()).map(|s| s.to_string()).unwrap_or_else(|| {
                    n.prop("EventOwner").and_then(|v| v.as_object()).map(|i| self.c.name(i)).unwrap_or_default()
                });
                format!("{comp}.{d}")
            }
            _ => {
                let (name, _, _) = member_name(n.prop("EventReference").or(n.prop("FunctionReference")));
                if name.is_empty() {
                    node_title(self.c, n)
                } else {
                    name
                }
            }
        }
    }

    // ------------------------------------------------------------------------------------------
    // Statements

    fn is_latent(n: &Object) -> bool {
        n.pins().iter().any(|p| p.name == "LatentInfo")
    }

    /// Captures linked data outputs of an impure node into variables.
    fn capture(&mut self, n: &'a Object, e: Expr) -> Stmt {
        let linked: Vec<&Pin> = Self::data_outputs(n).into_iter().filter(|p| !p.linked_to.is_empty()).collect();
        if linked.is_empty() {
            return if Self::is_latent(n) { Stmt::Await(e) } else { Stmt::Expr(e) };
        }
        let func = self.function_parts(n).0;
        let mut names = vec![];
        for p in &linked {
            let base = if p.name == "ReturnValue" {
                if func.is_empty() { "Result".to_string() } else { format!("{}Result", func.strip_prefix("K2_").unwrap_or(&func)) }
            } else {
                p.name.clone()
            };
            let name = self.fresh(&base);
            self.temps.insert((n.index, p.id), name.clone());
            names.push(name);
        }
        let e = if Self::is_latent(n) { Expr::Unary("await ".into(), Box::new(e)) } else { e };
        Stmt::Let(names, e)
    }

    fn reachable(&self, start: PinRef, limit: usize) -> HashMap<i32, usize> {
        let mut dist = HashMap::new();
        let mut q = VecDeque::new();
        q.push_back((start.0, 0usize));
        while let Some((n, d)) = q.pop_front() {
            if dist.contains_key(&n) || dist.len() > limit {
                continue;
            }
            dist.insert(n, d);
            let Some(o) = self.obj(n) else { continue };
            for p in Self::exec_outputs(o) {
                for l in &p.linked_to {
                    if let Some((t, _)) = self.resolve_exec((l.node, l.pin)) {
                        q.push_back((t, d + 1));
                    }
                }
            }
        }
        dist
    }

    /// Nearest node reached by every branch (where the branches merge again).
    fn find_join(&self, starts: &[Option<PinRef>]) -> Option<i32> {
        let live: Vec<PinRef> = starts.iter().flatten().copied().collect();
        if live.len() < 2 {
            return None;
        }
        let maps: Vec<HashMap<i32, usize>> = live.iter().map(|s| self.reachable(*s, 4000)).collect();
        let mut best: Option<(usize, i32)> = None;
        for (n, d0) in &maps[0] {
            if self.loops.contains(n) {
                continue;
            }
            let mut maxd = *d0;
            let mut ok = true;
            for m in &maps[1..] {
                match m.get(n) {
                    Some(d) => maxd = maxd.max(*d),
                    None => {
                        ok = false;
                        break;
                    }
                }
            }
            if ok && best.map(|(b, _)| maxd < b).unwrap_or(true) {
                best = Some((maxd, *n));
            }
        }
        best.map(|(_, n)| n)
    }

    /// Emits a chain of exec nodes starting at `start`.
    fn chain(&mut self, start: Option<PinRef>, stop: &HashSet<i32>, allow: Option<i32>) -> Vec<Stmt> {
        let mut out = vec![];
        let mut cur = start;
        let mut first = true;
        while let Some((n, pin)) = cur {
            if stop.contains(&n) {
                break;
            }
            if !self.set.contains(&n) {
                out.push(Stmt::Comment(format!("continues in {}", self.c.pkg.export_rel_path(n))));
                break;
            }
            if let Some(pos) = self.loops.iter().rposition(|l| *l == n) {
                let _ = pos;
                let pname = self.pin((n, pin)).map(|p| p.name.clone()).unwrap_or_default();
                if pname == "Break" {
                    out.push(Stmt::Break);
                } else {
                    let l = self.label_for(n);
                    out.push(Stmt::Goto(l));
                }
                break;
            }
            let shared = self.incoming.get(&n).copied().unwrap_or(0) > 1 && !(first && allow == Some(n));
            if shared || self.labels.contains_key(&n) && !(first && allow == Some(n)) || self.on_path.contains(&n) || self.emitted.contains(&n) {
                let l = self.label_for(n);
                out.push(Stmt::Goto(l));
                break;
            }
            first = false;
            self.emitted.insert(n);
            self.on_path.push(n);
            cur = self.node(n, pin, stop, &mut out);
            self.on_path.pop();
        }
        out
    }

    fn target_of(&self, n: &'a Object, pin_name: &str) -> Option<PinRef> {
        let p = Self::pin_named(n, pin_name, true)?;
        self.exec_target(n, p)
    }

    /// Emits branches of a structured node, returning (bodies, join).
    fn branches(&mut self, starts: &[Option<PinRef>], stop: &HashSet<i32>) -> (Vec<Vec<Stmt>>, Option<i32>) {
        let join = self.find_join(starts);
        let mut st = stop.clone();
        if let Some(j) = join {
            st.insert(j);
        }
        let bodies = starts.iter().map(|s| self.chain(*s, &st, None)).collect();
        (bodies, join)
    }

    fn after_join(&mut self, join: Option<i32>, stop: &HashSet<i32>, out: &mut Vec<Stmt>) {
        if let Some(j) = join {
            // continue after the merge point; incoming links beyond the branches make it shared
            let st = stop.clone();
            let entry = self.obj(j).and_then(|o| Self::exec_inputs(o).first().map(|p| (j, p.id)));
            let body = self.chain_allow_join(entry, &st, j);
            out.extend(body);
        }
    }

    fn chain_allow_join(&mut self, start: Option<PinRef>, stop: &HashSet<i32>, join: i32) -> Vec<Stmt> {
        // Allow the join node itself even though it has several predecessors.
        let saved = self.incoming.get(&join).copied();
        let reached_elsewhere = self.labels.contains_key(&join);
        if !reached_elsewhere {
            self.incoming.insert(join, 1);
        }
        let r = self.chain(start, stop, Some(join));
        if let Some(s) = saved {
            self.incoming.insert(join, s);
        }
        r
    }

    /// Emits one exec node; returns the next node of a linear chain.
    fn node(&mut self, ni: i32, entered: Guid, stop: &HashSet<i32>, out: &mut Vec<Stmt>) -> Option<PinRef> {
        let n = self.obj(ni)?;
        let class = n.class.as_str();
        let entered_name = n.pins().iter().find(|p| p.id == entered).map(|p| p.name.clone()).unwrap_or_default();
        let exec_ins = Self::exec_inputs(n);
        let exec_outs = Self::exec_outputs(n);
        if let Some(Value::Str(cm)) = n.prop("NodeComment") {
            if !cm.trim().is_empty() {
                out.push(Stmt::Comment(cm.replace('\r', "").replace('\n', " / ")));
            }
        }
        if let Some(Value::Enum(e)) = n.prop("EnabledState") {
            if e.ends_with("Disabled") {
                out.push(Stmt::Comment("disabled node:".into()));
            }
        }
        // Secondary exec inputs (DoOnce.Reset, Gate.Open, Timeline.Reverse ...)
        if exec_ins.len() > 1 && exec_ins.first().map(|p| p.id != entered).unwrap_or(false) && class != "K2Node_Tunnel" {
            let head = self.handle_name(n);
            out.push(Stmt::Expr(Expr::Call { target: Some(Box::new(Expr::var(head))), func: ident(&entered_name), args: vec![] }));
            return None;
        }
        let then = |s: &Self| exec_outs.iter().find(|p| matches!(p.name.as_str(), "then" | "Then" | "execute" | "Exec" | "OutputPin")).and_then(|p| s.exec_target(n, p));

        match class {
            "K2Node_FunctionResult" => {
                let mut args = vec![];
                for p in n.pins().iter().filter(|p| !p.is_output() && !is_exec(p) && !p.hidden) {
                    let e = self.input(p).or_else(|| self.literal(p)).unwrap_or(Expr::lit("default"));
                    args.push((p.name.clone(), e));
                }
                out.push(Stmt::Return(args));
                return None;
            }
            "K2Node_Tunnel" => {
                // macro output tunnel
                let args = self.args(n, &[]);
                let mut a = vec![(String::new(), Expr::lit(crate::render::json::quote(&entered_name)))];
                a.extend(args);
                out.push(Stmt::Expr(Expr::call("exit", a)));
                return None;
            }
            "K2Node_VariableSet" => {
                let (name, _, _) = member_name(n.prop("VariableReference"));
                let value_pin = n.pins().iter().find(|p| !p.is_output() && !is_exec(p) && p.name != "self");
                let value = value_pin.and_then(|p| self.input(p).or_else(|| self.literal(p))).unwrap_or(Expr::lit("default"));
                let lhs = match Self::pin_named(n, "self", false) {
                    Some(sp) if !sp.linked_to.is_empty() => Expr::Member(Box::new(self.input(sp).unwrap_or(Expr::SelfRef)), name),
                    _ => Expr::var(ident(&name)),
                };
                out.push(Stmt::Assign(lhs, value));
                return then(self);
            }
            "K2Node_IfThenElse" => {
                let cond = self.input_named(n, "Condition").unwrap_or(Expr::lit("false"));
                let (b, join) = self.branches(&[self.target_of(n, "then"), self.target_of(n, "else")], stop);
                let mut it = b.into_iter();
                let (t, e) = (it.next().unwrap_or_default(), it.next().unwrap_or_default());
                if t.is_empty() && !e.is_empty() {
                    out.push(Stmt::If { cond: negate(cond), then: e, els: vec![] });
                } else {
                    out.push(Stmt::If { cond, then: t, els: e });
                }
                self.after_join(join, stop, out);
                return None;
            }
            "K2Node_ExecutionSequence" | "K2Node_MultiGate" if class == "K2Node_ExecutionSequence" => {
                for p in exec_outs {
                    let s = self.exec_target(n, p);
                    let body = self.chain(s, stop, None);
                    out.extend(body);
                }
                return None;
            }
            "K2Node_DynamicCast" | "K2Node_ClassDynamicCast" if !exec_outs.is_empty() => {
                let t = n.prop("TargetType").and_then(|v| v.as_object()).map(|i| self.c.name(i)).unwrap_or("?".into());
                let obj = self.input_named(n, "Object").or_else(|| self.input_named(n, "Class")).unwrap_or(Expr::lit("?"));
                let as_pin = n.pins().iter().find(|p| p.is_output() && !is_exec(p) && p.name != "bSuccess");
                let var = self.fresh(&as_pin.map(|p| p.name.clone()).unwrap_or("Casted".into()));
                if let Some(ap) = as_pin {
                    self.temps.insert((ni, ap.id), var.clone());
                }
                if let Some(sp) = Self::pin_named(n, "bSuccess", true) {
                    self.temps.insert((ni, sp.id), format!("({var} != null)"));
                }
                let (b, join) = self.branches(&[self.target_of(n, "then"), self.target_of(n, "CastFailed")], stop);
                let mut it = b.into_iter();
                out.push(Stmt::IfLet { name: var, value: Expr::Cast(t, Box::new(obj)), then: it.next().unwrap_or_default(), els: it.next().unwrap_or_default() });
                self.after_join(join, stop, out);
                return None;
            }
            _ => {}
        }
        if class.starts_with("K2Node_Switch") {
            let value = self.input_named(n, "Selection").unwrap_or(Expr::lit("?"));
            let starts: Vec<Option<PinRef>> = exec_outs.iter().map(|p| self.exec_target(n, p)).collect();
            let (bodies, join) = self.branches(&starts, stop);
            let cases = exec_outs.iter().zip(bodies).filter(|(_, b)| !b.is_empty()).map(|(p, b)| (crate::render::json::quote(&p.name), b)).collect();
            out.push(Stmt::Switch { value, cases });
            self.after_join(join, stop, out);
            return None;
        }
        if class == "K2Node_MacroInstance" {
            let m = macro_name(self.c, n);
            if let Some(next) = self.macro_stmt(n, &m, stop, out) {
                return next;
            }
        }
        // Delegates
        match class {
            "K2Node_AddDelegate" | "K2Node_RemoveDelegate" | "K2Node_AssignDelegate" | "K2Node_ClearDelegate" | "K2Node_CallDelegate" => {
                let (name, _, _) = member_name(n.prop("DelegateReference"));
                let target = match Self::pin_named(n, "self", false) {
                    Some(sp) if !sp.linked_to.is_empty() => Expr::Member(Box::new(self.input(sp).unwrap_or(Expr::SelfRef)), name),
                    _ => Expr::var(ident(&name)),
                };
                let s = match class {
                    "K2Node_AddDelegate" | "K2Node_AssignDelegate" => {
                        let d = self.input_named(n, "Delegate").unwrap_or(Expr::lit("?"));
                        Stmt::Expr(Expr::Binary("+=".into(), Box::new(target), Box::new(d)))
                    }
                    "K2Node_RemoveDelegate" => {
                        let d = self.input_named(n, "Delegate").unwrap_or(Expr::lit("?"));
                        Stmt::Expr(Expr::Binary("-=".into(), Box::new(target), Box::new(d)))
                    }
                    "K2Node_ClearDelegate" => Stmt::Expr(Expr::Call { target: Some(Box::new(target)), func: "Clear".into(), args: vec![] }),
                    _ => {
                        let args = self.args(n, &["Delegate"]);
                        Stmt::Expr(Expr::Call { target: Some(Box::new(target)), func: "Broadcast".into(), args })
                    }
                };
                out.push(s);
                if class == "K2Node_AssignDelegate" {
                    // the bound custom event is part of this node in the graph; continue
                }
                return then(self);
            }
            _ => {}
        }

        // Generic call-like node
        let call = if class == "K2Node_Timeline" {
            let name = n.prop("TimelineName").and_then(|v| v.as_str()).unwrap_or("Timeline").to_string();
            Expr::Call { target: Some(Box::new(Expr::var(ident(&name)))), func: ident(&entered_name), args: self.args(n, &[]) }
        } else {
            self.call_expr(n)
        };
        let primary: Vec<&Pin> = exec_outs.iter().copied().filter(|p| matches!(p.name.as_str(), "then" | "Then" | "execute" | "Exec" | "OutputPin")).collect();
        let others: Vec<&Pin> = exec_outs.iter().copied().filter(|p| !primary.iter().any(|x| x.id == p.id)).collect();
        let stmt = self.capture(n, call);
        if others.iter().all(|p| p.linked_to.is_empty()) {
            out.push(stmt);
            return then(self);
        }
        // several exec outputs: handlers
        let head = match &stmt {
            Stmt::Expr(e) | Stmt::Await(e) => e.clone(),
            Stmt::Let(names, e) => {
                out.push(Stmt::Let(names.clone(), e.clone()));
                Expr::var(names.join(", "))
            }
            _ => Expr::lit("?"),
        };
        let mut handlers = vec![];
        for p in &others {
            if p.linked_to.is_empty() {
                continue;
            }
            let s = self.exec_target(n, p);
            let body = self.chain(s, stop, None);
            handlers.push((p.name.clone(), body));
        }
        out.push(Stmt::Handlers { head, handlers });
        then(self)
    }

    fn handle_name(&self, n: &Object) -> String {
        if n.class == "K2Node_Timeline" {
            return n.prop("TimelineName").and_then(|v| v.as_str()).unwrap_or("Timeline").to_string();
        }
        let base = ident(&node_title(self.c, n).replace("Macro ", ""));
        format!("{base}_{}", self.c.pkg.exports[n.index as usize - 1].object_name.rsplit('_').next().unwrap_or("0"))
    }

    fn macro_stmt(&mut self, n: &'a Object, m: &str, stop: &HashSet<i32>, out: &mut Vec<Stmt>) -> Option<Option<PinRef>> {
        let t = |s: &Self, name: &str| s.target_of(n, name);
        match m {
            "IsValid" | "IsValidClass" => {
                let obj = self.input_named(n, "InputObject").or_else(|| self.input_named(n, "InputClass")).unwrap_or(Expr::lit("?"));
                let (b, join) = self.branches(&[t(self, "Is Valid"), t(self, "Is Not Valid")], stop);
                let mut it = b.into_iter();
                let (yes, no) = (it.next().unwrap_or_default(), it.next().unwrap_or_default());
                let cond = Expr::call("IsValid", vec![(String::new(), obj)]);
                if yes.is_empty() && !no.is_empty() {
                    out.push(Stmt::If { cond: negate(cond), then: no, els: vec![] });
                } else {
                    out.push(Stmt::If { cond, then: yes, els: no });
                }
                self.after_join(join, stop, out);
                Some(None)
            }
            "Switch Has Authority" => {
                let (b, join) = self.branches(&[t(self, "Authority"), t(self, "Remote")], stop);
                let mut it = b.into_iter();
                let (a, r) = (it.next().unwrap_or_default(), it.next().unwrap_or_default());
                let cond = Expr::call("HasAuthority", vec![]);
                if a.is_empty() && !r.is_empty() {
                    out.push(Stmt::If { cond: negate(cond), then: r, els: vec![] });
                } else {
                    out.push(Stmt::If { cond, then: a, els: r });
                }
                self.after_join(join, stop, out);
                Some(None)
            }
            "ForEachLoop" | "ForEachLoopWithBreak" | "ReverseForEachLoop" => {
                let arr = self.input_named(n, "Array").unwrap_or(Expr::lit("?"));
                let elem = self.fresh("Element");
                let idx = self.fresh("Index");
                for p in n.pins().iter().filter(|p| p.is_output() && !is_exec(p)) {
                    let v = if p.name.contains("Index") { idx.clone() } else { elem.clone() };
                    self.temps.insert((n.index, p.id), v);
                }
                let rev = if m == "ReverseForEachLoop" { "reverse " } else { "" };
                let header = format!("for ({elem}, {idx} in {rev}{})", arr.to_code());
                self.loops.push(n.index);
                let body = self.chain(t(self, "LoopBody"), stop, None);
                self.loops.pop();
                let completed = self.chain(t(self, "Completed"), stop, None);
                out.push(Stmt::Loop { header, body, completed });
                Some(None)
            }
            "ForLoop" | "ForLoopWithBreak" => {
                let first = self.input_named(n, "FirstIndex").or_else(|| Self::pin_named(n, "FirstIndex", false).and_then(|p| self.literal(p))).unwrap_or(Expr::lit("0"));
                let last = self.input_named(n, "LastIndex").or_else(|| Self::pin_named(n, "LastIndex", false).and_then(|p| self.literal(p))).unwrap_or(Expr::lit("0"));
                let idx = self.fresh("Index");
                for p in n.pins().iter().filter(|p| p.is_output() && !is_exec(p)) {
                    self.temps.insert((n.index, p.id), idx.clone());
                }
                let header = format!("for ({idx} in {}..={})", first.to_code(), last.to_code());
                self.loops.push(n.index);
                let body = self.chain(t(self, "LoopBody"), stop, None);
                self.loops.pop();
                let completed = self.chain(t(self, "Completed"), stop, None);
                out.push(Stmt::Loop { header, body, completed });
                Some(None)
            }
            "WhileLoop" => {
                let cond = self.input_named(n, "Condition").unwrap_or(Expr::lit("false"));
                let header = format!("while ({})", cond.to_code());
                self.loops.push(n.index);
                let body = self.chain(t(self, "LoopBody"), stop, None);
                self.loops.pop();
                let completed = self.chain(t(self, "Completed"), stop, None);
                out.push(Stmt::Loop { header, body, completed });
                Some(None)
            }
            _ => None,
        }
    }

    // ------------------------------------------------------------------------------------------
    // Entry points

    fn is_entry(n: &'a Object) -> bool {
        matches!(
            n.class.as_str(),
            "K2Node_Event" | "K2Node_CustomEvent" | "K2Node_ComponentBoundEvent" | "K2Node_ActorBoundEvent" | "K2Node_FunctionEntry"
                | "K2Node_InputAction" | "K2Node_InputKey" | "K2Node_InputAxisEvent" | "K2Node_InputAxisKeyEvent"
                | "K2Node_InputVectorAxisEvent" | "K2Node_InputTouch" | "K2Node_EnhancedInputAction" | "K2Node_GeneratedBoundEvent"
        ) || (n.class == "K2Node_Tunnel" && n.prop("bCanHaveOutputs").and_then(|v| v.as_bool()) == Some(true))
            || (n.class.contains("Event") && n.class.starts_with("K2Node") && Self::exec_inputs(n).is_empty() && !Self::exec_outputs(n).is_empty())
    }

    fn net_tags(n: &Object) -> Vec<String> {
        let mut tags = vec![];
        if n.prop("bOverrideFunction").and_then(|v| v.as_bool()) == Some(true) {
            tags.push("override".to_string());
        }
        if let Some(f) = n.prop("FunctionFlags").and_then(|v| v.as_i64()) {
            let f = f as u32;
            if f & 0x200000 != 0 {
                tags.push("server".into());
            }
            if f & 0x1000000 != 0 {
                tags.push("client".into());
            }
            if f & 0x4000 != 0 {
                tags.push("multicast".into());
            }
            if f & 0x80 != 0 {
                tags.push("reliable".into());
            }
        }
        tags
    }

    fn entry_function(&mut self, n: &'a Object, kind: &str, graph_name: &str, result_nodes: &[&'a Object]) -> Vec<Function> {
        let mut params = vec![];
        let is_function = matches!(n.class.as_str(), "K2Node_FunctionEntry" | "K2Node_Tunnel" | "K2Node_CustomEvent");
        for p in n.pins().iter().filter(|p| p.is_output() && !is_exec(p) && p.parent_pin.is_none()) {
            if p.name == "OutputDelegate" || (p.hidden && p.sub_pins.is_empty()) {
                continue;
            }
            // events: only parameters that are used (split pins count through their sub pins)
            let used = !p.linked_to.is_empty() || p.sub_pins.iter().any(|sp| self.pin((sp.node, sp.pin)).map(|x| !x.linked_to.is_empty()).unwrap_or(false));
            if !is_function && !used {
                continue;
            }
            self.used_names.insert(ident(&p.name));
            params.push((p.name.clone(), p.pin_type.display(self.c.pkg)));
        }
        let mut tags = Self::net_tags(n);
        let mut locals = vec![];
        let mut returns = vec![];
        let mut meta_text: Vec<(String, String)> = vec![];
        let mut name = self.event_name(n);
        let mut fkind = match n.class.as_str() {
            "K2Node_FunctionEntry" => kind.to_string(),
            "K2Node_Tunnel" => kind.to_string(),
            _ => "event".to_string(),
        };
        if n.class == "K2Node_FunctionEntry" {
            name = graph_name.to_string();
            if let Some(f) = n.prop("ExtraFlags").and_then(|v| v.as_i64()) {
                let f = f as u32;
                for (bit, t) in [(0x10000000u32, "pure"), (0x40000000, "const"), (0x2000, "static"), (0x40000, "private"), (0x80000, "protected")] {
                    if f & bit != 0 {
                        tags.push(t.into());
                    }
                }
                if f & 0x40 != 0 {
                    tags.extend(Self::net_tags(n));
                }
            }
            if let Some(md) = n.prop("MetaData").and_then(|v| v.as_struct()) {
                for f in &md.fields {
                    match &f.value {
                        Value::Bool(true) => tags.push(f.name.strip_prefix('b').unwrap_or(&f.name).to_string()),
                        Value::Text(t) if !t.display().is_empty() => meta_text.push((f.name.clone(), t.display())),
                        Value::Str(s) if !s.is_empty() => meta_text.push((f.name.clone(), s.clone())),
                        _ => {}
                    }
                }
            }
            if let Some(Value::Array(lv)) = n.prop("LocalVariables") {
                for l in lv {
                    if let Some(s) = l.as_struct() {
                        let lname = s.get("VarName").and_then(|x| x.as_str()).unwrap_or("?").to_string();
                        let ty = s.get("VarType").map(|t| crate::render::blueprint::pin_type_value(self.c, t)).unwrap_or_default();
                        self.used_names.insert(ident(&lname));
                        locals.push((lname, ty));
                    }
                }
            }
            if let Some(r) = result_nodes.first() {
                for p in r.pins().iter().filter(|p| !p.is_output() && !is_exec(p) && !p.hidden) {
                    returns.push((p.name.clone(), p.pin_type.display(self.c.pkg)));
                }
            }
        } else if n.class == "K2Node_Tunnel" {
            if kind != "collapsed" {
                fkind = "macro".into();
            }
            name = graph_name.to_string();
        } else if n.class == "K2Node_CustomEvent" {
            fkind = "custom event".into();
        }
        let outs = Self::exec_outputs(n);
        let mut fns = vec![];
        let linked: Vec<&Pin> = outs.iter().copied().filter(|p| !p.linked_to.is_empty()).collect();
        let mut preface = vec![];
        for (k, v) in &meta_text {
            match k.as_str() {
                "ToolTip" => preface.push(Stmt::Comment(v.replace('\r', "").replace('\n', " / "))),
                _ => tags.push(format!("{k}={}", crate::render::json::quote(v))),
            }
        }
        if linked.len() <= 1 {
            let start = linked.first().and_then(|p| self.exec_target(n, p));
            let mut body = preface;
            body.extend(self.chain(start, &HashSet::new(), None));
            fns.push(Function { kind: fkind, name, params, returns, tags, locals, body });
        } else {
            for p in linked {
                let start = self.exec_target(n, p);
                let body = self.chain(start, &HashSet::new(), None);
                fns.push(Function {
                    kind: fkind.clone(),
                    name: format!("{name}.{}", p.name),
                    params: params.clone(),
                    returns: returns.clone(),
                    tags: tags.clone(),
                    locals: locals.clone(),
                    body,
                });
            }
        }
        fns
    }

    /// Converts an exec graph (event graph, function, macro, collapsed graph) to a program.
    pub fn exec_program(&mut self, graph_name: &str, kind: &str) -> Program {
        let pos = |n: &Object| {
            (n.prop("NodePosY").and_then(|v| v.as_i64()).unwrap_or(0), n.prop("NodePosX").and_then(|v| v.as_i64()).unwrap_or(0))
        };
        let mut entries: Vec<&'a Object> = self.nodes.iter().copied().filter(|n| Self::is_entry(n)).collect();
        entries.sort_by_key(|n| (if n.class == "K2Node_FunctionEntry" || n.class == "K2Node_Tunnel" { 0 } else { 1 }, pos(n)));
        let results: Vec<&'a Object> = self.nodes.iter().copied().filter(|n| n.class == "K2Node_FunctionResult").collect();
        let mut prog = Program::default();
        for e in &entries {
            let fns = self.entry_function(e, kind, graph_name, &results);
            prog.functions.extend(fns);
        }
        // Exec chains not reachable from any entry (unconnected / disabled code)
        let mut orphans: Vec<&'a Object> = self
            .nodes
            .iter()
            .copied()
            .filter(|n| !self.emitted.contains(&n.index) && !Self::is_entry(n) && n.class != "K2Node_Knot")
            .filter(|n| !Self::exec_outputs(n).is_empty() || !Self::exec_inputs(n).is_empty())
            .filter(|n| Self::exec_inputs(n).iter().all(|p| p.linked_to.is_empty()))
            .collect();
        orphans.sort_by_key(|n| pos(n));
        for o in orphans {
            if self.emitted.contains(&o.index) {
                continue;
            }
            let start = Self::exec_inputs(o).first().map(|p| (o.index, p.id)).or(Some((o.index, Guid::default())));
            let body = self.chain(start, &HashSet::new(), None);
            prog.functions.push(Function { kind: "unconnected".into(), name: node_title(self.c, o), body, ..Default::default() });
        }
        // Shared blocks
        let mut i = 0;
        while i < self.pending.len() {
            let n = self.pending[i];
            i += 1;
            let label = self.labels[&n].clone();
            let start = self.obj(n).and_then(|o| Self::exec_inputs(o).first().map(|p| (n, p.id)));
            self.emitted.remove(&n);
            let body = self.chain_allow_join(start, &HashSet::new(), n);
            prog.blocks.push(Block { label, body });
        }
        prog
    }

    // ------------------------------------------------------------------------------------------
    // Pure dataflow graphs (anim graphs, rule graphs)

    fn anim_node_expr(&mut self, n: &'a Object) -> Expr {
        let short = n.class.strip_prefix("AnimGraphNode_").unwrap_or(&n.class).to_string();
        let mut args: Vec<(String, Expr)> = vec![];
        // Linked anim layers / state machines get their name first
        match short.as_str() {
            "StateMachine" => {
                let g = n.prop("EditorStateMachineGraph").and_then(|v| v.as_object()).map(|i| self.c.name(i)).unwrap_or_default();
                args.push((String::new(), Expr::lit(crate::render::json::quote(&g))));
            }
            "LinkedAnimLayer" | "LinkedAnimGraph" => {
                if let Some(node) = n.prop("Node").and_then(|v| v.as_struct()) {
                    if let Some(l) = node.get("Layer").and_then(|v| v.as_str()) {
                        args.push((String::new(), Expr::lit(l.to_string())));
                    }
                }
            }
            _ => {}
        }
        for p in n.pins() {
            if p.is_output() || is_exec(p) || (p.hidden && p.linked_to.is_empty()) {
                continue;
            }
            if let Some(e) = self.input(p) {
                args.push((p.name.clone(), e));
            }
        }
        // node settings
        if let Some(node) = n.prop("Node").or(n.prop("BlendNode")).and_then(|v| v.as_struct()) {
            for f in &node.fields {
                if matches!(f.name.as_str(), "Layer" | "Tag") || args.iter().any(|(a, _)| *a == f.name) {
                    continue;
                }
                let v = self.c.inline(&f.value, 160).unwrap_or_else(|| "{...}".into());
                args.push((f.name.clone(), Expr::lit(self.short_object(&v))));
            }
        }
        if let Some(Value::Name(tag)) | Some(Value::Str(tag)) = n.prop("Tag") {
            if !tag.is_empty() && tag != "None" {
                args.push(("Tag".into(), Expr::lit(crate::render::json::quote(tag))));
            }
        }
        Expr::call(short, args)
    }

    /// Number of consumers of each node's outputs inside the graph.
    fn use_counts(&self) -> HashMap<i32, usize> {
        let mut m = HashMap::new();
        for n in &self.nodes {
            for p in n.pins().iter().filter(|p| !p.is_output() && !is_exec(p)) {
                for l in &p.linked_to {
                    *m.entry(l.node).or_default() += 1;
                }
            }
        }
        m
    }

    /// SSA style program for pose / rule graphs: shared or large nodes become `let`s.
    pub fn dataflow_body(&mut self, roots: &[(&'a Object, &'a Pin, String)]) -> Vec<Stmt> {
        let uses = self.use_counts();
        let mut order: Vec<i32> = vec![];
        let mut seen = HashSet::new();
        fn visit<'x>(b: &GraphBuilder<'x, '_>, n: i32, seen: &mut HashSet<i32>, order: &mut Vec<i32>, depth: u32) {
            if depth > 500 || !seen.insert(n) {
                return;
            }
            let Some(o) = b.obj(n) else { return };
            for p in o.pins().iter().filter(|p| !p.is_output() && !is_exec(p)) {
                for l in &p.linked_to {
                    if let Some(src) = b.resolve_data(l.node) {
                        visit(b, src, seen, order, depth + 1);
                    }
                }
            }
            order.push(n);
        }
        for (n, p, _) in roots {
            for l in &p.linked_to {
                if let Some(src) = self.resolve_data(l.node) {
                    visit(self, src, &mut seen, &mut order, 0);
                }
            }
            let _ = n;
        }
        let mut body = vec![];
        for idx in order {
            let Some(o) = self.obj(idx) else { continue };
            if o.class == "K2Node_Knot" {
                continue;
            }
            let has_inputs = o.pins().iter().any(|p| !p.is_output() && !p.linked_to.is_empty());
            let is_pose = o.class.starts_with("AnimGraphNode_");
            let shared = uses.get(&idx).copied().unwrap_or(0) > 1;
            if !(shared || (is_pose && has_inputs)) {
                continue;
            }
            let outs = Self::data_outputs(o);
            let Some(first) = outs.first() else { continue };
            let base = {
                let t = o.class.strip_prefix("AnimGraphNode_").or(o.class.strip_prefix("K2Node_")).unwrap_or(&o.class).to_string();
                let mut c = t.chars();
                match c.next() {
                    Some(f) => f.to_lowercase().collect::<String>() + c.as_str(),
                    None => "v".into(),
                }
            };
            let e = self.output((idx, first.id));
            let simple = match &e {
                Expr::Var(_) | Expr::Lit(_) | Expr::SelfRef => true,
                Expr::Member(b, _) => matches!(b.as_ref(), Expr::Var(_) | Expr::SelfRef),
                _ => false,
            };
            if simple && !is_pose {
                continue;
            }
            let name = self.fresh(&base);
            for p in &outs {
                if outs.len() == 1 || is_pose {
                    self.temps.insert((idx, p.id), name.clone());
                }
            }
            body.push(Stmt::Let(vec![name], e));
        }
        let mut rets = vec![];
        for (_n, p, label) in roots {
            let e = self.input(p).unwrap_or_else(|| self.literal(p).unwrap_or(Expr::lit("default")));
            if label == "return" || label.starts_with("return.") {
                rets.push((p.name.clone(), e));
            } else {
                body.push(Stmt::Assign(Expr::var(label.clone()), e));
            }
        }
        if !rets.is_empty() {
            if rets.len() == 1 {
                rets[0].0 = "ReturnValue".into();
            }
            body.push(Stmt::Return(rets));
        }
        body
    }

    fn resolve_data(&self, n: i32) -> Option<i32> {
        let o = self.obj(n)?;
        if o.class == "K2Node_Knot" {
            let inp = o.pins().iter().find(|p| !p.is_output())?;
            let l = inp.linked_to.first()?;
            return self.resolve_data(l.node);
        }
        Some(n)
    }

    pub fn nodes(&self) -> &[&'a Object] {
        &self.nodes
    }
}

fn negate(e: Expr) -> Expr {
    match e {
        Expr::Unary(op, inner) if op == "!" => *inner,
        Expr::Binary(op, a, b) if op == "==" => Expr::Binary("!=".into(), a, b),
        Expr::Binary(op, a, b) if op == "!=" => Expr::Binary("==".into(), a, b),
        other => Expr::Unary("!".into(), Box::new(other)),
    }
}

pub fn macro_name(c: &Ctx, n: &Object) -> String {
    n.prop("MacroGraphReference")
        .and_then(|v| v.as_struct())
        .and_then(|s| s.get("MacroGraph"))
        .and_then(|v| v.as_object())
        .map(|i| c.name(i))
        .unwrap_or_else(|| "Macro".into())
}

// ---------------------------------------------------------------------------------------------
// Graph level entry points

fn is_dataflow_graph(nodes: &[&Object]) -> bool {
    // graphs without any exec wiring (pure collapsed graphs, rule graphs)
    let has_exec = nodes.iter().any(|n| n.pins().iter().any(|p| p.is_exec()));
    if !has_exec && nodes.iter().any(|n| n.class == "K2Node_Tunnel" || n.class == "K2Node_FunctionResult") {
        return true;
    }
    nodes.iter().any(|n| {
        matches!(n.class.as_str(), "AnimGraphNode_Root" | "AnimGraphNode_StateResult" | "AnimGraphNode_TransitionResult" | "AnimGraphNode_CustomTransitionResult" | "AnimGraphNode_LinkedInputPose")
            || n.class.starts_with("AnimGraphNode_") && n.pins().iter().all(|p| !p.is_exec())
    }) && !nodes.iter().any(|n| n.class == "K2Node_FunctionEntry" || n.class == "K2Node_Event" || n.class == "K2Node_CustomEvent")
}

/// Pseudocode for one graph. Returns the text and the exports it covered.
pub fn graph_to_code(c: &Ctx, graph: i32, kind: &str) -> (String, Vec<i32>) {
    let Some(g) = c.obj(graph) else { return (String::new(), vec![]) };
    let name = c.pkg.exports[graph as usize - 1].object_name.clone();
    let mut b = GraphBuilder::new(c, g);
    let mut covered = vec![];
    let mut extra = String::new();
    let prog = if g.class == "AnimationStateMachineGraph" {
        let (stmts, sub_cov) = state_machine(c, &b);
        covered.extend(sub_cov);
        Program { functions: vec![Function { kind: "statemachine".into(), name: name.clone(), body: stmts, ..Default::default() }], blocks: vec![] }
    } else if is_dataflow_graph(b.nodes()) {
        let roots = dataflow_roots(&b);
        let body = b.dataflow_body(&roots);
        let mut f = Function { kind: if kind == "function" { "animgraph".into() } else { kind.into() }, name: name.clone(), body, ..Default::default() };
        if let Some(t) = anim_result_tags(&b) {
            f.tags = t;
        }
        Program { functions: vec![f], blocks: vec![] }
    } else {
        b.exec_program(&name, kind)
    };
    covered.extend(b.covered.iter().copied());
    let mut prog = prog;
    let trivial = strip_trivial_overrides(&mut prog);
    // Nested graphs: collapsed graphs, state machines, ...
    let mut subs = vec![];
    for n in b.nodes() {
        for k in ["BoundGraph", "EditorStateMachineGraph"] {
            if let Some(i) = n.prop(k).and_then(|v| v.as_object()) {
                if i > 0 && !covered.contains(&i) {
                    subs.push(i);
                }
            }
        }
    }
    let mut p = super::Printer::new(0);
    p.program(&prog);
    let mut text = p.out;
    for comment in comments(c, b.nodes()) {
        extra.push_str(&format!("// comment: {comment}\n"));
    }
    if !trivial.is_empty() {
        extra.push_str(&format!("// trivial overrides omitted (empty or only call super): {}\n", trivial.join(", ")));
    }
    if !extra.is_empty() {
        text = format!("{extra}{text}");
    }
    for s in subs {
        if covered.contains(&s) {
            continue;
        }
        let sub_kind = match c.obj(s).map(|o| o.class.as_str()) {
            Some("AnimationStateMachineGraph") => "statemachine",
            _ => "collapsed",
        };
        let (t, cov) = graph_to_code(c, s, sub_kind);
        covered.extend(cov);
        text.push('\n');
        text.push_str(&t);
    }
    (text, covered)
}

/// Removes override events/functions whose body is empty or only forwards to `super`.
fn strip_trivial_overrides(prog: &mut Program) -> Vec<String> {
    let mut removed = vec![];
    prog.functions.retain(|f| {
        if !f.tags.iter().any(|t| t == "override") {
            return true;
        }
        let trivial = match f.body.as_slice() {
            [] => true,
            [Stmt::Expr(Expr::Call { target: Some(t), .. })] => matches!(t.as_ref(), Expr::Var(v) if v == "super"),
            _ => false,
        };
        if trivial {
            removed.push(f.name.clone());
        }
        !trivial
    });
    removed
}

fn comments(c: &Ctx, nodes: &[&Object]) -> Vec<String> {
    nodes
        .iter()
        .filter(|n| n.class.contains("Comment"))
        .filter_map(|n| n.prop("NodeComment").and_then(|v| v.as_str()))
        .map(|s| {
            let s = s.replace('\r', "").replace('\n', " / ");
            let s = if c.opts.max_string > 0 && s.chars().count() > 300 && !c.opts.verbose {
                format!("{}...", s.chars().take(300).collect::<String>())
            } else {
                s
            };
            crate::render::json::quote(&s)
        })
        .collect()
}

fn dataflow_roots<'a>(b: &GraphBuilder<'a, '_>) -> Vec<(&'a Object, &'a Pin, String)> {
    let mut roots = vec![];
    for n in b.nodes() {
        let label = match n.class.as_str() {
            "AnimGraphNode_Root" => "Output",
            "AnimGraphNode_StateResult" => "Output",
            "AnimGraphNode_TransitionResult" | "AnimGraphNode_CustomTransitionResult" => "CanEnterTransition",
            "K2Node_FunctionResult" => "return",
            "K2Node_Tunnel" if n.pins().iter().any(|p| !p.is_output() && !p.is_exec()) => "return",
            _ => continue,
        };
        for p in n.pins().iter().filter(|p| !p.is_output() && !p.is_exec()) {
            if p.linked_to.is_empty() && (p.default_value.is_empty() || p.pin_type.category == "struct") {
                continue;
            }
            let l = if n.pins().iter().filter(|p| !p.is_output()).count() > 1 { format!("{label}.{}", p.name) } else { label.to_string() };
            roots.push((*n, p, l));
        }
    }
    roots
}

fn anim_result_tags(b: &GraphBuilder) -> Option<Vec<String>> {
    let r = b.nodes().iter().find(|n| n.class == "AnimGraphNode_StateResult")?;
    let mut tags = vec![];
    for k in ["StateEntryFunction", "UpdateFunction", "StateExitFunction", "BecomeRelevantFunction", "InitialUpdateFunction"] {
        let (name, _, _) = member_name(r.prop(k));
        if !name.is_empty() {
            tags.push(format!("{k}={name}"));
        }
    }
    Some(tags)
}

/// State machine: states, conduits, transitions with their rules.
fn state_machine<'a>(c: &Ctx, b: &GraphBuilder<'a, '_>) -> (Vec<Stmt>, Vec<i32>) {
    let mut out = vec![];
    let mut covered = vec![];
    let state_name = |n: &Object| -> String {
        if let Some(g) = n.prop("BoundGraph").and_then(|v| v.as_object()) {
            let name = c.name(g);
            if !name.trim().is_empty() {
                return name;
            }
        }
        if let Some(Value::Str(s)) | Some(Value::Name(s)) = n.prop("StateAliasName") {
            if !s.trim().is_empty() {
                return s.clone();
            }
        }
        format!("<unnamed {}>", c.pkg.exports[n.index as usize - 1].object_name)
    };
    // pin -> owning node lookup
    let owner_of = |r: &crate::graph::PinRef| r.node;
    for n in b.nodes() {
        if n.class == "AnimStateEntryNode" {
            for p in n.pins().iter().filter(|p| p.is_output()) {
                for l in &p.linked_to {
                    if let Some(t) = c.obj(owner_of(l)) {
                        out.push(Stmt::Line(format!("entry -> {}", crate::render::json::quote(&state_name(t)))));
                    }
                }
            }
        }
    }
    for n in b.nodes() {
        let kind = match n.class.as_str() {
            "AnimStateNode" => "state",
            "AnimStateConduitNode" => "conduit",
            "AnimStateAliasNode" => "alias",
            _ => continue,
        };
        let name = state_name(n);
        let mut body = vec![];
        if let Some(g) = n.prop("BoundGraph").and_then(|v| v.as_object()) {
            if let Some(go) = c.obj(g) {
                let mut sb = GraphBuilder::new(c, go);
                let roots = dataflow_roots(&sb);
                let mut stmts = sb.dataflow_body(&roots);
                if let Some(tags) = anim_result_tags(&sb) {
                    for t in tags {
                        stmts.push(Stmt::Line(t));
                    }
                }
                body.extend(stmts);
                covered.extend(sb.covered.iter().copied());
            }
        }
        if kind == "alias" {
            let targets: Vec<String> = n
                .prop("AliasedStateNodes")
                .and_then(|v| v.as_array())
                .unwrap_or(&[])
                .iter()
                .filter_map(|x| x.as_object())
                .filter_map(|i| c.obj(i))
                .map(|o| crate::render::json::quote(&state_name(o)))
                .collect();
            let global = n.prop("bGlobalAlias").and_then(|v| v.as_bool()) == Some(true);
            body.push(Stmt::Line(if global { "aliases: any state".into() } else { format!("aliases: [{}]", targets.join(", ")) }));
        }
        for p in &n.properties {
            if matches!(p.name.as_str(), "BoundGraph" | "NodePosX" | "NodePosY" | "NodeGuid" | "AliasedStateNodes" | "bGlobalAlias" | "StateAliasName" | "ErrorType")
                || matches!(p.value, Value::Raw { .. })
            {
                continue;
            }
            if let Some(v) = c.inline(&p.value, 120) {
                body.push(Stmt::Line(format!("{}: {v}", p.name)));
            }
        }
        out.push(Stmt::Block { head: format!("{kind} {}", crate::render::json::quote(&name)), body });
    }
    for n in b.nodes().iter().filter(|n| n.class == "AnimStateTransitionNode") {
        let from = n
            .pins()
            .iter()
            .find(|p| !p.is_output())
            .and_then(|p| p.linked_to.first())
            .and_then(|l| c.obj(l.node))
            .map(state_name)
            .unwrap_or("?".into());
        let to = n
            .pins()
            .iter()
            .find(|p| p.is_output())
            .and_then(|p| p.linked_to.first())
            .and_then(|l| c.obj(l.node))
            .map(state_name)
            .unwrap_or("?".into());
        let mut rule = String::new();
        if let Some(g) = n.prop("BoundGraph").and_then(|v| v.as_object()).and_then(|i| c.obj(i)) {
            let mut sb = GraphBuilder::new(c, g);
            let roots = dataflow_roots(&sb);
            let stmts = sb.dataflow_body(&roots);
            covered.extend(sb.covered.iter().copied());
            // single expression rules are printed inline
            if let [Stmt::Assign(_, e)] = stmts.as_slice() {
                rule = e.to_code();
            } else {
                let mut p = super::Printer::new(0);
                p.stmts(&stmts);
                rule = format!("{{ {} }}", p.out.trim().replace('\n', "; "));
            }
        }
        if n.prop("bAutomaticRuleBasedOnSequencePlayerInState").and_then(|v| v.as_bool()) == Some(true) {
            rule = "automatic (time remaining)".into();
        }
        let mut attrs = vec![];
        for p in &n.properties {
            if matches!(
                p.name.as_str(),
                "BoundGraph" | "NodePosX" | "NodePosY" | "NodeGuid" | "CustomTransitionGraph" | "bAutomaticRuleBasedOnSequencePlayerInState"
                    | "SharedColor" | "SharedRulesGuid" | "ErrorType" | "bSharedRules" | "bSharedCrossfade" | "SharedCrossfadeGuid"
                    | "SharedCrossfadeName" | "SharedCrossfadeIdx"
            ) || matches!(p.value, Value::Raw { .. }) {
                continue;
            }
            if let Some(v) = c.inline(&p.value, 120) {
                attrs.push(format!("{}={v}", p.name));
            }
        }
        let mut line = format!("transition {} -> {}", crate::render::json::quote(&from), crate::render::json::quote(&to));
        if !rule.is_empty() {
            line.push_str(&format!(" when {rule}"));
        }
        if !attrs.is_empty() {
            line.push_str(&format!("  [{}]", attrs.join(", ")));
        }
        out.push(Stmt::Line(line));
    }
    let _ = Extra::PureState(0);
    (out, covered)
}
