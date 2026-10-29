//! Blueprint view: variables, components, widget tree, defaults and graphs.

use std::collections::{HashMap, HashSet};

use super::{Ctx, Out, property_flags};
use crate::graph::{Pin, terminal_display};
use crate::object::Object;
use crate::property::{StructValue, Value, clean_field_name};

pub fn is_blueprint(o: &Object) -> bool {
    (o.class.ends_with("Blueprint") || o.class == "BlueprintGeneratedClass" && false)
        && (o.prop("ParentClass").is_some() || o.prop("GeneratedClass").is_some() || o.prop("UbergraphPages").is_some())
}

fn objects_in(v: Option<&Value>) -> Vec<i32> {
    match v {
        Some(Value::Array(a)) | Some(Value::Set(a)) => a.iter().filter_map(|x| x.as_object()).filter(|i| *i != 0).collect(),
        Some(Value::Object(i)) if *i != 0 => vec![*i],
        _ => vec![],
    }
}

/// Display for a pin type stored as a property value (FEdGraphPinType).
pub fn pin_type_value(c: &Ctx, v: &Value) -> String {
    let Some(s) = v.as_struct() else { return "?".into() };
    let cat = s.get("PinCategory").and_then(|x| x.as_str()).unwrap_or("?");
    let sub = s.get("PinSubCategory").and_then(|x| x.as_str()).unwrap_or("");
    let obj = s.get("PinSubCategoryObject").and_then(|x| x.as_object()).unwrap_or(0);
    let member = s.get("MemberName").and_then(|x| x.as_str()).unwrap_or("");
    let base = terminal_display(c.pkg, cat, sub, obj, member);
    let container = s.get("ContainerType").and_then(|x| x.as_str()).unwrap_or("None");
    let mut t = match container {
        "Array" => format!("Array<{base}>"),
        "Set" => format!("Set<{base}>"),
        "Map" => {
            let vt = s.get("PinValueType").and_then(|x| x.as_str()).unwrap_or("?:");
            let (vc, vs) = vt.split_once(':').unwrap_or((vt, ""));
            let vo = s.get("PinValueTypeObject").and_then(|x| x.as_object()).unwrap_or(0);
            format!("Map<{base}, {}>", terminal_display(c.pkg, vc, vs, vo, ""))
        }
        _ => base,
    };
    if s.get("bIsReference").and_then(|x| x.as_bool()) == Some(true) {
        t.push('&');
    }
    t
}

fn text_of(v: Option<&Value>) -> String {
    match v {
        Some(Value::Text(t)) => t.display(),
        Some(v) => v.as_str().unwrap_or("").to_string(),
        None => String::new(),
    }
}

fn member_ref(c: &Ctx, v: Option<&Value>) -> (String, String, bool) {
    let Some(s) = v.and_then(|v| v.as_struct()) else { return (String::new(), String::new(), false) };
    let name = s.get("MemberName").and_then(|x| x.as_str()).unwrap_or("").to_string();
    let parent = s.get("MemberParent").and_then(|x| x.as_object()).unwrap_or(0);
    let self_ctx = s.get("bSelfContext").and_then(|x| x.as_bool()).unwrap_or(false);
    let parent_name = if parent != 0 { c.name(parent) } else { String::new() };
    (parent_name, name, self_ctx)
}

fn member_str(c: &Ctx, v: Option<&Value>) -> String {
    let (p, n, self_ctx) = member_ref(c, v);
    if self_ctx || p.is_empty() { n } else { format!("{p}::{n}") }
}

fn short_class(class: &str) -> &str {
    for p in ["K2Node_", "AnimGraphNode_", "MaterialGraphNode_", "EdGraphNode_"] {
        if let Some(s) = class.strip_prefix(p) {
            return s;
        }
    }
    class
}

