//! Material / material function / material instance view.

use std::collections::{HashMap, HashSet};

use super::{Ctx, Out};
use crate::object::Object;
use crate::property::{Property, StructValue, Value};

pub fn is_material_like(class: &str) -> bool {
    matches!(
        class,
        "Material" | "MaterialFunction" | "MaterialFunctionMaterialLayer" | "MaterialFunctionMaterialLayerBlend"
            | "MaterialInstanceConstant" | "LandscapeMaterialInstanceConstant" | "MaterialInstanceDynamic"
            | "MaterialFunctionInstance" | "MaterialFunctionMaterialLayerInstance" | "MaterialFunctionMaterialLayerBlendInstance"
    )
}

/// (expression, output index, mask suffix, input name)
pub(crate) struct InputRef {
    pub expr: i32,
    pub output_index: i64,
    pub mask: String,
}

pub(crate) fn as_input(v: &Value) -> Option<InputRef> {
    let s = v.as_struct()?;
    let expr = s.get("Expression")?.as_object()?;
    let output_index = s.get("OutputIndex").and_then(|x| x.as_i64()).unwrap_or(0);
    let flag = |k: &str| s.get(k).map(|x| x.as_i64().unwrap_or(0) != 0 || x.as_bool() == Some(true)).unwrap_or(false);
    let mut mask = String::new();
    if flag("Mask") {
        for (k, ch) in [("MaskR", 'R'), ("MaskG", 'G'), ("MaskB", 'B'), ("MaskA", 'A')] {
            if flag(k) {
                mask.push(ch);
            }
        }
    }
    Some(InputRef { expr, output_index, mask })
}

pub(crate) fn is_input_struct(v: &Value) -> bool {
    match v {
        Value::Struct(s) => s.type_name.ends_with("Input") || (s.get("Expression").is_some() && s.get("OutputIndex").is_some()),
        _ => false,
    }
}

const EXPR_SKIP: &[&str] = &[
    "MaterialExpressionEditorX", "MaterialExpressionEditorY", "MaterialExpressionGuid", "Material", "Function",
    "ExpressionGUID", "bRealtimePreview", "bNeedToUpdatePreview", "bCollapsed", "Outputs", "bShowOutputNameOnPin",
    "bHidePreviewWindow", "bShowMaskColorsOnPin", "bShowInputs", "bShowOutputs", "ParameterName", "bIsParameterExpression",
    "SubgraphExpression", "bCommentBubbleVisible", "bPreviewExpression", "GraphNode", "bUseMaterialAttributes",
    "FunctionInputs", "FunctionOutputs", "MaterialFunction", "Desc", "Text", "SizeX", "SizeY", "CommentColor",
    "FontSize", "bColorCommentBubble", "bGroupMode", "bCommentBubbleVisible_InDetailsPanel", "ExpressionCollection",
    "bPinned", "OutputName", "InputName",
];

struct MatCtx {
    num: HashMap<i32, usize>,
}

impl MatCtx {
    fn label(&self, c: &Ctx, e: i32) -> String {
        self.num.get(&e).map(|n| format!("E{n}")).unwrap_or_else(|| c.obj_ref(e))
    }
}

