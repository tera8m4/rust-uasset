use std::io::{Read, Seek};

use crate::errors::Result;
use crate::{
    property_tag::PropertyTag,
    uasset_parser::{Parsable, UassetParser},
};
use crate::fname::FName;
use crate::property_data::PropertyValue::StrProperty;

pub enum PropertyValue {
    StrProperty(String),
    Undefined
}

pub struct PropertyData {
    pub tag: PropertyTag,
    pub value: PropertyValue,
}

impl<R: Read + Seek> Parsable<PropertyData> for UassetParser<R> {
    fn parse(&mut self) -> Result<PropertyData> {
        let tag: PropertyTag = self.read()?;
        let value: PropertyValue = if tag.type_name.name == "StrProperty" {
            StrProperty(self.read_fstring()?)
        } else if tag.type_name.name == "NameProperty" {
            let fname: FName = self.read()?;
            StrProperty(fname.as_string())
        } else {
            self.skip_bytes(tag.size as i64)?;
            PropertyValue::Undefined
        };


        Ok(PropertyData { tag, value })
    }
}
