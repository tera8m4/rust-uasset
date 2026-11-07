//! Minimal JSON writer for the `--json` output.

use std::fmt::Write as _;

use super::Options;
use crate::movie_scene::{Bound, BoundKind, Channel, Ease, FrameRange, FrameRate, Key, Section, Track, WidgetAnimation};
use crate::object::{Extra, Object};
use crate::package::Package;
use crate::property::{Property, Value};
use crate::reader::hex;
use crate::text::Text;

pub fn quote(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 2);
    o.push('"');
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\r' => o.push_str("\\r"),
            '\t' => o.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(o, "\\u{:04x}", c as u32);
            }
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

fn num(v: f64) -> String {
    if v.is_finite() { format!("{v}") } else { quote(&format!("{v}")) }
}

struct J<'a> {
    pkg: &'a Package,
}

impl J<'_> {
    fn obj_ref(&self, i: i32) -> String {
        if i == 0 { "null".into() } else { quote(&self.pkg.ref_string(i)) }
    }

    fn text(&self, t: &Text) -> String {
        match t.key() {
            Some((ns, k)) => format!("{{\"text\":{},\"namespace\":{},\"key\":{}}}", quote(&t.display()), quote(ns), quote(k)),
            None => quote(&t.display()),
        }
    }

    fn value(&self, v: &Value) -> String {
        match v {
            Value::Bool(b) => b.to_string(),
            Value::Int(i) => i.to_string(),
            Value::UInt(i) => i.to_string(),
            Value::Float(f) => if f.is_finite() { format!("{f}") } else { quote(&format!("{f}")) },
            Value::Double(f) => num(*f),
            Value::Str(s) | Value::Name(s) | Value::Enum(s) | Value::SoftObject(s) => quote(s),
            Value::Text(t) => self.text(t),
            Value::Object(i) => self.obj_ref(*i),
            Value::Guid(g) => quote(&g.to_string()),
            Value::Struct(sv) => {
                let mut s = String::from("{");
                if !sv.type_name.is_empty() && !sv.native {
                    let _ = write!(s, "\"$type\":{},", quote(&sv.type_name));
                }
                s.push_str(&self.props_body(&sv.fields));
                if s.ends_with(',') {
                    s.pop();
                }
                s.push('}');
                s
            }
            Value::Array(a) | Value::Set(a) => {
                let parts: Vec<String> = a.iter().map(|x| self.value(x)).collect();
                format!("[{}]", parts.join(","))
            }
            Value::Map(m) => {
                let parts: Vec<String> = m.iter().map(|(k, x)| format!("{{\"key\":{},\"value\":{}}}", self.value(k), self.value(x))).collect();
                format!("[{}]", parts.join(","))
            }
            Value::Delegate { object, function } => format!("{{\"object\":{},\"function\":{}}}", self.obj_ref(*object), quote(function)),
            Value::MulticastDelegate(v) => {
                let parts: Vec<String> = v.iter().map(|(o, f)| format!("{{\"object\":{},\"function\":{}}}", self.obj_ref(*o), quote(f))).collect();
                format!("[{}]", parts.join(","))
            }
            Value::FieldPath { path, owner } => format!("{{\"owner\":{},\"path\":{}}}", self.obj_ref(*owner), quote(&path.join("."))),
            Value::Optional(None) => "null".into(),
            Value::Optional(Some(b)) => self.value(b),
            Value::Raw { type_name, bytes, note } => format!(
                "{{\"$undecoded\":{},\"size\":{},\"note\":{},\"hex\":{}}}",
                quote(type_name),
                bytes.len(),
                quote(note),
                quote(&hex(&bytes[..bytes.len().min(256)]))
            ),
        }
    }

    fn props_body(&self, props: &[Property]) -> String {
        let mut s = String::new();
        for p in props {
            let _ = write!(s, "{}:{},", quote(&p.display_name()), self.value(&p.value));
        }
        s
    }

    fn props(&self, props: &[Property]) -> String {
        let mut b = self.props_body(props);
        if b.ends_with(',') {
            b.pop();
        }
        format!("{{{b}}}")
    }

    fn object(&self, o: &Object) -> String {
        let e = &self.pkg.exports[o.index as usize - 1];
        let mut s = String::from("{");
        let _ = write!(s, "\"index\":{},", o.index);
        let _ = write!(s, "\"name\":{},", quote(&e.object_name));
        let _ = write!(s, "\"path\":{},", quote(&self.pkg.export_rel_path(o.index)));
        let _ = write!(s, "\"class\":{},", quote(&o.class));
        if e.outer_index != 0 {
            let _ = write!(s, "\"outer\":{},", self.obj_ref(e.outer_index));
        }
        if e.template_index != 0 {
            let _ = write!(s, "\"template\":{},", self.obj_ref(e.template_index));
        }
        let _ = write!(s, "\"properties\":{}", self.props(&o.properties));
        for x in &o.extras {
            match x {
                Extra::DataTable(rows) => {
                    let parts: Vec<String> = rows.iter().map(|(n, p)| format!("{}:{}", quote(n), self.props(p))).collect();
                    let _ = write!(s, ",\"rows\":{{{}}}", parts.join(","));
                }
                Extra::Pins(pins) => {
                    let parts: Vec<String> = pins
                        .iter()
                        .map(|p| {
                            let links: Vec<String> = p.linked_to.iter().map(|l| format!("{{\"node\":{},\"pin\":{}}}", self.obj_ref(l.node), quote(&l.pin.to_string()))).collect();
                            format!(
                                "{{\"id\":{},\"name\":{},\"direction\":{},\"type\":{},\"default\":{},\"links\":[{}]{}}}",
                                quote(&p.id.to_string()),
                                quote(&p.name),
                                quote(if p.is_output() { "out" } else { "in" }),
                                quote(&p.pin_type.display(self.pkg)),
                                p.default_display(self.pkg).map(|d| quote(&d)).unwrap_or("null".into()),
                                links.join(","),
                                if p.hidden { ",\"hidden\":true" } else { "" }
                            )
                        })
                        .collect();
                    let _ = write!(s, ",\"pins\":[{}]", parts.join(","));
                }
                Extra::MetaData(md) => {
                    let objs: Vec<String> = md
                        .objects
                        .iter()
                        .map(|(o, kv)| {
                            let kvs: Vec<String> = kv.iter().map(|(k, v)| format!("{}:{}", quote(k), quote(v))).collect();
                            format!("{}:{{{}}}", quote(o), kvs.join(","))
                        })
                        .collect();
                    let _ = write!(s, ",\"metadata\":{{{}}}", objs.join(","));
                }
                Extra::Struct(si) => {
                    let fields: Vec<String> = si
                        .fields
                        .iter()
                        .map(|f| format!("{{\"name\":{},\"kind\":{},\"flags\":{}}}", quote(&f.name), quote(&f.kind), f.flags))
                        .collect();
                    let _ = write!(s, ",\"super\":{},\"fields\":[{}]", self.obj_ref(si.super_struct), fields.join(","));
                    if let Some(ff) = si.function_flags {
                        let _ = write!(s, ",\"functionFlags\":{}", quote(&super::function_flags(ff)));
                    }
                    if !si.defaults.is_empty() {
                        let _ = write!(s, ",\"defaults\":{}", self.props(&si.defaults));
                    }
                }
                Extra::Enum(names) => {
                    let parts: Vec<String> = names.iter().map(|(n, v)| format!("{}:{}", quote(n), v)).collect();
                    let _ = write!(s, ",\"enumValues\":{{{}}}", parts.join(","));
                }
                Extra::Redirector(i) => {
                    let _ = write!(s, ",\"destination\":{}", self.obj_ref(*i));
                }
                Extra::UserPins(pins) => {
                    let parts: Vec<String> = pins
                        .iter()
                        .map(|p| {
                            format!(
                                "{{\"name\":{},\"direction\":{},\"type\":{},\"default\":{}}}",
                                quote(&p.name),
                                quote(if p.direction == 1 { "out" } else { "in" }),
                                quote(&p.pin_type.display(self.pkg)),
                                quote(&p.default_value)
                            )
                        })
                        .collect();
                    let _ = write!(s, ",\"userPins\":[{}]", parts.join(","));
                }
                Extra::PureState(_) => {}
            }
        }
        if !o.errors.is_empty() {
            let errs: Vec<String> = o.errors.iter().map(|e| quote(e)).collect();
            let _ = write!(s, ",\"errors\":[{}]", errs.join(","));
        }
        if o.undecoded > 0 {
            let _ = write!(s, ",\"undecodedBytes\":{}", o.undecoded);
        }
        s.push('}');
        s
    }
}

