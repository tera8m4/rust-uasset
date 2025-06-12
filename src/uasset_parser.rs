use std::io::{self, Read, Seek};

use crate::uasset_summary::UassetSummary;

pub struct UassetParser<R: Read + Seek> {
    pub reader: R,
    pub summary: Option<UassetSummary>,
}

pub trait Parsable<T> {
    fn parse(&mut self) -> io::Result<T>;
}

impl<R: Read + Seek> UassetParser<R> {
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            summary: None,
        }
    }

    pub fn read<T>(&mut self) -> io::Result<T>
    where
        Self: Parsable<T>,
    {
        self.parse()
    }

    pub fn parse_asset(&mut self) -> io::Result<()> {
        let summary: UassetSummary = self.read()?;
        self.summary = Some(summary);
        Ok(())
    }
}
