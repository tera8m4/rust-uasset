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
    let summary = &parser.summary;
    assert!(summary.is_some());
}