/// Human readable node title.
pub fn node_title(c: &Ctx, n: &Object) -> String {
    let p = |name: &str| n.prop(name);
    let s = |name: &str| p(name).and_then(|v| v.as_str()).unwrap_or("").to_string();
    let obj = |name: &str| p(name).and_then(|v| v.as_object()).map(|i| c.name(i)).unwrap_or_default();
    let class = n.class.as_str();
    match class {
        "K2Node_Event" => {
            let custom = s("CustomFunctionName");
            let ev = member_str(c, p("EventReference"));
            let name = if !custom.is_empty() && custom != "None" { custom } else { ev };
            format!("Event {name}")
        }
        "K2Node_CustomEvent" => format!("Custom Event {}", s("CustomFunctionName")),
        "K2Node_ComponentBoundEvent" => format!("On {} ({})", s("DelegatePropertyName"), s("ComponentPropertyName")),
        "K2Node_ActorBoundEvent" => format!("On {} ({})", s("DelegatePropertyName"), obj("EventOwner")),
        "K2Node_InputAction" => format!("InputAction {}", s("InputActionName")),
        "K2Node_InputAxisEvent" => format!("InputAxis {}", s("InputAxisName")),
        "K2Node_InputKey" | "K2Node_InputAxisKeyEvent" | "K2Node_InputVectorAxisEvent" => {
            let key = p("InputKey").or(p("AxisKey")).and_then(|v| v.as_struct()).and_then(|s| s.get("KeyName")).and_then(|v| v.as_str()).unwrap_or("?").to_string();
            format!("{} {key}", short_class(class))
        }
        "K2Node_EnhancedInputAction" => format!("EnhancedInputAction {}", obj("InputAction")),
        "K2Node_FunctionEntry" => format!("Entry {}", member_str(c, p("FunctionReference"))),
        "K2Node_FunctionResult" => "Return".into(),
        "K2Node_CallParentFunction" => format!("Call Parent {}", member_str(c, p("FunctionReference"))),
        "K2Node_Message" => format!("Message {}", member_str(c, p("FunctionReference"))),
        "K2Node_VariableGet" => format!("Get {}", member_str(c, p("VariableReference"))),
        "K2Node_VariableSet" => format!("Set {}", member_str(c, p("VariableReference"))),
        "K2Node_VariableSetRef" => "Set by-ref".into(),
        "K2Node_Self" => "Self".into(),
        "K2Node_DynamicCast" | "K2Node_ClassDynamicCast" => format!("Cast to {}", obj("TargetType")),
        "K2Node_IfThenElse" => "Branch".into(),
        "K2Node_ExecutionSequence" => "Sequence".into(),
        "K2Node_Knot" => "Reroute".into(),
        "K2Node_MacroInstance" => {
            let m = p("MacroGraphReference").and_then(|v| v.as_struct()).and_then(|s| s.get("MacroGraph")).and_then(|v| v.as_object()).map(|i| c.name(i)).unwrap_or("?".into());
            format!("Macro {m}")
        }
        "K2Node_MakeStruct" | "K2Node_BreakStruct" | "K2Node_SetFieldsInStruct" => {
            let verb = match class {
                "K2Node_MakeStruct" => "Make",
                "K2Node_BreakStruct" => "Break",
                _ => "Set Fields In",
            };
            format!("{verb} {}", obj("StructType"))
        }
        "K2Node_StructMemberGet" | "K2Node_StructMemberSet" => format!("{} {}", short_class(class), obj("StructType")),
        "K2Node_SwitchEnum" => format!("Switch on {}", obj("Enum")),
        "K2Node_Timeline" => format!("Timeline {}", s("TimelineName")),
        "K2Node_AddDelegate" => format!("Bind Event to {}", member_str(c, p("DelegateReference"))),
        "K2Node_RemoveDelegate" => format!("Unbind Event from {}", member_str(c, p("DelegateReference"))),
        "K2Node_ClearDelegate" => format!("Unbind all from {}", member_str(c, p("DelegateReference"))),
        "K2Node_CallDelegate" => format!("Call {}", member_str(c, p("DelegateReference"))),
        "K2Node_AssignDelegate" => format!("Assign {}", member_str(c, p("DelegateReference"))),
        "K2Node_CreateDelegate" => format!("Create Event {}", s("SelectedFunctionName")),
        "K2Node_Literal" => format!("Literal {}", p("ObjectRef").map(|v| c.inline(v, 200).unwrap_or_default()).unwrap_or_default()),
        "K2Node_GetSubsystem" | "K2Node_GetSubsystemFromPC" | "K2Node_GetEngineSubsystem" | "K2Node_GetEditorSubsystem" => {
            format!("Get Subsystem {}", obj("CustomClass"))
        }
        "K2Node_SpawnActorFromClass" => "SpawnActor".into(),
        "K2Node_CreateWidget" => "Create Widget".into(),
        "K2Node_ConstructObjectFromClass" => "Construct Object".into(),
        "K2Node_Composite" => format!("Collapsed Graph {}", obj("BoundGraph")),
        "K2Node_Tunnel" => {
            if p("bCanHaveOutputs").and_then(|v| v.as_bool()) == Some(true) { "Tunnel Inputs".into() } else { "Tunnel Outputs".into() }
        }
        "EdGraphNode_Comment" | "MaterialGraphNode_Comment" => format!("Comment {}", c.quote(&s("NodeComment"))),
        "AnimGraphNode_StateMachine" => format!("StateMachine {}", obj("EditorStateMachineGraph")),
        "AnimStateNode" | "AnimStateConduitNode" => format!("State {}", obj("BoundGraph")),
        "AnimStateTransitionNode" => "Transition".into(),
        "AnimStateEntryNode" => "Entry".into(),
        _ => {
            if p("FunctionReference").is_some() {
                format!("Call {}", member_str(c, p("FunctionReference")))
            } else if class.starts_with("K2Node_") && p("ProxyFactoryFunctionName").is_some() {
                format!("{} ({}::{})", short_class(class), obj("ProxyFactoryClass"), s("ProxyFactoryFunctionName"))
            } else {
                short_class(class).to_string()
            }
        }
    }
}

