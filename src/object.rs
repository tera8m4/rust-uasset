//! Per-export deserialization: tagged properties plus class specific native data.

use crate::errors::{Result, invalid};
use crate::graph::{self, Pin};
use crate::package::{Package, PackageMetaData};
use crate::property::{PropReader, Property, TypeName, Value, ValueCtx};
use crate::reader::{Reader, hex};
use crate::versions::{custom, ue4, ue5};

#[derive(Debug, Clone)]
pub struct FieldInfo {
    pub name: String,
    /// FProperty class, e.g. "ObjectProperty"
    pub kind: String,
    pub array_dim: i32,
    pub flags: u64,
    /// Referenced object (struct / class / enum / signature function), 0 if none
    pub object: i32,
    /// Secondary object (meta class), 0 if none
    pub object2: i32,
    pub inner: Vec<FieldInfo>,
    pub metadata: Vec<(String, String)>,
}

#[derive(Debug, Clone, Default)]
pub struct StructInfo {
    pub super_struct: i32,
    pub children: Vec<i32>,
    pub fields: Vec<FieldInfo>,
    pub script_size: i32,
    /// UFunction only
    pub function_flags: Option<u32>,
    /// UClass only
    pub class: Option<ClassInfo>,
    /// UUserDefinedStruct default instance
    pub defaults: Vec<Property>,
}

#[derive(Debug, Clone, Default)]
pub struct ClassInfo {
    pub func_map: Vec<(String, i32)>,
    pub class_flags: u32,
    pub class_within: i32,
    pub config_name: String,
    pub generated_by: i32,
    pub interfaces: Vec<(i32, bool)>,
    pub cdo: i32,
}

#[derive(Debug, Clone)]
pub enum Extra {
    DataTable(Vec<(String, Vec<Property>)>),
    Pins(Vec<Pin>),
    MetaData(PackageMetaData),
    Struct(StructInfo),
    Enum(Vec<(String, i64)>),
    Redirector(i32),
    /// User defined pins of function entry/result, events and tunnels.
    UserPins(Vec<UserPin>),
    /// UK2Node_DynamicCast pure state
    PureState(u8),
}

#[derive(Debug, Clone)]
pub struct UserPin {
    pub name: String,
    pub pin_type: graph::PinType,
    /// 0 input, 1 output
    pub direction: u8,
    pub default_value: String,
}

#[derive(Debug, Clone)]
pub struct Object {
    /// 1-based package index of this export
    pub index: i32,
    pub class: String,
    pub properties: Vec<Property>,
    pub extras: Vec<Extra>,
    pub errors: Vec<String>,
    /// Bytes at the end of the export that were not decoded.
    pub undecoded: usize,
    pub undecoded_preview: String,
}

impl Object {
    pub fn prop(&self, name: &str) -> Option<&Value> {
        crate::property::find(&self.properties, name)
    }
    pub fn pins(&self) -> &[Pin] {
        for e in &self.extras {
            if let Extra::Pins(p) = e {
                return p;
            }
        }
        &[]
    }
    pub fn struct_info(&self) -> Option<&StructInfo> {
        self.extras.iter().find_map(|e| if let Extra::Struct(s) = e { Some(s) } else { None })
    }
}

fn is_struct_class(class: &str) -> bool {
    matches!(
        class,
        "Class" | "Function" | "ScriptStruct" | "UserDefinedStruct" | "DelegateFunction" | "SparseDelegateFunction"
            | "LinkerPlaceholderClass" | "LinkerPlaceholderFunction"
    ) || class.ends_with("GeneratedClass")
}

fn is_enum_class(class: &str) -> bool {
    matches!(class, "Enum" | "UserDefinedEnum")
}

