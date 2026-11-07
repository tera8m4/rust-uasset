//! MovieScene data of UMG widget animations: native channel serialization
//! (MovieSceneCurveChannelImpl.cpp, RangeBound.h) and a typed view over the exports of
//! `UWidgetAnimation -> UMovieScene -> tracks -> sections -> channels`.

use crate::errors::{Result, invalid};
use crate::object::Object;
use crate::package::Package;
use crate::property::{Property, StructValue, Value};
use crate::reader::{Guid, Reader};

const EXTRAPOLATION: &[&str] = &["RCCE_Cycle", "RCCE_CycleWithOffset", "RCCE_Oscillate", "RCCE_Linear", "RCCE_Constant", "RCCE_None"];
const INTERP_MODE: &[&str] = &["RCIM_Linear", "RCIM_Constant", "RCIM_Cubic", "RCIM_None"];
const TANGENT_MODE: &[&str] = &["RCTM_Auto", "RCTM_User", "RCTM_Break", "RCTM_None", "RCTM_SmartAuto"];
const TANGENT_WEIGHT_MODE: &[&str] = &["RCTWM_WeightedNone", "RCTWM_WeightedArrive", "RCTWM_WeightedLeave", "RCTWM_WeightedBoth"];
const BUILT_IN_EASING: &[&str] = &[
    "Linear", "SinIn", "SinOut", "SinInOut", "QuadIn", "QuadOut", "QuadInOut", "Cubic", "CubicIn", "CubicOut", "CubicInOut",
    "HermiteCubicInOut", "QuartIn", "QuartOut", "QuartInOut", "QuintIn", "QuintOut", "QuintInOut", "ExpoIn", "ExpoOut", "ExpoInOut",
    "CircIn", "CircOut", "CircInOut", "Custom",
];

/// sizeof(FMovieSceneFloatValue) / sizeof(FMovieSceneDoubleValue)
const FLOAT_VALUE_SIZE: i32 = 28;
const DOUBLE_VALUE_SIZE: i32 = 32;

fn native(name: &str, fields: Vec<(&str, Value)>) -> Value {
    Value::Struct(Box::new(StructValue {
        type_name: name.to_string(),
        fields: fields.into_iter().map(|(n, v)| Property::new(n, v)).collect(),
        native: true,
    }))
}

fn enum_value(names: &[&str], v: u8) -> Value {
    Value::Enum(names.get(v as usize).map(|s| s.to_string()).unwrap_or_else(|| v.to_string()))
}

fn frame_number(v: i32) -> Value {
    native("FrameNumber", vec![("Value", Value::Int(v as i64))])
}

fn read_bound(r: &mut Reader) -> Result<Value> {
    let start = r.pos();
    let ty = match r.u8()? {
        0 => "ERangeBoundTypes::Exclusive",
        1 => "ERangeBoundTypes::Inclusive",
        2 => "ERangeBoundTypes::Open",
        other => return invalid(format!("invalid range bound type {other} at {start}")),
    };
    let value = r.i32()?;
    Ok(native("FrameNumberRangeBound", vec![("Type", Value::Enum(ty.into())), ("Value", frame_number(value))]))
}

/// FMovieSceneFrameRange: TRange<FFrameNumber>. The bound value is stored even when the bound is open.
pub fn read_frame_range(r: &mut Reader) -> Result<Value> {
    let lower = read_bound(r)?;
    let upper = read_bound(r)?;
    Ok(native("MovieSceneFrameRange", vec![("LowerBound", lower), ("UpperBound", upper)]))
}

