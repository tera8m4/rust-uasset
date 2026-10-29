//! Natively serialized structs (immutable core types and structs with custom Serialize).

use crate::errors::Result;
use crate::property::{PropReader, Property, StructValue, Value};
use crate::reader::Reader;
use crate::versions::custom;

/// Core math structs that are always binary serialized.
const CORE_IMMUTABLE: &[&str] = &[
    "Vector", "Vector3d", "Vector3f", "Vector2D", "Vector2d", "Vector2f", "Vector4", "Vector4d", "Vector4f",
    "Rotator", "Rotator3d", "Rotator3f", "Quat", "Quat4d", "Quat4f", "Plane", "Plane4d", "Plane4f", "Box",
    "Box3d", "Box3f", "Box2D", "Box2d", "Box2f", "Sphere", "Sphere3d", "Sphere3f", "TwoVectors", "Matrix",
    "Matrix44d", "Matrix44f", "Transform3d", "Transform3f", "Color", "LinearColor", "IntPoint", "Int32Point",
    "Int64Point", "UintPoint", "Uint32Point", "Uint64Point", "IntVector", "Int32Vector", "Int64Vector",
    "UintVector", "Uint32Vector", "Uint64Vector", "IntVector2", "Int32Vector2", "Int64Vector2", "UintVector2",
    "Uint32Vector2", "Uint64Vector2", "IntVector4", "Int32Vector4", "Int64Vector4", "UintVector4",
    "Uint32Vector4", "Uint64Vector4", "IntRect", "Int32Rect", "Int64Rect", "UintRect", "Uint32Rect",
    "Uint64Rect", "Guid", "DateTime", "Timespan", "OrientedBox", "PackedNormal", "PackedRGB10A2N",
    "PackedRGBA16N",
];

/// Structs with custom native serialization (extracted from TStructOpsTypeTraits WithSerializer in UE 5.8),
/// used to decide tagged vs. native for packages older than UE 5.4.
const KNOWN_NATIVE: &[&str] = &[
    "AIDataProviderTypedValue", "AlphaBlend", "AlphaBlendArgs", "AnimCurveBase", "AnimNotifyEvent",
    "AnimSyncMarker", "AnimationAttributeIdentifier", "AttributeCurve", "BindableValueBase", "BoolAnimProperty",
    "ColorMaterialInput", "CompressedRichCurve", "ConstraintInstance", "CustomAttribute", "DateTime",
    "EdGraphPinType", "EnumAnimProperty", "ExpressionInput", "FloatAnimProperty", "FloatCurve", "FontCharacter",
    "FontData", "FontOutlineSettings", "FrameNumber", "FrameRate", "FrameTime", "GameplayEffectModifierMagnitude",
    "GameplayEffectVersion", "GameplayTagContainer", "HierarchicalSimplification", "InputAlphaBoolBlend",
    "InstancedPropertyBag", "InstancedStruct", "InstancedStructContainer", "KeyHandleLookupTable", "KeyHandleMap",
    "LensBloomSettings", "LevelSequenceObjectReferenceMap", "MaterialAttributesInput", "MaterialCachedExpressionData",
    "MaterialOverrideNanite", "MovieSceneDoubleChannel", "MovieSceneDoubleValue", "MovieSceneEvalTemplatePtr",
    "MovieSceneEvaluationFieldEntityTree", "MovieSceneEvaluationKey", "MovieSceneEventParameters",
    "MovieSceneFloatChannel", "MovieSceneFloatValue", "MovieSceneFrameRange", "MovieSceneKeyHandleMap",
    "MovieSceneSegment", "MovieSceneSegmentIdentifier", "MovieSceneSequenceID", "MovieSceneSubSequenceTree",
    "MovieSceneTangentData", "MovieSceneTrackIdentifier", "MovieSceneTrackImplementationPtr", "NameCurveKey",
    "NavAgentSelector", "NiagaraDataInterfaceGPUParamInfo", "NiagaraTypeDefinition", "NiagaraVariable",
    "NiagaraVariableBase", "NiagaraVariableWithOffset", "PerPlatformBool", "PerPlatformFloat",
    "PerPlatformFrameRate", "PerPlatformInt", "PerQualityLevelFloat", "PerQualityLevelInt", "PostProcessSettings",
    "RawAnimSequenceTrack", "RichCurveKey", "ScalarMaterialInput", "ShadingModelMaterialInput", "SimpleCurveKey",
    "SmartName", "SoftClassPath", "SoftObjectPath", "StringAssetReference", "StringClassReference",
    "SoftClassPtr", "StringCurveKey", "SubstrateMaterialInput", "Timespan", "TransformCurve", "UniqueNetIdRepl",
    "Vector2MaterialInput", "VectorCurve", "VectorMaterialInput", "WorldConditionQueryDefinition",
    "StateTreeInstanceData", "StateTreeReference", "PCGPoint", "MidiEvent", "Timeline", "DeprecateSlateVector2D",
    "WorldPartitionActorFilter",
];

