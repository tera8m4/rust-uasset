//! Tagged property serialization (PropertyTag.cpp, Property*.cpp).

use crate::errors::{ParseError, Result, invalid};
use crate::package::Package;
use crate::reader::{Guid, Reader};
use crate::structs;
use crate::text::{Text, read_text};
use crate::versions::{ue4, ue5};

/// Hierarchical property type name, e.g. `ArrayProperty(StructProperty(Vector(/Script/CoreUObject)))`.
#[derive(Debug, Clone, PartialEq)]
pub struct TypeName {
    pub name: String,
    pub params: Vec<TypeName>,
}

impl TypeName {
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into(), params: vec![] }
    }

    pub fn with(name: impl Into<String>, params: Vec<TypeName>) -> Self {
        Self { name: name.into(), params }
    }

    pub fn param(&self, i: usize) -> Option<&TypeName> {
        self.params.get(i)
    }

    pub fn param_name(&self, i: usize) -> &str {
        self.params.get(i).map(|p| p.name.as_str()).unwrap_or("None")
    }

    /// Struct name for StructProperty, enum name for Byte/EnumProperty.
    pub fn inner_name(&self) -> &str {
        self.param_name(0)
    }

    /// Path of a struct/enum type when known: `/Script/CoreUObject.Vector`.
    pub fn inner_path(&self) -> String {
        match self.params.first() {
            Some(p) if !p.params.is_empty() => {
                let outer: Vec<&str> = p.params.iter().map(|x| x.name.as_str()).collect();
                format!("{}.{}", outer.join("."), p.name)
            }
            Some(p) => p.name.clone(),
            None => "None".into(),
        }
    }

    /// Compact, human-readable type: `Array<Struct<Vector>>`.
    pub fn short(&self) -> String {
        let base = self.name.strip_suffix("Property").unwrap_or(&self.name);
        match self.name.as_str() {
            "StructProperty" => self.inner_name().to_string(),
            "ByteProperty" if self.inner_name() != "None" => self.inner_name().to_string(),
            "EnumProperty" => self.inner_name().to_string(),
            "ArrayProperty" | "SetProperty" | "OptionalProperty" => {
                format!("{}<{}>", base, self.params.first().map(|p| p.short()).unwrap_or("?".into()))
            }
            "MapProperty" => format!(
                "Map<{}, {}>",
                self.params.first().map(|p| p.short()).unwrap_or("?".into()),
                self.params.get(1).map(|p| p.short()).unwrap_or("?".into())
            ),
            _ => base.to_string(),
        }
    }
}

impl std::fmt::Display for TypeName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)?;
        if !self.params.is_empty() {
            write!(f, "(")?;
            for (i, p) in self.params.iter().enumerate() {
                if i > 0 {
                    write!(f, ",")?;
                }
                write!(f, "{p}")?;
            }
            write!(f, ")")?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct Tag {
    pub name: String,
    pub ty: TypeName,
    pub size: i32,
    pub array_index: i32,
    pub bool_val: bool,
    pub property_guid: Option<Guid>,
    /// Some(true) when the tag says the value uses binary/native serialization (UE 5.4+ only).
    pub binary_or_native: Option<bool>,
    pub skipped: bool,
}

#[derive(Debug, Clone)]
pub enum Value {
    Bool(bool),
    Int(i64),
    UInt(u64),
    Float(f32),
    Double(f64),
    Str(String),
    Name(String),
    Text(Text),
    Enum(String),
    /// FPackageIndex into the import/export table.
    Object(i32),
    SoftObject(String),
    Guid(Guid),
    Struct(Box<StructValue>),
    Array(Vec<Value>),
    Set(Vec<Value>),
    Map(Vec<(Value, Value)>),
    Delegate { object: i32, function: String },
    MulticastDelegate(Vec<(i32, String)>),
    FieldPath { path: Vec<String>, owner: i32 },
    Optional(Option<Box<Value>>),
    /// Undecoded bytes.
    Raw { type_name: String, bytes: Vec<u8>, note: String },
}

#[derive(Debug, Clone)]
pub struct StructValue {
    pub type_name: String,
    pub fields: Vec<Property>,
    /// True for natively serialized math-like structs (render compact).
    pub native: bool,
}

#[derive(Debug, Clone)]
pub struct Property {
    pub name: String,
    pub array_index: i32,
    pub ty: Option<TypeName>,
    pub value: Value,
}

impl Property {
    pub fn new(name: impl Into<String>, value: Value) -> Self {
        Self { name: name.into(), array_index: 0, ty: None, value }
    }

    pub fn display_name(&self) -> String {
        if self.array_index != 0 {
            format!("{}[{}]", self.name, self.array_index)
        } else {
            self.name.clone()
        }
    }
}

impl StructValue {
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.fields.iter().find(|f| f.name == name).map(|f| &f.value)
    }
}

