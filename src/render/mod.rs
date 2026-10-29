//! LLM friendly text rendering.

pub mod blueprint;
pub mod json;
pub mod material;

use std::collections::HashSet;
use std::fmt::Write as _;

use crate::object::{Extra, FieldInfo, Object};
use crate::package::Package;
use crate::property::{Property, Value};
use crate::reader::hex;

#[derive(Clone, Debug)]
pub struct Options {
    /// Show everything: pin types, positions, all exports, undecoded bytes.
    pub verbose: bool,
    /// Print leftover exports not covered by specialised views.
    pub show_other_objects: bool,
    /// Print import table.
    pub show_imports: bool,
    /// Print export table.
    pub show_exports: bool,
    /// Strings longer than this are shortened (0 = unlimited).
    pub max_string: usize,
    /// Only print the header/summary.
    pub summary_only: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self { verbose: false, show_other_objects: true, show_imports: false, show_exports: false, max_string: 2000, summary_only: false }
    }
}

/// Output accumulator with indentation helpers.
pub struct Out {
    pub s: String,
}

impl Out {
    pub fn new() -> Self {
        Self { s: String::new() }
    }
    pub fn line(&mut self, indent: usize, text: impl AsRef<str>) {
        for _ in 0..indent {
            self.s.push_str("  ");
        }
        self.s.push_str(text.as_ref());
        self.s.push('\n');
    }
    pub fn blank(&mut self) {
        if !self.s.ends_with("\n\n") && !self.s.is_empty() {
            self.s.push('\n');
        }
    }
}

impl Default for Out {
    fn default() -> Self {
        Self::new()
    }
}

/// Shared rendering context.
pub struct Ctx<'a> {
    pub pkg: &'a Package,
    pub objs: &'a [Object],
    pub opts: &'a Options,
    /// Exports already rendered by a specialised view (1-based indices).
    pub covered: HashSet<i32>,
}