pub fn read_object(pkg: &Package, export_index: usize) -> Object {
    let e = &pkg.exports[export_index];
    let index = export_index as i32 + 1;
    let class = pkg.class_name(index);
    let mut obj = Object {
        index,
        class: class.clone(),
        properties: vec![],
        extras: vec![],
        errors: vec![],
        undecoded: 0,
        undecoded_preview: String::new(),
    };
    let start = e.serial_offset as usize;
    let end = start + e.serial_size as usize;
    let mut r = pkg.reader();
    if r.seek(start).is_err() {
        obj.errors.push("export data out of range".into());
        return obj;
    }
    if pkg.uses_unversioned_properties() {
        obj.errors.push("unversioned (cooked) property serialization is not supported".into());
        obj.undecoded = end - start;
        return obj;
    }
    let pr = PropReader::new(pkg);

    // Exact class objects (UClass itself) don't serialize script properties.
    let serializes_props = class != "Class" || e.class_index != 0;
    let mut props_end = None;
    if e.script_serialization_end_offset > e.script_serialization_start_offset && e.script_serialization_start_offset >= 0 {
        let _ = r.seek(start + e.script_serialization_start_offset as usize);
        props_end = Some(start + e.script_serialization_end_offset as usize);
    }
    let mut pre_props = vec![];
    if class.ends_with("ImportData") && pkg.ue4() >= ue4::ASSET_IMPORT_DATA_AS_JSON && !pkg.is_filter_editor_only() && props_end.is_none() {
        match r.fstring() {
            Ok(json) => pre_props.push(Property::new("SourceData", Value::Str(json))),
            Err(err) => obj.errors.push(format!("import data json: {err}")),
        }
    }
    if serializes_props {
        let res = (|| -> Result<Vec<Property>> {
            if pkg.ue5() >= ue5::PROPERTY_TAG_EXTENSION_AND_OVERRIDABLE_SERIALIZATION {
                let ctl = r.u8()?;
                if ctl & 0x02 != 0 {
                    r.u8()?; // OverridableOperation
                }
            }
            pr.read_properties(&mut r, props_end.unwrap_or(end), 0)
        })();
        match res {
            Ok(p) => {
                obj.properties = pre_props;
                obj.properties.extend(p);
            }
            Err(err) => {
                obj.errors.push(format!("properties: {err}"));
                obj.undecoded = end.saturating_sub(start);
                return obj;
            }
        }
        if let Some(pe) = props_end {
            if r.pos() != pe {
                obj.errors.push(format!("property data ended at +{} but expected +{}", r.pos() - start, pe - start));
                let _ = r.seek(pe);
            }
        }
        // FLazyObjectPtr::PossiblySerializeObjectGuid
        if r.pos() + 4 <= end {
            let mut t = r.clone();
            match t.u32() {
                Ok(0) => r = t,
                Ok(1) if t.remaining() >= 16 => {
                    let _ = t.guid();
                    r = t;
                }
                _ => {}
            }
        }
    }

    let tail = (|| -> Result<()> {
        if class == "DataTable" || class == "CompositeDataTable" || class.ends_with("DataTable") {
            obj.extras.push(Extra::DataTable(read_data_table(&pr, &obj, &mut r, end)?));
        } else if class == "ObjectRedirector" {
            if r.pos() + 4 <= end {
                obj.extras.push(Extra::Redirector(r.i32()?));
            }
        } else if class == "MetaData" || class == "DEPRECATED_MetaData" {
            obj.extras.push(Extra::MetaData(read_metadata_export(pkg, &mut r)?));
        } else if is_enum_class(&class) {
            obj.extras.push(Extra::Enum(read_enum(pkg, &mut r)?));
        } else if is_struct_class(&class) {
            obj.extras.push(Extra::Struct(read_struct_info(&pr, &mut r, &class, end)?));
        } else if r.pos() < end && graph::pins_are_native(pkg) && looks_like_node(&class, &r, end, index) {
            obj.extras.push(Extra::Pins(graph::read_node_pins(pkg, &mut r, index)?));
            if class == "K2Node_DynamicCast" && r.pos() < end {
                obj.extras.push(Extra::PureState(r.u8()?));
            } else if r.pos() + 4 <= end && class.starts_with("K2Node") {
                let mut t = r.clone();
                if let Ok(pins) = read_user_pins(pkg, &mut t) {
                    r = t;
                    if !pins.is_empty() {
                        obj.extras.push(Extra::UserPins(pins));
                    }
                }
            }
        }
        Ok(())
    })();
    if let Err(err) = tail {
        obj.errors.push(format!("native data: {err}"));
    }
    // Small all-zero tails are empty native arrays/flags (e.g. UActorComponent::UCSModifiedProperties).
    if r.pos() < end && end - r.pos() <= 16 && pkg.data[r.pos()..end].iter().all(|&b| b == 0) {
        let _ = r.seek(end);
    }
    if r.pos() < end {
        obj.undecoded = end - r.pos();
        let n = obj.undecoded.min(32);
        obj.undecoded_preview = hex(&pkg.data[r.pos()..r.pos() + n]);
    }
    obj
}

/// Detects the UEdGraphNode pin array: count followed by (bNullPtr=0, OwningNode=self).
fn looks_like_node(class: &str, r: &Reader, end: usize, index: i32) -> bool {
    let named = class.contains("Node") || class.starts_with("K2") || class.contains("Graph");
    let mut t = r.clone();
    let Ok(n) = t.i32() else { return false };
    if n == 0 {
        return named && t.pos() <= end;
    }
    if !(0..10000).contains(&n) {
        return false;
    }
    matches!((t.u32(), t.i32()), (Ok(0), Ok(owner)) if owner == index)
}

