//! Material / material function expression graphs to HLSL-like pseudocode.

use std::collections::{HashMap, HashSet};

use super::{Expr, Printer, Stmt, ident};
use crate::object::Object;
use crate::property::Value;
use crate::render::Ctx;
use crate::render::material::{InputRef, as_input, is_input_struct};

const SKIP: &[&str] = &[
    "MaterialExpressionEditorX", "MaterialExpressionEditorY", "MaterialExpressionGuid", "Material", "Function",
    "ExpressionGUID", "bRealtimePreview", "bNeedToUpdatePreview", "bCollapsed", "Outputs", "bShowOutputNameOnPin",
    "bHidePreviewWindow", "bShowMaskColorsOnPin", "bShowInputs", "bShowOutputs", "ParameterName", "bIsParameterExpression",
    "SubgraphExpression", "bCommentBubbleVisible", "bPreviewExpression", "GraphNode", "FunctionInputs", "FunctionOutputs",
    "MaterialFunction", "Desc", "SortPriority", "Group", "ExpressionGuid", "bUseCustomPrimitiveData", "PreviewValue",
    "bUsePreviewValueAsDefault", "InputName", "OutputName", "Description", "Code", "Inputs", "AdditionalOutputs",
    "OutputType", "Declaration", "Name", "NodeColor", "VariableGuid", "ChannelNames", "bCanRenameNode",
];

pub struct MatBuilder<'a, 'c> {
    c: &'a Ctx<'c>,
    temps: HashMap<i32, String>,
    used: HashSet<String>,
    uses: HashMap<i32, usize>,
    pub params: Vec<Stmt>,
    seen_params: HashSet<String>,
    pub body: Vec<Stmt>,
    building: HashSet<i32>,
    roots: Vec<i32>,
}

fn swizzle(mask: &str) -> String {
    mask.to_lowercase()
}

impl<'a, 'c> MatBuilder<'a, 'c> {
    pub fn new(c: &'a Ctx<'c>, exprs: &[i32], roots: &[&Value]) -> Self {
        let mut uses: HashMap<i32, usize> = HashMap::new();
        for e in exprs {
            if let Some(o) = c.obj(*e) {
                for i in inputs_of(o) {
                    *uses.entry(i.expr).or_default() += 1;
                }
            }
        }
        for r in roots {
            if let Some(i) = as_input(r) {
                *uses.entry(i.expr).or_default() += 1;
            }
        }
        let mut root_ids: Vec<i32> = roots.iter().filter_map(|r| as_input(r)).map(|i| i.expr).collect();
        root_ids.extend(exprs.iter().copied().filter(|e| c.obj(*e).map(|o| o.class == "MaterialExpressionFunctionOutput").unwrap_or(false)));
        MatBuilder {
            c,
            temps: HashMap::new(),
            used: HashSet::new(),
            uses,
            params: vec![],
            seen_params: HashSet::new(),
            body: vec![],
            building: HashSet::new(),
            roots: root_ids,
        }
    }

    fn fresh(&mut self, base: &str) -> String {
        let base = ident(base);
        let base = if base.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(true) { format!("v{base}") } else { base };
        let mut n = base.clone();
        let mut i = 2;
        while self.used.contains(&n) {
            n = format!("{base}{i}");
            i += 1;
        }
        self.used.insert(n.clone());
        n
    }

    fn lit(&self, v: &Value) -> Expr {
        let s = self.c.inline(v, 200).unwrap_or_else(|| "{...}".into());
        Expr::lit(short_path(&s))
    }

    /// Expression for a connected input (with output selection / mask).
    fn input(&mut self, inp: &InputRef) -> Expr {
        let base = self.node(inp.expr);
        if !inp.mask.is_empty() {
            return Expr::Member(Box::new(base), swizzle(&inp.mask));
        }
        let multi_output = self
            .c
            .obj(inp.expr)
            .and_then(|o| o.prop("FunctionOutputs"))
            .and_then(|v| v.as_array())
            .map(|a| a.len() > 1)
            .unwrap_or(false);
        if inp.output_index > 0 || multi_output {
            let name = self.output_name(inp.expr, inp.output_index);
            return Expr::Member(Box::new(base), name);
        }
        base
    }

