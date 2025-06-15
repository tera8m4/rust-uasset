use std::{fs::File, io::Seek};

use byteorder::{LittleEndian, ReadBytesExt};
use common::test_data_path;
use rust_uasset::{data::uclass::UClassData, uasset_parser::UassetParser};

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
    let file_path = test_data_path("DT_MyTable1.uasset");
    let file = File::open(&file_path).expect("Failed to open test file");
    let mut parser = UassetParser::new(file);
    parser.parse_asset().expect("managed to parse the asset");

    parser
        .reader
        .seek(std::io::SeekFrom::Start(
            parser.entries[1].serial_offset as u64,
        ))
        .expect("failed to seek");
    let uclass_data: UClassData = parser.read().expect("Failed to parse uclass data");
    for p in uclass_data.properties.iter() {
        println!(
            "name {} type {} size {}",
            p.tag.name.as_string(),
            p.tag.type_name.name,
            p.tag.size
        );
    }

    let rows: i32 = parser.reader.read_i32::<LittleEndian>().expect("read i32");
    println!("rows {rows}");

    assert_eq!(rows, 2);

    assert_eq!(uclass_data.properties.len(), 3);

    let pos = parser.reader.stream_position().expect("pos");
    println!("pos after reading rows: {:x}", pos);

    let row: UClassData = parser.read().expect("parsed row");
    println!("properties: {}", row.properties.len());
    for p in row.properties.iter() {
        println!(
            "p name: {} type: {}",
            p.tag.name.as_string(),
            p.tag.type_name.name
        );
    }
}
