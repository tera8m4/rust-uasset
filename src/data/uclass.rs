use byteorder::ReadBytesExt;
use std::io::{Read, Seek};

use crate::errors::Result;
use crate::{
    property_data::PropertyData,
    uasset_parser::{Parsable, UassetParser},
};

pub struct UClassData {
    pub properties: Vec<PropertyData>,
}

impl<R: Read + Seek> Parsable<UClassData> for UassetParser<R> {
    fn parse(&mut self) -> Result<UClassData> {
        let mut properties = Vec::new();

        loop {
            let property: PropertyData = self.read()?;
            if property.tag.name.is_none() {
                break;
            }
            properties.push(property);
        }

        self.skip_bytes(4)?; // todo: figure out why it's needed

        Ok(UClassData { properties, })
    }
}