    fn output_name(&self, e: i32, idx: i64) -> String {
        if let Some(o) = self.c.obj(e) {
            if let Some(Value::Array(outs)) = o.prop("FunctionOutputs") {
                if let Some(n) = outs.get(idx as usize).and_then(|x| x.as_struct()).and_then(|s| s.get("Output")).and_then(|x| x.as_struct()).and_then(|s| s.get("OutputName")).and_then(|x| x.as_str()) {
                    return n.to_string();
                }
            }
            if o.class.contains("TextureSample") {
                return ["rgb", "r", "g", "b", "a", "rgba"].get(idx as usize).unwrap_or(&"out").to_string();
            }
        }
        format!("out{idx}")
    }

    /// Value of an input property: connected expression, or the matching Const* default.
    fn prop_input(&mut self, o: &Object, name: &str) -> Option<Expr> {
        if let Some(v) = o.prop(name) {
            if let Some(i) = as_input(v) {
                if i.expr != 0 {
                    return Some(self.input(&i));
                }
            }
        }
        for k in [format!("Const{name}"), format!("{name}Default"), format!("Const{}", name.trim_start_matches('A'))] {
            if let Some(v) = o.prop(&k) {
                return Some(self.lit(v));
            }
        }
        None
    }

    fn param(&mut self, o: &Object, ty: &str) -> Expr {
        let pname = o.prop("ParameterName").and_then(|v| v.as_str()).unwrap_or("Param").to_string();
        let var = ident(&pname);
        if self.seen_params.insert(var.clone()) {
            let default = o.prop("DefaultValue").or(o.prop("Texture")).map(|v| self.c.inline(v, 160).unwrap_or_default()).unwrap_or_default();
            let mut line = format!("param {ty} {var}");
            if !default.is_empty() {
                line.push_str(&format!(" = {}", short_path(&default)));
            }
            let mut attrs = vec![];
            if let Some(g) = o.prop("Group").and_then(|v| v.as_str()) {
                if !g.is_empty() && g != "None" {
                    attrs.push(format!("group {}", crate::render::json::quote(g)));
                }
            }
            for k in ["SliderMin", "SliderMax"] {
                if let Some(v) = o.prop(k).and_then(|v| v.as_f64()) {
                    attrs.push(format!("{k} {}", Ctx::float(v)));
                }
            }
            if let Some(Value::Str(d)) = o.prop("Desc") {
                if !d.is_empty() {
                    attrs.push(crate::render::json::quote(d));
                }
            }
            if var != pname {
                attrs.push(format!("name {}", crate::render::json::quote(&pname)));
            }
            if !attrs.is_empty() {
                line.push_str(&format!("  // {}", attrs.join(", ")));
            }
            self.params.push(Stmt::Line(line));
        }
        Expr::var(var)
    }

    fn node(&mut self, e: i32) -> Expr {
        if let Some(t) = self.temps.get(&e) {
            return Expr::var(t.clone());
        }
        let Some(o) = self.c.obj(e) else { return Expr::lit(self.c.obj_ref(e)) };
        if !self.building.insert(e) {
            return Expr::lit("<cycle>");
        }
        let ex = self.node_expr(o);
        self.building.remove(&e);
        let shared = self.uses.get(&e).copied().unwrap_or(0) > 1;
        let is_simple = matches!(ex, Expr::Var(_) | Expr::Lit(_));
        if (shared && !is_simple) || ex.size() > 14 {
            let base = self.name_hint(o);
            let name = self.fresh(&base);
            if o.class == "MaterialExpressionCustom" {
                if let Some(Value::Str(code)) = o.prop("Code") {
                    let code = code.replace('\r', "");
                    let shown: Vec<&str> = code.lines().take(40).collect();
                    self.body.push(Stmt::Comment(format!("custom HLSL:\n{}", shown.join("\n"))));
                }
            }
            self.body.push(Stmt::Let(vec![name.clone()], ex));
            self.temps.insert(e, name.clone());
            return Expr::var(name);
        }
        ex
    }

