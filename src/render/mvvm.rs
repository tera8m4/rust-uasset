//! Model-View-ViewModel plugin: view bindings of a widget blueprint (UMVVMBlueprintView).

use super::{Ctx, Out};
use crate::object::Object;
use crate::property::{StructValue, Value};
use crate::reader::Guid;

/// View models of a view: context id -> name.
pub type ViewModels = Vec<(Guid, String)>;

fn enum_short(v: Option<&Value>, default: &str) -> String {
    v.and_then(|v| v.as_str()).map(|s| s.rsplit("::").next().unwrap_or(s).to_string()).unwrap_or_else(|| default.into())
}

/// FMVVMBlueprintPropertyPath as `ViewModel.Member.Member`, `Widget.Member` or `Member` (self).
pub fn path_text(vms: &ViewModels, v: Option<&Value>) -> String {
    let Some(p) = v.and_then(|v| v.as_struct()) else { return "?".into() };
    let mut parts = vec![];
    match enum_short(p.get("Source"), "None").as_str() {
        "ViewModel" => {
            let id = match p.get("ContextId") {
                Some(Value::Guid(g)) => Some(*g),
                _ => None,
            };
            parts.push(vms.iter().find(|(g, _)| Some(*g) == id).map(|(_, n)| n.clone()).unwrap_or_else(|| "?ViewModel".into()));
        }
        "Widget" => parts.push(p.get("WidgetName").and_then(|v| v.as_str()).unwrap_or("?").to_string()),
        _ => {}
    }
    for f in p.get("Paths").and_then(|v| v.as_array()).unwrap_or(&[]) {
        let member = f.as_struct().and_then(|f| f.get("BindingReference")).and_then(|r| r.as_struct()).and_then(|r| r.get("MemberName"));
        parts.push(member.and_then(|m| m.as_str()).unwrap_or("?").to_string());
    }
    if parts.is_empty() { "?".into() } else { parts.join(".") }
}

fn has_path(v: Option<&Value>) -> bool {
    v.and_then(|v| v.as_struct()).and_then(|p| p.get("Paths")).and_then(|v| v.as_array()).is_some_and(|a| !a.is_empty())
}

/// UMVVMBlueprintViewConversionFunction as a call: `Function(Pin: value, ...)`.
fn conversion_text(c: &Ctx, vms: &ViewModels, conv: &Object) -> String {
    let func = conv.prop("ConversionFunction").and_then(|v| v.as_struct()).and_then(|f| f.get("FunctionReference")).and_then(|r| r.as_struct());
    let name = func.and_then(|f| f.get("MemberName")).and_then(|v| v.as_str()).unwrap_or("?");
    let mut args = vec![];
    for pin in conv.prop("SavedPins").and_then(|v| v.as_array()).unwrap_or(&[]) {
        let Some(pin) = pin.as_struct() else { continue };
        let names: Vec<&str> = pin
            .get("Id")
            .and_then(|v| v.as_struct())
            .and_then(|i| i.get("PinNames"))
            .and_then(|v| v.as_array())
            .unwrap_or(&[])
            .iter()
            .filter_map(|n| n.as_str())
            .collect();
        let value = if has_path(pin.get("Path")) {
            path_text(vms, pin.get("Path"))
        } else if let Some(o) = pin.get("DefaultObject").and_then(|v| v.as_object()).filter(|&o| o != 0) {
            c.obj_ref(o)
        } else if let Some(t) = pin.get("DefaultText").and_then(|v| if let Value::Text(t) = v { Some(t.display()) } else { None }).filter(|t| !t.is_empty()) {
            c.quote(&t)
        } else {
            pin.get("DefaultString").and_then(|v| v.as_str()).unwrap_or("").to_string()
        };
        args.push(format!("{}: {value}", names.join(".")));
    }
    format!("{name}({})", args.join(", "))
}