/// Properties used in titles or that are pure noise.
const NODE_SKIP: &[&str] = &[
    "NodePosX", "NodePosY", "NodeWidth", "NodeHeight", "NodeGuid", "bCommentBubbleVisible", "bCommentBubblePinned",
    "bCommentBubbleMakeVisible", "ErrorType", "ErrorMsg", "bHasCompilerMessage", "FunctionReference", "EventReference",
    "VariableReference", "CustomFunctionName", "bOverrideFunction", "bInternalEvent", "bIsEditable", "ExtraFlags",
    "MacroGraphReference", "TargetType", "StructType", "DelegateReference", "Pins", "DeprecatedPins", "NodeComment",
    "EnabledState", "bUserSetEnabledState", "AdvancedPinDisplay", "ShowPinForProperties", "bIsPureFunc", "bIsConstFunc",
    "InputActionName", "InputAxisName", "InputKey", "AxisKey", "DelegatePropertyName", "ComponentPropertyName",
    "DelegateOwnerClass", "EventOwner", "TimelineName", "TimelineGuid", "SelectedFunctionName", "SelectedFunctionGuid",
    "Enum", "BoundGraph", "MetaData", "LocalVariables", "UserDefinedPins", "bCanRenameNode", "bCanHaveInputs",
    "bCanHaveOutputs", "InputPinsDefaultValues", "OutputPinsDefaultValues", "CustomClass", "ProxyFactoryFunctionName",
    "ProxyFactoryClass", "ProxyClass", "bIsDeprecated", "bIsInterfaceCall", "bIsFinalFunction", "bIsBeadFunction",
    "bWantsEnumToExecExpansion", "bCommentBubbleMakeVisible", "bCanRenameNode", "NodeUpgradeMessage", "bIsSelfContext",
    "bDefaultsToPureFunc", "bIsPureCast", "PureState", "bIsPure", "MoveMode", "bColorCommentBubble",
    "CommentColor", "FontSize", "EditorStateMachineGraph", "bIsCompact", "bDisplayNameIsEditable", "PinNames",
    "NumAdditionalInputs", "EnumEntries", "EnumFriendlyNames", "bHasDefaultPin", "PinTypeCategory",
    "bShowExecPins", "InputAction", "ObjectRef", "SelfContextInfo", "bMadeAfterOverridePinRemoval",
    "DelegatePropertyDisplayName", "bIsPureGet", "bIsConstFunc", "OrphanedPinSaveMode", "bEnforceConstCorrectness", "bDisableOrphanPinSaving",
];

struct GraphCtx<'a> {
    /// node export index -> local number
    num: HashMap<i32, usize>,
    nodes: Vec<&'a Object>,
}

impl GraphCtx<'_> {
    fn label(&self, c: &Ctx, node: i32) -> String {
        match self.num.get(&node) {
            Some(n) => format!("N{n}"),
            None => c.pkg.export_rel_path(node),
        }
    }

    fn pin<'b>(&'b self, c: &'b Ctx, node: i32, id: crate::reader::Guid) -> Option<&'b Pin> {
        c.obj(node)?.pins().iter().find(|p| p.id == id)
    }
}

fn pin_label(p: &Pin) -> String {
    match &p.friendly_name {
        Some(t) if t.display() != p.name && !t.display().is_empty() => format!("{} ({})", p.name, t.display()),
        _ => p.name.clone(),
    }
}

fn is_pure(n: &Object) -> bool {
    !n.pins().iter().any(|p| p.is_exec())
}

/// Orders nodes: entry points first, then following execution, then the rest by position.
fn order_nodes<'a>(c: &Ctx<'a>, nodes: &[&'a Object]) -> Vec<&'a Object> {
    let pos = |n: &Object| {
        (n.prop("NodePosY").and_then(|v| v.as_i64()).unwrap_or(0), n.prop("NodePosX").and_then(|v| v.as_i64()).unwrap_or(0))
    };
    let set: HashSet<i32> = nodes.iter().map(|n| n.index).collect();
    let mut entries: Vec<&Object> = nodes
        .iter()
        .copied()
        .filter(|n| {
            let pins = n.pins();
            pins.iter().any(|p| p.is_exec() && p.is_output()) && !pins.iter().any(|p| p.is_exec() && !p.is_output() && !p.linked_to.is_empty())
        })
        .collect();
    entries.sort_by_key(|n| {
        let rank = if n.class.contains("FunctionEntry") { 0 } else if n.class.contains("Event") { 1 } else { 2 };
        (rank, pos(n))
    });
    let mut out: Vec<&Object> = vec![];
    let mut seen: HashSet<i32> = HashSet::new();
    fn visit<'a>(c: &Ctx<'a>, n: &'a Object, set: &HashSet<i32>, seen: &mut HashSet<i32>, out: &mut Vec<&'a Object>) {
        if !seen.insert(n.index) {
            return;
        }
        // pure data providers first
        for p in n.pins().iter().filter(|p| !p.is_output() && !p.is_exec()) {
            for l in &p.linked_to {
                if let Some(src) = c.obj(l.node) {
                    if set.contains(&src.index) && is_pure(src) && !seen.contains(&src.index) {
                        visit(c, src, set, seen, out);
                    }
                }
            }
        }
        out.push(n);
        for p in n.pins().iter().filter(|p| p.is_output() && p.is_exec()) {
            for l in &p.linked_to {
                if let Some(next) = c.obj(l.node) {
                    if set.contains(&next.index) {
                        visit(c, next, set, seen, out);
                    }
                }
            }
        }
    }
    for e in entries {
        visit(c, e, &set, &mut seen, &mut out);
    }
    let mut rest: Vec<&Object> = nodes.iter().copied().filter(|n| !seen.contains(&n.index)).collect();
    rest.sort_by_key(|n| (n.class.contains("Comment"), pos(n)));
    for n in rest {
        if !seen.contains(&n.index) {
            visit(c, n, &set, &mut seen, &mut out);
        }
    }
    out
}

