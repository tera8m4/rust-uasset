use std::{env, fs::File, io::Seek, process};

use rust_uasset::{data::data_table::DataTable, uasset_parser::UassetParser};

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() != 2 {
        eprintln!("Usage: {} <filename>", args[0]);
        process::exit(1);
    }

    let file_path = &args[1];
    let file = File::open(&file_path).unwrap();
    let mut parser = UassetParser::new(file);
    parser.parse_asset().expect("managed to parse the asset");

    parser
        .reader
        .seek(std::io::SeekFrom::Start(
            parser.entries[1].serial_offset as u64,
        ))
        .expect("failed to seek");

    let data_table: DataTable = parser.read().expect("Failed to parse data table");

    println!("{:#?}", data_table.rows);
}