impl Value {
    pub fn as_struct(&self) -> Option<&StructValue> {
        match self {
            Value::Struct(s) => Some(s),
            _ => None,
        }
    }
    pub fn as_array(&self) -> Option<&[Value]> {
        match self {
            Value::Array(v) | Value::Set(v) => Some(v),
            _ => None,
        }
    }
    pub fn as_object(&self) -> Option<i32> {
        match self {
            Value::Object(i) => Some(*i),
            _ => None,
        }
    }
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) | Value::Name(s) | Value::Enum(s) | Value::SoftObject(s) => Some(s),
            _ => None,
        }
    }
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Value::Int(v) => Some(*v),
            Value::UInt(v) => Some(*v as i64),
            _ => None,
        }
    }
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Float(v) => Some(*v as f64),
            Value::Double(v) => Some(*v),
            Value::Int(v) => Some(*v as f64),
            Value::UInt(v) => Some(*v as f64),
            _ => None,
        }
    }
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }
}

/// Finds a property by name in a list.
pub fn find<'a>(props: &'a [Property], name: &str) -> Option<&'a Value> {
    props.iter().find(|p| p.name == name).map(|p| &p.value)
}

/// Strips the `_12_0123456789ABCDEF0123456789ABCDEF` suffix used by user defined struct members.
pub fn clean_field_name(name: &str) -> String {
    let b = name.as_bytes();
    if b.len() > 34 && b[b.len() - 33] == b'_' && name[b.len() - 32..].bytes().all(|c| c.is_ascii_hexdigit()) {
        let head = &name[..b.len() - 33];
        if let Some(pos) = head.rfind('_') {
            if head[pos + 1..].bytes().all(|c| c.is_ascii_digit()) && pos + 1 < head.len() {
                return head[..pos].to_string();
            }
        }
    }
    name.to_string()
}

/// Context passed to value readers.
#[derive(Clone, Copy, Debug, Default)]
pub struct ValueCtx {
    /// Byte size available for this value (known for top level tagged values).
    pub size: Option<usize>,
    /// Bool value stored in the tag.
    pub bool_val: Option<bool>,
    /// Native serialization flag from the tag (5.4+).
    pub native: Option<bool>,
    /// Whether this value is an element of a container.
    pub in_container: bool,
    /// For legacy containers: the per-element size if it can be inferred.
    pub elem_size: Option<usize>,
}

pub struct PropReader<'p> {
    pub pkg: &'p Package,
}

const MAX_DEPTH: u32 = 48;

impl<'p> PropReader<'p> {
    pub fn new(pkg: &'p Package) -> Self {
        Self { pkg }
    }

    fn complete_type_names(&self) -> bool {
        self.pkg.ue5() >= ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME
    }

    fn fname(&self, r: &mut Reader) -> Result<String> {
        self.pkg.read_fname(r)
    }