fn write_node(c: &Ctx, out: &mut Out, g: &GraphCtx, n: &Object) {
    let num = g.num.get(&n.index).copied().unwrap_or(0);
    let mut title = format!("N{num} {}", node_title(c, n));
    let mut tags = vec![];
    if is_pure(n) && !n.pins().is_empty() && !n.class.contains("Comment") && n.class != "K2Node_Knot" {
        tags.push("pure".to_string());
    }
    if n.prop("bOverrideFunction").and_then(|v| v.as_bool()) == Some(true) {
        tags.push("override".into());
    }
    if let Some(Value::Enum(e)) = n.prop("EnabledState") {
        if e.ends_with("Disabled") {
            tags.push("DISABLED".into());
        } else if e.ends_with("DevelopmentOnly") {
            tags.push("development only".into());
        }
    }
    if c.opts.verbose {
        tags.push(n.class.clone());
        let x = n.prop("NodePosX").and_then(|v| v.as_i64()).unwrap_or(0);
        let y = n.prop("NodePosY").and_then(|v| v.as_i64()).unwrap_or(0);
        tags.push(format!("@{x},{y}"));
    }
    if !tags.is_empty() {
        title.push_str(&format!("  [{}]", tags.join(", ")));
    }
    out.line(1, title);
    if !n.class.contains("Comment") {
        if let Some(Value::Str(cm)) = n.prop("NodeComment") {
            if !cm.is_empty() {
                out.line(2, format!("// {}", cm.replace('\n', " / ")));
            }
        }
    }
    if let Some(Value::Str(e)) = n.prop("ErrorMsg") {
        if !e.is_empty() {
            out.line(2, format!("!! {}", e.replace('\n', " / ")));
        }
    }
    // Function metadata / locals on entry nodes
    if let Some(md) = n.prop("MetaData").and_then(|v| v.as_struct()) {
        let mut parts = vec![];
        for f in &md.fields {
            match &f.value {
                Value::Text(t) if !t.display().is_empty() => parts.push(format!("{}={}", f.name, c.quote(&t.display()))),
                Value::Str(s) if !s.is_empty() => parts.push(format!("{}={}", f.name, c.quote(s))),
                Value::Bool(true) => parts.push(f.name.clone()),
                _ => {}
            }
        }
        if !parts.is_empty() {
            out.line(2, format!("meta: {}", parts.join(", ")));
        }
    }
    if let Some(Value::Array(locals)) = n.prop("LocalVariables") {
        for l in locals {
            if let Some(s) = l.as_struct() {
                out.line(2, format!("local {}", variable_line(c, s, None)));
            }
        }
    }
    // other properties
    for p in &n.properties {
        if NODE_SKIP.contains(&p.name.as_str()) && !c.opts.verbose {
            continue;
        }
        if c.opts.verbose && matches!(p.name.as_str(), "Pins" | "NodePosX" | "NodePosY" | "NodeGuid") {
            continue;
        }
        c.write_kv(out, 2, &p.display_name(), &p.value);
    }
    // pins
    let pins = n.pins();
    for p in pins {
        let linked = !p.linked_to.is_empty();
        if p.hidden && !linked && !c.opts.verbose {
            continue;
        }
        let ty = if c.opts.verbose && !p.is_exec() { format!(": {}", p.pin_type.display(c.pkg)) } else { String::new() };
        if p.is_output() {
            if !linked {
                if c.opts.verbose {
                    out.line(2, format!("out {}{ty}", pin_label(p)));
                }
                continue;
            }
            if !p.is_exec() && !c.opts.verbose {
                continue; // data links are listed on the consuming input
            }
            let targets: Vec<String> = p
                .linked_to
                .iter()
                .map(|l| match g.pin(c, l.node, l.pin) {
                    Some(tp) if !tp.is_exec() || c.opts.verbose => format!("{}.{}", g.label(c, l.node), tp.name),
                    _ => g.label(c, l.node),
                })
                .collect();
            let exec_outs = pins.iter().filter(|x| x.is_exec() && x.is_output()).count();
            if p.is_exec() && exec_outs == 1 && (p.name == "then" || p.name == "execute") && !c.opts.verbose {
                out.line(2, format!("-> {}", targets.join(", ")));
            } else {
                out.line(2, format!("{}{ty} -> {}", pin_label(p), targets.join(", ")));
            }
        } else {
            if p.is_exec() {
                if c.opts.verbose && linked {
                    let src: Vec<String> = p.linked_to.iter().map(|l| g.label(c, l.node)).collect();
                    out.line(2, format!("{} <- {}", pin_label(p), src.join(", ")));
                }
                continue;
            }
            if linked {
                let src: Vec<String> = p
                    .linked_to
                    .iter()
                    .map(|l| {
                        let sp = g.pin(c, l.node, l.pin).map(|sp| sp.name.clone()).unwrap_or("?".into());
                        let mut s = format!("{}.{}", g.label(c, l.node), sp);
                        if let Some(src) = c.obj(l.node) {
                            if is_pure(src) {
                                s.push_str(&format!(" ({})", node_title(c, src)));
                            }
                        }
                        s
                    })
                    .collect();
                out.line(2, format!("{}{ty} <- {}", pin_label(p), src.join(", ")));
            } else if let Some(d) = p.default_display(c.pkg) {
                if !c.opts.verbose && d == p.autogenerated_default {
                    continue;
                }
                let d = if p.pin_type.category == "string" || p.pin_type.category == "name" { c.quote(&d) } else { d };
                out.line(2, format!("{}{ty} = {d}", pin_label(p)));
            } else if c.opts.verbose {
                out.line(2, format!("{}{ty}", pin_label(p)));
            }
        }
    }
    for e in &n.errors {
        out.line(2, format!("!! decode error: {e}"));
    }
}

