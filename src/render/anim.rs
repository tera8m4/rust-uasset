//! UMG widget animation view.

use crate::movie_scene::{Bound, BoundKind, Channel, FrameRange, FrameRate, Key, MovieScene, Section, Track, WidgetAnimation};
use crate::property::{Property, Value};

use super::{Ctx, Out};

/// Editor bookkeeping of tracks and sections, only shown with `--verbose`.
const NOISE: &[&str] = &[
    "Signature", "EvaluationField", "EvaluationFieldGuid", "EvaluationFieldVersion", "TrackName", "UniqueTrackName", "BrushPropertyNamePath",
    "SortingOrder", "TrackTint", "SectionToKey",
];

const SLATE_VISIBILITY: &[&str] = &["Visible", "Collapsed", "Hidden", "HitTestInvisible", "SelfHitTestInvisible"];

/// Line width at which channel keys wrap.
const MAX_LINE: usize = 110;

fn number(v: f64) -> String {
    let s = format!("{v:.4}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() || s == "-0" { "0".into() } else { s.to_string() }
}

fn seconds(rate: FrameRate, frame: i32) -> String {
    format!("{}s", number(rate.seconds(frame)))
}

fn short_class(class: &str) -> &str {
    class.strip_prefix("MovieScene").unwrap_or(class)
}

fn short_enum<'a>(s: &'a str, prefix: &str) -> &'a str {
    s.strip_prefix(prefix).unwrap_or(s)
}

fn animated_channels(scene: &MovieScene) -> usize {
    scene.all_tracks().flat_map(|t| t.sections.iter()).flat_map(|s| s.channels.iter()).filter(|c| c.is_animated()).count()
}

fn duration_text(scene: &MovieScene) -> String {
    scene.duration().map(|d| format!("{}s", number(d))).unwrap_or_else(|| "open range".into())
}

/// Prints the animation list; keys only with `--animations`.
pub fn write_animations(c: &Ctx, out: &mut Out, anims: &[WidgetAnimation]) {
    if anims.is_empty() {
        return;
    }
    out.blank();
    if !c.opts.show_animations() {
        out.line(0, format!("### Widget animations ({}, keys shown with --animations)", anims.len()));
        for a in anims {
            let scene = &a.scene;
            out.line(
                0,
                format!(
                    "- {}: {}, {} bindings, {} tracks, {} channels",
                    a.name,
                    duration_text(scene),
                    scene.bindings.len(),
                    scene.all_tracks().count(),
                    animated_channels(scene)
                ),
            );
        }
        return;
    }
    out.line(0, format!("### Widget animations ({})", anims.len()));
    for a in anims {
        write_animation(c, out, a);
    }
}

fn write_animation(c: &Ctx, out: &mut Out, a: &WidgetAnimation) {
    let s = &a.scene;
    out.blank();
    let display = s.display_rate;
    let frames = match s.playback_range {
        Some(r) if r.frame_count().is_some() => {
            let at_display = |f: i32| number(s.tick_resolution.seconds(f) * display.fps());
            format!(", frames {}..{}", at_display(r.lower.frame), at_display(r.upper.frame))
        }
        _ => String::new(),
    };
    out.line(0, format!("#### {}: {}{frames} @ {} fps (tick {}/s)", a.name, duration_text(s), number(display.fps()), number(s.tick_resolution.fps())));
    if let Some(l) = a.display_label.as_ref().filter(|l| **l != a.name) {
        out.line(0, format!("DisplayLabel: {}", c.quote(l)));
    }
    if let Some(v) = a.legacy_finish_on_stop {
        out.line(0, format!("bLegacyFinishOnStop: {v}"));
    }
    if !s.marked_frames.is_empty() {
        let marks: Vec<String> = s.marked_frames.iter().map(|(f, l)| format!("{}={}", seconds(s.tick_resolution, *f), c.quote(l))).collect();
        out.line(0, format!("Marks: {}", marks.join(", ")));
    }
    for b in &s.bindings {
        // A slot binding's widget is often bound only as the parent, without tracks of its own.
        if b.tracks.is_empty() && !c.opts.verbose {
            continue;
        }
        let mut title = b.name.clone();
        if b.slot.is_some() {
            title.push_str(".Slot");
        }
        if let Some(class) = &b.class {
            title.push_str(&format!(" ({class})"));
        }
        if b.is_root {
            title.push_str(" [root]");
        }
        if c.opts.verbose {
            title.push_str(&format!(" guid={}", b.guid));
            if let Some(p) = &b.parent {
                title.push_str(&format!(" parent={p}"));
            }
        }
        out.line(0, format!("{title}:"));
        for t in &b.tracks {
            write_track(c, out, s, t);
        }
    }
    if !s.tracks.is_empty() {
        out.line(0, "Unbound:");
        for t in &s.tracks {
            write_track(c, out, s, t);
        }
    }
}