    fn read_type_name(&self, r: &mut Reader) -> Result<TypeName> {
        // Flattened pre-order list of (name, inner count)
        let mut nodes = vec![];
        let mut remaining: i32 = 1;
        while remaining > 0 {
            let name = self.fname(r)?;
            let inner = r.i32()?;
            if !(0..=64).contains(&inner) || nodes.len() > 256 {
                return invalid(format!("invalid property type name node count {inner}"));
            }
            nodes.push((name, inner));
            remaining += inner - 1;
        }
        fn build(nodes: &[(String, i32)], pos: &mut usize) -> TypeName {
            let (name, n) = nodes[*pos].clone();
            *pos += 1;
            let mut t = TypeName::new(name);
            for _ in 0..n {
                if *pos < nodes.len() {
                    t.params.push(build(nodes, pos));
                }
            }
            t
        }
        let mut pos = 0;
        Ok(build(&nodes, &mut pos))
    }

    fn read_extensions(&self, r: &mut Reader) -> Result<()> {
        let ext = r.u8()?;
        if ext & 0x02 != 0 {
            r.u8()?; // OverriddenPropertyOperation
            r.bool32()?; // bExperimentalOverridableLogic
        }
        if ext & 0x04 != 0 {
            r.bool32()?; // bExperimentalExternalObjects
        }
        Ok(())
    }

    /// Reads a property tag. Returns None at the terminating "None" tag.
    pub fn read_tag(&self, r: &mut Reader) -> Result<Option<Tag>> {
        let name = self.fname(r)?;
        if name == "None" {
            return Ok(None);
        }
        if self.complete_type_names() {
            let ty = self.read_type_name(r)?;
            let size = r.i32()?;
            let flags = r.u8()?;
            let array_index = if flags & 0x01 != 0 { r.i32()? } else { 0 };
            let property_guid = if flags & 0x02 != 0 { Some(r.guid()?) } else { None };
            if flags & 0x04 != 0 {
                self.read_extensions(r)?;
            }
            if size < 0 {
                return invalid(format!("negative property size for '{name}'"));
            }
            return Ok(Some(Tag {
                name,
                ty,
                size,
                array_index,
                bool_val: flags & 0x10 != 0,
                property_guid,
                binary_or_native: Some(flags & 0x08 != 0),
                skipped: flags & 0x20 != 0,
            }));
        }

        let ver = self.pkg.ue4();
        let type_name = self.fname(r)?;
        let size = r.i32()?;
        let array_index = r.i32()?;
        if size < 0 {
            return invalid(format!("negative property size for '{name}'"));
        }
        let mut ty = TypeName::new(type_name.clone());
        let mut bool_val = false;
        match type_name.as_str() {
            "StructProperty" => {
                let sname = self.fname(r)?;
                if ver >= ue4::STRUCT_GUID_IN_PROPERTY_TAG {
                    r.guid()?;
                }
                ty.params.push(TypeName::new(sname));
            }
            "BoolProperty" => bool_val = r.u8()? != 0,
            "ByteProperty" => {
                let e = self.fname(r)?;
                if e != "None" {
                    ty.params.push(TypeName::new(e));
                }
            }
            "EnumProperty" => {
                let e = self.fname(r)?;
                ty.params.push(TypeName::new(e));
                ty.params.push(TypeName::new("ByteProperty"));
            }
            "ArrayProperty" => {
                let inner = if ver >= ue4::ARRAY_PROPERTY_INNER_TAGS { self.fname(r)? } else { "None".into() };
                ty.params.push(TypeName::new(inner));
            }
            "OptionalProperty" => {
                let inner = self.fname(r)?;
                ty.params.push(TypeName::new(inner));
            }
            "SetProperty" if ver >= ue4::PROPERTY_TAG_SET_MAP_SUPPORT => {
                let inner = self.fname(r)?;
                ty.params.push(TypeName::new(inner));
            }
            "MapProperty" if ver >= ue4::PROPERTY_TAG_SET_MAP_SUPPORT => {
                let k = self.fname(r)?;
                let v = self.fname(r)?;
                ty.params.push(TypeName::new(k));
                ty.params.push(TypeName::new(v));
            }
            _ => {}
        }
        let mut property_guid = None;
        if ver >= ue4::PROPERTY_GUID_IN_PROPERTY_TAG && r.u8()? != 0 {
            property_guid = Some(r.guid()?);
        }
        if self.pkg.ue5() >= ue5::PROPERTY_TAG_EXTENSION_AND_OVERRIDABLE_SERIALIZATION {
            self.read_extensions(r)?;
        }
        Ok(Some(Tag {
            name,
            ty,
            size,
            array_index,
            bool_val,
            property_guid,
            binary_or_native: None,
            skipped: false,
        }))
    }

