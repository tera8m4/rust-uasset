use std::io::{Read, Seek};

use byteorder::{LittleEndian, ReadBytesExt};

use crate::errors::Result;
use crate::uasset_parser::{Parsable, UassetParser};

use super::uclass::UClassData;

pub struct DataTable {
    pub class_data: UClassData,
    pub rows: Vec<String>,
}

impl<R: Read + Seek> Parsable<DataTable> for UassetParser<R> {
    fn parse(&mut self) -> Result<DataTable> {
        let flags = self.reader.read_u8()?;
        let class_data: UClassData = self.read()?;
        let rows: i32 = self.reader.read_i32::<LittleEndian>()?;

        Ok(DataTable {
            class_data,
            rows: vec![],
        })
    }
}