fn expr_title(c: &Ctx, e: &Object) -> String {
    let short = e.class.strip_prefix("MaterialExpression").unwrap_or(&e.class).to_string();
    let p = |k: &str| e.prop(k);
    let s = |k: &str| p(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
    let mut t = short.clone();
    let pname = s("ParameterName");
    if !pname.is_empty() && pname != "None" {
        t.push_str(&format!(" {}", c.quote(&pname)));
    }
    match short.as_str() {
        "MaterialFunctionCall" | "MaterialLayerFunctionCall" => {
            if let Some(f) = p("MaterialFunction") {
                t.push_str(&format!(" {}", c.inline(f, 200).unwrap_or_default()));
            }
        }
        "FunctionInput" => t.push_str(&format!(" {}", c.quote(&s("InputName")))),
        "FunctionOutput" => t.push_str(&format!(" {}", c.quote(&s("OutputName")))),
        "Custom" => {
            if let Some(Value::Str(d)) = p("Description") {
                t.push_str(&format!(" {}", c.quote(d)));
            }
        }
        "NamedRerouteDeclaration" => t.push_str(&format!(" {}", c.quote(&s("Name")))),
        "NamedRerouteUsage" => {
            if let Some(d) = p("Declaration").and_then(|v| v.as_object()) {
                t.push_str(&format!(" -> {}", c.name(d)));
            }
        }
        _ => {}
    }
    t
}

fn output_name(c: &Ctx, inp: &InputRef) -> String {
    if !inp.mask.is_empty() {
        return format!(".{}", inp.mask);
    }
    if let Some(src) = c.obj(inp.expr) {
        if let Some(Value::Array(outs)) = src.prop("FunctionOutputs") {
            if let Some(o) = outs.get(inp.output_index as usize).and_then(|o| o.as_struct()) {
                if let Some(n) = o.get("Output").and_then(|x| x.as_struct()).and_then(|x| x.get("OutputName")).and_then(|x| x.as_str()) {
                    return format!(".{n}");
                }
            }
        }
    }
    if inp.output_index > 0 { format!(".out{}", inp.output_index) } else { String::new() }
}

fn write_input(c: &Ctx, out: &mut Out, m: &MatCtx, indent: usize, name: &str, v: &Value) -> bool {
    let Some(inp) = as_input(v) else { return false };
    if inp.expr == 0 {
        // unconnected: show constant if any
        if let Some(s) = v.as_struct() {
            if s.get("UseConstant").and_then(|x| x.as_bool()) == Some(true) {
                if let Some(k) = s.get("Constant") {
                    out.line(indent, format!("{name} = {}", c.inline(k, 120).unwrap_or_default()));
                }
            }
        }
        return true;
    }
    let src = c.obj(inp.expr).map(|e| format!(" ({})", expr_title(c, e))).unwrap_or_default();
    out.line(indent, format!("{name} <- {}{}{src}", m.label(c, inp.expr), output_name(c, &inp)));
    true
}

fn collect_inputs(e: &Object) -> Vec<i32> {
    let mut v = vec![];
    fn rec(val: &Value, v: &mut Vec<i32>) {
        if let Some(i) = as_input(val) {
            if i.expr != 0 {
                v.push(i.expr);
            }
            return;
        }
        match val {
            Value::Struct(s) => s.fields.iter().for_each(|f| rec(&f.value, v)),
            Value::Array(a) => a.iter().for_each(|x| rec(x, v)),
            _ => {}
        }
    }
    for p in &e.properties {
        rec(&p.value, &mut v);
    }
    v
}

fn write_expression(c: &mut Ctx, out: &mut Out, m: &MatCtx, e: &Object) {
    c.cover(e.index);
    let num = m.num.get(&e.index).copied().unwrap_or(0);
    let mut title = format!("E{num} {}", expr_title(c, e));
    if c.opts.verbose {
        let x = e.prop("MaterialExpressionEditorX").and_then(|v| v.as_i64()).unwrap_or(0);
        let y = e.prop("MaterialExpressionEditorY").and_then(|v| v.as_i64()).unwrap_or(0);
        title.push_str(&format!("  [@{x},{y}]"));
    }
    out.line(1, title);
    if let Some(Value::Str(d)) = e.prop("Desc") {
        if !d.is_empty() {
            out.line(2, format!("// {}", d.replace('\n', " / ")));
        }
    }
    // Function call inputs
    if let Some(Value::Array(fins)) = e.prop("FunctionInputs") {
        for fi in fins {
            if let Some(s) = fi.as_struct() {
                if let Some(inp) = s.get("Input") {
                    let name = inp.as_struct().and_then(|x| x.get("InputName")).and_then(|x| x.as_str()).unwrap_or("?").to_string();
                    write_input(c, out, m, 2, &name, inp);
                }
            }
        }
    }
    for p in &e.properties {
        if matches!(p.name.as_str(), "FunctionInputs" | "FunctionOutputs") && !c.opts.verbose {
            continue;
        }
        if is_input_struct(&p.value) {
            write_input(c, out, m, 2, &p.display_name(), &p.value);
            continue;
        }
        // arrays of inputs (e.g. Custom node Inputs, Switch inputs)
        if let Value::Array(a) = &p.value {
            if a.iter().any(|x| x.as_struct().map(|s| s.fields.iter().any(|f| is_input_struct(&f.value))).unwrap_or(false)) {
                for (i, x) in a.iter().enumerate() {
                    let Some(s) = x.as_struct() else { continue };
                    let label = s.fields.iter().find(|f| matches!(f.value, Value::Name(_) | Value::Str(_))).and_then(|f| f.value.as_str()).map(|n| n.to_string()).unwrap_or(format!("{}[{i}]", p.name));
                    for f in &s.fields {
                        if is_input_struct(&f.value) {
                            write_input(c, out, m, 2, &label, &f.value);
                        }
                    }
                }
                continue;
            }
        }
        if EXPR_SKIP.contains(&p.name.as_str()) && !c.opts.verbose {
            if !matches!(p.name.as_str(), "Text") || !e.class.contains("Comment") {
                continue;
            }
        }
        c.write_kv(out, 2, &p.display_name(), &p.value);
    }
    for err in &e.errors {
        out.line(2, format!("!! decode error: {err}"));
    }
}

fn param_name(v: Option<&Value>) -> String {
    let Some(s) = v.and_then(|v| v.as_struct()) else { return "?".into() };
    let name = s.get("Name").and_then(|x| x.as_str()).unwrap_or("?").to_string();
    match s.get("Association").and_then(|x| x.as_str()) {
        Some(a) if !a.ends_with("GlobalParameter") => {
            let idx = s.get("Index").and_then(|x| x.as_i64()).unwrap_or(-1);
            format!("{name} [{} {idx}]", a.rsplit("::").next().unwrap_or(a))
        }
        _ => name,
    }
}

fn write_instance_params(c: &Ctx, out: &mut Out, props: &[Property]) -> Vec<&'static str> {
    const LISTS: &[(&str, &str, &str)] = &[
        ("ScalarParameterValues", "Scalar parameters", "ParameterValue"),
        ("VectorParameterValues", "Vector parameters", "ParameterValue"),
        ("DoubleVectorParameterValues", "Double vector parameters", "ParameterValue"),
        ("TextureParameterValues", "Texture parameters", "ParameterValue"),
        ("TextureCollectionParameterValues", "Texture collection parameters", "ParameterValue"),
        ("RuntimeVirtualTextureParameterValues", "Runtime virtual texture parameters", "ParameterValue"),
        ("SparseVolumeTextureParameterValues", "Sparse volume texture parameters", "ParameterValue"),
        ("FontParameterValues", "Font parameters", "FontValue"),
        ("ParameterCollectionParameterValues", "Parameter collection parameters", "ParameterValue"),
    ];
    let mut used = vec![];
    for (prop, title, value_key) in LISTS {
        let Some(Value::Array(items)) = crate::property::find(props, prop) else { continue };
        used.push(*prop);
        if items.is_empty() {
            continue;
        }
        out.line(0, format!("{title}:"));
        for it in items {
            let Some(s) = it.as_struct() else { continue };
            let name = param_name(s.get("ParameterInfo"));
            let mut val = s.get(value_key).map(|v| c.inline(v, 140).unwrap_or("<complex>".into())).unwrap_or("?".into());
            if *prop == "FontParameterValues" {
                if let Some(pg) = s.get("FontPage") {
                    val.push_str(&format!(" page {}", c.inline(pg, 20).unwrap_or_default()));
                }
            }
            out.line(1, format!("{name} = {val}"));
        }
    }
    used
}

