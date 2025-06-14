use crate::errors::Result;
use crate::uasset_parser::Parsable;
use crate::uasset_parser::UassetParser;
use byteorder::LittleEndian;
use byteorder::ReadBytesExt;
use std::io::{Read, Seek};

#[derive(Debug)]
pub struct FName {
    pub index: i32,
    pub number: i32,
    value: String,
}

impl<R: Read + Seek> Parsable<FName> for UassetParser<R> {
    fn parse(&mut self) -> Result<FName> {
        let index: usize = self.reader.read_i32::<LittleEndian>()? as usize;
        let number = self.reader.read_i32::<LittleEndian>()?;
        let value: String = if index < self.names.len() {
            self.names[index].clone()
        } else {
            "None".into()
        };

        Ok(FName {
            index: index as i32,
            number,
            value,
        })
    }
}

impl FName {
    pub fn as_string(&self) -> String {
        self.value.clone()
    }

    pub fn is_none(&self) -> bool {
        self.value == "None"
    }
}
