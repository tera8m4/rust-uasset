use std::io::Cursor;

use rust_uasset::{fname::FName, uasset_parser::UassetParser};

#[test]
fn test_fname_parsing() {
    let mut data = vec![];
    data.extend_from_slice(&1233u32.to_le_bytes());
    data.extend_from_slice(&1025u32.to_le_bytes());

    let cursor = Cursor::new(data);

    let mut parser = UassetParser::new(cursor);
    let test_name: FName = parser.read().expect("managed to read FName");
    assert_eq!(test_name.index, 1233);
    assert_eq!(test_name.number, 1025);
}