fn read_user_pins(pkg: &Package, r: &mut Reader) -> Result<Vec<UserPin>> {
    let n = r.count(8)?;
    let mut v = vec![];
    for _ in 0..n {
        let name = if pkg.custom_ver(custom::framework()) >= custom::FW_PINS_STORE_FNAME { pkg.read_fname(r)? } else { r.fstring()? };
        if pkg.ue4() < ue4::SERIALIZE_PINTYPE_CONST {
            return invalid("user pin info too old");
        }
        let pin_type = graph::read_pin_type(pkg, r)?;
        let direction = r.u8()?;
        let default_value = r.fstring()?;
        v.push(UserPin { name, pin_type, direction, default_value });
    }
    Ok(v)
}

fn read_data_table(pr: &PropReader, _obj: &Object, r: &mut Reader, end: usize) -> Result<Vec<(String, Vec<Property>)>> {
    let pkg = pr.pkg;
    let n = r.count(8)?;
    let mut rows = vec![];
    for _ in 0..n {
        let name = pkg.read_fname(r)?;
        let props = pr.read_properties(r, end, 1)?;
        rows.push((name, props));
    }
    Ok(rows)
}

fn read_string_map(pkg: &Package, r: &mut Reader) -> Result<Vec<(String, String)>> {
    let n = r.count(12)?;
    let mut m = vec![];
    for _ in 0..n {
        let k = pkg.read_fname(r)?;
        let v = r.fstring()?;
        m.push((k, v));
    }
    Ok(m)
}

fn read_metadata_export(pkg: &Package, r: &mut Reader) -> Result<PackageMetaData> {
    let mut md = PackageMetaData::default();
    let n = r.count(8)?;
    for _ in 0..n {
        let obj = r.i32()?;
        let map = read_string_map(pkg, r)?;
        md.objects.push((pkg.ref_string(obj), map));
    }
    if pkg.custom_ver(custom::editor()) >= custom::ED_ROOT_META_DATA_SUPPORT {
        md.root = read_string_map(pkg, r)?;
    }
    Ok(md)
}

fn read_enum(pkg: &Package, r: &mut Reader) -> Result<Vec<(String, i64)>> {
    if pkg.custom_ver(custom::framework()) < custom::FW_REMOVE_UFIELD_NEXT {
        r.i32()?; // UField::Next
    }
    let mut names = vec![];
    if pkg.ue4() < ue4::TIGHTLY_PACKED_ENUMS {
        let n = r.count(8)?;
        for i in 0..n {
            names.push((pkg.read_fname(r)?, i as i64));
        }
    } else if pkg.custom_ver(custom::core()) < custom::CORE_ENUM_PROPERTIES {
        let n = r.count(9)?;
        for _ in 0..n {
            let k = pkg.read_fname(r)?;
            let v = r.u8()? as i64;
            names.push((k, v));
        }
    } else {
        let n = r.count(16)?;
        for _ in 0..n {
            let k = pkg.read_fname(r)?;
            let v = r.i64()?;
            names.push((k, v));
        }
    }
    // CppForm (+ underlying type in newer versions)
    if r.remaining() > 0 {
        let mut t = r.clone();
        if let Ok(form) = t.u8() {
            if form <= 2 {
                *r = t;
            }
        }
    }
    Ok(names)
}

fn read_field(pkg: &Package, r: &mut Reader, depth: u32) -> Result<Option<FieldInfo>> {
    let kind = pkg.read_fname(r)?;
    if kind == "None" {
        return Ok(None);
    }
    read_field_body(pkg, r, kind, depth).map(Some)
}

