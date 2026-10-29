//! UEdGraphPin / FEdGraphPinType native serialization (EdGraphPin.cpp, EdGraphNode.cpp).

use crate::errors::{Result, invalid};
use crate::package::Package;
use crate::property::{Property, StructValue, Value};
use crate::reader::{Guid, Reader};
use crate::text::{Text, read_text};
use crate::versions::{custom, ue4};

#[derive(Debug, Clone, Default)]
pub struct TerminalType {
    pub category: String,
    pub sub_category: String,
    pub sub_category_object: i32,
}

#[derive(Debug, Clone, Default)]
pub struct PinType {
    pub category: String,
    pub sub_category: String,
    pub sub_category_object: i32,
    /// 0 none, 1 array, 2 set, 3 map
    pub container: u8,
    pub value_type: Option<TerminalType>,
    pub is_reference: bool,
    pub is_weak: bool,
    pub member_parent: i32,
    pub member_name: String,
    pub member_guid: Guid,
    pub is_const: bool,
}

impl PinType {
    pub fn to_value(&self) -> Value {
        let mut f = vec![
            Property::new("PinCategory", Value::Name(self.category.clone())),
        ];
        if self.sub_category != "None" && !self.sub_category.is_empty() {
            f.push(Property::new("PinSubCategory", Value::Name(self.sub_category.clone())));
        }
        if self.sub_category_object != 0 {
            f.push(Property::new("PinSubCategoryObject", Value::Object(self.sub_category_object)));
        }
        if self.container != 0 {
            f.push(Property::new("ContainerType", Value::Enum(["None", "Array", "Set", "Map"][self.container.min(3) as usize].into())));
        }
        if let Some(vt) = &self.value_type {
            f.push(Property::new("PinValueType", Value::Str(format!("{}:{}", vt.category, vt.sub_category))));
            if vt.sub_category_object != 0 {
                f.push(Property::new("PinValueTypeObject", Value::Object(vt.sub_category_object)));
            }
        }
        if self.is_reference {
            f.push(Property::new("bIsReference", Value::Bool(true)));
        }
        if self.is_const {
            f.push(Property::new("bIsConst", Value::Bool(true)));
        }
        if self.member_name != "None" && !self.member_name.is_empty() {
            f.push(Property::new("MemberName", Value::Name(self.member_name.clone())));
            if self.member_parent != 0 {
                f.push(Property::new("MemberParent", Value::Object(self.member_parent)));
            }
        }
        Value::Struct(Box::new(StructValue { type_name: "EdGraphPinType".into(), fields: f, native: true }))
    }

    /// Human readable type: `bool`, `object<Actor>`, `Array<struct<Vector>>`, `Map<Name, int>`.
    pub fn display(&self, pkg: &Package) -> String {
        let base = terminal_display(pkg, &self.category, &self.sub_category, self.sub_category_object, &self.member_name);
        let mut s = match self.container {
            1 => format!("Array<{base}>"),
            2 => format!("Set<{base}>"),
            3 => {
                let v = self
                    .value_type
                    .as_ref()
                    .map(|v| terminal_display(pkg, &v.category, &v.sub_category, v.sub_category_object, ""))
                    .unwrap_or("?".into());
                format!("Map<{base}, {v}>")
            }
            _ => base,
        };
        if self.is_const {
            s = format!("const {s}");
        }
        if self.is_reference {
            s.push('&');
        }
        s
    }
}

pub fn terminal_display(pkg: &Package, cat: &str, sub: &str, obj: i32, member: &str) -> String {
    let sub_ok = !sub.is_empty() && sub != "None";
    let obj_name = if obj != 0 { Some(pkg.obj_name(obj)) } else { None };
    match cat {
        "real" => {
            if sub_ok { sub.to_string() } else { "real".into() }
        }
        "object" | "class" | "interface" | "softobject" | "softclass" => {
            let target = obj_name.unwrap_or_else(|| if sub_ok { sub.to_string() } else { "Object".into() });
            let prefix = match cat {
                "object" => "",
                "class" => "class:",
                "interface" => "interface:",
                "softobject" => "soft:",
                _ => "softclass:",
            };
            format!("{prefix}{target}")
        }
        "struct" | "byte" | "enum" => match obj_name {
            Some(o) => o,
            None => cat.to_string(),
        },
        "delegate" | "mcdelegate" => {
            if !member.is_empty() && member != "None" { format!("{cat}({member})") } else { cat.to_string() }
        }
        _ => {
            if sub_ok && sub != "self" {
                format!("{cat}:{sub}")
            } else {
                cat.to_string()
            }
        }
    }
}