    /// Heuristic: does the data at the reader look like a tagged property stream?
    pub fn looks_like_tagged(&self, r: &Reader, size: Option<usize>) -> bool {
        let mut t = r.clone();
        let Ok(name) = self.fname(&mut t) else { return false };
        if name == "None" {
            return size.map(|s| s == 8).unwrap_or(true);
        }
        if name.is_empty() || name.contains(' ') && name.len() > 128 {
            return false;
        }
        if self.complete_type_names() {
            let Ok(ty) = self.read_type_name(&mut t) else { return false };
            ty.name.ends_with("Property")
        } else {
            let Ok(ty) = self.fname(&mut t) else { return false };
            ty.ends_with("Property")
        }
    }

    /// Reads a tagged property list terminated by "None".
    pub fn read_properties(&self, r: &mut Reader, end: usize, depth: u32) -> Result<Vec<Property>> {
        if depth > MAX_DEPTH {
            return invalid("property nesting too deep");
        }
        let mut props = vec![];
        loop {
            if r.pos() >= end {
                return invalid("property list not terminated before end of data");
            }
            let Some(tag) = self.read_tag(r)? else { break };
            let start = r.pos();
            let size = tag.size as usize;
            if start + size > end {
                return invalid(format!("property '{}' size {} exceeds data", tag.name, size));
            }
            let value = if tag.skipped {
                Value::Raw { type_name: tag.ty.to_string(), bytes: vec![], note: "skipped".into() }
            } else {
                let ctx = ValueCtx {
                    size: Some(size),
                    bool_val: Some(tag.bool_val),
                    native: tag.binary_or_native,
                    in_container: false,
                    elem_size: None,
                };
                let mut sub = r.clone();
                match self.read_value(&mut sub, &tag.ty, ctx, depth + 1) {
                    Ok(v) if sub.pos() == start + size => v,
                    Ok(v) => {
                        // Size mismatch: keep the decoded value if it consumed less but looks sane? No: be strict.
                        let consumed = sub.pos() as i64 - start as i64;
                        let _ = v;
                        Value::Raw {
                            type_name: tag.ty.to_string(),
                            bytes: r.data()[start..start + size].to_vec(),
                            note: format!("decoded {consumed} of {size} bytes"),
                        }
                    }
                    Err(e) => Value::Raw {
                        type_name: tag.ty.to_string(),
                        bytes: r.data()[start..start + size].to_vec(),
                        note: e.to_string(),
                    },
                }
            };
            r.seek(start + size)?;
            props.push(Property {
                name: clean_field_name(&tag.name),
                array_index: tag.array_index,
                ty: Some(tag.ty),
                value,
            });
        }
        Ok(props)
    }