    fn name_hint(&self, o: &Object) -> String {
        if o.class == "MaterialExpressionReroute" {
            if let Some(src) = o.prop("Input").and_then(as_input).filter(|i| i.expr != 0).and_then(|i| self.c.obj(i.expr)) {
                return self.name_hint(src);
            }
        }
        if let Some(p) = o.prop("ParameterName").and_then(|v| v.as_str()) {
            return p.to_string();
        }
        let short = o.class.strip_prefix("MaterialExpression").unwrap_or(&o.class);
        match short {
            "MaterialFunctionCall" => o.prop("MaterialFunction").and_then(|v| v.as_object()).map(|i| self.c.name(i)).unwrap_or("call".into()),
            "Custom" => o.prop("Description").and_then(|v| v.as_str()).unwrap_or("custom").to_string(),
            _ => {
                let mut ch = short.chars();
                match ch.next() {
                    Some(f) => f.to_lowercase().collect::<String>() + ch.as_str(),
                    None => "v".into(),
                }
            }
        }
    }

    fn node_expr(&mut self, o: &Object) -> Expr {
        let short = o.class.strip_prefix("MaterialExpression").unwrap_or(&o.class).to_string();
        let bin = |op: &str| op.to_string();
        let f = |o: &Object, k: &str| o.prop(k).and_then(|v| v.as_f64());
        match short.as_str() {
            "Constant" => return Expr::lit(Ctx::float(f(o, "R").unwrap_or(0.0))),
            "Constant2Vector" | "Constant3Vector" | "Constant4Vector" => {
                if let Some(Value::Struct(sv)) = o.prop("Constant") {
                    let n = match short.as_str() {
                        "Constant3Vector" => 3,
                        "Constant4Vector" => 4,
                        _ => 2,
                    };
                    let comps: Vec<String> = ["R", "G", "B", "A"].iter().take(n).map(|k| sv.get(k).and_then(|v| v.as_f64()).map(Ctx::float).unwrap_or("0.0".into())).collect();
                    return Expr::lit(format!("float{n}({})", comps.join(", ")));
                }
                let comps: Vec<String> = ["R", "G", "B", "A"].iter().filter_map(|k| f(o, k)).map(Ctx::float).collect();
                let n = comps.len().max(2);
                return Expr::lit(format!("float{n}({})", if comps.is_empty() { "0.0, 0.0".into() } else { comps.join(", ") }));
            }
            "ScalarParameter" | "CurveAtlasRowParameter" => return self.param(o, "float"),
            "VectorParameter" | "DoubleVectorParameter" | "ChannelMaskParameter" => return self.param(o, "float4"),
            "StaticBoolParameter" => return self.param(o, "static bool"),
            "TextureObjectParameter" => return self.param(o, "Texture"),
            "StaticSwitchParameter" => {
                let p = self.param(o, "static bool");
                let a = self.prop_input(o, "A").unwrap_or(Expr::lit("0"));
                let b = self.prop_input(o, "B").unwrap_or(Expr::lit("0"));
                return Expr::lit(format!("({} ? {} : {})", p.to_code(), a.to_code(), b.to_code()));
            }
            "TextureSampleParameter2D" | "TextureSampleParameterCube" | "TextureSampleParameter2DArray" | "TextureSampleParameterVolume" => {
                let p = self.param(o, "Texture");
                let mut args = vec![(String::new(), p)];
                if let Some(uv) = self.prop_input(o, "Coordinates") {
                    args.push(("UV".into(), uv));
                }
                return Expr::call("Sample", args);
            }
            "TextureSample" | "TextureObject" => {
                let tex = o.prop("Texture").map(|v| self.lit(v)).unwrap_or(Expr::lit("None"));
                let tex = match self.prop_input(o, "TextureObject") {
                    Some(t) => t,
                    None => tex,
                };
                if short == "TextureObject" {
                    return tex;
                }
                let mut args = vec![(String::new(), tex)];
                if let Some(uv) = self.prop_input(o, "Coordinates") {
                    args.push(("UV".into(), uv));
                }
                if let Some(Value::Enum(st)) = o.prop("SamplerType") {
                    if !st.ends_with("SAMPLERTYPE_Color") {
                        args.push(("Sampler".into(), Expr::lit(st.rsplit("SAMPLERTYPE_").next().unwrap_or(st).to_string())));
                    }
                }
                return Expr::call("Sample", args);
            }
            "TextureCoordinate" => {
                let idx = o.prop("CoordinateIndex").and_then(|v| v.as_i64()).unwrap_or(0);
                let base = Expr::var(format!("TexCoord{idx}"));
                let u = f(o, "UTiling").unwrap_or(1.0);
                let v = f(o, "VTiling").unwrap_or(1.0);
                if u == 1.0 && v == 1.0 {
                    return base;
                }
                let scale = if u == v { Expr::lit(Ctx::float(u)) } else { Expr::lit(format!("float2({}, {})", Ctx::float(u), Ctx::float(v))) };
                return Expr::Binary("*".into(), Box::new(base), Box::new(scale));
            }
            "Add" | "Subtract" | "Multiply" | "Divide" => {
                let op = match short.as_str() {
                    "Add" => "+",
                    "Subtract" => "-",
                    "Multiply" => "*",
                    _ => "/",
                };
                let a = self.prop_input(o, "A").unwrap_or(Expr::lit(if op == "*" || op == "/" { "1.0" } else { "0.0" }));
                let b = self.prop_input(o, "B").unwrap_or(Expr::lit(if op == "*" || op == "/" { "1.0" } else { "0.0" }));
                return Expr::Binary(bin(op), Box::new(a), Box::new(b));
            }
            "OneMinus" => {
                let x = self.prop_input(o, "Input").unwrap_or(Expr::lit("0.0"));
                return Expr::Binary("-".into(), Box::new(Expr::lit("1")), Box::new(x));
            }
            "LinearInterpolate" => {
                let a = self.prop_input(o, "A").unwrap_or(Expr::lit("0.0"));
                let b = self.prop_input(o, "B").unwrap_or(Expr::lit("1.0"));
                let t = self.prop_input(o, "Alpha").unwrap_or(Expr::lit("0.5"));
                return Expr::call("lerp", vec![(String::new(), a), (String::new(), b), (String::new(), t)]);
            }
            "ComponentMask" => {
                let x = self.prop_input(o, "Input").unwrap_or(Expr::lit("0.0"));
                let m: String = ["R", "G", "B", "A"].iter().filter(|k| o.prop(k).and_then(|v| v.as_bool()) == Some(true)).map(|k| k.to_lowercase()).collect();
                return Expr::Member(Box::new(x), if m.is_empty() { "r".into() } else { m });
            }
            "Reroute" | "NamedRerouteDeclaration" => {
                return self.prop_input(o, "Input").unwrap_or(Expr::lit("0.0"));
            }
            "NamedRerouteUsage" => {
                if let Some(d) = o.prop("Declaration").and_then(|v| v.as_object()) {
                    return self.node(d);
                }
            }
            "FunctionInput" => {
                let n = o.prop("InputName").and_then(|v| v.as_str()).unwrap_or("Input");
                return Expr::var(ident(n));
            }
            "MaterialFunctionCall" | "MaterialLayerFunctionCall" => {
                let fname = o.prop("MaterialFunction").and_then(|v| v.as_object()).map(|i| self.c.name(i)).unwrap_or("Function".into());
                let mut args = vec![];
                if let Some(Value::Array(fins)) = o.prop("FunctionInputs") {
                    for fi in fins {
                        let Some(s) = fi.as_struct() else { continue };
                        let Some(inp) = s.get("Input") else { continue };
                        let name = inp.as_struct().and_then(|x| x.get("InputName")).and_then(|x| x.as_str()).unwrap_or("?").to_string();
                        if let Some(i) = as_input(inp) {
                            if i.expr != 0 {
                                let e = self.input(&i);
                                args.push((name, e));
                            }
                        }
                    }
                }
                return Expr::call(ident(&fname), args);
            }
            "Comment" => return Expr::lit("<comment>"),
            _ => {}
        }
        // generic: inputs first, then settings
        let mut args = vec![];
        let mut consts: HashSet<String> = HashSet::new();
        for p in &o.properties {
            if is_input_struct(&p.value) {
                if let Some(e) = self.prop_input(o, &p.name) {
                    args.push((p.name.clone(), e));
                    consts.insert(format!("Const{}", p.name));
                }
            } else if let Value::Array(a) = &p.value {
                // input arrays (Custom node inputs, switch cases ...)
                for (i, x) in a.iter().enumerate() {
                    let Some(s) = x.as_struct() else { continue };
                    let label = s.fields.iter().find(|f| matches!(f.value, Value::Name(_) | Value::Str(_))).and_then(|f| f.value.as_str()).map(|s| s.to_string()).unwrap_or(format!("{}{i}", p.name));
                    for fld in &s.fields {
                        if let Some(inp) = as_input(&fld.value) {
                            if inp.expr != 0 {
                                let e = self.input(&inp);
                                args.push((label.clone(), e));
                            }
                        }
                    }
                }
            }
        }
        for p in &o.properties {
            if is_input_struct(&p.value) || SKIP.contains(&p.name.as_str()) || consts.contains(&p.name) || p.name.starts_with("bOverride_") {
                continue;
            }
            if let Value::Array(a) = &p.value {
                if a.iter().any(|x| x.as_struct().map(|s| s.fields.iter().any(|f| is_input_struct(&f.value))).unwrap_or(false)) {
                    continue;
                }
            }
            args.push((p.name.clone(), self.lit(&p.value)));
        }
        let func = match short.as_str() {
            "Custom" => format!("Custom_{}", ident(o.prop("Description").and_then(|v| v.as_str()).unwrap_or("HLSL"))),
            _ => short.clone(),
        };
        Expr::call(func, args)
    }
}