impl<'a> Ctx<'a> {
    pub fn obj(&self, index: i32) -> Option<&'a Object> {
        if index > 0 { self.objs.get(index as usize - 1) } else { None }
    }

    pub fn name(&self, index: i32) -> String {
        self.pkg.obj_name(index)
    }

    pub fn cover(&mut self, index: i32) {
        if index > 0 {
            self.covered.insert(index);
        }
    }

    /// Marks an export and everything nested inside it as rendered.
    pub fn cover_subtree(&mut self, root: i32) {
        if root <= 0 {
            return;
        }
        self.cover(root);
        for i in 0..self.pkg.exports.len() {
            let mut cur = self.pkg.exports[i].outer_index;
            let mut guard = 0;
            while cur > 0 && guard < 64 {
                if cur == root {
                    self.covered.insert(i as i32 + 1);
                    break;
                }
                cur = self.pkg.exports[cur as usize - 1].outer_index;
                guard += 1;
            }
        }
    }

    /// Reference text for an object index.
    pub fn obj_ref(&self, index: i32) -> String {
        if index == 0 {
            return "None".into();
        }
        self.pkg.ref_string(index)
    }

    pub fn quote(&self, s: &str) -> String {
        let s = if self.opts.max_string > 0 && s.chars().count() > self.opts.max_string {
            let cut: String = s.chars().take(self.opts.max_string).collect();
            format!("{cut}...<{} chars>", s.chars().count())
        } else {
            s.to_string()
        };
        json::quote(&s)
    }

    pub fn name_str(&self, s: &str) -> String {
        if s.is_empty() || s.chars().any(|c| c.is_whitespace() || matches!(c, ',' | '{' | '}' | '[' | ']' | '"' | '=' | ':' | '#'))
        {
            self.quote(s)
        } else {
            s.to_string()
        }
    }

    pub fn float(v: f64) -> String {
        if v.is_finite() && v == v.trunc() && v.abs() < 1e15 {
            return format!("{v:.1}");
        }
        // Values that were stored as float (or converted from float) print at float precision.
        let f = v as f32;
        if f as f64 == v { format!("{f}") } else { format!("{v}") }
    }

    /// Single-line representation if it fits in `budget` chars.
    pub fn inline(&self, v: &Value, budget: usize) -> Option<String> {
        let s = match v {
            Value::Bool(b) => b.to_string(),
            Value::Int(i) => i.to_string(),
            Value::UInt(i) => i.to_string(),
            Value::Float(f) => Self::float(*f as f64),
            Value::Double(f) => Self::float(*f),
            Value::Str(s) => self.quote(s),
            Value::Name(s) => self.name_str(s),
            Value::Text(t) => {
                let mut s = self.quote(&t.display());
                if self.opts.verbose {
                    if let Some((ns, k)) = t.key() {
                        let _ = write!(s, " [{ns}:{k}]");
                    }
                }
                s
            }
            Value::Enum(s) => s.clone(),
            Value::Object(i) => self.obj_ref(*i),
            Value::SoftObject(s) => if s.is_empty() { "None".into() } else { s.clone() },
            Value::Guid(g) => g.to_string(),
            Value::Delegate { object, function } => {
                if *object == 0 { function.clone() } else { format!("{}.{}", self.obj_ref(*object), function) }
            }
            Value::MulticastDelegate(v) => {
                let parts: Vec<String> = v.iter().map(|(o, f)| format!("{}.{}", self.obj_ref(*o), f)).collect();
                format!("[{}]", parts.join(", "))
            }
            Value::FieldPath { path, owner } => {
                if *owner == 0 { path.join(".") } else { format!("{}:{}", self.obj_ref(*owner), path.join(".")) }
            }
            Value::Optional(None) => "unset".into(),
            Value::Optional(Some(b)) => self.inline(b, budget)?,
            Value::Raw { type_name, bytes, note } => {
                if self.opts.verbose && bytes.len() <= 64 {
                    format!("<undecoded {type_name}, {} bytes: {note}: {}>", bytes.len(), hex(bytes))
                } else {
                    format!("<undecoded {type_name}, {} bytes: {note}>", bytes.len())
                }
            }
            Value::Struct(sv) => {
                if sv.fields.is_empty() {
                    return Some("{}".into());
                }
                if let (Some(p), Some(a), 2) = (sv.get("PackageName"), sv.get("AssetName"), sv.fields.len()) {
                    // FTopLevelAssetPath
                    let (p, a) = (p.as_str().unwrap_or(""), a.as_str().unwrap_or(""));
                    return Some(if a.is_empty() || a == "None" { p.to_string() } else { format!("{p}.{a}") });
                }
                let mut parts = vec![];
                let mut len = 0;
                for f in &sv.fields {
                    let fv = self.inline(&f.value, budget.saturating_sub(len))?;
                    let p = if sv.native { format!("{}={}", f.display_name(), fv) } else { format!("{}: {}", f.display_name(), fv) };
                    len += p.len() + 2;
                    if len > budget {
                        return None;
                    }
                    parts.push(p);
                }
                if sv.native { format!("({})", parts.join(", ")) } else { format!("{{{}}}", parts.join(", ")) }
            }
            Value::Array(a) | Value::Set(a) => {
                let mut parts = vec![];
                let mut len = 2;
                for x in a {
                    let xs = self.inline(x, budget.saturating_sub(len))?;
                    len += xs.len() + 2;
                    if len > budget {
                        return None;
                    }
                    parts.push(xs);
                }
                format!("[{}]", parts.join(", "))
            }
            Value::Map(m) => {
                let mut parts = vec![];
                let mut len = 2;
                for (k, x) in m {
                    let ks = self.inline(k, budget.saturating_sub(len))?;
                    let xs = self.inline(x, budget.saturating_sub(len))?;
                    len += ks.len() + xs.len() + 4;
                    if len > budget {
                        return None;
                    }
                    parts.push(format!("{ks}: {xs}"));
                }
                format!("{{{}}}", parts.join(", "))
            }
        };
        if s.len() > budget && !matches!(v, Value::Str(_) | Value::Text(_) | Value::Name(_) | Value::SoftObject(_) | Value::Object(_)) {
            return None;
        }
        Some(s)
    }

    /// Writes `key: value`, expanding into a block when it does not fit on one line.
    pub fn write_kv(&self, out: &mut Out, indent: usize, key: &str, v: &Value) {
        let budget = 110usize.saturating_sub(indent * 2 + key.len());
        if let Some(s) = self.inline(v, budget.max(30)) {
            out.line(indent, format!("{key}: {s}"));
            return;
        }
        match v {
            Value::Struct(sv) => {
                out.line(indent, format!("{key}:"));
                for f in &sv.fields {
                    self.write_kv(out, indent + 1, &f.display_name(), &f.value);
                }
            }
            Value::Array(a) | Value::Set(a) => {
                out.line(indent, format!("{key}: ({} items)", a.len()));
                for x in a {
                    self.write_item(out, indent + 1, x);
                }
            }
            Value::Map(m) => {
                out.line(indent, format!("{key}: ({} entries)", m.len()));
                for (k, x) in m {
                    let ks = self.inline(k, 80).unwrap_or_else(|| "<key>".into());
                    self.write_kv(out, indent + 1, &format!("[{ks}]"), x);
                }
            }
            Value::Optional(Some(b)) => self.write_kv(out, indent, key, b),
            _ => out.line(indent, format!("{key}: {}", self.inline(v, usize::MAX).unwrap_or_default())),
        }
    }

    fn write_item(&self, out: &mut Out, indent: usize, v: &Value) {
        let budget = 110usize.saturating_sub(indent * 2 + 2);
        if let Some(s) = self.inline(v, budget.max(30)) {
            out.line(indent, format!("- {s}"));
            return;
        }
        match v {
            Value::Struct(sv) => {
                let mut first = true;
                for f in &sv.fields {
                    let mut tmp = Out::new();
                    self.write_kv(&mut tmp, 0, &f.display_name(), &f.value);
                    for (i, l) in tmp.s.lines().enumerate() {
                        if first && i == 0 {
                            out.line(indent, format!("- {l}"));
                        } else {
                            out.line(indent + 1, l);
                        }
                    }
                    first = false;
                }
                if sv.fields.is_empty() {
                    out.line(indent, "- {}");
                }
            }
            _ => {
                let mut tmp = Out::new();
                self.write_kv(&mut tmp, 0, "-", v);
                for l in tmp.s.lines() {
                    out.line(indent, l.replacen("-:", "-", 1));
                }
            }
        }
    }

    pub fn write_props(&self, out: &mut Out, indent: usize, props: &[Property]) {
        for p in props {
            self.write_kv(out, indent, &p.display_name(), &p.value);
        }
    }

    pub fn write_props_filtered(&self, out: &mut Out, indent: usize, props: &[Property], skip: &[&str]) {
        for p in props {
            if skip.contains(&p.name.as_str()) {
                continue;
            }
            self.write_kv(out, indent, &p.display_name(), &p.value);
        }
    }

    pub fn write_object(&mut self, out: &mut Out, indent: usize, o: &Object, title: Option<String>) {
        self.cover(o.index);
        let title = title.unwrap_or_else(|| format!("{} ({})", self.pkg.export_rel_path(o.index), o.class));
        out.line(indent, title);
        self.write_props(out, indent + 1, &o.properties);
        self.write_extras(out, indent + 1, o);
        for e in &o.errors {
            out.line(indent + 1, format!("!! decode error: {e}"));
        }
        if o.undecoded > 0 && self.opts.verbose {
            out.line(indent + 1, format!("(+{} bytes of native data not decoded: {}...)", o.undecoded, o.undecoded_preview));
        }
    }

    pub fn write_extras(&self, out: &mut Out, indent: usize, o: &Object) {
        for e in &o.extras {
            match e {
                Extra::DataTable(_) => {}
                Extra::Pins(pins) if self.opts.verbose => {
                    out.line(indent, format!("Pins: ({})", pins.len()));
                    for p in pins {
                        out.line(indent + 1, format!("{} {}: {}", if p.is_output() { "out" } else { "in" }, p.name, p.pin_type.display(self.pkg)));
                    }
                }
                Extra::Pins(_) => {}
                Extra::MetaData(md) => {
                    for (k, v) in &md.root {
                        out.line(indent, format!("root meta {k} = {}", self.quote(v)));
                    }
                    for (obj, kv) in &md.objects {
                        let parts: Vec<String> = kv.iter().map(|(k, v)| format!("{k}={}", self.quote(v))).collect();
                        out.line(indent, format!("meta {obj}: {}", parts.join(", ")));
                    }
                }
                Extra::Struct(s) => {
                    if s.super_struct != 0 {
                        out.line(indent, format!("Super: {}", self.obj_ref(s.super_struct)));
                    }
                    if let Some(f) = s.function_flags {
                        out.line(indent, format!("FunctionFlags: {}", function_flags(f)));
                    }
                    if !s.fields.is_empty() {
                        out.line(indent, "Fields:");
                        for f in &s.fields {
                            out.line(indent + 1, format!("{}: {}{}", f.name, self.field_type(f), field_flags_suffix(f.flags)));
                        }
                    }
                    if !s.defaults.is_empty() {
                        out.line(indent, "Defaults:");
                        self.write_props(out, indent + 1, &s.defaults);
                    }
                    if let Some(c) = &s.class {
                        if !c.interfaces.is_empty() {
                            let v: Vec<String> = c.interfaces.iter().map(|(i, _)| self.obj_ref(*i)).collect();
                            out.line(indent, format!("Interfaces: {}", v.join(", ")));
                        }
                    }
                }
                Extra::Enum(names) => {
                    out.line(indent, "Values:");
                    for (n, v) in names {
                        out.line(indent + 1, format!("{n} = {v}"));
                    }
                }
                Extra::Redirector(i) => out.line(indent, format!("DestinationObject: {}", self.obj_ref(*i))),
                Extra::UserPins(pins) => {
                    for p in pins {
                        let dir = if p.direction == 1 { "out" } else { "in" };
                        let def = if p.default_value.is_empty() { String::new() } else { format!(" = {}", p.default_value) };
                        out.line(indent, format!("UserPin {dir} {}: {}{def}", p.name, p.pin_type.display(self.pkg)));
                    }
                }
                Extra::PureState(_) => {}
            }
        }
    }

    /// Type of an FField, e.g. `Array<Vector>`.
    pub fn field_type(&self, f: &FieldInfo) -> String {
        let obj = |i: i32| if i == 0 { "?".to_string() } else { self.name(i) };
        let base = f.kind.strip_suffix("Property").unwrap_or(&f.kind);
        match f.kind.as_str() {
            "StructProperty" | "EnumProperty" => obj(f.object),
            "ByteProperty" if f.object != 0 => obj(f.object),
            "ObjectProperty" | "ObjectPtrProperty" => obj(f.object),
            "WeakObjectProperty" => format!("weak:{}", obj(f.object)),
            "LazyObjectProperty" => format!("lazy:{}", obj(f.object)),
            "SoftObjectProperty" => format!("soft:{}", obj(f.object)),
            "ClassProperty" | "ClassPtrProperty" => format!("class:{}", obj(f.object2)),
            "SoftClassProperty" => format!("softclass:{}", obj(f.object2)),
            "InterfaceProperty" => format!("interface:{}", obj(f.object)),
            "ArrayProperty" | "SetProperty" | "OptionalProperty" => {
                format!("{}<{}>", base, f.inner.first().map(|i| self.field_type(i)).unwrap_or("?".into()))
            }
            "MapProperty" => format!(
                "Map<{}, {}>",
                f.inner.first().map(|i| self.field_type(i)).unwrap_or("?".into()),
                f.inner.get(1).map(|i| self.field_type(i)).unwrap_or("?".into())
            ),
            "DelegateProperty" | "MulticastDelegateProperty" | "MulticastInlineDelegateProperty" | "MulticastSparseDelegateProperty" => {
                format!("{}({})", base, obj(f.object))
            }
            "IntProperty" => "int32".into(),
            "FloatProperty" => "float".into(),
            "DoubleProperty" => "double".into(),
            "StrProperty" => "string".into(),
            other => other.strip_suffix("Property").unwrap_or(other).to_lowercase(),
        }
    }
}

