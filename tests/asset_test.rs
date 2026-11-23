use std::collections::HashSet;

use rust_uasset::movie_scene;
use rust_uasset::object::Extra;
use rust_uasset::property::{Value, clean_field_name};
use rust_uasset::reader::Reader;
use rust_uasset::render::Options;
use rust_uasset::{Asset, guess_package_path};

mod common;
use common::test_data_path;

fn rows(asset: &Asset) -> &[(String, Vec<rust_uasset::property::Property>)] {
    asset
        .objects
        .iter()
        .find_map(|o| o.extras.iter().find_map(|e| if let Extra::DataTable(r) = e { Some(r.as_slice()) } else { None }))
        .expect("data table rows")
}

fn assert_fully_decoded(asset: &Asset) {
    for o in &asset.objects {
        assert!(o.errors.is_empty(), "export {} has errors: {:?}", o.index, o.errors);
        assert_eq!(o.undecoded, 0, "export {} ({}) has undecoded bytes", o.index, o.class);
    }
}

#[test]
fn data_table_ue54() {
    let asset = Asset::open(&test_data_path("test_table_ue54.uasset")).unwrap();
    assert_eq!(asset.package.exports.len(), 3);
    assert_eq!(asset.package.names.len(), 30);
    assert_fully_decoded(&asset);
    assert_eq!(rows(&asset).len(), 2);
}

#[test]
fn data_table_values() {
    let asset = Asset::open(&test_data_path("DT_MyTable1.uasset")).unwrap();
    assert_fully_decoded(&asset);
    let rows = rows(&asset);
    assert_eq!(rows[0].0, "NewRow1");
    let tag = rows[0].1.iter().find(|p| p.name == "Tag").unwrap();
    // FGameplayTagTableRow::Tag is an FName
    assert!(matches!(&tag.value, Value::Name(s) if s == "Tag.Foo.Bar"), "{:?}", tag.value);
    let comment = rows[0].1.iter().find(|p| p.name == "DevComment").unwrap();
    assert!(matches!(&comment.value, Value::Str(s) if s == "Comment2"));
}

#[test]
fn data_table_ue57() {
    let asset = Asset::open(&test_data_path("DT_UE57.uasset")).unwrap();
    assert_eq!(asset.package.ue5(), 1018);
    assert_fully_decoded(&asset);
    let rows = rows(&asset);
    assert_eq!(rows.len(), 4);
    // FName numbers are rendered UE style: number 1 => "_0"
    assert_eq!(rows[1].0, "NewRow_0");
}

#[test]
fn text_output() {
    let asset = Asset::open(&test_data_path("DT_MyTable1.uasset")).unwrap();
    let text = asset.to_text(&Options::default());
    assert!(text.starts_with("# DT_MyTable1 (DataTable)"), "{text}");
    assert!(text.contains("RowStruct: /Script/GameplayTags.GameplayTagTableRow"), "{text}");
    assert!(text.contains("- NewRow1: {Tag: Tag.Foo.Bar, DevComment: \"Comment2\"}"), "{text}");
}

#[test]
fn json_output() {
    let asset = Asset::open(&test_data_path("DT_MyTable1.uasset")).unwrap();
    let json = asset.to_json("DT_MyTable1.uasset");
    assert!(json.starts_with('{') && json.ends_with('}'));
    assert!(json.contains("\"rows\":{\"NewRow1\":"), "{json}");
}

#[test]
fn not_a_package() {
    assert!(Asset::from_bytes(b"definitely not an unreal package".to_vec()).is_err());
    assert!(Asset::from_bytes(vec![]).is_err());
}

#[test]
fn helpers() {
    assert_eq!(clean_field_name("Health_12_0123456789ABCDEF0123456789ABCDEF"), "Health");
    assert_eq!(clean_field_name("Plain_Name"), "Plain_Name");
    assert_eq!(
        guess_package_path(std::path::Path::new("D:/Proj/Content/Maps/BP_Door.uasset")).as_deref(),
        Some("/Game/Maps/BP_Door")
    );
}