fn write_static_params(c: &Ctx, out: &mut Out, v: &Value) {
    let Some(s) = v.as_struct() else { return };
    for (key, title) in [
        ("StaticSwitchParameters", "Static switch parameters"),
        ("StaticComponentMaskParameters", "Static component mask parameters"),
        ("TerrainLayerWeightParameters", "Terrain layer weight parameters"),
        ("MaterialLayersParameters", "Material layers"),
    ] {
        let Some(Value::Array(items)) = s.get(key) else { continue };
        if items.is_empty() {
            continue;
        }
        out.line(0, format!("{title}:"));
        for it in items {
            let Some(p) = it.as_struct() else { continue };
            let name = param_name(p.get("ParameterInfo"));
            let overridden = p.get("bOverride").and_then(|x| x.as_bool()).unwrap_or(true);
            let val = match key {
                "StaticComponentMaskParameters" => {
                    let ch: String = [("R", 'R'), ("G", 'G'), ("B", 'B'), ("A", 'A')]
                        .iter()
                        .filter(|(k, _)| p.get(k).and_then(|x| x.as_bool()) == Some(true))
                        .map(|(_, c)| *c)
                        .collect();
                    ch
                }
                _ => p.get("Value").map(|v| c.inline(v, 140).unwrap_or_default()).unwrap_or_default(),
            };
            out.line(1, format!("{name} = {val}{}", if overridden { "" } else { "  (not overridden)" }));
        }
    }
    if let Some(Value::Struct(_)) = s.get("MaterialLayers") {
        c.write_kv(out, 0, "MaterialLayers", s.get("MaterialLayers").unwrap());
    }
}