fn opt_str(v: Option<&str>) -> String {
    v.map(quote).unwrap_or_else(|| "null".into())
}

fn frame_json(rate: FrameRate, frame: i32) -> String {
    format!("\"frame\":{},\"seconds\":{}", frame, num(rate.seconds(frame)))
}

fn bound_json(rate: FrameRate, b: Bound) -> String {
    let kind = match b.kind {
        BoundKind::Exclusive => "exclusive",
        BoundKind::Inclusive => "inclusive",
        BoundKind::Open => return "{\"type\":\"open\"}".into(),
    };
    format!("{{\"type\":{},{}}}", quote(kind), frame_json(rate, b.frame))
}

fn range_json(rate: FrameRate, r: Option<FrameRange>) -> String {
    match r {
        Some(r) => format!("{{\"lower\":{},\"upper\":{}}}", bound_json(rate, r.lower), bound_json(rate, r.upper)),
        None => "null".into(),
    }
}

fn rate_json(r: FrameRate) -> String {
    format!("{{\"numerator\":{},\"denominator\":{}}}", r.numerator, r.denominator)
}

impl J<'_> {
    fn key(&self, rate: FrameRate, k: &Key) -> String {
        let mut s = format!("{{{},\"value\":{}", frame_json(rate, k.frame), self.value(&k.value));
        if let Some(c) = &k.curve {
            let _ = write!(
                s,
                ",\"interp\":{},\"tangentMode\":{},\"tangentWeightMode\":{},\"arriveTangent\":{},\"leaveTangent\":{},\"arriveTangentWeight\":{},\"leaveTangentWeight\":{}",
                quote(&c.interp),
                quote(&c.tangent_mode),
                quote(&c.weight_mode),
                num(c.arrive as f64),
                num(c.leave as f64),
                num(c.arrive_weight as f64),
                num(c.leave_weight as f64)
            );
        }
        s.push('}');
        s
    }

    fn channel(&self, rate: FrameRate, c: &Channel) -> String {
        let keys: Vec<String> = c.keys.iter().map(|k| self.key(rate, k)).collect();
        format!(
            "{{\"name\":{},\"type\":{},\"preInfinity\":{},\"postInfinity\":{},\"default\":{},\"enum\":{},\"keys\":[{}]}}",
            quote(&c.name),
            quote(&c.type_name),
            opt_str(c.pre_infinity.as_deref()),
            opt_str(c.post_infinity.as_deref()),
            c.default.as_ref().map(|d| self.value(d)).unwrap_or_else(|| "null".into()),
            opt_str(c.enum_name.as_deref()),
            keys.join(",")
        )
    }

    fn ease(&self, rate: FrameRate, e: Option<&Ease>) -> String {
        match e {
            Some(e) => format!("{{\"duration\":{},\"seconds\":{},\"function\":{}}}", e.duration, num(rate.seconds(e.duration)), opt_str(e.function.as_deref())),
            None => "null".into(),
        }
    }

    fn section(&self, rate: FrameRate, s: &Section) -> String {
        let channels: Vec<String> = s.channels.iter().map(|c| self.channel(rate, c)).collect();
        format!(
            "{{\"class\":{},\"range\":{},\"easeIn\":{},\"easeOut\":{},\"channels\":[{}],\"properties\":{}}}",
            quote(&s.class),
            range_json(rate, s.range),
            self.ease(rate, s.ease_in.as_ref()),
            self.ease(rate, s.ease_out.as_ref()),
            channels.join(","),
            self.props(&s.extra_props)
        )
    }

    fn track(&self, rate: FrameRate, t: &Track) -> String {
        let sections: Vec<String> = t.sections.iter().map(|s| self.section(rate, s)).collect();
        format!(
            "{{\"class\":{},\"property\":{},\"displayName\":{},\"sections\":[{}]}}",
            quote(&t.class),
            opt_str(t.property_path.as_deref()),
            opt_str(t.display_name.as_deref()),
            sections.join(",")
        )
    }

    fn animation(&self, a: &WidgetAnimation) -> String {
        let scene = &a.scene;
        let rate = scene.tick_resolution;
        let tracks = |ts: &[Track]| ts.iter().map(|t| self.track(rate, t)).collect::<Vec<_>>().join(",");
        let bindings: Vec<String> = scene
            .bindings
            .iter()
            .map(|b| {
                format!(
                    "{{\"guid\":{},\"name\":{},\"slot\":{},\"isRoot\":{},\"class\":{},\"parent\":{},\"tracks\":[{}]}}",
                    quote(&b.guid.to_string()),
                    quote(&b.name),
                    opt_str(b.slot.as_deref()),
                    b.is_root,
                    opt_str(b.class.as_deref()),
                    b.parent.map(|p| quote(&p.to_string())).unwrap_or_else(|| "null".into()),
                    tracks(&b.tracks)
                )
            })
            .collect();
        let marks: Vec<String> = scene.marked_frames.iter().map(|(f, l)| format!("{{{},\"label\":{}}}", frame_json(rate, *f), quote(l))).collect();
        format!(
            "{{\"name\":{},\"displayLabel\":{},\"tickResolution\":{},\"displayRate\":{},\"playbackRange\":{},\"durationSeconds\":{},\"markedFrames\":[{}],\"bindings\":[{}],\"tracks\":[{}]}}",
            quote(&a.name),
            opt_str(a.display_label.as_deref()),
            rate_json(rate),
            rate_json(scene.display_rate),
            range_json(rate, scene.playback_range),
            scene.duration().map(num).unwrap_or_else(|| "null".into()),
            marks.join(","),
            bindings.join(","),
            tracks(&scene.tracks)
        )
    }
}