fn read_name_or_string(pkg: &Package, r: &mut Reader) -> Result<String> {
    if pkg.custom_ver(custom::framework()) >= custom::FW_PINS_STORE_FNAME {
        pkg.read_fname(r)
    } else {
        r.fstring()
    }
}

fn read_terminal_type(pkg: &Package, r: &mut Reader) -> Result<TerminalType> {
    let category = read_name_or_string(pkg, r)?;
    let sub_category = read_name_or_string(pkg, r)?;
    let sub_category_object = r.i32()?;
    r.bool32()?; // bTerminalIsConst
    r.bool32()?; // bTerminalIsWeakPointer
    if pkg.custom_ver(custom::release()) >= custom::REL_PIN_TYPE_INCLUDES_UOBJECT_WRAPPER_FLAG {
        r.bool32()?;
    }
    Ok(TerminalType { category, sub_category, sub_category_object })
}

pub fn read_pin_type(pkg: &Package, r: &mut Reader) -> Result<PinType> {
    if pkg.ue4() < ue4::EDGRAPHPINTYPE_SERIALIZATION {
        return invalid("pin type serialization too old");
    }
    let mut t = PinType { category: read_name_or_string(pkg, r)?, sub_category: read_name_or_string(pkg, r)?, ..Default::default() };
    if pkg.ue4() < ue4::ADDED_SOFT_OBJECT_PATH {
        if t.category == "asset" {
            t.category = "softobject".into();
        } else if t.category == "assetclass" {
            t.category = "softclass".into();
        }
    }
    t.sub_category_object = r.i32()?;
    if pkg.custom_ver(custom::framework()) >= custom::FW_EDGRAPH_PIN_CONTAINER_TYPE {
        t.container = r.u8()?;
        if t.container == 3 {
            t.value_type = Some(read_terminal_type(pkg, r)?);
        }
    } else {
        let mut is_map = false;
        let mut is_set = false;
        if pkg.custom_ver(custom::blueprints()) >= custom::BP_ADVANCED_CONTAINER_SUPPORT {
            is_map = r.bool32()?;
            if is_map {
                t.value_type = Some(read_terminal_type(pkg, r)?);
            }
            is_set = r.bool32()?;
        }
        let is_array = r.bool32()?;
        t.container = if is_array { 1 } else if is_set { 2 } else if is_map { 3 } else { 0 };
    }
    t.is_reference = r.bool32()?;
    t.is_weak = r.bool32()?;
    if pkg.ue4() >= ue4::MEMBERREFERENCE_IN_PINTYPE {
        t.member_parent = r.i32()?;
        t.member_name = pkg.read_fname(r)?;
        t.member_guid = r.guid()?;
    }
    if pkg.ue4() >= ue4::SERIALIZE_PINTYPE_CONST {
        t.is_const = r.bool32()?;
    }
    if pkg.custom_ver(custom::release()) >= custom::REL_PIN_TYPE_INCLUDES_UOBJECT_WRAPPER_FLAG {
        r.bool32()?;
    }
    if pkg.custom_ver(custom::ue5_release()) >= custom::UE5R_SERIALIZE_FLOAT_PIN_DEFAULT_VALUES_AS_SINGLE_PRECISION {
        r.bool32()?;
    }
    if t.category == "float" || t.category == "double" {
        t.sub_category = t.category.clone();
        t.category = "real".into();
    }
    Ok(t)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PinRef {
    pub node: i32,
    pub pin: Guid,
}

#[derive(Debug, Clone)]
pub struct Pin {
    pub owning_node: i32,
    pub id: Guid,
    pub name: String,
    pub friendly_name: Option<Text>,
    pub tooltip: String,
    /// 0 input, 1 output
    pub direction: u8,
    pub pin_type: PinType,
    pub default_value: String,
    pub autogenerated_default: String,
    pub default_object: i32,
    pub default_text: Option<Text>,
    pub linked_to: Vec<PinRef>,
    pub sub_pins: Vec<PinRef>,
    pub parent_pin: Option<PinRef>,
    pub hidden: bool,
    pub not_connectable: bool,
    pub advanced: bool,
    pub orphaned: bool,
}

impl Pin {
    pub fn is_exec(&self) -> bool {
        self.pin_type.category == "exec"
    }
    pub fn is_output(&self) -> bool {
        self.direction == 1
    }
    /// The default value as a display string (None when nothing is set).
    pub fn default_display(&self, pkg: &Package) -> Option<String> {
        if self.default_object != 0 {
            return Some(pkg.ref_string(self.default_object));
        }
        if let Some(t) = &self.default_text {
            let d = t.display();
            if !d.is_empty() {
                return Some(format!("\"{d}\""));
            }
        }
        if !self.default_value.is_empty() {
            return Some(self.default_value.clone());
        }
        None
    }
}

fn read_pin_ref(r: &mut Reader) -> Result<Option<PinRef>> {
    let null = r.bool32()?;
    if null {
        return Ok(None);
    }
    let node = r.i32()?;
    let pin = r.guid()?;
    Ok(Some(PinRef { node, pin }))
}

fn read_pin_ref_array(r: &mut Reader) -> Result<Vec<PinRef>> {
    let n = r.count(4)?;
    let mut v = vec![];
    for _ in 0..n {
        if let Some(p) = read_pin_ref(r)? {
            v.push(p);
        }
    }
    Ok(v)
}

fn read_pin(pkg: &Package, r: &mut Reader) -> Result<Pin> {
    let owning_node = r.i32()?;
    let id = r.guid()?;
    let name = read_name_or_string(pkg, r)?;
    let mut friendly_name = None;
    if !pkg.is_filter_editor_only() {
        let t = read_text(pkg, r)?;
        if !t.display().is_empty() {
            friendly_name = Some(t);
        }
    }
    if pkg.custom_ver(custom::ue5_main()) >= custom::UE5M_EDGRAPH_PIN_SOURCE_INDEX {
        r.i32()?; // SourceIndex
    }
    let tooltip = r.fstring()?;
    let direction = r.u8()?;
    let pin_type = read_pin_type(pkg, r)?;
    let default_value = r.fstring()?;
    let autogenerated_default = r.fstring()?;
    let default_object = r.i32()?;
    let dt = read_text(pkg, r)?;
    let default_text = if dt.display().is_empty() { None } else { Some(dt) };
    let linked_to = read_pin_ref_array(r)?;
    let sub_pins = read_pin_ref_array(r)?;
    let parent_pin = read_pin_ref(r)?;
    let _ref_passthrough = read_pin_ref(r)?;
    let mut bits = 0u32;
    if !pkg.is_filter_editor_only() {
        r.guid()?; // PersistentGuid
        bits = r.u32()?;
    }
    Ok(Pin {
        owning_node,
        id,
        name,
        friendly_name,
        tooltip,
        direction,
        pin_type,
        default_value,
        autogenerated_default,
        default_object,
        default_text,
        linked_to,
        sub_pins,
        parent_pin,
        hidden: bits & 1 != 0,
        not_connectable: bits & 2 != 0,
        advanced: bits & 16 != 0,
        orphaned: bits & 32 != 0,
    })
}

/// Reads the owning-node pin array that follows UEdGraphNode tagged properties.
pub fn read_node_pins(pkg: &Package, r: &mut Reader, self_index: i32) -> Result<Vec<Pin>> {
    let n = r.count(4)?;
    let mut pins = vec![];
    for _ in 0..n {
        let null = r.bool32()?;
        if null {
            continue;
        }
        let owner = r.i32()?;
        let _guid = r.guid()?;
        if owner != self_index {
            return invalid(format!("pin owner {owner} does not match node {self_index}"));
        }
        let at = r.pos();
        let pin = read_pin(pkg, r).map_err(|e| crate::errors::ParseError::Invalid(format!("pin #{} at offset {at:#x}: {e}", pins.len())))?;
        pins.push(pin);
    }
    Ok(pins)
}

/// Whether node pins are stored natively for this package.
pub fn pins_are_native(pkg: &Package) -> bool {
    pkg.custom_ver(custom::blueprints()) >= custom::BP_EDGRAPH_PIN_OPTIMIZED
}