fn signature(c: &Ctx, nodes: &[&Object]) -> Option<String> {
    let entry = nodes.iter().find(|n| n.class == "K2Node_FunctionEntry")?;
    let data_pins = |n: &Object, output: bool| -> Vec<String> {
        n.pins()
            .iter()
            .filter(|p| !p.is_exec() && p.is_output() == output && !p.hidden)
            .map(|p| format!("{}: {}", p.name, p.pin_type.display(c.pkg)))
            .collect()
    };
    let inputs = data_pins(entry, true);
    let mut outputs = vec![];
    if let Some(r) = nodes.iter().find(|n| n.class == "K2Node_FunctionResult") {
        outputs = data_pins(r, false);
    }
    let mut s = format!("({})", inputs.join(", "));
    if !outputs.is_empty() {
        s.push_str(&format!(" -> ({})", outputs.join(", ")));
    }
    if let Some(f) = entry.prop("ExtraFlags").and_then(|v| v.as_i64()) {
        let flags = super::function_flags(f as u32);
        let keep: Vec<&str> = flags
            .split('|')
            .filter(|x| matches!(*x, "BlueprintPure" | "Const" | "Static" | "Private" | "Protected" | "Net" | "NetServer" | "NetClient" | "NetMulticast" | "NetReliable" | "Exec" | "BlueprintAuthorityOnly"))
            .collect();
        if !keep.is_empty() {
            s.push_str(&format!("  [{}]", keep.join(", ")));
        }
    }
    Some(s)
}

fn write_graph(c: &mut Ctx, out: &mut Out, graph: i32, kind: &str, done: &mut HashSet<i32>) {
    if !done.insert(graph) {
        return;
    }
    let Some(g) = c.obj(graph) else { return };
    c.cover(graph);
    let name = c.pkg.exports[graph as usize - 1].object_name.clone();
    let node_ids = objects_in(g.prop("Nodes"));
    let nodes: Vec<&Object> = node_ids.iter().filter_map(|i| c.obj(*i)).collect();
    for n in &nodes {
        c.cover(n.index);
    }
    let ordered = order_nodes(c, &nodes);
    let gc = GraphCtx { num: ordered.iter().enumerate().map(|(i, n)| (n.index, i + 1)).collect(), nodes: ordered.clone() };
    out.blank();
    let sig = if kind == "function" || kind == "interface function" { signature(c, &nodes).unwrap_or_default() } else { String::new() };
    out.line(0, format!("### {} {name}{sig}", capitalize(kind)));
    if g.class != "EdGraph" && c.opts.verbose {
        out.line(0, format!("(graph class {})", g.class));
    }
    if nodes.is_empty() {
        out.line(1, "(empty)");
    }
    // comment boxes: list contained nodes
    for n in gc.nodes.iter().filter(|n| n.class.contains("Comment")) {
        let get = |o: &Object, k: &str| o.prop(k).and_then(|v| v.as_i64()).unwrap_or(0);
        let (x, y, w, h) = (get(n, "NodePosX"), get(n, "NodePosY"), get(n, "NodeWidth"), get(n, "NodeHeight"));
        let inside: Vec<String> = gc
            .nodes
            .iter()
            .filter(|m| m.index != n.index && !m.class.contains("Comment"))
            .filter(|m| {
                let (mx, my) = (get(m, "NodePosX"), get(m, "NodePosY"));
                mx >= x && mx <= x + w && my >= y && my <= y + h
            })
            .map(|m| gc.label(c, m.index))
            .collect();
        let text = n.prop("NodeComment").and_then(|v| v.as_str()).unwrap_or("").replace('\n', " / ");
        if inside.is_empty() {
            out.line(1, format!("// comment {}", c.quote(&text)));
        } else {
            out.line(1, format!("// comment {} around {}", c.quote(&text), inside.join(", ")));
        }
    }
    for n in &gc.nodes {
        if n.class.contains("Comment") {
            continue;
        }
        write_node(c, out, &gc, n);
    }
    // nested graphs (collapsed graphs, state machines, ...)
    let mut subs = objects_in(g.prop("SubGraphs"));
    for n in &nodes {
        for k in ["BoundGraph", "EditorStateMachineGraph"] {
            subs.extend(objects_in(n.prop(k)));
        }
    }
    for s in subs {
        write_graph(c, out, s, "sub-graph", done);
    }
}

fn capitalize(s: &str) -> String {
    let mut ch = s.chars();
    match ch.next() {
        Some(f) => f.to_uppercase().collect::<String>() + ch.as_str(),
        None => String::new(),
    }
}

