use std::collections::HashMap;
use std::io::{Read, Seek};

use byteorder::{LittleEndian, ReadBytesExt};

use crate::errors::Result;
use crate::fname::FName;
use crate::property_data::{PropertyData, PropertyValue};
use crate::uasset_parser::{Parsable, UassetParser};

use super::uclass::UClassData;

pub struct DataTable {
    pub class_data: UClassData,
    pub rows: Vec<HashMap<String, String>>,
}

impl<R: Read + Seek> Parsable<DataTable> for UassetParser<R> {
    fn parse(&mut self) -> Result<DataTable> {
        let flags = self.reader.read_u8()?;
        let class_data: UClassData = self.read()?;
        let mut rows_count: i32 = self.reader.read_i32::<LittleEndian>()?;
        let mut rows: Vec<HashMap<String, String>> = vec![];

        while rows_count > 0 {
            let row_name: FName = self.read()?;

            let mut values: HashMap<String, String> = HashMap::new();
            values.insert("Name".into(), row_name.as_string());

            loop {
                let property_data: PropertyData = self.read()?;
                if property_data.tag.name.is_none() {
                    rows.push(values);
                    break;
                }
                let tag = &property_data.tag;
                let value: String = match property_data.value {
                    PropertyValue::StrProperty(value) => value,
                    _ => String::new(),
                };
                values.insert(property_data.tag.name.as_string(), value);
            }

            rows_count -= 1;
        }

        Ok(DataTable {
            class_data,
            rows,
        })
    }
}