pub fn is_known_native(name: &str) -> bool {
    CORE_IMMUTABLE.contains(&name) || KNOWN_NATIVE.contains(&name)
}

/// Structs whose layout changed from tagged to native in some engine version.
pub fn may_be_tagged(name: &str) -> bool {
    !CORE_IMMUTABLE.contains(&name)
}

pub fn has_native_parser(name: &str) -> bool {
    CORE_IMMUTABLE.contains(&name)
        || matches!(
            name,
            "FrameNumber" | "FrameRate" | "FrameTime" | "SoftObjectPath" | "SoftClassPath" | "StringAssetReference"
                | "StringClassReference" | "GameplayTagContainer" | "PerPlatformFloat" | "PerPlatformInt"
                | "PerPlatformBool" | "PerPlatformFrameRate" | "PerQualityLevelFloat" | "PerQualityLevelInt"
                | "RichCurveKey" | "SimpleCurveKey" | "ExpressionInput" | "MaterialAttributesInput"
                | "ColorMaterialInput" | "ScalarMaterialInput" | "ShadingModelMaterialInput"
                | "SubstrateMaterialInput" | "VectorMaterialInput" | "Vector2MaterialInput" | "NavAgentSelector"
                | "InstancedStruct" | "EdGraphPinType" | "NameCurveKey" | "StringCurveKey" | "FontCharacter"
                | "DeprecateSlateVector2D"
        )
}

fn sv(name: &str, fields: Vec<(&str, Value)>) -> Value {
    Value::Struct(Box::new(StructValue {
        type_name: name.to_string(),
        fields: fields.into_iter().map(|(n, v)| Property::new(n, v)).collect(),
        native: true,
    }))
}

struct N<'a, 'b> {
    r: &'a mut Reader<'b>,
    lwc: bool,
}