pub fn function_flags(f: u32) -> String {
    const NAMES: &[(u32, &str)] = &[
        (0x1, "Final"), (0x4, "BlueprintAuthorityOnly"), (0x8, "BlueprintCosmetic"), (0x40, "Net"), (0x80, "NetReliable"),
        (0x200, "Exec"), (0x400, "Native"), (0x800, "Event"), (0x2000, "Static"), (0x4000, "NetMulticast"),
        (0x8000, "UbergraphFunction"), (0x10000, "MulticastDelegate"), (0x20000, "Public"), (0x40000, "Private"),
        (0x80000, "Protected"), (0x100000, "Delegate"), (0x200000, "NetServer"), (0x400000, "HasOutParms"),
        (0x1000000, "NetClient"), (0x4000000, "BlueprintCallable"), (0x8000000, "BlueprintEvent"),
        (0x10000000, "BlueprintPure"), (0x20000000, "EditorOnly"), (0x40000000, "Const"),
    ];
    let v: Vec<&str> = NAMES.iter().filter(|(b, _)| f & b != 0).map(|(_, n)| *n).collect();
    v.join("|")
}

pub fn property_flags(f: u64) -> Vec<&'static str> {
    const NAMES: &[(u64, &str)] = &[
        (0x1, "Edit"), (0x4, "BlueprintVisible"), (0x10, "BlueprintReadOnly"), (0x20, "Replicated"), (0x80, "Parm"),
        (0x100, "OutParm"), (0x400, "ReturnParm"), (0x800, "DisableEditOnTemplate"), (0x2000, "Transient"),
        (0x4000, "Config"), (0x10000, "DisableEditOnInstance"), (0x20000, "EditConst"), (0x80000, "Instanced"),
        (0x1000000, "SaveGame"), (0x8000000, "Ref"), (0x10000000, "BlueprintAssignable"), (0x20000000, "Deprecated"),
        (0x100000000, "RepNotify"), (0x200000000, "Interp"), (0x40000000000, "AdvancedDisplay"),
        (0x80000000000, "Protected"), (0x100000000000, "BlueprintCallable"), (0x1000000000000, "ExposeOnSpawn"),
    ];
    NAMES.iter().filter(|(b, _)| f & b != 0).map(|(_, n)| *n).collect()
}