/// Serialized float (28 byte) or double (32 byte) channel with one key per `(frame, value)`.
fn curve_channel_bytes(keys: &[(i32, f64)], double: bool, value_size: i32, show_curve: bool) -> Vec<u8> {
    let mut b = vec![3u8, 4u8]; // RCCE_Linear, RCCE_Constant
    b.extend(4i32.to_le_bytes());
    b.extend((keys.len() as i32).to_le_bytes());
    for (frame, _) in keys {
        b.extend(frame.to_le_bytes());
    }
    b.extend(value_size.to_le_bytes());
    b.extend((keys.len() as i32).to_le_bytes());
    for (_, value) in keys {
        if double {
            b.extend(value.to_le_bytes());
        } else {
            b.extend((*value as f32).to_le_bytes());
        }
        for tangent in [0.5f32, 0.25, 0.0, 0.0] {
            b.extend(tangent.to_le_bytes());
        }
        b.extend([0, 0, 0, 0]); // RCTWM_WeightedNone + padding
        b.extend([2, 1, 0, 0]); // RCIM_Cubic, RCTM_User + padding
    }
    if double {
        b.extend(7.0f64.to_le_bytes());
    } else {
        b.extend(7.0f32.to_le_bytes());
    }
    b.extend(1u32.to_le_bytes()); // bHasDefaultValue
    b.extend(60000i32.to_le_bytes());
    b.extend(1i32.to_le_bytes());
    if show_curve {
        b.extend(1u32.to_le_bytes());
    }
    b
}

fn field<'a>(v: &'a Value, name: &str) -> &'a Value {
    v.as_struct().and_then(|s| s.get(name)).unwrap_or_else(|| panic!("no field {name} in {v:?}"))
}