impl MatBuilder<'_, '_> {
    /// Expressions referenced while building the outputs.
    fn reachable(&self) -> HashSet<i32> {
        let mut seen = HashSet::new();
        let mut stack: Vec<i32> = self.roots.clone();
        while let Some(e) = stack.pop() {
            if e <= 0 || !seen.insert(e) {
                continue;
            }
            if let Some(o) = self.c.obj(e) {
                stack.extend(inputs_of(o).into_iter().map(|i| i.expr));
            }
        }
        seen
    }
}

fn inputs_of(o: &Object) -> Vec<InputRef> {
    let mut v = vec![];
    fn rec(val: &Value, v: &mut Vec<InputRef>) {
        if let Some(i) = as_input(val) {
            if i.expr != 0 {
                v.push(i);
            }
            return;
        }
        match val {
            Value::Struct(s) => s.fields.iter().for_each(|f| rec(&f.value, v)),
            Value::Array(a) => a.iter().for_each(|x| rec(x, v)),
            _ => {}
        }
    }
    for p in &o.properties {
        rec(&p.value, &mut v);
    }
    if let Some(d) = o.prop("Declaration").and_then(|v| v.as_object()) {
        v.push(InputRef { expr: d, output_index: 0, mask: String::new() });
    }
    v
}

fn short_path(s: &str) -> String {
    if let Some(rest) = s.strip_prefix("/Script/") {
        return rest.rsplit('.').next().unwrap_or(rest).to_string();
    }
    s.to_string()
}