fn field_flags_suffix(f: u64) -> String {
    let interesting: Vec<&str> = property_flags(f)
        .into_iter()
        .filter(|n| matches!(*n, "Parm" | "OutParm" | "ReturnParm" | "Ref" | "Replicated" | "RepNotify" | "Config" | "SaveGame" | "ExposeOnSpawn"))
        .collect();
    if interesting.is_empty() { String::new() } else { format!("  [{}]", interesting.join(", ")) }
}

/// Main asset export (the one named after the package, or flagged as asset).
pub fn main_export(pkg: &Package, objs: &[Object]) -> Option<usize> {
    let pkg_name = pkg.package_path().rsplit('/').next().unwrap_or("").to_string();
    let is_top = |i: usize| pkg.exports[i].outer_index == 0;
    let not_cdo = |i: usize| pkg.exports[i].object_flags & crate::package::RF_CLASS_DEFAULT_OBJECT == 0;
    (0..objs.len())
        .find(|&i| is_top(i) && pkg.exports[i].is_asset && not_cdo(i) && !objs[i].class.ends_with("GeneratedClass"))
        .or_else(|| (0..objs.len()).find(|&i| is_top(i) && pkg.exports[i].object_name == pkg_name && !objs[i].class.ends_with("GeneratedClass")))
        .or_else(|| (0..objs.len()).find(|&i| is_top(i) && not_cdo(i) && objs[i].class != "MetaData" && !objs[i].class.ends_with("GeneratedClass")))
        .or_else(|| (0..objs.len()).find(|&i| is_top(i)))
}

