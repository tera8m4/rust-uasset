use std::{fs::File, io::Seek};

use byteorder::{LittleEndian, ReadBytesExt};
use common::test_data_path;
use rust_uasset::{data::uclass::UClassData, uasset_parser::UassetParser};
use rust_uasset::data::data_table::DataTable;

mod common;

#[test]
fn test_uasset_summary_parser() {
    let file_path = test_data_path("test_table_ue54.uasset");
    let file = File::open(&file_path).expect("Failed to open test file");
    let mut parser = UassetParser::new(file);
    parser.parse_asset().expect("managed to parse the asset");
    let summary = parser.get_summary();

    assert_eq!(summary.export_count, 3);
    assert_eq!(summary.name_count, 30);

    assert_eq!(parser.names.len(), summary.name_count as usize);
    assert!(!parser.names.last().unwrap().is_empty());
    assert_eq!(parser.entries.len(), summary.export_count as usize);
}

#[test]
fn test_export_data_parser() {
    let file_path = test_data_path("test_table_ue54.uasset");
    let file = File::open(&file_path).expect("Failed to open test file");
    let mut parser = UassetParser::new(file);
    parser.parse_asset().expect("managed to parse the asset");

    parser
        .reader
        .seek(std::io::SeekFrom::Start(
            parser.entries[1].serial_offset as u64,
        ))
        .expect("failed to seek");

    let data_table: DataTable = parser.read().expect("Failed to parse data table");
    assert_eq!(data_table.rows.len(), 2);
}
