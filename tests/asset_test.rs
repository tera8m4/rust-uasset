use rust_uasset::object::Extra;
use rust_uasset::property::{Value, clean_field_name};
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
        count += 1;
    }
    assert!(count > 0, "no assets in {dir}");
}