    pub fn read_value(&self, r: &mut Reader, ty: &TypeName, ctx: ValueCtx, depth: u32) -> Result<Value> {
        if depth > MAX_DEPTH {
            return invalid("value nesting too deep");
        }
        let pkg = self.pkg;
        Ok(match ty.name.as_str() {
            "BoolProperty" => {
                if !ctx.in_container {
                    if let Some(b) = ctx.bool_val {
                        return Ok(Value::Bool(b));
                    }
                }
                Value::Bool(r.u8()? != 0)
            }
            "ByteProperty" => {
                let as_name = if !ctx.in_container {
                    match ctx.size {
                        Some(8) => true,
                        Some(1) => false,
                        _ => ty.inner_name() != "None",
                    }
                } else if let Some(es) = ctx.elem_size {
                    es == 8
                } else {
                    ty.inner_name() != "None"
                };
                if as_name { Value::Enum(self.fname(r)?) } else { Value::UInt(r.u8()? as u64) }
            }
            "EnumProperty" => {
                if ctx.in_container && ctx.elem_size == Some(1) {
                    Value::UInt(r.u8()? as u64)
                } else {
                    Value::Enum(self.fname(r)?)
                }
            }
            "Int8Property" => Value::Int(r.i8()? as i64),
            "Int16Property" => Value::Int(r.i16()? as i64),
            "IntProperty" => Value::Int(r.i32()? as i64),
            "Int64Property" => Value::Int(r.i64()?),
            "UInt16Property" => Value::UInt(r.u16()? as u64),
            "UInt32Property" => Value::UInt(r.u32()? as u64),
            "UInt64Property" => Value::UInt(r.u64()?),
            "FloatProperty" => Value::Float(r.f32()?),
            "DoubleProperty" | "LargeWorldCoordinatesRealProperty" => Value::Double(r.f64()?),
            "StrProperty" | "Utf8StrProperty" | "AnsiStrProperty" | "VerseStringProperty" => Value::Str(r.fstring()?),
            "NameProperty" => Value::Name(self.fname(r)?),
            "TextProperty" => Value::Text(read_text(pkg, r)?),
            "ObjectProperty" | "ClassProperty" | "ObjectPtrProperty" | "ClassPtrProperty" | "WeakObjectProperty"
            | "InterfaceProperty" => {
                let idx = r.i32()?;
                if !pkg.is_valid_index(idx) {
                    return invalid(format!("invalid object index {idx}"));
                }
                Value::Object(idx)
            }
            "LazyObjectProperty" => Value::Guid(r.guid()?),
            "SoftObjectProperty" | "SoftClassProperty" | "AssetObjectProperty" | "AssetClassProperty" => {
                Value::SoftObject(pkg.read_soft_object_path(r)?)
            }
            "DelegateProperty" => {
                let object = r.i32()?;
                let function = self.fname(r)?;
                Value::Delegate { object, function }
            }
            "MulticastDelegateProperty" | "MulticastInlineDelegateProperty" | "MulticastSparseDelegateProperty" => {
                let n = r.count(12)?;
                let mut v = vec![];
                for _ in 0..n {
                    let o = r.i32()?;
                    let f = self.fname(r)?;
                    v.push((o, f));
                }
                Value::MulticastDelegate(v)
            }
            "FieldPathProperty" => {
                let n = r.count(8)?;
                let mut path = vec![];
                for _ in 0..n {
                    path.push(self.fname(r)?);
                }
                let owner = r.i32()?;
                Value::FieldPath { path, owner }
            }
            "OptionalProperty" => {
                let set = r.bool32()?;
                if set {
                    let inner = ty.param(0).cloned().unwrap_or(TypeName::new("None"));
                    let ictx = ValueCtx { size: ctx.size.map(|s| s.saturating_sub(4)), ..ctx };
                    Value::Optional(Some(Box::new(self.read_value(r, &inner, ictx, depth + 1)?)))
                } else {
                    Value::Optional(None)
                }
            }
            "StructProperty" => {
                let sname = ty.inner_name().to_string();
                self.read_struct(r, &sname, ctx.size, ctx.native, depth)?
            }
            "ArrayProperty" => self.read_array(r, ty, ctx, depth)?,
            "SetProperty" => {
                let inner = ty.param(0).cloned().unwrap_or(TypeName::new("None"));
                let start = r.pos();
                let removed = r.count(1)?;
                let ectx = self.container_ctx(ctx, &inner, None, 0);
                for _ in 0..removed {
                    self.read_container_elem(r, &inner, ectx, depth, "SetProperty")?;
                }
                let n = r.count(1)?;
                let ectx = self.container_ctx(ctx, &inner, ctx.size.map(|s| s.saturating_sub(r.pos() - start)), n);
                let mut v = Vec::with_capacity(n.min(4096));
                for _ in 0..n {
                    v.push(self.read_container_elem(r, &inner, ectx, depth, "SetProperty")?);
                }
                Value::Set(v)
            }
            "MapProperty" => {
                let kt = ty.param(0).cloned().unwrap_or(TypeName::new("None"));
                let vt = ty.param(1).cloned().unwrap_or(TypeName::new("None"));
                let removed = r.count(1)?;
                let kctx = self.container_ctx(ctx, &kt, None, 0);
                for _ in 0..removed {
                    self.read_container_elem(r, &kt, kctx, depth, "MapProperty")?;
                }
                let n = r.count(1)?;
                let kctx = self.container_ctx(ctx, &kt, None, 0);
                let vctx = self.container_ctx(ctx, &vt, None, 0);
                let mut v = Vec::with_capacity(n.min(4096));
                for _ in 0..n {
                    let k = self.read_container_elem(r, &kt, kctx, depth, "MapProperty")?;
                    let val = self.read_container_elem(r, &vt, vctx, depth, "MapProperty")?;
                    v.push((k, val));
                }
                Value::Map(v)
            }
            other => {
                return Err(ParseError::Unsupported(format!("unsupported property type {other}")));
            }
        })
    }