/// One FMVVMBlueprintViewBinding: `Target <- Value [Mode]`.
pub fn binding_line(c: &Ctx, vms: &ViewModels, b: &StructValue) -> String {
    let mode = enum_short(b.get("BindingType"), "OneWayToDestination");
    let conversion = |k: &str| -> Option<String> {
        let conv = b.get("Conversion")?.as_struct()?.get(k)?.as_object()?;
        Some(conversion_text(c, vms, c.obj(conv)?))
    };
    let (src, dst) = (path_text(vms, b.get("SourcePath")), path_text(vms, b.get("DestinationPath")));
    let forward = conversion("SourceToDestinationConversion").unwrap_or_else(|| src.clone());
    let backward = conversion("DestinationToSourceConversion").unwrap_or_else(|| dst.clone());
    let mut line = match mode.as_str() {
        "TwoWay" if forward == src && backward == dst => format!("{dst} <-> {src}"),
        "TwoWay" => format!("{dst} <- {forward}, {src} <- {backward}"),
        m if m.ends_with("ToSource") => format!("{src} <- {backward}"),
        _ => format!("{dst} <- {forward}"),
    };
    let mut flags = vec![mode];
    if b.get("bOverrideExecutionMode").and_then(|v| v.as_bool()) == Some(true) {
        flags.push(enum_short(b.get("OverrideExecutionMode"), "Immediate"));
    }
    if b.get("bEnabled").and_then(|v| v.as_bool()) == Some(false) {
        flags.push("disabled".into());
    }
    if b.get("bCompile").and_then(|v| v.as_bool()) == Some(false) {
        flags.push("not compiled".into());
    }
    line.push_str(&format!(" [{}]", flags.join(", ")));
    line
}

/// Prints the view models and bindings of the blueprint's MVVM extensions. Returns true when every blueprint
/// extension was rendered.
pub fn write_view(c: &mut Ctx, out: &mut Out, bp: &Object) -> bool {
    let exts: Vec<i32> = bp.prop("Extensions").and_then(|v| v.as_array()).unwrap_or(&[]).iter().filter_map(|v| v.as_object()).collect();
    let mut all = true;
    for ext in exts {
        let Some(view) = c.obj(ext).filter(|e| e.class == "MVVMWidgetBlueprintExtension_View").and_then(|e| e.prop("BlueprintView")).and_then(|v| v.as_object()).and_then(|v| c.obj(v)) else {
            all = false;
            continue;
        };
        c.cover_subtree(ext);
        let mut vms = ViewModels::new();
        let mut lines = vec![];
        for vm in view.prop("AvailableViewModels").and_then(|v| v.as_array()).unwrap_or(&[]) {
            let Some(vm) = vm.as_struct() else { continue };
            let name = vm.get("ViewModelName").and_then(|v| v.as_str()).unwrap_or("?").to_string();
            if let Some(Value::Guid(g)) = vm.get("ViewModelContextId") {
                vms.push((*g, name.clone()));
            }
            let class = vm.get("NotifyFieldValueClass").and_then(|v| v.as_object()).map(|o| c.name(o)).unwrap_or_default();
            let creation = enum_short(vm.get("CreationType"), "Manual");
            lines.push(format!("viewmodel {name}: {class} ({creation})"));
        }
        let bindings = view.prop("Bindings").and_then(|v| v.as_array()).unwrap_or(&[]);
        for b in bindings.iter().filter_map(|b| b.as_struct()) {
            lines.push(binding_line(c, &vms, b));
            for k in ["SourceToDestinationConversion", "DestinationToSourceConversion"] {
                if let Some(o) = b.get("Conversion").and_then(|v| v.as_struct()).and_then(|v| v.get(k)).and_then(|v| v.as_object()) {
                    c.cover_subtree(o);
                }
            }
        }
        if !lines.is_empty() {
            out.blank();
            out.line(0, format!("### View bindings ({})", bindings.len()));
            for l in lines {
                out.line(0, l);
            }
        }
    }
    // The generated class carries the compiled view.
    let compiled: Vec<i32> = c.objs.iter().filter(|o| o.class == "MVVMViewClass").map(|o| o.index).collect();
    for o in compiled {
        c.cover_subtree(o);
    }
    all
}
