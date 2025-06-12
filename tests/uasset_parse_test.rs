use std::fs::File;

use common::test_data_path;
use rust_uasset::uasset_parser::UassetParser;

mod common;

#[test]
fn test_uasset_summary_parser() {
    let file_path = test_data_path("test_table_ue54.uasset");
    let file = File::open(&file_path).expect("Failed to open test file");
    let mut parser = UassetParser::new(file);
    parser.parse_asset().expect("managed to parse the asset");
    let summary = parser.summary.as_ref().expect("summary should present");

    assert_eq!(summary.export_count, 3);
    assert_eq!(summary.name_count, 30);

    assert_eq!(parser.names.len(), summary.name_count as usize);
    assert!(!parser.names.last().unwrap().is_empty());
}
