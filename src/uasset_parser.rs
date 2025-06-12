use byteorder::{LittleEndian, ReadBytesExt};

use crate::{
    errors::{ParseError, Result},
    uasset_summary::UassetSummary,
};
use std::io::{Read, Seek};

pub struct UassetParser<R: Read + Seek> {
    pub reader: R,
    pub summary: Option<UassetSummary>,
    pub allow_unversioned: bool,
}

pub trait Parsable<T> {
    fn parse(&mut self) -> Result<T>;
}

impl<R: Read + Seek> UassetParser<R> {
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            summary: None,
            allow_unversioned: true,
        }
    }

    pub fn read<T>(&mut self) -> Result<T>
    where
        Self: Parsable<T>,
    {
        self.parse()
    }

    pub fn parse_asset(&mut self) -> Result<()> {
        let summary: UassetSummary = self.read()?;
        self.summary = Some(summary);
        Ok(())
    }

    pub fn read_fstring(&mut self) -> Result<String> {
        let size = self.reader.read_i32::<LittleEndian>()?;

        if size == 0 {
            return Ok(String::new());
        }

        let (load_ucs2_char, actual_size) = if size < 0 {
            (true, (-size) as usize)
        } else {
            (false, size as usize)
        };

        let byte_size = if load_ucs2_char {
            actual_size * 2
        } else {
            actual_size
        };

        let mut buffer = vec![0u8; byte_size];
        self.reader.read_exact(&mut buffer)?;

        // Remove null terminator
        if load_ucs2_char {
            buffer.truncate(byte_size - 2);
            // Convert UTF-16LE to String
            let u16_vec: Vec<u16> = buffer
                .chunks_exact(2)
                .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
                .collect();
            String::from_utf16(&u16_vec).map_err(|_| ParseError::InvalidUtf16)
        } else {
            buffer.truncate(byte_size - 1);
            String::from_utf8(buffer).map_err(|e| e.into())
        }
    }

    pub fn check_file_offset(&self, offset: i64) -> Result<()> {
        if offset < 0 || offset as u64 > 1024 * 1024 * 64 {
            return Err(ParseError::InvalidFileOffset {
                offset,
                file_size: 1024,
            });
        }
        Ok(())
    }

    pub fn read_tarray<T, F>(&mut self, mut reader_fn: F, max_elements: usize) -> Result<Vec<T>>
    where
        F: FnMut(&mut Self) -> Result<T>,
    {
        let n = self.reader.read_i32::<LittleEndian>()?;

        if n < 0 || n as usize > max_elements {
            return Err(ParseError::InvalidArraySize(n));
        }

        let mut array = Vec::with_capacity(n as usize);
        for _ in 0..n {
            array.push(reader_fn(self)?);
        }
        Ok(array)
    }

    pub fn check_compression_flags(&self, flags: u32) -> Result<()> {
        const COMPRESS_DEPRECATED_FORMAT_FLAGS_MASK: u32 = 0x0F;
        const COMPRESS_OPTIONS_FLAGS_MASK: u32 = 0xF0;
        const COMPRESSION_FLAGS_MASK: u32 =
            COMPRESS_DEPRECATED_FORMAT_FLAGS_MASK | COMPRESS_OPTIONS_FLAGS_MASK;

        if flags & (!COMPRESSION_FLAGS_MASK) != 0 {
            return Err(ParseError::InvalidCompressionFlags);
        }
        Ok(())
    }

    pub fn check_asset_version(&self, major: u16, minor: u16, _patch: u16) -> Result<()> {
        const MIN_MAJOR: u16 = 4;
        const MIN_MINOR: u16 = 27;

        if major == 0 {
            if !self.allow_unversioned {
                return Err(ParseError::UnversionedAssetNotAllowed);
            }
        } else if major < MIN_MAJOR || (major == MIN_MAJOR && minor < MIN_MINOR) {
            return Err(ParseError::AssetVersionTooOld { major, minor });
        }
        Ok(())
    }
}