fn write_header(c: &Ctx, out: &mut Out) {
    let pkg = c.pkg;
    let s = &pkg.summary;
    let main = main_export(pkg, c.objs);
    let (name, class) = match main {
        Some(i) => (pkg.exports[i].object_name.clone(), c.objs[i].class.clone()),
        None => ("?".into(), "?".into()),
    };
    out.line(0, format!("# {name} ({class})"));
    let path = pkg.package_path();
    if !path.is_empty() {
        let guessed = pkg.path_hint.as_deref() == Some(path.as_str());
        out.line(0, format!("Package: {path}{}", if guessed { " (guessed from file location)" } else { "" }));
    }
    let ver = if s.unversioned {
        "unversioned".to_string()
    } else {
        format!("{} (UE4 ver {}, UE5 ver {})", s.saved_by, s.file_version_ue4, s.file_version_ue5)
    };
    out.line(0, format!("Saved by: {ver}"));
    let mut flags = vec![];
    if s.package_flags & crate::package::PKG_COOKED != 0 {
        flags.push("cooked");
    }
    if pkg.uses_unversioned_properties() {
        flags.push("unversioned-properties");
    }
    if s.package_flags & 0x0002_0000 != 0 {
        flags.push("contains-map");
    }
    if s.package_flags & 0x0004_0000 != 0 {
        // PKG_RequiresLocalizationGather
    }
    out.line(
        0,
        format!(
            "Exports: {}, Imports: {}{}",
            pkg.exports.len(),
            pkg.imports.len(),
            if flags.is_empty() { String::new() } else { format!(", Flags: {}", flags.join(", ")) }
        ),
    );
    for w in &pkg.warnings {
        out.line(0, format!("!! warning: {w}"));
    }
}