/// Pseudocode for a material (outputs) or a material function (FunctionOutput expressions).
pub fn material_to_code(c: &Ctx, name: &str, kind: &str, outputs: &[(String, &Value)], exprs: &[i32]) -> String {
    let roots: Vec<&Value> = outputs.iter().map(|(_, v)| *v).collect();
    let mut b = MatBuilder::new(c, exprs, &roots);
    let mut assigns = vec![];
    for (n, v) in outputs {
        if let Some(i) = as_input(v) {
            if i.expr != 0 {
                let e = b.input(&i);
                assigns.push(Stmt::Assign(Expr::var(ident(n)), e));
            }
        }
    }
    // material function outputs
    let mut fouts: Vec<&Object> = exprs.iter().filter_map(|e| c.obj(*e)).filter(|o| o.class == "MaterialExpressionFunctionOutput").collect();
    fouts.sort_by_key(|o| o.prop("SortPriority").and_then(|v| v.as_i64()).unwrap_or(0));
    let mut rets = vec![];
    for fo in &fouts {
        let oname = fo.prop("OutputName").and_then(|v| v.as_str()).unwrap_or("Result").to_string();
        let e = b.prop_input(fo, "A").unwrap_or(Expr::lit("0.0"));
        rets.push((oname, e));
    }
    // function inputs as parameters
    let mut fins: Vec<&Object> = exprs.iter().filter_map(|e| c.obj(*e)).filter(|o| o.class == "MaterialExpressionFunctionInput").collect();
    fins.sort_by_key(|o| o.prop("SortPriority").and_then(|v| v.as_i64()).unwrap_or(0));
    let mut params = vec![];
    for fi in &fins {
        let n = fi.prop("InputName").and_then(|v| v.as_str()).unwrap_or("Input").to_string();
        let ty = fi.prop("InputType").and_then(|v| v.as_str()).map(|t| t.rsplit("FunctionInput_").next().unwrap_or(t).to_string()).unwrap_or("Vector3".into());
        let mut ty = ty;
        if fi.prop("bUsePreviewValueAsDefault").and_then(|v| v.as_bool()) == Some(true) {
            if let Some(p) = fi.prop("PreviewValue") {
                ty.push_str(&format!(" = {}", c.inline(p, 80).unwrap_or_default()));
            }
        }
        params.push((n, ty));
    }
    // Expressions not contributing to any output
    let mut unused: Vec<String> = vec![];
    let reachable = b.reachable();
    for e in exprs {
        let Some(o) = c.obj(*e) else { continue };
        let short = o.class.strip_prefix("MaterialExpression").unwrap_or(&o.class);
        if reachable.contains(e) || matches!(short, "Comment" | "FunctionOutput" | "FunctionInput") {
            continue;
        }
        let label = match o.prop("ParameterName").and_then(|v| v.as_str()) {
            Some(p) => format!("{short} \"{p}\""),
            None if short == "MaterialFunctionCall" => o.prop("MaterialFunction").and_then(|v| v.as_object()).map(|i| c.name(i)).unwrap_or(short.to_string()),
            None => short.to_string(),
        };
        unused.push(label);
    }
    let mut body = std::mem::take(&mut b.params);
    if !unused.is_empty() {
        let mut grouped: Vec<(String, usize)> = vec![];
        for u in &unused {
            match grouped.iter_mut().find(|(n, _)| n == u) {
                Some((_, k)) => *k += 1,
                None => grouped.push((u.clone(), 1)),
            }
        }
        let parts: Vec<String> = grouped.into_iter().map(|(n, k)| if k > 1 { format!("{n} x{k}") } else { n }).collect();
        body.push(Stmt::Comment(format!("{} expressions are not connected to any output: {}", unused.len(), parts.join(", "))));
    }
    body.extend(std::mem::take(&mut b.body));
    body.extend(assigns);
    if !rets.is_empty() {
        body.push(Stmt::Return(rets));
    }
    let f = super::Function { kind: kind.into(), name: name.into(), params, body, ..Default::default() };
    let mut p = Printer::new(0);
    p.function(&f);
    p.out
}
