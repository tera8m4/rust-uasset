use std::io;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ParseError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("Invalid uasset tag")]
    InvalidTag,

    #[error("Unsupported legacy file version: {0}")]
    UnsupportedLegacyVersion(i32),

    #[error("Invalid file offset: {offset} (file size: {file_size})")]
    InvalidFileOffset { offset: i64, file_size: u64 },

    #[error("Invalid array size: {0}")]
    InvalidArraySize(i32),

    #[error("Invalid compression flags")]
    InvalidCompressionFlags,

    #[error("Compressed chunks not supported")]
    CompressedChunksNotSupported,

    #[error("Unversioned asset parsing not allowed")]
    UnversionedAssetNotAllowed,

    #[error("Asset version too old: {major}.{minor} (minimum: 4.27)")]
    AssetVersionTooOld { major: u16, minor: u16 },

    #[error("Invalid UTF-8 string")]
    InvalidUtf8(#[from] std::string::FromUtf8Error),

    #[error("Invalid UTF-16 string")]
    InvalidUtf16,
}

pub type Result<T> = std::result::Result<T, ParseError>;