fn write_registry(c: &Ctx, out: &mut Out) {
    const NOISY: &[&str] = &["LinterCoreResult", "LinterCoreAssetDependencies", "AssetImportData", "FiBData", "ClassFlags", "NumReplicatedProperties", "ImportedNamespaces"];
    for e in &c.pkg.asset_registry {
        let tags: Vec<&(String, String)> = e
            .tags
            .iter()
            .filter(|(k, v)| c.opts.verbose || (!NOISY.contains(&k.as_str()) && v.len() < 400))
            .collect();
        if tags.is_empty() {
            continue;
        }
        out.blank();
        out.line(0, format!("## Asset registry: {} ({})", e.object_path, e.object_class));
        for (k, v) in tags {
            out.line(0, format!("{k}: {v}"));
        }
    }
}

fn write_dependencies(c: &Ctx, out: &mut Out) {
    // Asset dependencies: imported packages that are not script packages.
    let mut deps: Vec<String> = c
        .pkg
        .imports
        .iter()
        .filter(|i| i.outer_index == 0 && i.class_name == "Package" && !i.object_name.starts_with("/Script/"))
        .map(|i| i.object_name.clone())
        .collect();
    deps.sort();
    deps.dedup();
    let mut soft: Vec<String> = c.pkg.soft_package_references.iter().filter(|p| !deps.contains(p) && **p != c.pkg.package_path()).cloned().collect();
    soft.sort();
    soft.dedup();
    if deps.is_empty() && soft.is_empty() {
        return;
    }
    out.blank();
    out.line(0, "## Dependencies");
    for d in deps {
        out.line(0, format!("- {d}"));
    }
    for d in soft {
        out.line(0, format!("- {d} (soft)"));
    }
}

fn write_import_table(c: &Ctx, out: &mut Out) {
    out.blank();
    out.line(0, format!("## Imports ({})", c.pkg.imports.len()));
    for (i, imp) in c.pkg.imports.iter().enumerate() {
        out.line(0, format!("{:>4}  {} ({})", -(i as i32) - 1, c.pkg.obj_path(-(i as i32) - 1), imp.class_name));
    }
}