fn variable_line(c: &Ctx, v: &StructValue, cdo: Option<&Object>) -> String {
    let name = v.get("VarName").and_then(|x| x.as_str()).unwrap_or("?").to_string();
    let ty = v.get("VarType").map(|t| pin_type_value(c, t)).unwrap_or("?".into());
    let mut s = format!("{name}: {ty}");
    let cdo_val = cdo.and_then(|o| o.properties.iter().find(|p| p.name == clean_field_name(&name)));
    if let Some(p) = cdo_val {
        if let Some(i) = c.inline(&p.value, 120) {
            s.push_str(&format!(" = {i}"));
        } else {
            s.push_str(" = (see class defaults)");
        }
    } else if let Some(Value::Str(d)) = v.get("DefaultValue") {
        if !d.is_empty() {
            s.push_str(&format!(" = {d}"));
        }
    }
    let mut attrs = vec![];
    let cat = text_of(v.get("Category"));
    if !cat.is_empty() && cat != "Default" {
        attrs.push(format!("Category={}", c.quote(&cat)));
    }
    if let Some(f) = v.get("PropertyFlags").and_then(|x| x.as_i64()) {
        let f = f as u64;
        if f & 0x1 != 0 && f & 0x10000 == 0 {
            attrs.push("InstanceEditable".into());
        }
        if f & 0x10 != 0 {
            attrs.push("BlueprintReadOnly".into());
        }
        for n in property_flags(f) {
            if matches!(n, "Replicated" | "RepNotify" | "Transient" | "Config" | "SaveGame" | "ExposeOnSpawn" | "Interp" | "BlueprintAssignable" | "AdvancedDisplay") {
                attrs.push(n.to_string());
            }
        }
    }
    if let Some(r) = v.get("RepNotifyFunc").and_then(|x| x.as_str()) {
        if r != "None" && !r.is_empty() {
            attrs.push(format!("RepNotify={r}"));
        }
    }
    if let Some(Value::Enum(e)) = v.get("ReplicationCondition") {
        if !e.ends_with("COND_None") {
            attrs.push(e.clone());
        }
    }
    if let Some(Value::Array(md)) = v.get("MetaDataArray") {
        for m in md {
            if let Some(ms) = m.as_struct() {
                let k = ms.get("DataKey").and_then(|x| x.as_str()).unwrap_or("");
                let val = ms.get("DataValue").and_then(|x| x.as_str()).unwrap_or("");
                if k.is_empty() {
                    continue;
                }
                if val.is_empty() || val == "true" {
                    attrs.push(k.to_string());
                } else {
                    attrs.push(format!("{k}={}", c.quote(val)));
                }
            }
        }
    }
    let fname = v.get("FriendlyName").and_then(|x| x.as_str()).unwrap_or("");
    if !fname.is_empty() && fname != name {
        attrs.push(format!("DisplayName={}", c.quote(fname)));
    }
    if !attrs.is_empty() {
        s.push_str(&format!("  [{}]", attrs.join("; ")));
    }
    s
}

fn write_scs_node(c: &mut Ctx, out: &mut Out, node: i32, indent: usize, depth: u32) {
    if depth > 32 {
        return;
    }
    let Some(n) = c.obj(node) else { return };
    c.cover(node);
    let var = n.prop("InternalVariableName").and_then(|v| v.as_str()).unwrap_or("?").to_string();
    let class = n.prop("ComponentClass").and_then(|v| v.as_object()).map(|i| c.name(i)).unwrap_or("?".into());
    let mut title = format!("- {var} ({class})");
    if let Some(p) = n.prop("ParentComponentOrVariableName").and_then(|v| v.as_str()) {
        if p != "None" && depth == 0 {
            title.push_str(&format!("  attached to inherited {p}"));
        }
    }
    out.line(indent, title);
    if let Some(t) = n.prop("ComponentTemplate").and_then(|v| v.as_object()) {
        if let Some(to) = c.obj(t) {
            c.cover(t);
            c.write_props_filtered(out, indent + 2, &to.properties, &["CreationMethod", "UCSSerializationIndex", "bNetAddressable"]);
        }
    }
    for child in objects_in(n.prop("ChildNodes")) {
        write_scs_node(c, out, child, indent + 1, depth + 1);
    }
}

fn write_widget(c: &mut Ctx, out: &mut Out, w: i32, indent: usize, depth: u32) {
    if depth > 64 {
        return;
    }
    let Some(o) = c.obj(w) else { return };
    c.cover(w);
    let name = c.pkg.exports[w as usize - 1].object_name.clone();
    out.line(indent, format!("- {name} ({})", o.class));
    c.write_props_filtered(out, indent + 2, &o.properties, &["Slot", "Slots", "bIsVariable", "DisplayLabel", "bExpandedInDesigner", "bLockedInDesigner", "bHiddenInDesigner"]);
    if let Some(slot) = o.prop("Slot").and_then(|v| v.as_object()).and_then(|i| c.obj(i)) {
        c.cover(slot.index);
        let props: Vec<_> = slot.properties.iter().filter(|p| p.name != "Parent" && p.name != "Content").cloned().collect();
        if !props.is_empty() {
            let sv = Value::Struct(Box::new(StructValue { type_name: slot.class.clone(), fields: props, native: false }));
            c.write_kv(out, indent + 2, &format!("Slot ({})", slot.class), &sv);
        }
    }
    for s in objects_in(o.prop("Slots")) {
        if let Some(slot) = c.obj(s) {
            if let Some(content) = slot.prop("Content").and_then(|v| v.as_object()) {
                write_widget(c, out, content, indent + 1, depth + 1);
            }
        }
    }
    // Named slot content (user widgets)
    if let Some(Value::Map(m)) = o.prop("NamedSlotBindings") {
        for (_, v) in m {
            if let Some(i) = v.as_object() {
                write_widget(c, out, i, indent + 1, depth + 1);
            }
        }
    }
}