pub fn render(pkg: &Package, objs: &[Object], file: &str) -> String {
    render_with(pkg, objs, file, &Options::default())
}

pub fn render_with(pkg: &Package, objs: &[Object], file: &str, opts: &Options) -> String {
    let j = J { pkg };
    let s = &pkg.summary;
    let mut out = String::from("{");
    let _ = write!(out, "\"file\":{},", quote(file));
    let _ = write!(out, "\"package\":{},", quote(&pkg.package_path()));
    let _ = write!(out, "\"savedBy\":{},", quote(&s.saved_by.to_string()));
    let _ = write!(out, "\"fileVersionUE4\":{},\"fileVersionUE5\":{},", s.file_version_ue4, s.file_version_ue5);
    let _ = write!(out, "\"packageFlags\":{},", s.package_flags);
    let imports: Vec<String> = (0..pkg.imports.len())
        .map(|i| {
            let idx = -(i as i32) - 1;
            format!("{{\"index\":{},\"path\":{},\"class\":{}}}", idx, quote(&pkg.obj_path(idx)), quote(&pkg.imports[i].class_name))
        })
        .collect();
    let _ = write!(out, "\"imports\":[{}],", imports.join(","));
    let reg: Vec<String> = pkg
        .asset_registry
        .iter()
        .map(|e| {
            let tags: Vec<String> = e.tags.iter().map(|(k, v)| format!("{}:{}", quote(k), quote(v))).collect();
            format!("{{\"object\":{},\"class\":{},\"tags\":{{{}}}}}", quote(&e.object_path), quote(&e.object_class), tags.join(","))
        })
        .collect();
    let _ = write!(out, "\"assetRegistry\":[{}],", reg.join(","));
    if let Some(md) = &pkg.metadata {
        let objs: Vec<String> = md
            .objects
            .iter()
            .map(|(o, kv)| {
                let kvs: Vec<String> = kv.iter().map(|(k, v)| format!("{}:{}", quote(k), quote(v))).collect();
                format!("{}:{{{}}}", quote(o), kvs.join(","))
            })
            .collect();
        let _ = write!(out, "\"metadata\":{{{}}},", objs.join(","));
    }
    if opts.show_animations() {
        let anims: Vec<String> = crate::movie_scene::widget_animations(pkg, objs).iter().map(|a| j.animation(a)).collect();
        let _ = write!(out, "\"widgetAnimations\":[{}],", anims.join(","));
    }
    let exports: Vec<String> = objs.iter().map(|o| j.object(o)).collect();
    let _ = write!(out, "\"exports\":[{}]", exports.join(","));
    out.push('}');
    out
}