fn write_export_table(c: &Ctx, out: &mut Out) {
    out.blank();
    out.line(0, format!("## Exports ({})", c.pkg.exports.len()));
    for (i, e) in c.pkg.exports.iter().enumerate() {
        let idx = i as i32 + 1;
        out.line(0, format!("{:>4}  {} ({}) {} bytes", idx, c.pkg.export_rel_path(idx), c.objs[i].class, e.serial_size));
    }
}

fn write_package_metadata(c: &Ctx, out: &mut Out) {
    let Some(md) = &c.pkg.metadata else { return };
    let objects: Vec<_> = md.objects.iter().filter(|(_, kv)| c.opts.verbose || kv.iter().any(|(k, _)| k != "DefaultGraphNode")).collect();
    if objects.is_empty() {
        return;
    }
    out.blank();
    out.line(0, "## Metadata");
    for (obj, kv) in objects {
        let parts: Vec<String> = kv.iter().map(|(k, v)| format!("{k}={}", c.quote(v))).collect();
        out.line(0, format!("{obj}: {}", parts.join(", ")));
    }
}

fn write_data_table(c: &mut Ctx, out: &mut Out, o: &Object) {
    let rows = o.extras.iter().find_map(|e| if let Extra::DataTable(r) = e { Some(r) } else { None });
    out.blank();
    let row_struct = o.prop("RowStruct").map(|v| c.inline(v, 200).unwrap_or_default()).unwrap_or("?".into());
    out.line(0, format!("## DataTable {} (RowStruct: {row_struct})", c.pkg.exports[o.index as usize - 1].object_name));
    c.cover(o.index);
    c.write_props_filtered(out, 0, &o.properties, &["RowStruct", "RowStructPathName", "RowStructName"]);
    let Some(rows) = rows else { return };
    out.line(0, format!("Rows ({}):", rows.len()));
    for (name, props) in rows {
        let sv = Value::Struct(Box::new(crate::property::StructValue { type_name: String::new(), fields: props.clone(), native: false }));
        c.write_kv(out, 0, &format!("- {}", c.name_str(name)), &sv);
    }
}

fn write_user_enum(c: &mut Ctx, out: &mut Out, o: &Object) {
    c.cover(o.index);
    out.blank();
    out.line(0, format!("## Enum {}", c.pkg.exports[o.index as usize - 1].object_name));
    // Display names live in the DisplayNameMap property.
    let mut display: Vec<(String, String)> = vec![];
    if let Some(Value::Map(m)) = o.prop("DisplayNameMap") {
        for (k, v) in m {
            if let (Some(k), Value::Text(t)) = (k.as_str(), v) {
                display.push((k.to_string(), t.display()));
            }
        }
    }
    if let Some(Extra::Enum(names)) = o.extras.iter().find(|e| matches!(e, Extra::Enum(_))) {
        for (n, v) in names {
            let short = n.rsplit("::").next().unwrap_or(n);
            let d = display.iter().find(|(k, _)| k == short || k == n).map(|(_, d)| format!(" \"{d}\"")).unwrap_or_default();
            out.line(0, format!("- {n} = {v}{d}"));
        }
    }
    c.write_props_filtered(out, 0, &o.properties, &["DisplayNameMap"]);
}

fn write_user_struct(c: &mut Ctx, out: &mut Out, o: &Object) {
    c.cover(o.index);
    out.blank();
    out.line(0, format!("## Struct {}", c.pkg.exports[o.index as usize - 1].object_name));
    let Some(si) = o.struct_info() else { return };
    // Friendly names and tooltips are stored in the editor data object.
    let mut tooltips: Vec<(String, String)> = vec![];
    if let Some(Value::Object(ed)) = o.prop("EditorData") {
        if let Some(edo) = c.obj(*ed) {
            c.cover(edo.index);
            if let Some(Value::Array(vars)) = edo.prop("VariablesDescriptions") {
                for v in vars {
                    if let Some(s) = v.as_struct() {
                        let name = s.get("VarName").and_then(|x| x.as_str()).unwrap_or("").to_string();
                        let fname = s.get("FriendlyName").and_then(|x| x.as_str()).unwrap_or("").to_string();
                        let tip = s.get("ToolTip").and_then(|x| x.as_str()).unwrap_or("").to_string();
                        tooltips.push((crate::property::clean_field_name(&name), if tip.is_empty() { fname } else { format!("{fname}: {tip}") }));
                    }
                }
            }
        }
    }
    out.line(0, "Fields:");
    for f in &si.fields {
        let name = crate::property::clean_field_name(&f.name);
        let def = si.defaults.iter().find(|p| p.name == name).and_then(|p| c.inline(&p.value, 100)).map(|s| format!(" = {s}")).unwrap_or_default();
        let tip = tooltips.iter().find(|(n, _)| *n == name).map(|(_, t)| t.clone()).filter(|t| !t.is_empty() && *t != name).map(|t| format!("  // {t}")).unwrap_or_default();
        out.line(1, format!("{name}: {}{def}{tip}", c.field_type(f)));
    }
    c.write_props_filtered(out, 0, &o.properties, &["EditorData", "Guid", "Status", "ErrorMessage", "PrimaryStruct"]);
}