pub fn render(c: &mut Ctx, out: &mut Out, bp: &Object) {
    c.cover(bp.index);
    let name = c.pkg.exports[bp.index as usize - 1].object_name.clone();
    out.blank();
    out.line(0, format!("## {} {name}", bp.class));
    let parent = bp.prop("ParentClass").map(|v| c.inline(v, 200).unwrap_or_default()).unwrap_or_default();
    out.line(0, format!("ParentClass: {parent}"));
    if let Some(Value::Enum(t)) = bp.prop("BlueprintType") {
        if t != "BPTYPE_Normal" {
            out.line(0, format!("BlueprintType: {t}"));
        }
    }
    for k in ["BlueprintDescription", "BlueprintCategory", "BlueprintDisplayName"] {
        if let Some(Value::Str(s)) = bp.prop(k) {
            if !s.is_empty() {
                out.line(0, format!("{k}: {}", c.quote(s)));
            }
        }
    }
    let mut interface_graphs = vec![];
    if let Some(Value::Array(ifaces)) = bp.prop("ImplementedInterfaces") {
        let mut names = vec![];
        for i in ifaces {
            if let Some(s) = i.as_struct() {
                if let Some(cls) = s.get("Interface").and_then(|v| v.as_object()) {
                    names.push(c.obj_ref(cls));
                }
                interface_graphs.extend(objects_in(s.get("Graphs")));
            }
        }
        if !names.is_empty() {
            out.line(0, format!("Interfaces: {}", names.join(", ")));
        }
    }

    // Generated class & CDO
    let gen_class = bp.prop("GeneratedClass").and_then(|v| v.as_object()).filter(|i| *i > 0);
    let mut cdo: Option<&Object> = None;
    if let Some(g) = gen_class.and_then(|g| c.obj(g)) {
        c.cover(g.index);
        if let Some(ci) = g.struct_info().and_then(|s| s.class.as_ref()) {
            cdo = c.obj(ci.cdo);
        }
        // functions and other children of the generated class are compiled data
        for (i, e) in c.pkg.exports.iter().enumerate() {
            if e.outer_index == g.index && (c.objs[i].class.ends_with("Function")) {
                c.covered.insert(i as i32 + 1);
            }
        }
    }
    if cdo.is_none() {
        let cdo_name = format!("Default__{name}_C");
        cdo = c.objs.iter().find(|o| c.pkg.exports[o.index as usize - 1].object_name == cdo_name);
    }

    // Variables
    if let Some(Value::Array(vars)) = bp.prop("NewVariables") {
        out.blank();
        out.line(0, format!("### Variables ({})", vars.len()));
        for v in vars {
            if let Some(s) = v.as_struct() {
                out.line(0, format!("- {}", variable_line(c, s, cdo)));
            }
        }
    }

    // Components
    let scs = bp
        .prop("SimpleConstructionScript")
        .and_then(|v| v.as_object())
        .or_else(|| gen_class.and_then(|g| c.obj(g)).and_then(|g| g.prop("SimpleConstructionScript")).and_then(|v| v.as_object()));
    if let Some(scs_o) = scs.and_then(|s| c.obj(s)) {
        c.cover(scs_o.index);
        let mut roots = objects_in(scs_o.prop("RootNodes"));
        roots.extend(objects_in(scs_o.prop("DefaultSceneRootNode")));
        let all = objects_in(scs_o.prop("AllNodes"));
        if !roots.is_empty() || !all.is_empty() {
            out.blank();
            out.line(0, "### Components");
            for r in roots {
                if !c.covered.contains(&r) {
                    write_scs_node(c, out, r, 0, 0);
                }
            }
            // nodes not reachable from roots (attached to inherited components)
            for n in all {
                if !c.covered.contains(&n) {
                    write_scs_node(c, out, n, 0, 0);
                }
            }
        }
    }
    let ich = gen_class
        .and_then(|g| c.obj(g))
        .and_then(|g| g.prop("InheritableComponentHandler"))
        .or(bp.prop("InheritableComponentHandler"))
        .and_then(|v| v.as_object());
    if let Some(h) = ich.and_then(|i| c.obj(i)) {
        c.cover(h.index);
        if let Some(Value::Array(records)) = h.prop("Records") {
            out.blank();
            out.line(0, "### Inherited component overrides");
            for r in records {
                let Some(s) = r.as_struct() else { continue };
                let key = s.get("ComponentKey").and_then(|v| v.as_struct());
                let var = key.and_then(|k| k.get("SCSVariableName")).and_then(|v| v.as_str()).unwrap_or("?").to_string();
                let owner = key.and_then(|k| k.get("OwnerClass")).and_then(|v| v.as_object()).map(|i| c.name(i)).unwrap_or_default();
                let tmpl = s.get("ComponentTemplate").and_then(|v| v.as_object());
                let class = tmpl.map(|t| c.pkg.class_name(t)).unwrap_or_default();
                out.line(0, format!("- {var} ({class}) from {owner}"));
                if let Some(t) = tmpl.and_then(|t| c.obj(t)) {
                    c.cover(t.index);
                    c.write_props_filtered(out, 2, &t.properties, &["CreationMethod", "UCSSerializationIndex"]);
                }
            }
        }
    }

    // Widget tree
    if let Some(wt) = bp.prop("WidgetTree").and_then(|v| v.as_object()).and_then(|i| c.obj(i)) {
        c.cover(wt.index);
        if let Some(root) = wt.prop("RootWidget").and_then(|v| v.as_object()) {
            out.blank();
            out.line(0, "### Widget tree");
            write_widget(c, out, root, 0, 0);
        }
        // widgets of the tree that are not reachable (e.g. named slot contents)
        let all = objects_in(wt.prop("AllWidgets"));
        for w in all {
            if !c.covered.contains(&w) {
                write_widget(c, out, w, 0, 0);
            }
        }
    }
    // The generated class carries a runtime copy of the widget tree.
    if let Some(wt) = gen_class.and_then(|g| c.obj(g)).and_then(|g| g.prop("WidgetTree")).and_then(|v| v.as_object()) {
        if bp.prop("WidgetTree").is_some() {
            c.cover_subtree(wt);
        }
    }
    // Delegate bindings duplicate bound events in the graph.
    let bindings: Vec<i32> = c.objs.iter().filter(|o| o.class.ends_with("DelegateBinding")).map(|o| o.index).collect();
    for b in bindings {
        c.cover(b);
    }
    let anims = objects_in(bp.prop("Animations"));
    if !anims.is_empty() {
        let names: Vec<String> = anims.iter().map(|a| c.name(*a)).collect();
        out.line(0, format!("Widget animations: {}", names.join(", ")));
    }

    // Class defaults
    if let Some(d) = cdo {
        c.cover(d.index);
        let vars: HashSet<String> = match bp.prop("NewVariables") {
            Some(Value::Array(vs)) => vs
                .iter()
                .filter_map(|v| v.as_struct().and_then(|s| s.get("VarName")).and_then(|x| x.as_str()).map(clean_field_name))
                .collect(),
            _ => HashSet::new(),
        };
        let props: Vec<_> = d
            .properties
            .iter()
            .filter(|p| p.name != "UberGraphFrame" && (!vars.contains(&p.name) || c.inline(&p.value, 120).is_none()))
            .cloned()
            .collect();
        if !props.is_empty() {
            out.blank();
            out.line(0, "### Class defaults");
            c.write_props(out, 0, &props);
        }
        // default subobjects of the CDO
        for (i, e) in c.pkg.exports.iter().enumerate() {
            if e.outer_index == d.index {
                let o = &c.objs[i];
                if !o.properties.is_empty() {
                    c.write_object(out, 0, o, Some(format!("{} ({}) [default subobject]", e.object_name, o.class)));
                } else {
                    c.covered.insert(o.index);
                }
            }
        }
    }

    // Graphs
    let mut done = HashSet::new();
    let lists: [(&str, Vec<i32>); 5] = [
        ("event graph", objects_in(bp.prop("UbergraphPages"))),
        ("function", objects_in(bp.prop("FunctionGraphs"))),
        ("interface function", interface_graphs),
        ("event dispatcher", objects_in(bp.prop("DelegateSignatureGraphs"))),
        ("macro", objects_in(bp.prop("MacroGraphs"))),
    ];
    for (kind, graphs) in lists {
        for g in graphs {
            if kind == "event dispatcher" {
                if done.insert(g) {
                    c.cover(g);
                    let nodes: Vec<&Object> = objects_in(c.obj(g).and_then(|o| o.prop("Nodes"))).iter().filter_map(|i| c.obj(*i)).collect();
                    for n in &nodes {
                        c.cover(n.index);
                    }
                    let sig = signature(c, &nodes).unwrap_or_default();
                    out.blank();
                    out.line(0, format!("### Event dispatcher {}{sig}", c.name(g)));
                }
                continue;
            }
            write_graph(c, out, g, kind, &mut done);
        }
    }
    // Any other graphs in the package (anim graphs, state machines...)
    let others: Vec<i32> = c
        .objs
        .iter()
        .filter(|o| !c.covered.contains(&o.index) && o.prop("Nodes").is_some() && (o.class.contains("Graph")))
        .map(|o| o.index)
        .collect();
    for g in others {
        write_graph(c, out, g, "graph", &mut done);
    }

    // Blueprints without editor graphs: list compiled functions
    if done.is_empty() {
        if let Some(g) = gen_class {
            let funcs: Vec<String> = c
                .pkg
                .exports
                .iter()
                .enumerate()
                .filter(|(i, e)| e.outer_index == g && c.objs[*i].class.ends_with("Function"))
                .map(|(i, e)| {
                    let params: Vec<String> = c.objs[i]
                        .struct_info()
                        .map(|s| s.fields.iter().filter(|f| f.flags & 0x80 != 0).map(|f| format!("{}: {}", f.name, c.field_type(f))).collect())
                        .unwrap_or_default();
                    format!("- {}({})", e.object_name, params.join(", "))
                })
                .collect();
            if !funcs.is_empty() {
                out.blank();
                out.line(0, "### Compiled functions (no editor graphs in package)");
                for f in funcs {
                    out.line(0, f);
                }
            }
        }
    }
    // Remaining blueprint-level properties
    let shown = [
        "ParentClass", "BlueprintType", "GeneratedClass", "NewVariables", "SimpleConstructionScript", "UbergraphPages",
        "FunctionGraphs", "DelegateSignatureGraphs", "MacroGraphs", "ImplementedInterfaces", "WidgetTree", "Animations",
        "BlueprintDescription", "BlueprintCategory", "BlueprintDisplayName", "LastEditedDocuments", "CategorySorting",
        "BlueprintSystemVersion", "BlueprintGuid", "SkeletonGeneratedClass", "ThumbnailInfo", "InheritableComponentHandler",
        "bLegacyNeedToPurgeSkelRefs", "IntermediateGeneratedGraphs", "EventGraphs", "PaletteCategory",
        "bLegacyGeneratedClassIsAuthoritative", "BlueprintNamespace", "ImportedNamespaces", "ComponentTemplates", "Timelines",
    ];
    let rest: Vec<_> = bp.properties.iter().filter(|p| !shown.contains(&p.name.as_str())).cloned().collect();
    let timelines = objects_in(bp.prop("Timelines"));
    if !rest.is_empty() || !timelines.is_empty() {
        out.blank();
        out.line(0, "### Other blueprint properties");
        c.write_props(out, 0, &rest);
        for t in timelines {
            if let Some(o) = c.obj(t) {
                c.write_object(out, 0, o, Some(format!("Timeline {}", c.name(t))));
            }
        }
    }

}