    fn container_ctx(&self, outer: ValueCtx, _inner: &TypeName, data_size: Option<usize>, count: usize) -> ValueCtx {
        let elem_size = match (data_size, count) {
            (Some(s), n) if n > 0 && s % n == 0 => Some(s / n),
            _ => None,
        };
        ValueCtx { size: None, bool_val: None, native: outer.native, in_container: true, elem_size }
    }

    fn read_container_elem(&self, r: &mut Reader, ty: &TypeName, ctx: ValueCtx, depth: u32, container: &str) -> Result<Value> {
        if ty.name == "StructProperty" {
            let sname = ty.inner_name().to_string();
            if sname == "None" {
                // Legacy set/map: the struct type is not stored. Guess.
                return self.read_unknown_struct(r, depth, container);
            }
            // In 5.4+ the container flag is "any element is native"; resolve per struct.
            let native = if structs::has_native_parser(&sname) || structs::is_known_native(&sname) {
                None
            } else if container == "MapProperty" {
                None
            } else {
                ctx.native
            };
            let es = if structs::is_known_native(&sname) { ctx.elem_size } else { None };
            return self.read_struct(r, &sname, es, native, depth);
        }
        self.read_value(r, ty, ctx, depth + 1)
    }

    /// Struct with unknown type inside a legacy Set/Map.
    fn read_unknown_struct(&self, r: &mut Reader, depth: u32, _container: &str) -> Result<Value> {
        if self.looks_like_tagged(r, None) {
            let end = r.len();
            let fields = self.read_properties(r, end, depth + 1)?;
            return Ok(Value::Struct(Box::new(StructValue { type_name: "?".into(), fields, native: false })));
        }
        // Most common natively serialized key type in maps is FGuid.
        Ok(Value::Guid(r.guid()?))
    }

