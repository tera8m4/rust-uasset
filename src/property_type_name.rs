use crate::errors::Result;
use crate::fname::FName;
use crate::uasset_parser::{Parsable, UassetParser};
use byteorder::{LittleEndian, ReadBytesExt};
use std::io::{Read, Seek};

pub struct PropertyTypeName {
    pub name: String,
}

impl<R: Read + Seek> Parsable<PropertyTypeName> for UassetParser<R> {
    fn parse(&mut self) -> Result<PropertyTypeName> {
        let mut remaning: i32 = 1;
        let mut names: Vec<String> = vec![];
        loop {
            let name: FName = self.read()?;
            let inner_count: i32 = self.reader.read_i32::<LittleEndian>()?;

            names.push(name.as_string());

            remaning += inner_count - 1;

            if remaning <= 0 {
                break;
            }
        }

        Ok(PropertyTypeName {
            name: names.join("_"),
        })
    }
}