fn write_extras(c: &Ctx, out: &mut Out, indent: usize, props: &[Property]) {
    for p in props {
        let default_blend = p.name == "BlendType" && p.value.as_struct().is_some_and(|s| s.get("BlendType").is_none());
        // Visibility values are printed by name already.
        let visibility = p.name == "Enum" && p.value.as_object().is_some_and(|i| c.name(i) == "ESlateVisibility");
        if !c.opts.verbose && (NOISE.contains(&p.name.as_str()) || default_blend || visibility) {
            continue;
        }
        c.write_kv(out, indent, &p.display_name(), &p.value);
    }
}

fn write_track(c: &Ctx, out: &mut Out, s: &MovieScene, t: &Track) {
    out.line(1, format!("{} ({}):", t.title(), short_class(&t.class)));
    write_extras(c, out, 2, &t.extra_props);
    for sec in &t.sections {
        write_section(c, out, s, t, sec);
    }
}

fn bound_text(rate: FrameRate, b: Bound, lower: bool) -> String {
    match (b.kind, lower) {
        (BoundKind::Open, true) => "(-inf".into(),
        (BoundKind::Open, false) => "+inf)".into(),
        (BoundKind::Inclusive, true) => format!("[{}", seconds(rate, b.frame)),
        (BoundKind::Inclusive, false) => format!("{}]", seconds(rate, b.frame)),
        (BoundKind::Exclusive, true) => format!("({}", seconds(rate, b.frame)),
        (BoundKind::Exclusive, false) => format!("{})", seconds(rate, b.frame)),
    }
}

fn range_text(rate: FrameRate, r: FrameRange) -> String {
    format!("{}, {}", bound_text(rate, r.lower, true), bound_text(rate, r.upper, false))
}

fn write_section(c: &Ctx, out: &mut Out, s: &MovieScene, t: &Track, sec: &Section) {
    let rate = s.tick_resolution;
    let mut notes = vec![];
    for (label, ease) in [("ease in", &sec.ease_in), ("ease out", &sec.ease_out)] {
        if let Some(e) = ease {
            let f = e.function.as_ref().map(|f| format!(" {f}")).unwrap_or_default();
            notes.push(format!("{label} {}{f}", seconds(rate, e.duration)));
        }
    }
    let show_header = t.sections.len() > 1 || sec.range != s.playback_range && sec.range.is_some() || !notes.is_empty();
    let mut indent = 2;
    if show_header {
        let range = sec.range.map(|r| range_text(rate, r)).unwrap_or_else(|| "default range".into());
        let notes = if notes.is_empty() { String::new() } else { format!(", {}", notes.join(", ")) };
        out.line(2, format!("{} {range}{notes}:", short_class(&sec.class)));
        indent = 3;
    }
    for ch in &sec.channels {
        if ch.is_animated() || c.opts.verbose {
            write_channel(c, out, indent, rate, ch);
        }
    }
    write_extras(c, out, indent, &sec.extra_props);
}