    fn read_array(&self, r: &mut Reader, ty: &TypeName, ctx: ValueCtx, depth: u32) -> Result<Value> {
        let inner = ty.param(0).cloned().unwrap_or(TypeName::new("None"));
        let start = r.pos();
        let n = r.count(1)?;
        let data_size = ctx.size.map(|s| s.saturating_sub(r.pos() - start));
        if inner.name == "StructProperty" || (inner.name == "None" && !self.complete_type_names()) {
            if !self.complete_type_names() && self.pkg.ue4() >= ue4::INNER_ARRAY_TAG_INFO {
                // Legacy inner tag describing the struct type
                if n == 0 && data_size == Some(0) {
                    return Ok(Value::Array(vec![]));
                }
                let Some(itag) = self.read_tag(r)? else {
                    return invalid("missing array inner tag");
                };
                let sname = itag.ty.inner_name().to_string();
                let isize = itag.size as usize;
                let elem_size = if n > 0 && isize % n == 0 && structs::is_known_native(&sname) { Some(isize / n) } else { None };
                let istart = r.pos();
                let mut v = Vec::with_capacity(n.min(4096));
                for _ in 0..n {
                    v.push(self.read_struct(r, &sname, elem_size, None, depth)?);
                }
                if r.pos() != istart + isize {
                    return invalid(format!("array of {sname}: decoded {} of {isize} bytes", r.pos() - istart));
                }
                return Ok(Value::Array(v));
            }
            let sname = inner.inner_name().to_string();
            let elem_size = match data_size {
                Some(s) if n > 0 && s % n == 0 && ctx.native != Some(false) && structs::is_known_native(&sname) => Some(s / n),
                _ => None,
            };
            let mut v = Vec::with_capacity(n.min(4096));
            for _ in 0..n {
                v.push(self.read_struct(r, &sname, elem_size, ctx.native, depth)?);
            }
            return Ok(Value::Array(v));
        }
        let ectx = self.container_ctx(ctx, &inner, data_size, n);
        let mut v = Vec::with_capacity(n.min(4096));
        for _ in 0..n {
            v.push(self.read_value(r, &inner, ectx, depth + 1)?);
        }
        Ok(Value::Array(v))
    }

    /// Reads a struct value. `native`: Some(true/false) when known from the tag.
    pub fn read_struct(&self, r: &mut Reader, sname: &str, size: Option<usize>, native: Option<bool>, depth: u32) -> Result<Value> {
        if depth > MAX_DEPTH {
            return invalid("struct nesting too deep");
        }
        let tagged = |r: &mut Reader| -> Result<Value> {
            let end = size.map(|s| r.pos() + s).unwrap_or(r.len());
            let fields = self.read_properties(r, end, depth + 1)?;
            Ok(Value::Struct(Box::new(StructValue { type_name: sname.to_string(), fields, native: false })))
        };
        match native {
            Some(false) => return tagged(r),
            Some(true) => {
                if size == Some(0) {
                    return Ok(Value::Struct(Box::new(StructValue { type_name: sname.to_string(), fields: vec![], native: true })));
                }
                if let Some(v) = structs::read_native(self, r, sname, size, depth)? {
                    return Ok(v);
                }
                // Many "native" structs only hook Serialize and fall back to tagged properties.
                if self.looks_like_tagged(r, size) {
                    let start = r.pos();
                    let mut t = r.clone();
                    if let Ok(v) = tagged(&mut t) {
                        if size.map(|s| t.pos() == start + s).unwrap_or(true) {
                            *r = t;
                            return Ok(v);
                        }
                    }
                }
                return match size {
                    Some(s) => Ok(Value::Raw { type_name: sname.into(), bytes: r.bytes(s)?.to_vec(), note: "native struct".into() }),
                    None => Err(ParseError::Unsupported(format!("native struct {sname} of unknown size"))),
                };
            }
            None => {}
        }
        // Legacy: decide between tagged and native.
        let has_parser = structs::has_native_parser(sname);
        if !has_parser && !structs::is_known_native(sname) {
            return tagged(r);
        }
        if structs::may_be_tagged(sname) && self.looks_like_tagged(r, size) {
            return tagged(r);
        }
        if let Some(v) = structs::read_native(self, r, sname, size, depth)? {
            return Ok(v);
        }
        if self.looks_like_tagged(r, size) {
            return tagged(r);
        }
        match size {
            Some(s) => Ok(Value::Raw { type_name: sname.into(), bytes: r.bytes(s)?.to_vec(), note: "native struct".into() }),
            None => Err(ParseError::Unsupported(format!("native struct {sname} of unknown size"))),
        }
    }
}