fn render_instance(c: &mut Ctx, out: &mut Out, o: &Object) {
    let name = c.pkg.exports[o.index as usize - 1].object_name.clone();
    out.blank();
    out.line(0, format!("## {} {name}", o.class));
    if let Some(p) = o.prop("Parent") {
        out.line(0, format!("Parent: {}", c.inline(p, 300).unwrap_or_default()));
    }
    let mut used = write_instance_params(c, out, &o.properties);
    used.push("Parent");
    for k in ["StaticParameters", "StaticParametersRuntime"] {
        if let Some(v) = o.prop(k) {
            write_static_params(c, out, v);
            used.push(k);
        }
    }
    // Editor-only data object holds static parameters in UE 5.1+
    let editor_data: Vec<&Object> = c
        .objs
        .iter()
        .filter(|x| c.pkg.exports[x.index as usize - 1].outer_index == o.index && x.class.ends_with("EditorOnlyData"))
        .collect();
    for ed in editor_data {
        c.cover(ed.index);
        if let Some(v) = ed.prop("StaticParameters") {
            write_static_params(c, out, v);
        }
        let rest: Vec<_> = ed.properties.iter().filter(|p| p.name != "StaticParameters").cloned().collect();
        if !rest.is_empty() {
            c.write_props(out, 0, &rest);
        }
    }
    let skip = ["ParameterStateId", "LightingGuid", "ThumbnailInfo", "ReferencedTextureGuids", "CachedReferencedTextures", "bHasStaticPermutationResource"];
    let rest: Vec<_> = o.properties.iter().filter(|p| !used.contains(&p.name.as_str()) && (c.opts.verbose || !skip.contains(&p.name.as_str()))).cloned().collect();
    if !rest.is_empty() {
        out.line(0, "Other properties:");
        c.write_props(out, 1, &rest);
    }
}

