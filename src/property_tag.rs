use byteorder::{LittleEndian, ReadBytesExt};

use crate::errors::Result;
use crate::{
    fname::FName,
    property_type_name::PropertyTypeName,
    uasset_parser::{Parsable, UassetParser},
};
use std::io::{Read, Seek};

#[repr(u8)]
pub enum EPropertyTagFlags {
    None = 0x00,
    HasArrayIndex = 0x01,
    HasPropertyGuid = 0x02,
    HasPropertyExtensions = 0x04,
    HasBinaryOrNativeSerialize = 0x08,
    BoolTrue = 0x10,
    SkippedSerialize = 0x20,
}

pub struct PropertyTag {
    pub name: FName,
    pub type_name: PropertyTypeName,
    pub size: i32,
    pub flags: u8,
    pub array_index: i32,
    pub guid: u128,
}

impl PropertyTag {
    fn new(name: FName) -> Self {
        PropertyTag {
            name,
            type_name: PropertyTypeName {
                name: String::new(),
            },
            size: 0,
            flags: 0,
            array_index: -1,
            guid: 0,
        }
    }
}

impl<R: Read + Seek> Parsable<PropertyTag> for UassetParser<R> {
    fn parse(&mut self) -> Result<PropertyTag> {
        let name: FName = self.read()?;
        if name.is_none() {
            return Ok(PropertyTag::new(name));
        }

        let type_name: PropertyTypeName = self.read()?;
        let size = self.reader.read_i32::<LittleEndian>()?;
        let flags: u8 = self.reader.read_u8()?;
        let array_index: i32 = if flags & (EPropertyTagFlags::HasArrayIndex as u8) != 0 {
            self.reader.read_i32::<LittleEndian>()?
        } else {
            0
        };

        let guid: u128 = if flags & (EPropertyTagFlags::HasPropertyGuid as u8) != 0 {
            self.reader.read_u128::<LittleEndian>()?
        } else {
            0
        };

        if flags & (EPropertyTagFlags::HasPropertyExtensions as u8) != 0 {
            todo!();
        }

        Ok(PropertyTag {
            name,
            type_name,
            size,
            flags,
            array_index,
            guid,
        })
    }
}
