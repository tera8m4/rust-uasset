use std::io::{Read, Seek};

use crate::errors::Result;
use crate::{
    property_tag::PropertyTag,
    uasset_parser::{Parsable, UassetParser},
};

pub struct PropertyData {
    pub tag: PropertyTag,
    pub value: Option<String>,
}

impl<R: Read + Seek> Parsable<PropertyData> for UassetParser<R> {
    fn parse(&mut self) -> Result<PropertyData> {
        let tag: PropertyTag = self.read()?;
        self.skip_bytes(tag.size as i64)?;

        Ok(PropertyData { tag, value: None })
    }
}