/// Renders a whole package.
pub fn render(pkg: &Package, objs: &[Object], opts: &Options) -> String {
    let mut c = Ctx { pkg, objs, opts, covered: HashSet::new() };
    let mut out = Out::new();
    write_header(&c, &mut out);
    if opts.summary_only {
        write_registry(&c, &mut out);
        write_dependencies(&c, &mut out);
        return out.s;
    }
    if pkg.uses_unversioned_properties() {
        out.line(0, "!! This package uses unversioned (cooked) property serialization; property values cannot be decoded without engine type information.");
    }
    if opts.verbose {
        write_registry(&c, &mut out);
    }
    write_dependencies(&c, &mut out);
    if opts.show_imports {
        write_import_table(&c, &mut out);
    }
    if opts.show_exports {
        write_export_table(&c, &mut out);
    }

    // Specialised views
    let main = main_export(pkg, objs);
    let mut handled = false;
    if let Some(m) = main {
        let o = &objs[m];
        let class = o.class.as_str();
        if blueprint::is_blueprint(o) {
            blueprint::render(&mut c, &mut out, o);
            handled = true;
        } else if material::is_material_like(class) {
            material::render(&mut c, &mut out, o);
            handled = true;
        } else if o.extras.iter().any(|e| matches!(e, Extra::DataTable(_))) {
            write_data_table(&mut c, &mut out, o);
            handled = true;
        } else if class == "UserDefinedEnum" {
            write_user_enum(&mut c, &mut out, o);
            handled = true;
        } else if class == "UserDefinedStruct" {
            write_user_struct(&mut c, &mut out, o);
            handled = true;
        }
        if !handled {
            out.blank();
            out.line(0, format!("## {} ({})", pkg.exports[m].object_name, o.class));
            c.cover(o.index);
            c.write_props(&mut out, 0, &o.properties);
            c.write_extras(&mut out, 0, o);
            for e in &o.errors {
                out.line(0, format!("!! decode error: {e}"));
            }
        }
    }

    // Leftover exports
    if opts.show_other_objects {
        let skip_classes = ["MetaData", "DEPRECATED_MetaData"];
        let rest: Vec<&Object> = objs
            .iter()
            .filter(|o| !c.covered.contains(&o.index))
            .filter(|o| opts.verbose || !skip_classes.contains(&o.class.as_str()))
            .filter(|o| opts.verbose || !is_boring(pkg, o))
            .collect();
        if !rest.is_empty() {
            out.blank();
            out.line(0, "## Other objects");
            for o in rest {
                c.write_object(&mut out, 0, o, None);
            }
        }
    }
    if opts.verbose {
        write_package_metadata(&c, &mut out);
    }
    out.s
}

/// Objects without any interesting content.
fn is_boring(pkg: &Package, o: &Object) -> bool {
    let no_content = o.properties.is_empty() && o.extras.is_empty() && o.errors.is_empty();
    let thumb = o.class.ends_with("ThumbnailInfo") || o.class.ends_with("ThumbnailInfoWithPrimitive");
    let _ = pkg;
    no_content || thumb
}