impl N<'_, '_> {
    fn real(&mut self) -> Result<Value> {
        Ok(if self.lwc { Value::Double(self.r.f64()?) } else { Value::Float(self.r.f32()?) })
    }
    fn f(&mut self) -> Result<Value> {
        Ok(Value::Float(self.r.f32()?))
    }
    fn reals(&mut self, names: &[&'static str], double: bool) -> Result<Vec<(&'static str, Value)>> {
        let mut v = vec![];
        for n in names {
            v.push((*n, if double { self.real()? } else { self.f()? }));
        }
        Ok(v)
    }
}

const XYZ: &[&str] = &["X", "Y", "Z"];
const XY: &[&str] = &["X", "Y"];
const XYZW: &[&str] = &["X", "Y", "Z", "W"];
const PYR: &[&str] = &["Pitch", "Yaw", "Roll"];

/// Reads a native struct. Returns Ok(None) when no parser exists for this struct.
pub fn read_native(pr: &PropReader, r: &mut Reader, name: &str, size: Option<usize>, depth: u32) -> Result<Option<Value>> {
    let pkg = pr.pkg;
    let lwc = pkg.lwc();
    let mut n = N { r, lwc };
    // `d`: whether the struct uses `real` (double under LWC) components
    let v = match name {
        "Vector" | "Vector3d" => sv(name, n.reals(XYZ, true)?),
        "Vector3f" => sv(name, n.reals(XYZ, false)?),
        "Vector2D" | "Vector2d" => sv(name, n.reals(XY, true)?),
        "Vector2f" | "DeprecateSlateVector2D" => sv(name, n.reals(XY, false)?),
        "FontCharacter" => {
            let mut f = vec![];
            for c in ["StartU", "StartV", "USize", "VSize"] {
                f.push((c, Value::Int(n.r.i32()? as i64)));
            }
            f.push(("TextureIndex", Value::UInt(n.r.u8()? as u64)));
            f.push(("VerticalOffset", Value::Int(n.r.i32()? as i64)));
            sv(name, f)
        }
        "Vector4" | "Vector4d" => sv(name, n.reals(XYZW, true)?),
        "Vector4f" => sv(name, n.reals(XYZW, false)?),
        "Rotator" | "Rotator3d" => sv(name, n.reals(PYR, true)?),
        "Rotator3f" => sv(name, n.reals(PYR, false)?),
        "Quat" | "Quat4d" | "Plane" | "Plane4d" => sv(name, n.reals(XYZW, true)?),
        "Quat4f" | "Plane4f" => sv(name, n.reals(XYZW, false)?),
        "Box" | "Box3d" | "Box3f" | "Box2D" | "Box2d" | "Box2f" => {
            let d = !name.ends_with('f');
            let comps = if name.starts_with("Box2") { XY } else { XYZ };
            let min = sv("Vector", n.reals(comps, d)?);
            let max = sv("Vector", n.reals(comps, d)?);
            let valid = Value::Bool(n.r.u8()? != 0);
            sv(name, vec![("Min", min), ("Max", max), ("IsValid", valid)])
        }
        "Sphere" | "Sphere3d" | "Sphere3f" => {
            let d = !name.ends_with('f');
            let c = sv("Vector", n.reals(XYZ, d)?);
            let w = if d { n.real()? } else { n.f()? };
            sv(name, vec![("Center", c), ("W", w)])
        }
        "TwoVectors" => {
            let a = sv("Vector", n.reals(XYZ, true)?);
            let b = sv("Vector", n.reals(XYZ, true)?);
            sv(name, vec![("v1", a), ("v2", b)])
        }
        "Matrix" | "Matrix44d" | "Matrix44f" => {
            let d = name != "Matrix44f";
            let mut rows = vec![];
            for row in ["XPlane", "YPlane", "ZPlane", "WPlane"] {
                rows.push((row, sv("Plane", n.reals(XYZW, d)?)));
            }
            sv(name, rows)
        }
        "Transform3d" | "Transform3f" => {
            let d = name == "Transform3d";
            let rot = sv("Quat", n.reals(XYZW, d)?);
            let tr = sv("Vector", n.reals(XYZ, d)?);
            let sc = sv("Vector", n.reals(XYZ, d)?);
            sv(name, vec![("Rotation", rot), ("Translation", tr), ("Scale3D", sc)])
        }
        "OrientedBox" => {
            let mut f = vec![];
            for c in ["Center", "AxisX", "AxisY", "AxisZ"] {
                f.push((c, sv("Vector", n.reals(XYZ, true)?)));
            }
            for c in ["ExtentX", "ExtentY", "ExtentZ"] {
                f.push((c, n.real()?));
            }
            sv(name, f)
        }
        "Color" => {
            let b = n.r.u8()?;
            let g = n.r.u8()?;
            let rr = n.r.u8()?;
            let a = n.r.u8()?;
            sv(name, vec![("R", Value::UInt(rr as u64)), ("G", Value::UInt(g as u64)), ("B", Value::UInt(b as u64)), ("A", Value::UInt(a as u64))])
        }
        "LinearColor" => sv(name, n.reals(&["R", "G", "B", "A"], false)?),
        "PackedNormal" | "PackedRGB10A2N" => sv(name, vec![("Packed", Value::UInt(n.r.u32()? as u64))]),
        "PackedRGBA16N" => sv(name, vec![("Packed", Value::UInt(n.r.u64()?))]),
        "IntPoint" | "Int32Point" | "IntVector2" | "Int32Vector2" => int_fields(&mut n, name, XY, 4, true)?,
        "Int64Point" | "Int64Vector2" => int_fields(&mut n, name, XY, 8, true)?,
        "UintPoint" | "Uint32Point" | "UintVector2" | "Uint32Vector2" => int_fields(&mut n, name, XY, 4, false)?,
        "Uint64Point" | "Uint64Vector2" => int_fields(&mut n, name, XY, 8, false)?,
        "IntVector" | "Int32Vector" => int_fields(&mut n, name, XYZ, 4, true)?,
        "Int64Vector" => int_fields(&mut n, name, XYZ, 8, true)?,
        "UintVector" | "Uint32Vector" => int_fields(&mut n, name, XYZ, 4, false)?,
        "Uint64Vector" => int_fields(&mut n, name, XYZ, 8, false)?,
        "IntVector4" | "Int32Vector4" => int_fields(&mut n, name, XYZW, 4, true)?,
        "Int64Vector4" => int_fields(&mut n, name, XYZW, 8, true)?,
        "UintVector4" | "Uint32Vector4" => int_fields(&mut n, name, XYZW, 4, false)?,
        "Uint64Vector4" => int_fields(&mut n, name, XYZW, 8, false)?,
        "IntRect" | "Int32Rect" | "Int64Rect" | "UintRect" | "Uint32Rect" | "Uint64Rect" => {
            let w = if name.contains("64") { 8 } else { 4 };
            let signed = name.starts_with("Int");
            let min = int_fields(&mut n, "IntPoint", XY, w, signed)?;
            let max = int_fields(&mut n, "IntPoint", XY, w, signed)?;
            sv(name, vec![("Min", min), ("Max", max)])
        }
        "Guid" => Value::Guid(n.r.guid()?),
        "DateTime" | "Timespan" => sv(name, vec![("Ticks", Value::Int(n.r.i64()?))]),
        "FrameNumber" => sv(name, vec![("Value", Value::Int(n.r.i32()? as i64))]),
        "FrameRate" => {
            let a = n.r.i32()?;
            let b = n.r.i32()?;
            sv(name, vec![("Numerator", Value::Int(a as i64)), ("Denominator", Value::Int(b as i64))])
        }
        "FrameTime" => {
            let f = n.r.i32()?;
            let s = n.r.f32()?;
            sv(name, vec![("FrameNumber", Value::Int(f as i64)), ("SubFrame", Value::Float(s))])
        }
        "SoftObjectPath" | "SoftClassPath" | "StringAssetReference" | "StringClassReference" => {
            Value::SoftObject(pkg.read_soft_object_path(n.r)?)
        }
        "GameplayTagContainer" => {
            let c = n.r.count(8)?;
            let mut tags = vec![];
            for _ in 0..c {
                tags.push(Value::Name(pkg.read_fname(n.r)?));
            }
            Value::Array(tags)
        }
        "PerPlatformFloat" | "PerPlatformInt" | "PerPlatformBool" | "PerPlatformFrameRate" | "PerQualityLevelFloat"
        | "PerQualityLevelInt" => {
            let cooked = n.r.bool32()?;
            let read_v = |n: &mut N| -> Result<Value> {
                Ok(match name {
                    "PerPlatformFloat" | "PerQualityLevelFloat" => Value::Float(n.r.f32()?),
                    "PerPlatformInt" | "PerQualityLevelInt" => Value::Int(n.r.i32()? as i64),
                    "PerPlatformBool" => Value::Bool(n.r.bool32()?),
                    _ => {
                        let a = n.r.i32()?;
                        let b = n.r.i32()?;
                        Value::Str(format!("{a}/{b}"))
                    }
                })
            };
            let default = read_v(&mut n)?;
            let mut fields = vec![("Default", default)];
            let quality = name.starts_with("PerQuality");
            if quality || !cooked {
                let c = n.r.count(4)?;
                let mut m = vec![];
                for _ in 0..c {
                    let k = if quality { Value::Int(n.r.i32()? as i64) } else { Value::Name(pkg.read_fname(n.r)?) };
                    let v = read_v(&mut n)?;
                    m.push((k, v));
                }
                if !m.is_empty() {
                    fields.push((if quality { "PerQuality" } else { "PerPlatform" }, Value::Map(m)));
                }
            }
            sv(name, fields)
        }
        "RichCurveKey" => {
            let interp = n.r.u8()?;
            let tangent_mode = n.r.u8()?;
            let weight_mode = n.r.u8()?;
            let mut f = vec![
                ("InterpMode", Value::Enum(["RCIM_Linear", "RCIM_Constant", "RCIM_Cubic", "RCIM_None"].get(interp as usize).unwrap_or(&"?").to_string())),
                ("TangentMode", Value::UInt(tangent_mode as u64)),
                ("TangentWeightMode", Value::UInt(weight_mode as u64)),
            ];
            for c in ["Time", "Value", "ArriveTangent", "ArriveTangentWeight", "LeaveTangent", "LeaveTangentWeight"] {
                f.push((c, n.f()?));
            }
            sv(name, f)
        }
        "SimpleCurveKey" => sv(name, n.reals(&["Time", "Value"], false)?),
        "NameCurveKey" => {
            let t = n.f()?;
            sv(name, vec![("Time", t), ("Value", Value::Name(pkg.read_fname(n.r)?))])
        }
        "StringCurveKey" => {
            let t = n.f()?;
            sv(name, vec![("Time", t), ("Value", Value::Str(n.r.fstring()?))])
        }
        "NavAgentSelector" => sv(name, vec![("PackedBits", Value::UInt(n.r.u32()? as u64))]),
        "ExpressionInput" | "MaterialAttributesInput" | "ColorMaterialInput" | "ScalarMaterialInput"
        | "ShadingModelMaterialInput" | "SubstrateMaterialInput" | "VectorMaterialInput" | "Vector2MaterialInput" => {
            if pkg.custom_ver(custom::core()) < custom::CORE_MATERIAL_INPUT_NATIVE_SERIALIZE {
                return Ok(None);
            }
            let expr = n.r.i32()?;
            let output_index = n.r.i32()?;
            let input_name = if pkg.custom_ver(custom::framework()) >= custom::FW_PINS_STORE_FNAME {
                pkg.read_fname(n.r)?
            } else {
                n.r.fstring()?
            };
            let mask = n.r.i32()?;
            let mr = n.r.i32()?;
            let mg = n.r.i32()?;
            let mb = n.r.i32()?;
            let ma = n.r.i32()?;
            let mut f = vec![
                ("Expression", Value::Object(expr)),
                ("OutputIndex", Value::Int(output_index as i64)),
                ("InputName", Value::Name(input_name)),
                ("Mask", Value::Int(mask as i64)),
                ("MaskR", Value::Int(mr as i64)),
                ("MaskG", Value::Int(mg as i64)),
                ("MaskB", Value::Int(mb as i64)),
                ("MaskA", Value::Int(ma as i64)),
            ];
            if name != "ExpressionInput" && name != "MaterialAttributesInput" {
                let use_constant = n.r.bool32()?;
                let constant = match name {
                    "ColorMaterialInput" => {
                        if pkg.custom_ver(custom::fortnite_main()) < custom::FN_MATERIAL_INPUT_USES_LINEAR_COLOR {
                            read_native(pr, n.r, "Color", None, depth)?.unwrap()
                        } else {
                            sv("LinearColor", n.reals(&["R", "G", "B", "A"], false)?)
                        }
                    }
                    "ScalarMaterialInput" => n.f()?,
                    "ShadingModelMaterialInput" | "SubstrateMaterialInput" => Value::UInt(n.r.u32()? as u64),
                    "VectorMaterialInput" => sv("Vector3f", n.reals(XYZ, false)?),
                    _ => sv("Vector2f", n.reals(XY, false)?),
                };
                f.push(("UseConstant", Value::Bool(use_constant)));
                f.push(("Constant", constant));
            }
            sv(name, f)
        }
        "InstancedStruct" => {
            let _version = n.r.u8()?;
            let st = n.r.i32()?;
            let serial = n.r.i32()?;
            if serial < 0 || serial as usize > n.r.remaining() {
                return Ok(None);
            }
            let sname = pkg.obj_name(st);
            let start = n.r.pos();
            let inner = if serial == 0 {
                Value::Optional(None)
            } else {
                let mut sub = n.r.clone();
                match pr.read_struct(&mut sub, &sname, Some(serial as usize), None, depth + 1) {
                    Ok(v) if sub.pos() == start + serial as usize => v,
                    _ => Value::Raw { type_name: sname.clone(), bytes: n.r.data()[start..start + serial as usize].to_vec(), note: "instanced struct".into() },
                }
            };
            n.r.seek(start + serial as usize)?;
            sv(name, vec![("StructType", Value::Object(st)), ("Value", inner)])
        }
        "EdGraphPinType" => crate::graph::read_pin_type(pkg, n.r)?.to_value(),
        _ => return Ok(None),
    };
    let _ = size;
    Ok(Some(v))
}

fn int_fields(n: &mut N, name: &str, comps: &[&'static str], width: usize, signed: bool) -> Result<Value> {
    let mut f = vec![];
    for c in comps {
        let v = match (width, signed) {
            (4, true) => Value::Int(n.r.i32()? as i64),
            (4, false) => Value::UInt(n.r.u32()? as u64),
            (_, true) => Value::Int(n.r.i64()?),
            (_, false) => Value::UInt(n.r.u64()?),
        };
        f.push((*c, v));
    }
    Ok(sv(name, f))
}