pub fn render(c: &mut Ctx, out: &mut Out, o: &Object) {
    c.cover(o.index);
    if o.class.contains("Instance") {
        render_instance(c, out, o);
        return;
    }
    let name = c.pkg.exports[o.index as usize - 1].object_name.clone();
    // Editor only data (UE 5.1+) holds the inputs and the expression collection.
    let editor_data: Vec<&Object> = c
        .objs
        .iter()
        .filter(|x| c.pkg.exports[x.index as usize - 1].outer_index == o.index && x.class.ends_with("EditorOnlyData"))
        .collect();
    for ed in &editor_data {
        c.cover(ed.index);
    }
    let all_props: Vec<&Property> = o.properties.iter().chain(editor_data.iter().flat_map(|e| e.properties.iter())).collect();

    // Expressions
    let mut exprs: Vec<i32> = vec![];
    let mut comments: Vec<i32> = vec![];
    for p in &all_props {
        match (p.name.as_str(), &p.value) {
            ("ExpressionCollection", Value::Struct(s)) => {
                for f in &s.fields {
                    match f.name.as_str() {
                        "Expressions" => exprs.extend(f.value.as_array().unwrap_or(&[]).iter().filter_map(|x| x.as_object())),
                        "EditorComments" => comments.extend(f.value.as_array().unwrap_or(&[]).iter().filter_map(|x| x.as_object())),
                        _ => {}
                    }
                }
            }
            ("Expressions" | "FunctionExpressions", Value::Array(a)) => exprs.extend(a.iter().filter_map(|x| x.as_object())),
            ("EditorComments", Value::Array(a)) => comments.extend(a.iter().filter_map(|x| x.as_object())),
            _ => {}
        }
    }
    for x in c.objs.iter() {
        if x.class.starts_with("MaterialExpression") && !exprs.contains(&x.index) && !comments.contains(&x.index) {
            if x.class == "MaterialExpressionComment" {
                comments.push(x.index);
            } else {
                exprs.push(x.index);
            }
        }
    }
    exprs.retain(|e| *e > 0);

    // Outputs: material input properties
    let outputs: Vec<(String, &Value)> = all_props
        .iter()
        .filter(|p| is_input_struct(&p.value))
        .map(|p| (p.display_name(), &p.value))
        .chain(all_props.iter().filter(|p| p.name == "CustomizedUVs").flat_map(|p| {
            p.value.as_array().unwrap_or(&[]).iter().enumerate().map(|(i, v)| (format!("CustomizedUVs[{i}]"), v)).collect::<Vec<_>>()
        }))
        .collect();

    // Number expressions: DFS from outputs, then function outputs, then the rest.
    let mut order: Vec<i32> = vec![];
    let mut seen: HashSet<i32> = HashSet::new();
    fn visit(c: &Ctx, e: i32, seen: &mut HashSet<i32>, order: &mut Vec<i32>, depth: u32) {
        if e <= 0 || depth > 2000 || !seen.insert(e) {
            return;
        }
        order.push(e);
        if let Some(o) = c.obj(e) {
            for i in collect_inputs(o) {
                visit(c, i, seen, order, depth + 1);
            }
        }
    }
    for (_, v) in &outputs {
        if let Some(i) = as_input(v) {
            visit(c, i.expr, &mut seen, &mut order, 0);
        }
    }
    let mut func_outputs: Vec<i32> = exprs.iter().copied().filter(|e| c.obj(*e).map(|o| o.class == "MaterialExpressionFunctionOutput").unwrap_or(false)).collect();
    func_outputs.sort_by_key(|e| c.obj(*e).and_then(|o| o.prop("SortPriority")).and_then(|v| v.as_i64()).unwrap_or(0));
    for e in &func_outputs {
        visit(c, *e, &mut seen, &mut order, 0);
    }
    let mut rest: Vec<i32> = exprs.iter().copied().filter(|e| !seen.contains(e)).collect();
    rest.sort_by_key(|e| {
        let o = c.obj(*e);
        let y = o.and_then(|o| o.prop("MaterialExpressionEditorY")).and_then(|v| v.as_i64()).unwrap_or(0);
        let x = o.and_then(|o| o.prop("MaterialExpressionEditorX")).and_then(|v| v.as_i64()).unwrap_or(0);
        (x, y)
    });
    for e in rest {
        visit(c, e, &mut seen, &mut order, 0);
    }
    let m = MatCtx { num: order.iter().enumerate().map(|(i, e)| (*e, i + 1)).collect() };

    out.blank();
    out.line(0, format!("## {} {name}", o.class));
    let skip_props = [
        "ExpressionCollection", "Expressions", "FunctionExpressions", "EditorComments", "StateId", "LightingGuid",
        "ThumbnailInfo", "ReferencedTextureGuids", "CachedExpressionData", "EditorX", "EditorY", "EditorPitch", "EditorYaw",
        "MaterialGraph", "ParameterGroupData", "bCanMaskedBeAssumedOpaque", "CustomizedUVs", "Expressions",
        "DependentFunctionExpressionCandidates", "CombinedInputTypes", "CombinedOutputTypes", "EditorOnlyData",
        "ParameterOverviewExpansion", "PreviewMesh", "bEnableExecWire", "bEnableNewHLSLGenerator",
    ];
    let settings: Vec<Property> = all_props
        .iter()
        .filter(|p| !is_input_struct(&p.value) && (c.opts.verbose || !skip_props.contains(&p.name.as_str())))
        .map(|p| (*p).clone())
        .collect();
    if !settings.is_empty() {
        out.line(0, "Settings:");
        c.write_props(out, 1, &settings);
    }
    if !c.opts.nodes {
        if !comments.is_empty() {
            out.line(0, "Comments:");
            for cm in &comments {
                if let Some(co) = c.obj(*cm) {
                    c.cover(co.index);
                    let text = co.prop("Text").and_then(|v| v.as_str()).unwrap_or("").replace('\r', "").replace('\n', " / ");
                    out.line(1, format!("// {}", c.quote(&text)));
                }
            }
        }
        let kind = if o.class.starts_with("MaterialFunction") { "function" } else { "material" };
        let code = crate::ast::material::material_to_code(c, &name, kind, &outputs, &order);
        out.line(0, "```");
        out.s.push_str(&code);
        out.line(0, "```");
        for e in &order {
            c.cover(*e);
        }
        let graph_objs: Vec<i32> = c.objs.iter().filter(|x| x.class.starts_with("MaterialGraph")).map(|x| x.index).collect();
        for g in graph_objs {
            c.cover(g);
        }
        return;
    }
    if !outputs.is_empty() {
        out.line(0, "Outputs:");
        let mut any = false;
        for (n, v) in &outputs {
            if let Some(i) = as_input(v) {
                if i.expr == 0 && !c.opts.verbose {
                    continue;
                }
            }
            any |= write_input(c, out, &m, 1, n, v);
        }
        if !any {
            out.line(1, "(none connected)");
        }
    }
    if !comments.is_empty() {
        out.line(0, "Comments:");
        for cm in &comments {
            if let Some(co) = c.obj(*cm) {
                c.cover(co.index);
                let text = co.prop("Text").and_then(|v| v.as_str()).unwrap_or("").replace('\n', " / ");
                // expressions inside the comment box
                let get = |o: &Object, k: &str| o.prop(k).and_then(|v| v.as_i64()).unwrap_or(0);
                let (x, y) = (get(co, "MaterialExpressionEditorX"), get(co, "MaterialExpressionEditorY"));
                let (w, h) = (get(co, "SizeX"), get(co, "SizeY"));
                let inside: Vec<String> = order
                    .iter()
                    .filter_map(|e| c.obj(*e))
                    .filter(|e| {
                        let (ex, ey) = (get(e, "MaterialExpressionEditorX"), get(e, "MaterialExpressionEditorY"));
                        ex >= x && ex <= x + w && ey >= y && ey <= y + h
                    })
                    .map(|e| m.label(c, e.index))
                    .collect();
                if inside.is_empty() {
                    out.line(1, format!("// {}", c.quote(&text)));
                } else {
                    out.line(1, format!("// {} around {}", c.quote(&text), inside.join(", ")));
                }
            }
        }
    }
    if !order.is_empty() {
        out.line(0, format!("Expressions ({}):", order.len()));
        for e in &order {
            if let Some(eo) = c.obj(*e) {
                write_expression(c, out, &m, eo);
            } else {
                out.line(1, format!("E{} {}", m.num[e], c.obj_ref(*e)));
            }
        }
    }
    // Material graph editor objects duplicate expression data
    let graph_objs: Vec<i32> = c.objs.iter().filter(|x| x.class.starts_with("MaterialGraph")).map(|x| x.index).collect();
    for g in graph_objs {
        c.cover(g);
    }
    let _ = StructValue { type_name: String::new(), fields: vec![], native: false };
}