fn write_channel(c: &Ctx, out: &mut Out, indent: usize, rate: FrameRate, ch: &Channel) {
    let mut title = ch.name.clone();
    let mut notes = vec![];
    for (label, extrap) in [("pre", &ch.pre_infinity), ("post", &ch.post_infinity)] {
        if let Some(e) = extrap.as_deref().filter(|e| c.opts.verbose || !e.ends_with("RCCE_Constant")) {
            notes.push(format!("{label}={}", short_enum(e, "RCCE_").to_lowercase()));
        }
    }
    if let Some(d) = &ch.default {
        notes.push(format!("default={}", value_text(c, ch, d)));
    }
    if ch.value_count != ch.keys.len() {
        notes.push(format!("!! {} times but {} values", ch.keys.len(), ch.value_count));
    }
    if !notes.is_empty() {
        title.push_str(&format!(" [{}]", notes.join(", ")));
    }
    if ch.keys.is_empty() {
        out.line(indent, if ch.default.is_some() { title } else { format!("{title}: no keys") });
        return;
    }
    let keys: Vec<String> = ch.keys.iter().map(|k| key_text(c, rate, ch, k)).collect();
    let joined = keys.join(", ");
    if indent * 2 + title.len() + 2 + joined.len() <= MAX_LINE {
        out.line(indent, format!("{title}: {joined}"));
    } else {
        out.line(indent, format!("{title}:"));
        for k in keys {
            out.line(indent + 1, format!("- {k}"));
        }
    }
}

fn key_text(c: &Ctx, rate: FrameRate, ch: &Channel, k: &Key) -> String {
    let mut s = seconds(rate, k.frame);
    if c.opts.verbose {
        s.push_str(&format!("(tick {})", k.frame));
    }
    s.push('=');
    s.push_str(&value_text(c, ch, &k.value));
    let Some(curve) = &k.curve else { return s };
    let interp = short_enum(&curve.interp, "RCIM_").to_lowercase();
    s.push(' ');
    s.push_str(&interp);
    let mut parts = vec![];
    let weighted = !curve.weight_mode.is_empty() && !curve.weight_mode.ends_with("WeightedNone");
    let manual = interp == "cubic" && matches!(curve.tangent_mode.as_str(), "RCTM_User" | "RCTM_Break");
    if c.opts.verbose {
        parts.push(short_enum(&curve.tangent_mode, "RCTM_").to_lowercase());
    }
    if c.opts.verbose || manual || weighted {
        parts.push(format!("arrive={}", Ctx::float(curve.arrive as f64)));
        parts.push(format!("leave={}", Ctx::float(curve.leave as f64)));
    }
    if c.opts.verbose || weighted {
        parts.push(format!("arriveWeight={}", Ctx::float(curve.arrive_weight as f64)));
        parts.push(format!("leaveWeight={}", Ctx::float(curve.leave_weight as f64)));
    }
    if !parts.is_empty() {
        s.push_str(&format!("({})", parts.join(", ")));
    }
    s
}

fn value_text(c: &Ctx, ch: &Channel, v: &Value) -> String {
    if ch.enum_name.as_deref() == Some("ESlateVisibility") {
        if let Some(name) = v.as_i64().and_then(|i| SLATE_VISIBILITY.get(i as usize)) {
            return name.to_string();
        }
    }
    if let Value::Struct(s) = v {
        // FMovieSceneEvent: the function to call
        if let Some(f) = s.get("Ptrs").and_then(|p| p.as_struct()).and_then(|p| p.get("Function")).and_then(|f| f.as_object()) {
            return c.name(f);
        }
        // FMovieSceneObjectPathChannelKeyValue
        if let Some(hard) = s.get("HardPtr") {
            let soft = s.get("SoftPtr").filter(|p| p.as_str().is_some_and(|x| !x.is_empty()));
            return c.inline(soft.unwrap_or(hard), 200).unwrap_or_else(|| "?".into());
        }
    }
    if matches!(v, Value::Optional(None)) {
        return "<no value>".into();
    }
    c.inline(v, 200).unwrap_or_else(|| "<...>".into())
}