fn read_field_body(pkg: &Package, r: &mut Reader, kind: String, depth: u32) -> Result<FieldInfo> {
    if depth > 16 {
        return invalid("field nesting too deep");
    }
    let name = pkg.read_fname(r)?;
    if !pkg.is_filter_editor_only() {
        r.u32()?; // flags
    }
    let mut metadata = vec![];
    if pkg.summary.package_flags & crate::package::PKG_COOKED == 0 && r.bool32()? {
        metadata = read_string_map(pkg, r)?;
    }
    let array_dim = r.i32()?;
    let _element_size = r.i32()?;
    let flags = r.u64()?;
    r.u16()?; // RepIndex
    pkg.read_fname(r)?; // RepNotifyFunc
    r.u8()?; // BlueprintReplicationCondition
    let mut f = FieldInfo { name, kind: kind.clone(), array_dim, flags, object: 0, object2: 0, inner: vec![], metadata };
    match kind.as_str() {
        "ByteProperty" | "StructProperty" | "InterfaceProperty" | "DelegateProperty" | "MulticastDelegateProperty"
        | "MulticastInlineDelegateProperty" | "MulticastSparseDelegateProperty" | "ObjectProperty"
        | "ObjectPtrProperty" | "WeakObjectProperty" | "LazyObjectProperty" | "SoftObjectProperty" => {
            f.object = r.i32()?;
        }
        "ClassProperty" | "ClassPtrProperty" | "SoftClassProperty" => {
            f.object = r.i32()?;
            f.object2 = r.i32()?;
        }
        "BoolProperty" => {
            r.skip(6)?;
        }
        "EnumProperty" => {
            f.object = r.i32()?;
            if let Some(i) = read_field(pkg, r, depth + 1)? {
                f.inner.push(i);
            }
        }
        "ArrayProperty" | "SetProperty" | "OptionalProperty" => {
            if let Some(i) = read_field(pkg, r, depth + 1)? {
                f.inner.push(i);
            }
        }
        "MapProperty" => {
            for _ in 0..2 {
                if let Some(i) = read_field(pkg, r, depth + 1)? {
                    f.inner.push(i);
                }
            }
        }
        "FieldPathProperty" => {
            pkg.read_fname(r)?;
        }
        "IntProperty" | "Int8Property" | "Int16Property" | "Int64Property" | "UInt16Property" | "UInt32Property"
        | "UInt64Property" | "FloatProperty" | "DoubleProperty" | "StrProperty" | "NameProperty" | "TextProperty"
        | "Utf8StrProperty" | "AnsiStrProperty" | "LargeWorldCoordinatesRealProperty" => {}
        other => return invalid(format!("unknown field class {other}")),
    }
    Ok(f)
}

fn read_struct_info(pr: &PropReader, r: &mut Reader, class: &str, end: usize) -> Result<StructInfo> {
    let pkg = pr.pkg;
    let mut s = StructInfo::default();
    let old_next = pkg.custom_ver(custom::framework()) < custom::FW_REMOVE_UFIELD_NEXT;
    if old_next {
        r.i32()?; // UField::Next
    }
    s.super_struct = r.i32()?;
    if old_next {
        s.children.push(r.i32()?);
    } else {
        let n = r.count(4)?;
        for _ in 0..n {
            s.children.push(r.i32()?);
        }
    }
    if pkg.custom_ver(custom::core()) >= custom::CORE_FPROPERTIES {
        let n = r.count(8)?;
        for _ in 0..n {
            let kind = pkg.read_fname(r)?;
            s.fields.push(read_field_body(pkg, r, kind, 0)?);
        }
    }
    let _bytecode_size = r.i32()?;
    let script_size = r.i32()?;
    if script_size < 0 || r.pos() + script_size as usize > end {
        return invalid("invalid script size");
    }
    s.script_size = script_size;
    r.skip(script_size as usize)?;
    if class.ends_with("Function") && !class.starts_with("K2") {
        let flags = r.u32()?;
        s.function_flags = Some(flags);
        if flags & 0x40 != 0 {
            r.u16()?; // RepOffset (FUNC_Net)
        }
        if pkg.ue4() >= ue4::SERIALIZE_BLUEPRINT_EVENTGRAPH_FASTCALLS_IN_UFUNCTION {
            r.i32()?;
            r.i32()?;
        }
    }
    if class.ends_with("Struct") {
        r.u32()?; // StructFlags
        if class == "UserDefinedStruct" && r.pos() < end && pr.looks_like_tagged(r, None) {
            s.defaults = pr.read_properties(r, end, 0)?;
        }
    }
    if class.ends_with("Class") {
        let mut c = ClassInfo::default();
        let n = r.count(12)?;
        for _ in 0..n {
            let k = pkg.read_fname(r)?;
            let v = r.i32()?;
            c.func_map.push((k, v));
        }
        c.class_flags = r.u32()?;
        c.class_within = r.i32()?;
        c.config_name = pkg.read_fname(r)?;
        if !pkg.is_filter_editor_only() {
            c.generated_by = r.i32()?;
        }
        let n = r.count(12)?;
        for _ in 0..n {
            let cls = r.i32()?;
            r.i32()?; // PointerOffset
            let k2 = r.bool32()?;
            c.interfaces.push((cls, k2));
        }
        r.bool32()?; // bDeprecatedForceScriptOrder
        pkg.read_fname(r)?; // Dummy
        r.bool32()?; // bCooked
        c.cdo = r.i32()?;
        s.class = Some(c);
    }
    Ok(s)
}

/// Parses a free-standing value (used by tests / debugging).
pub fn read_value_at(pkg: &Package, offset: usize, ty: &TypeName) -> Result<Value> {
    let pr = PropReader::new(pkg);
    let mut r = pkg.reader();
    r.seek(offset)?;
    pr.read_value(&mut r, ty, ValueCtx::default(), 0)
}

pub fn read_all(pkg: &Package) -> Vec<Object> {
    (0..pkg.exports.len()).map(|i| read_object(pkg, i)).collect()
}
