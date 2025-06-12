use byteorder::LittleEndian;
use byteorder::ReadBytesExt;
use std::io::{Read, Seek};

use crate::uasset_parser::Parsable;
use crate::uasset_parser::UassetParser;

pub struct FName {
    pub index: i32,
    pub number: i32,
}

impl<R: Read + Seek> Parsable<FName> for UassetParser<R> {
    fn parse(&mut self) -> std::io::Result<FName> {
        let index = self.reader.read_i32::<LittleEndian>()?;
        let number = self.reader.read_i32::<LittleEndian>()?;
        Ok(FName { index, number })
    }
}