#[test]
fn movie_scene_frame_range() {
    let mut bytes = vec![1u8];
    bytes.extend((-365i32).to_le_bytes());
    bytes.push(0);
    bytes.extend(30000i32.to_le_bytes());
    let mut r = Reader::new(&bytes);
    let v = movie_scene::read_frame_range(&mut r).unwrap();
    assert_eq!(r.pos(), 10);
    let (lower, upper) = (field(&v, "LowerBound"), field(&v, "UpperBound"));
    assert_eq!(field(lower, "Type").as_str(), Some("ERangeBoundTypes::Inclusive"));
    assert_eq!(field(field(lower, "Value"), "Value").as_i64(), Some(-365));
    assert_eq!(field(upper, "Type").as_str(), Some("ERangeBoundTypes::Exclusive"));
    assert_eq!(field(field(upper, "Value"), "Value").as_i64(), Some(30000));

    // Open bounds still store a value.
    let mut open = vec![2u8];
    open.extend(0i32.to_le_bytes());
    open.push(2);
    open.extend(0i32.to_le_bytes());
    let mut r = Reader::new(&open);
    let v = movie_scene::read_frame_range(&mut r).unwrap();
    assert_eq!(r.pos(), 10);
    assert_eq!(field(field(&v, "UpperBound"), "Type").as_str(), Some("ERangeBoundTypes::Open"));

    let mut r = Reader::new(&[9u8, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    assert!(movie_scene::read_frame_range(&mut r).is_err());
}

#[test]
fn movie_scene_float_channel() {
    for show_curve in [false, true] {
        let bytes = curve_channel_bytes(&[(0, 1.0), (30000, 2.5)], false, 28, show_curve);
        assert_eq!(bytes.len(), 34 + 2 * 32 + if show_curve { 4 } else { 0 });
        let mut r = Reader::new(&bytes);
        let v = movie_scene::read_curve_channel(&mut r, "MovieSceneFloatChannel", false, show_curve).unwrap().unwrap();
        assert_eq!(r.pos(), bytes.len());
        assert_eq!(field(&v, "PreInfinityExtrap").as_str(), Some("RCCE_Linear"));
        assert_eq!(field(&v, "PostInfinityExtrap").as_str(), Some("RCCE_Constant"));
        assert_eq!(field(&v, "DefaultValue").as_f64(), Some(7.0));
        assert_eq!(field(&v, "bHasDefaultValue").as_bool(), Some(true));
        assert_eq!(field(field(&v, "TickResolution"), "Numerator").as_i64(), Some(60000));
        assert_eq!(field(&v, "Times").as_array().unwrap().len(), 2);
        let key = &field(&v, "Values").as_array().unwrap()[1];
        assert_eq!(field(key, "Value").as_f64(), Some(2.5));
        assert_eq!(field(key, "InterpMode").as_str(), Some("RCIM_Cubic"));
        assert_eq!(field(key, "TangentMode").as_str(), Some("RCTM_User"));
        assert_eq!(field(key, "ArriveTangent").as_f64(), Some(0.5));
        assert_eq!(field(key, "LeaveTangent").as_f64(), Some(0.25));
    }
}

#[test]
fn movie_scene_double_channel() {
    let bytes = curve_channel_bytes(&[(100, 0.125)], true, 32, true);
    let mut r = Reader::new(&bytes);
    let v = movie_scene::read_curve_channel(&mut r, "MovieSceneDoubleChannel", true, true).unwrap().unwrap();
    assert_eq!(r.pos(), bytes.len());
    let key = &field(&v, "Values").as_array().unwrap()[0];
    assert!(matches!(field(key, "Value"), Value::Double(d) if *d == 0.125));
    assert!(matches!(field(&v, "DefaultValue"), Value::Double(d) if *d == 7.0));
}

#[test]
fn movie_scene_empty_channel() {
    let bytes = curve_channel_bytes(&[], false, 28, true);
    let mut r = Reader::new(&bytes);
    let v = movie_scene::read_curve_channel(&mut r, "MovieSceneFloatChannel", false, true).unwrap().unwrap();
    assert_eq!(r.pos(), bytes.len());
    assert!(field(&v, "Times").as_array().unwrap().is_empty());
    assert!(field(&v, "Values").as_array().unwrap().is_empty());
}

#[test]
fn movie_scene_unexpected_value_size_is_rejected() {
    let bytes = curve_channel_bytes(&[(0, 1.0)], false, 24, true);
    let mut r = Reader::new(&bytes);
    assert!(movie_scene::read_curve_channel(&mut r, "MovieSceneFloatChannel", false, true).unwrap().is_none());
    assert_eq!(r.pos(), 0, "the reader must stay at the start of the channel");
}

#[test]
fn movie_scene_frame_rate() {
    let rate = movie_scene::FrameRate::default();
    assert_eq!((rate.numerator, rate.denominator), (60000, 1));
    assert_eq!(rate.seconds(30000), 0.5);
    assert_eq!(movie_scene::FrameRate { numerator: 30, denominator: 0 }.seconds(30), 0.0);
}

fn channel_keys(c: &movie_scene::Channel) -> Vec<(i32, Option<i64>)> {
    c.keys.iter().map(|k| (k.frame, k.value.as_i64())).collect()
}

#[test]
fn widget_animation_tracks() {
    let path = test_data_path("WBP_AnimTest.uasset");
    let asset = Asset::open(&path).unwrap();
    assert_fully_decoded(&asset);
    check_widget_animations(&path, &asset);

    let anims = asset.widget_animations();
    assert_eq!(anims.len(), 1);
    let anim = &anims[0];
    assert_eq!(anim.name, "SwitchImages");
    let scene = &anim.scene;
    assert_eq!(scene.tick_resolution, movie_scene::FrameRate { numerator: 60000, denominator: 1 });
    assert_eq!(scene.display_rate, movie_scene::FrameRate { numerator: 20, denominator: 1 });
    assert!((scene.duration().unwrap() - 1.0).abs() < 0.001, "{:?}", scene.duration());
    assert_eq!(scene.bindings.len(), 2);

    // A switcher's active index is an integer track bound to the widget.
    let switcher = scene.bindings.iter().find(|b| b.name == "Switcher").unwrap();
    assert_eq!(switcher.class.as_deref(), Some("WidgetSwitcher"));
    assert_eq!(switcher.tracks.len(), 1);
    let track = &switcher.tracks[0];
    assert_eq!(track.class, "MovieSceneIntegerTrack");
    assert_eq!(track.property_path.as_deref(), Some("ActiveWidgetIndex"));
    let channel = &track.sections[0].channels[0];
    assert_eq!(channel.name, "IntegerCurve");
    assert_eq!(channel_keys(channel), [(0, Some(0)), (30000, Some(1)), (60000, Some(2))]);

    let fader = scene.bindings.iter().find(|b| b.name == "Fader").unwrap();
    let opacity = fader.tracks.iter().find(|t| t.class == "MovieSceneFloatTrack").unwrap();
    assert_eq!(opacity.property_path.as_deref(), Some("RenderOpacity"));
    let keys = &opacity.sections[0].channels[0].keys;
    assert_eq!(keys.iter().map(|k| (k.frame, k.value.as_f64())).collect::<Vec<_>>(), [(0, Some(0.0)), (60000, Some(1.0))]);
    assert_eq!(keys[0].curve.as_ref().unwrap().interp, "RCIM_Cubic");

    let visibility = fader.tracks.iter().find(|t| t.class == "MovieSceneByteTrack").unwrap();
    let channel = &visibility.sections[0].channels[0];
    assert_eq!(channel.enum_name.as_deref(), Some("ESlateVisibility"));
    assert_eq!(channel_keys(channel), [(0, Some(0)), (60000, Some(1))]);

    let text = asset.to_text(&Options { animations: true, ..Options::default() });
    assert!(text.contains("IntegerCurve: 0s[f0]=0, 0.5s[f10]=1, 1s[f20]=2"), "{text}");
    assert!(text.contains("FloatCurve: 0s[f0]=0.0 cubic, 1s[f20]=1.0 cubic"), "{text}");
    assert!(text.contains("ByteCurve: 0s[f0]=Visible, 1s[f20]=Collapsed"), "{text}");

    let summary = asset.to_text(&Options::default());
    assert!(summary.contains("- SwitchImages: 1s, 2 bindings, 3 tracks, 3 channels"), "{summary}");
    assert!(!summary.contains("IntegerCurve"), "{summary}");
}

fn check_widget_animations(path: &std::path::Path, asset: &Asset) {
    let anims = asset.widget_animations();
    let default_text = asset.to_text(&Options::default());
    let anim_opts = Options { animations: true, ..Options::default() };
    let anim_text = asset.to_text(&anim_opts);
    assert!(!asset.to_json("x").contains("\"widgetAnimations\""), "{}", path.display());
    assert!(!anim_text.contains("!! decode error"), "{}", path.display());
    if anims.is_empty() {
        return;
    }
    let verbose_text = asset.to_text(&Options { verbose: true, ..Options::default() });
    assert!(asset.to_json_with("x", &anim_opts).contains("\"widgetAnimations\""), "{}", path.display());
    for text in [&default_text, &anim_text, &verbose_text] {
        assert!(!text.contains("<undecoded MovieScene"), "{}", path.display());
        // Neither the animations nor their compiled copies may be repeated as raw objects.
        assert!(!text.contains(" (WidgetAnimation)") && !text.contains(" (MovieScene)"), "{}", path.display());
    }
    let is_under = |index: i32, root: i32| {
        let mut cur = asset.package.exports[index as usize - 1].outer_index;
        while cur > 0 {
            if cur == root {
                return true;
            }
            cur = asset.package.exports[cur as usize - 1].outer_index;
        }
        false
    };
    for a in &anims {
        assert!(!default_text.contains(&format!("#### {}:", a.name)), "{}: keys in default output", path.display());
        assert!(anim_text.contains(&format!("#### {}:", a.name)), "{}: missing {}", path.display(), a.name);
        let mut tracks = HashSet::new();
        let mut sections = HashSet::new();
        for t in a.scene.all_tracks() {
            tracks.insert(t.index);
            for s in &t.sections {
                sections.insert(s.index);
                for c in &s.channels {
                    assert_eq!(c.keys.len(), c.value_count, "{}: {} in {}", path.display(), c.name, a.name);
                }
            }
        }
        for o in &asset.objects {
            if is_under(o.index, a.index) && o.class.starts_with("MovieScene") {
                if o.class.ends_with("Track") {
                    assert!(tracks.contains(&o.index), "{}: track {} not in model", path.display(), o.index);
                } else if o.class.ends_with("Section") {
                    assert!(sections.contains(&o.index), "{}: section {} not in model", path.display(), o.index);
                }
            }
        }
    }
}

/// Decodes every asset in `$UASSET_TEST_DIR` (if set) and requires that no export fails.
/// Use it with local test assets that must not be committed to the repository.
#[test]
fn local_asset_dir() {
    let Ok(dir) = std::env::var("UASSET_TEST_DIR") else { return };
    let mut count = 0;
    for entry in std::fs::read_dir(&dir).unwrap().flatten() {
        let path = entry.path();
        if !matches!(path.extension().and_then(|e| e.to_str()), Some("uasset") | Some("umap")) {
            continue;
        }
        let asset = Asset::open(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        for o in &asset.objects {
            assert!(o.errors.is_empty(), "{} export {}: {:?}", path.display(), o.index, o.errors);
        }
        let text = asset.to_text(&Options::default());
        assert!(!text.contains("!! decode error"), "{}", path.display());
        assert!(!text.contains("<cycle>") && !text.contains("<...>"), "{}", path.display());
        let nodes = asset.to_text(&Options { nodes: true, ..Options::default() });
        assert!(!nodes.contains("!! decode error"), "{}", path.display());
        check_widget_animations(&path, &asset);
        count += 1;
    }
    assert!(count > 0, "no assets in {dir}");
}

/// FInstancedStruct with and without the pre-custom-version u8 header:
/// struct type, serial size 8, body = "None" (empty tagged struct).
#[test]
fn instanced_struct_layouts() {
    use rust_uasset::property::PropReader;
    let asset = Asset::open(&test_data_path("WBP_AnimTest.uasset")).unwrap();
    let pkg = &asset.package;
    let none = pkg.names.iter().position(|n| n == "None").unwrap() as i32;
    let mut body = vec![];
    body.extend((-1i32).to_le_bytes());
    body.extend(8i32.to_le_bytes());
    body.extend(none.to_le_bytes());
    body.extend(0i32.to_le_bytes());
    for prefix in [vec![], vec![0u8]] {
        let bytes = [prefix.clone(), body.clone()].concat();
        let mut r = Reader::new(&bytes);
        let v = PropReader::new(pkg).read_struct(&mut r, "InstancedStruct", Some(bytes.len()), Some(true), 0).unwrap();
        assert_eq!(r.pos(), bytes.len(), "prefix {prefix:?}");
        assert!(matches!(field(&v, "StructType"), Value::Object(-1)));
        assert!(matches!(field(&v, "Value"), Value::Struct(s) if s.fields.is_empty()), "{v:?}");
    }
}

/// A section without a stored SectionRange uses the class default: [0, 0] for most sections, infinite for
/// those whose constructor says so (event trigger, bool, ...). Keys outside the range are never reached.
#[test]
fn section_default_range() {
    use movie_scene::{Channel, Key, Section};
    let key = |frame| Key { frame, value: Value::Float(1.0), curve: None };
    let section = |class: &str| Section {
        index: 1,
        class: class.into(),
        range: None,
        ease_in: None,
        ease_out: None,
        channels: vec![Channel {
            name: "Curve".into(),
            type_name: "MovieSceneFloatChannel".into(),
            pre_infinity: None,
            post_infinity: None,
            default: None,
            enum_name: None,
            keys: vec![key(0), key(6000)],
            value_count: 2,
        }],
        extra_props: vec![],
    };
    let float = section("MovieSceneFloatSection");
    assert!(float.effective_range().reaches(0));
    assert_eq!(float.keys_outside_range(), 1);
    let event = section("MovieSceneEventTriggerSection");
    assert_eq!(event.effective_range(), movie_scene::FrameRange::ALL);
    assert_eq!(event.keys_outside_range(), 0);
    assert_eq!(section("MovieSceneBoolSection").effective_range(), movie_scene::FrameRange::ALL);
    assert_eq!(section("MovieSceneEventRepeaterSection").keys_outside_range(), 1);
}

/// The blueprint's animations are compared with the compiled copies in the generated class.
#[test]
fn compiled_animation_differences() {
    let asset = Asset::open(&test_data_path("WBP_AnimTest.uasset")).unwrap();
    let (pkg, objs) = (&asset.package, &asset.objects);
    assert!(movie_scene::widget_animation_differences(pkg, objs).is_empty());

    let saved = asset.widget_animations();
    let mut compiled = saved.clone();
    let fader = compiled[0].scene.bindings.iter_mut().find(|b| b.name == "Fader").unwrap();
    let opacity = fader.tracks.iter_mut().find(|t| t.class == "MovieSceneFloatTrack").unwrap();
    opacity.sections[0].channels[0].keys[1].value = Value::Float(0.5);
    assert_eq!(movie_scene::compiled_differences(pkg, &saved, &compiled), ["SwitchImages: Fader.RenderOpacity"]);
    assert_eq!(movie_scene::compiled_differences(pkg, &saved, &[]), ["SwitchImages: missing from the compiled class"]);
    assert_eq!(movie_scene::compiled_differences(pkg, &[], &saved), ["SwitchImages: only in the compiled class"]);
}

fn fstring(s: &str) -> Vec<u8> {
    let mut b = ((s.len() + 1) as i32).to_le_bytes().to_vec();
    b.extend(s.as_bytes());
    b.push(0);
    b
}

/// Package for hand-built struct data, with the given custom versions overridden.
fn test_package(versions: &[(rust_uasset::reader::Guid, Option<i32>)]) -> rust_uasset::package::Package {
    let mut pkg = Asset::open(&test_data_path("WBP_AnimTest.uasset")).unwrap().package;
    for (g, v) in versions {
        pkg.set_custom_ver(*g, *v);
    }
    pkg
}

fn none_name(pkg: &rust_uasset::package::Package) -> Vec<u8> {
    let none = pkg.names.iter().position(|n| n == "None").unwrap() as i32;
    [none.to_le_bytes(), 0i32.to_le_bytes()].concat()
}

fn read_native_struct(pkg: &rust_uasset::package::Package, name: &str, bytes: &[u8]) -> Value {
    let mut r = Reader::new(bytes);
    let v = rust_uasset::property::PropReader::new(pkg).read_struct(&mut r, name, Some(bytes.len()), Some(true), 0).unwrap();
    assert_eq!(r.pos(), bytes.len(), "{name}: {v:?}");
    v
}

/// FFontData::Serialize: bIsCooked, then tagged properties (editor data) or the compact cooked layout.
#[test]
fn font_data_native() {
    use rust_uasset::versions::custom;
    let pkg = test_package(&[(custom::editor(), Some(custom::ED_ADDED_FONT_FACE_ASSETS))]);
    let v = read_native_struct(&pkg, "FontData", &[0u32.to_le_bytes().to_vec(), none_name(&pkg)].concat());
    assert!(matches!(&v, Value::Struct(s) if s.fields.is_empty() && !s.native), "{v:?}");

    let bytes = [1u32.to_le_bytes(), (-2i32).to_le_bytes(), 3i32.to_le_bytes()].concat();
    let v = read_native_struct(&pkg, "FontData", &bytes);
    assert!(matches!(field(&v, "FontFaceAsset"), Value::Object(-2)));
    assert!(matches!(field(&v, "SubFaceIndex"), Value::Int(3)));
    assert!(v.as_struct().unwrap().get("FontFilename").is_none());

    let bytes = [1u32.to_le_bytes().to_vec(), 0i32.to_le_bytes().to_vec(), fstring("a.ttf"), vec![1, 2], 0i32.to_le_bytes().to_vec()].concat();
    let v = read_native_struct(&pkg, "FontData", &bytes);
    assert!(matches!(field(&v, "FontFilename"), Value::Str(s) if s == "a.ttf"));
    assert!(matches!(field(&v, "Hinting"), Value::UInt(1)));
    assert!(matches!(field(&v, "LoadingPolicy"), Value::UInt(2)));

    // Before AddedFontFaceAssets: plain tagged properties.
    let old = test_package(&[(custom::editor(), Some(custom::ED_ADDED_FONT_FACE_ASSETS - 1))]);
    let v = read_native_struct(&old, "FontData", &none_name(&old));
    assert!(matches!(&v, Value::Struct(s) if s.fields.is_empty()), "{v:?}");
}

/// FMaterialOverrideNanite::Serialize: legacy native fields, or bSerializeAsCookedData (+ material) followed by
/// tagged properties.
#[test]
fn material_override_nanite_native() {
    use rust_uasset::versions::custom;
    let pkg = test_package(&[(custom::fortnite_release(), Some(custom::FNR_NANITE_MATERIAL_OVERRIDE_USES_EDITOR_ONLY))]);
    let v = read_native_struct(&pkg, "MaterialOverrideNanite", &[0u32.to_le_bytes().to_vec(), none_name(&pkg)].concat());
    assert!(matches!(&v, Value::Struct(s) if s.fields.is_empty()), "{v:?}");
    let bytes = [1u32.to_le_bytes().to_vec(), (-1i32).to_le_bytes().to_vec(), none_name(&pkg)].concat();
    let v = read_native_struct(&pkg, "MaterialOverrideNanite", &bytes);
    assert!(matches!(field(&v, "OverrideMaterial"), Value::Object(-1)));

    let mut old = test_package(&[(custom::fortnite_release(), None)]);
    old.soft_object_paths = vec!["/Game/M.M".into()];
    let bytes = [0i32.to_le_bytes(), 1u32.to_le_bytes(), (-3i32).to_le_bytes()].concat();
    let v = read_native_struct(&old, "MaterialOverrideNanite", &bytes);
    assert!(matches!(field(&v, "OverrideMaterialRef"), Value::SoftObject(s) if s == "/Game/M.M"));
    assert!(matches!(field(&v, "bEnableOverride"), Value::Bool(true)));
    assert!(matches!(field(&v, "OverrideMaterial"), Value::Object(-3)));
}

/// With FInstancedStructCustomVersion there is no version byte; without it an optional 0xABABABAB header
/// precedes the version byte.
#[test]
fn instanced_struct_custom_version() {
    use rust_uasset::versions::custom;
    let pkg = test_package(&[(custom::instanced_struct(), Some(custom::IS_CUSTOM_VERSION_ADDED))]);
    let body = [(-1i32).to_le_bytes().to_vec(), 8i32.to_le_bytes().to_vec(), none_name(&pkg)].concat();
    let v = read_native_struct(&pkg, "InstancedStruct", &body);
    assert!(matches!(field(&v, "Value"), Value::Struct(s) if s.fields.is_empty()), "{v:?}");
    // A version byte is not expected when the custom version is present.
    let v = read_native_struct(&pkg, "InstancedStruct", &[vec![0u8], body.clone()].concat());
    assert!(matches!(v, Value::Raw { .. }), "{v:?}");

    let legacy = test_package(&[(custom::instanced_struct(), None)]);
    let v = read_native_struct(&legacy, "InstancedStruct", &[vec![0xAB; 4], vec![0u8], body].concat());
    assert!(matches!(field(&v, "StructType"), Value::Object(-1)), "{v:?}");
}

/// UInterchangeBaseNode serializes only its FAttributeStorage (no tagged properties).
#[test]
fn interchange_attribute_storage() {
    use rust_uasset::versions::custom;
    let storage: Vec<u8> = [
        1.5f32.to_le_bytes().to_vec(),
        "hi\0".encode_utf16().flat_map(|c| c.to_le_bytes()).collect(),
        [1.0f32.to_le_bytes(), 2.0f32.to_le_bytes()].concat(),
        vec![1, 2, 3],
    ]
    .concat();
    // key, offset, size, EAttributeTypes
    let table = [("F", 0u64, 4u64, 8i32), ("S", 4, 6, 18), ("A", 10, 8, 63), ("X", 18, 3, 17)];
    for multi in [true, false] {
        let pkg = test_package(&[(custom::interchange(), multi.then_some(custom::IC_MULTIPLE_ALLOCATIONS_PER_ATTRIBUTE_IN_STORAGE_FIXED))]);
        let mut b = [0u64.to_le_bytes().to_vec(), 0.1f32.to_le_bytes().to_vec(), (table.len() as i32).to_le_bytes().to_vec()].concat();
        for (key, offset, size, ty) in table {
            b.extend(fstring(key));
            for v in [offset, size] {
                if multi {
                    b.extend(1i32.to_le_bytes());
                }
                b.extend(v.to_le_bytes());
            }
            b.extend(ty.to_le_bytes());
            b.extend([0u8; 20]); // EAttributeProperty + hash
        }
        b.extend((storage.len() as i64).to_le_bytes());
        b.extend(&storage);
        let mut r = Reader::new(&b);
        let attrs = rust_uasset::object::read_attribute_storage(&rust_uasset::property::PropReader::new(&pkg), &mut r, b.len()).unwrap();
        assert_eq!(r.pos(), b.len());
        let get = |k: &str| &attrs.iter().find(|p| p.name == k).unwrap().value;
        assert!(matches!(get("F"), Value::Float(f) if *f == 1.5));
        assert!(matches!(get("S"), Value::Str(s) if s == "hi"));
        assert!(matches!(get("A"), Value::Array(a) if a.len() == 2));
        assert!(matches!(get("X"), Value::Raw { bytes, .. } if bytes == &[1, 2, 3]), "{:?}", get("X"));
    }
}