/// FMovieSceneFloatChannel / FMovieSceneDoubleChannel: times and values are dumped as raw arrays.
/// Returns Ok(None) with the reader untouched when the layout is not the expected one.
pub fn read_curve_channel(r: &mut Reader, type_name: &str, double: bool, has_show_curve: bool) -> Result<Option<Value>> {
    let mut t = r.clone();
    let pre = t.u8()?;
    let post = t.u8()?;
    if t.i32()? != 4 {
        return Ok(None);
    }
    let n = t.count(4)?;
    let mut times = Vec::with_capacity(n);
    for _ in 0..n {
        times.push(frame_number(t.i32()?));
    }
    let value_size = if double { DOUBLE_VALUE_SIZE } else { FLOAT_VALUE_SIZE };
    if t.i32()? != value_size {
        return Ok(None);
    }
    if t.count(value_size as usize)? != n {
        return Ok(None);
    }
    let mut values = Vec::with_capacity(n);
    for _ in 0..n {
        let value = if double { Value::Double(t.f64()?) } else { Value::Float(t.f32()?) };
        let arrive = t.f32()?;
        let leave = t.f32()?;
        let arrive_weight = t.f32()?;
        let leave_weight = t.f32()?;
        let weight_mode = t.u8()?;
        t.skip(3)?;
        let interp = t.u8()?;
        let tangent_mode = t.u8()?;
        t.skip(2)?;
        values.push(native(
            if double { "MovieSceneDoubleValue" } else { "MovieSceneFloatValue" },
            vec![
                ("Value", value),
                ("InterpMode", enum_value(INTERP_MODE, interp)),
                ("TangentMode", enum_value(TANGENT_MODE, tangent_mode)),
                ("TangentWeightMode", enum_value(TANGENT_WEIGHT_MODE, weight_mode)),
                ("ArriveTangent", Value::Float(arrive)),
                ("LeaveTangent", Value::Float(leave)),
                ("ArriveTangentWeight", Value::Float(arrive_weight)),
                ("LeaveTangentWeight", Value::Float(leave_weight)),
            ],
        ));
    }
    let default = if double { Value::Double(t.f64()?) } else { Value::Float(t.f32()?) };
    let has_default = t.bool32()?;
    let numerator = t.i32()?;
    let denominator = t.i32()?;
    let mut fields = vec![
        ("PreInfinityExtrap", enum_value(EXTRAPOLATION, pre)),
        ("PostInfinityExtrap", enum_value(EXTRAPOLATION, post)),
        ("Times", Value::Array(times)),
        ("Values", Value::Array(values)),
        ("DefaultValue", default),
        ("bHasDefaultValue", Value::Bool(has_default)),
        ("TickResolution", native("FrameRate", vec![("Numerator", Value::Int(numerator as i64)), ("Denominator", Value::Int(denominator as i64))])),
    ];
    if has_show_curve {
        fields.push(("bShowCurve", Value::Bool(t.bool32()?)));
    }
    *r = t;
    Ok(Some(native(type_name, fields)))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameRate {
    pub numerator: i32,
    pub denominator: i32,
}

impl Default for FrameRate {
    /// FFrameRate(): the value that a missing (delta serialized) property stands for.
    fn default() -> Self {
        Self { numerator: 60000, denominator: 1 }
    }
}

impl FrameRate {
    /// Frames per second, 0 for an invalid rate.
    pub fn fps(&self) -> f64 {
        if self.denominator == 0 { 0.0 } else { self.numerator as f64 / self.denominator as f64 }
    }

    /// Converts a frame number at this rate to seconds.
    pub fn seconds(&self, frame: i32) -> f64 {
        let fps = self.fps();
        if fps == 0.0 { 0.0 } else { frame as f64 / fps }
    }

    fn from_value(v: Option<&Value>) -> Self {
        let mut rate = Self::default();
        if let Some(s) = v.and_then(|v| v.as_struct()) {
            if let Some(n) = s.get("Numerator").and_then(|x| x.as_i64()) {
                rate.numerator = n as i32;
            }
            if let Some(d) = s.get("Denominator").and_then(|x| x.as_i64()) {
                rate.denominator = d as i32;
            }
        }
        rate
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoundKind {
    Exclusive,
    Inclusive,
    Open,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bound {
    pub kind: BoundKind,
    pub frame: i32,
}

impl Bound {
    fn from_value(v: Option<&Value>) -> Self {
        let s = v.and_then(|v| v.as_struct());
        let kind = match s.and_then(|s| s.get("Type")).and_then(|t| t.as_str()) {
            Some(t) if t.ends_with("Inclusive") => BoundKind::Inclusive,
            Some(t) if t.ends_with("Exclusive") => BoundKind::Exclusive,
            _ => BoundKind::Open,
        };
        Self { kind, frame: s.and_then(|s| s.get("Value")).and_then(frame_of).unwrap_or(0) }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameRange {
    pub lower: Bound,
    pub upper: Bound,
}

impl FrameRange {
    fn from_value(v: Option<&Value>) -> Option<Self> {
        let s = v?.as_struct()?;
        Some(Self { lower: Bound::from_value(s.get("LowerBound")), upper: Bound::from_value(s.get("UpperBound")) })
    }

    /// Length in frames, None when a bound is open.
    pub fn frame_count(&self) -> Option<i32> {
        if self.lower.kind == BoundKind::Open || self.upper.kind == BoundKind::Open {
            return None;
        }
        Some(self.upper.frame.wrapping_sub(self.lower.frame))
    }
}

/// Interpolation data of a float or double channel key. Tangents are stored per tick.
#[derive(Debug, Clone, PartialEq)]
pub struct KeyCurve {
    pub interp: String,
    pub tangent_mode: String,
    pub weight_mode: String,
    pub arrive: f32,
    pub leave: f32,
    pub arrive_weight: f32,
    pub leave_weight: f32,
}

#[derive(Debug, Clone)]
pub struct Key {
    /// Frame in the tick resolution of the movie scene.
    pub frame: i32,
    /// Stored value, `Value::Optional(None)` when the channel has fewer values than times.
    pub value: Value,
    pub curve: Option<KeyCurve>,
}

#[derive(Debug, Clone)]
pub struct Channel {
    pub name: String,
    /// e.g. "MovieSceneFloatChannel"
    pub type_name: String,
    pub pre_infinity: Option<String>,
    pub post_infinity: Option<String>,
    pub default: Option<Value>,
    /// Enum of a byte channel.
    pub enum_name: Option<String>,
    pub keys: Vec<Key>,
    /// Number of stored values; differs from `keys.len()` only in malformed data.
    pub value_count: usize,
}

impl Channel {
    /// A channel without keys or default value has no effect.
    pub fn is_animated(&self) -> bool {
        !self.keys.is_empty() || self.default.is_some()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Ease {
    /// Duration in ticks.
    pub duration: i32,
    /// Built-in easing name or the class of a custom easing function.
    pub function: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Section {
    /// 1-based export index.
    pub index: i32,
    pub class: String,
    /// None when the range is not stored (class default).
    pub range: Option<FrameRange>,
    pub ease_in: Option<Ease>,
    pub ease_out: Option<Ease>,
    pub channels: Vec<Channel>,
    /// Properties that are not channels, the range or the easing.
    pub extra_props: Vec<Property>,
}

#[derive(Debug, Clone)]
pub struct Track {
    /// 1-based export index.
    pub index: i32,
    pub class: String,
    /// Animated property, e.g. "RenderOpacity".
    pub property_path: Option<String>,
    pub display_name: Option<String>,
    pub sections: Vec<Section>,
    pub extra_props: Vec<Property>,
}

impl Track {
    pub fn title(&self) -> String {
        self.property_path.clone().or_else(|| self.display_name.clone()).unwrap_or_else(|| self.class.clone())
    }
}

/// A widget (or widget slot) animated by a movie scene.
#[derive(Debug, Clone)]
pub struct Binding {
    pub guid: Guid,
    pub name: String,
    /// The binding targets the slot of this widget.
    pub slot: Option<String>,
    pub is_root: bool,
    /// Class stored in the possessable.
    pub class: Option<String>,
    pub parent: Option<Guid>,
    pub tracks: Vec<Track>,
}

#[derive(Debug, Clone, Default)]
pub struct MovieScene {
    /// Resolution of every frame number in this movie scene.
    pub tick_resolution: FrameRate,
    pub display_rate: FrameRate,
    pub playback_range: Option<FrameRange>,
    /// (frame, label)
    pub marked_frames: Vec<(i32, String)>,
    pub bindings: Vec<Binding>,
    /// Tracks that are not bound to a widget, e.g. event tracks.
    pub tracks: Vec<Track>,
}

impl MovieScene {
    pub fn all_tracks(&self) -> impl Iterator<Item = &Track> {
        self.bindings.iter().flat_map(|b| b.tracks.iter()).chain(self.tracks.iter())
    }

    /// Playback length in seconds, None when the playback range is open or missing.
    pub fn duration(&self) -> Option<f64> {
        self.playback_range.and_then(|r| r.frame_count()).map(|n| self.tick_resolution.seconds(n))
    }
}

#[derive(Debug, Clone)]
pub struct WidgetAnimation {
    /// 1-based export index.
    pub index: i32,
    pub name: String,
    pub display_label: Option<String>,
    pub legacy_finish_on_stop: Option<bool>,
    pub scene: MovieScene,
}

fn frame_of(v: &Value) -> Option<i32> {
    match v {
        Value::Struct(s) => s.get("Value").and_then(|x| x.as_i64()).map(|x| x as i32),
        _ => v.as_i64().map(|x| x as i32),
    }
}

fn text_of(v: Option<&Value>) -> Option<String> {
    match v? {
        Value::Text(t) => Some(t.display()),
        other => other.as_str().map(|s| s.to_string()),
    }
    .filter(|s| !s.is_empty())
}

fn name_of(v: Option<&Value>) -> Option<String> {
    v.and_then(|x| x.as_str()).filter(|s| !s.is_empty() && *s != "None").map(|s| s.to_string())
}

fn guid_of(v: Option<&Value>) -> Guid {
    match v {
        Some(Value::Guid(g)) => *g,
        _ => Guid::default(),
    }
}

fn objects_of(v: Option<&Value>) -> Vec<i32> {
    match v {
        Some(Value::Array(a)) | Some(Value::Set(a)) => a.iter().filter_map(|x| x.as_object()).filter(|i| *i != 0).collect(),
        Some(Value::Object(i)) if *i != 0 => vec![*i],
        _ => vec![],
    }
}

fn export_of(objs: &[Object], index: i32) -> Option<&Object> {
    if index > 0 { objs.get(index as usize - 1) } else { None }
}

fn is_channel(s: &StructValue) -> bool {
    s.type_name.starts_with("MovieScene") && s.type_name.ends_with("Channel")
}

fn key_curve(s: &StructValue) -> Option<KeyCurve> {
    let text = |n: &str| s.get(n).and_then(|v| v.as_str()).unwrap_or("").to_string();
    let float = |n: &str| s.get(n).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
    s.get("InterpMode")?;
    Some(KeyCurve {
        interp: text("InterpMode"),
        tangent_mode: text("TangentMode"),
        weight_mode: text("TangentWeightMode"),
        arrive: float("ArriveTangent"),
        leave: float("LeaveTangent"),
        arrive_weight: float("ArriveTangentWeight"),
        leave_weight: float("LeaveTangentWeight"),
    })
}

fn build_channel(pkg: &Package, name: String, s: &StructValue) -> Channel {
    let list = |a: &str, b: &str| s.get(a).or_else(|| s.get(b)).and_then(|v| v.as_array()).unwrap_or(&[]).to_vec();
    let times = list("Times", "KeyTimes");
    let values = list("Values", "KeyValues");
    let mut keys = Vec::with_capacity(times.len());
    for (i, t) in times.iter().enumerate() {
        let Some(frame) = frame_of(t) else { continue };
        let (value, curve) = match values.get(i) {
            Some(Value::Struct(k)) if k.type_name.starts_with("MovieScene") && k.get("InterpMode").is_some() => {
                (k.get("Value").cloned().unwrap_or(Value::Float(0.0)), key_curve(k))
            }
            Some(v) => (v.clone(), None),
            None => (Value::Optional(None), None),
        };
        keys.push(Key { frame, value, curve });
    }
    // Delta serialization omits a zero default.
    let default = if s.get("bHasDefaultValue").and_then(|v| v.as_bool()) == Some(true) {
        Some(s.get("DefaultValue").cloned().unwrap_or(if s.type_name.contains("Bool") { Value::Bool(false) } else { Value::Int(0) }))
    } else {
        None
    };
    Channel {
        name,
        type_name: s.type_name.clone(),
        pre_infinity: s.get("PreInfinityExtrap").and_then(|v| v.as_str()).map(|x| x.to_string()),
        post_infinity: s.get("PostInfinityExtrap").and_then(|v| v.as_str()).map(|x| x.to_string()),
        default,
        enum_name: s.get("Enum").and_then(|v| v.as_object()).filter(|i| *i != 0).map(|i| pkg.obj_name(i)),
        keys,
        value_count: values.len(),
    }
}

/// Name of a channel property: `Scale[1]` of a 2D transform is `Scale.Y`, `RedCurve` of a parameter is `<param>.R`.
fn channel_name(p: &Property, section_class: &str, prefix: &str) -> String {
    let base = if section_class == "MovieScene2DTransformSection" && matches!(p.name.as_str(), "Translation" | "Scale" | "Shear") {
        format!("{}.{}", p.name, if p.array_index == 0 { "X" } else { "Y" })
    } else {
        p.display_name()
    };
    if prefix.is_empty() {
        return base;
    }
    match base.as_str() {
        "ParameterCurve" => prefix.to_string(),
        "XCurve" => format!("{prefix}.X"),
        "YCurve" => format!("{prefix}.Y"),
        "ZCurve" => format!("{prefix}.Z"),
        "RedCurve" => format!("{prefix}.R"),
        "GreenCurve" => format!("{prefix}.G"),
        "BlueCurve" => format!("{prefix}.B"),
        "AlphaCurve" => format!("{prefix}.A"),
        _ => format!("{prefix}.{base}"),
    }
}

fn channels_in_value(pkg: &Package, v: &Value, section_class: &str, name: &str, depth: u32) -> Vec<Channel> {
    if depth > 8 {
        return vec![];
    }
    match v {
        Value::Struct(s) if is_channel(s) => vec![build_channel(pkg, name.to_string(), s)],
        Value::Struct(s) => {
            let prefix = s.get("ParameterName").and_then(|x| x.as_str()).unwrap_or(name);
            s.fields.iter().flat_map(|f| channels_in_value(pkg, &f.value, section_class, &channel_name(f, section_class, prefix), depth + 1)).collect()
        }
        Value::Array(a) | Value::Set(a) => a.iter().flat_map(|x| channels_in_value(pkg, x, section_class, name, depth + 1)).collect(),
        _ => vec![],
    }
}

fn read_ease(pkg: &Package, objs: &[Object], easing: &StructValue, side: &str) -> Option<Ease> {
    let int = |n: String| easing.get(&n).and_then(|v| v.as_i64()).unwrap_or(0) as i32;
    let manual = easing.get(&format!("bManualEase{side}")).and_then(|v| v.as_bool()).unwrap_or(false);
    let duration = int(format!("{}Ease{side}Duration", if manual { "Manual" } else { "Auto" }));
    if duration == 0 {
        return None;
    }
    let function = easing.get(&format!("Ease{side}")).and_then(|v| v.as_object()).filter(|i| *i != 0).map(|i| {
        let class = pkg.class_name(i);
        if class != "MovieSceneBuiltInEasingFunction" {
            return class;
        }
        match export_of(objs, i).and_then(|o| o.prop("Type")) {
            Some(Value::Enum(e)) => e.rsplit("::").next().unwrap_or(e).to_string(),
            Some(v) => v.as_i64().and_then(|t| BUILT_IN_EASING.get(t as usize)).unwrap_or(&"?").to_string(),
            None => BUILT_IN_EASING[0].to_string(),
        }
    });
    Some(Ease { duration, function })
}

fn read_section(pkg: &Package, objs: &[Object], index: i32) -> Option<Section> {
    let o = export_of(objs, index)?;
    let mut s = Section {
        index,
        class: o.class.clone(),
        range: FrameRange::from_value(o.prop("SectionRange")),
        ease_in: None,
        ease_out: None,
        channels: vec![],
        extra_props: vec![],
    };
    if let Some(easing) = o.prop("Easing").and_then(|v| v.as_struct()) {
        s.ease_in = read_ease(pkg, objs, easing, "In");
        s.ease_out = read_ease(pkg, objs, easing, "Out");
    }
    for p in &o.properties {
        if matches!(p.name.as_str(), "SectionRange" | "Easing") {
            continue;
        }
        let channels = channels_in_value(pkg, &p.value, &o.class, &channel_name(p, &o.class, ""), 0);
        if channels.is_empty() {
            s.extra_props.push(p.clone());
        } else {
            s.channels.extend(channels);
        }
    }
    Some(s)
}

fn read_track(pkg: &Package, objs: &[Object], index: i32) -> Option<Track> {
    let o = export_of(objs, index)?;
    let binding = o.prop("PropertyBinding").and_then(|v| v.as_struct());
    let consumed = ["PropertyBinding", "Sections", "DisplayName"];
    Some(Track {
        index,
        class: o.class.clone(),
        property_path: name_of(binding.and_then(|b| b.get("PropertyPath"))),
        display_name: text_of(o.prop("DisplayName")),
        sections: objects_of(o.prop("Sections")).into_iter().filter_map(|i| read_section(pkg, objs, i)).collect(),
        extra_props: o.properties.iter().filter(|p| !consumed.contains(&p.name.as_str())).cloned().collect(),
    })
}

fn read_tracks(pkg: &Package, objs: &[Object], v: Option<&Value>) -> Vec<Track> {
    objects_of(v).into_iter().filter_map(|i| read_track(pkg, objs, i)).collect()
}

/// Reads a UMovieScene export. `widget_bindings` is `UWidgetAnimation::AnimationBindings`, if any.
pub fn read_movie_scene(pkg: &Package, objs: &[Object], index: i32, widget_bindings: &[Value]) -> Option<MovieScene> {
    let o = export_of(objs, index)?;
    let structs = |v: Option<&Value>| -> Vec<StructValue> {
        v.and_then(|x| x.as_array()).unwrap_or(&[]).iter().filter_map(|x| x.as_struct().cloned()).collect()
    };
    let possessables = structs(o.prop("Possessables"));
    let spawnables = structs(o.prop("Spawnables"));
    let widgets = structs(Some(&Value::Array(widget_bindings.to_vec())));

    let mut bindings = vec![];
    for b in structs(o.prop("ObjectBindings")) {
        let guid = guid_of(b.get("ObjectGuid"));
        let obj = possessables.iter().chain(spawnables.iter()).find(|p| guid_of(p.get("Guid")) == guid);
        let widget = widgets.iter().find(|w| guid_of(w.get("AnimationGuid")) == guid);
        let name = name_of(widget.and_then(|w| w.get("WidgetName")))
            .or_else(|| name_of(obj.and_then(|p| p.get("Name"))))
            .or_else(|| name_of(b.get("BindingName")))
            .unwrap_or_else(|| guid.to_string());
        let class = obj.and_then(|p| p.get("PossessedObjectClass")).and_then(|v| match v {
            Value::Object(i) if *i != 0 => Some(pkg.obj_name(*i)),
            Value::SoftObject(s) if !s.is_empty() => s.rsplit('.').next().map(|x| x.to_string()),
            _ => None,
        });
        bindings.push(Binding {
            guid,
            name,
            slot: name_of(widget.and_then(|w| w.get("SlotWidgetName"))),
            is_root: widget.and_then(|w| w.get("bIsRootWidget")).and_then(|v| v.as_bool()).unwrap_or(false),
            class,
            parent: Some(guid_of(obj.and_then(|p| p.get("ParentGuid")))).filter(|g| !g.is_zero()),
            tracks: read_tracks(pkg, objs, b.get("Tracks")),
        });
    }
    let mut marked_frames = vec![];
    for m in structs(o.prop("MarkedFrames")) {
        let frame = m.get("FrameNumber").and_then(frame_of).unwrap_or(0);
        marked_frames.push((frame, text_of(m.get("Label")).unwrap_or_default()));
    }
    let unbound = o.prop("Tracks").or_else(|| o.prop("MasterTracks"));
    Some(MovieScene {
        tick_resolution: FrameRate::from_value(o.prop("TickResolution")),
        display_rate: FrameRate::from_value(o.prop("DisplayRate")),
        playback_range: FrameRange::from_value(o.prop("PlaybackRange")),
        marked_frames,
        bindings,
        tracks: read_tracks(pkg, objs, unbound),
    })
}

/// Reads a UWidgetAnimation export.
pub fn read_widget_animation(pkg: &Package, objs: &[Object], index: i32) -> Option<WidgetAnimation> {
    let o = export_of(objs, index)?;
    let name = pkg.exports[index as usize - 1].object_name.clone();
    let widgets = o.prop("AnimationBindings").and_then(|v| v.as_array()).unwrap_or(&[]);
    let scene = o.prop("MovieScene").and_then(|v| v.as_object()).and_then(|i| read_movie_scene(pkg, objs, i, widgets)).unwrap_or_default();
    Some(WidgetAnimation {
        index,
        name: name.strip_suffix("_INST").map(|s| s.to_string()).unwrap_or(name),
        display_label: text_of(o.prop("DisplayLabel")),
        legacy_finish_on_stop: o.prop("bLegacyFinishOnStop").and_then(|v| v.as_bool()),
        scene,
    })
}

/// Exports listed in `Animations` of `owner`.
pub fn animation_exports(owner: &Object) -> Vec<i32> {
    objects_of(owner.prop("Animations"))
}

/// Animations of a widget blueprint. The generated class holds a compiled copy of the same data
/// and is only used when the blueprint does not list any.
pub fn widget_animations(pkg: &Package, objs: &[Object]) -> Vec<WidgetAnimation> {
    let listed = |o: &&Object| !animation_exports(o).is_empty();
    let owner = objs
        .iter()
        .filter(|o| !o.class.ends_with("GeneratedClass"))
        .find(listed)
        .or_else(|| objs.iter().filter(|o| o.class.ends_with("GeneratedClass")).find(listed));
    let Some(owner) = owner else { return vec![] };
    animation_exports(owner).into_iter().filter_map(|i| read_widget_animation(pkg, objs, i)).collect()
}
