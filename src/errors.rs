use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum ParseError {
    #[error("IO error: {0}")]
    Io(String),

    #[error("not a uasset file (bad magic tag)")]
    InvalidTag,

    #[error("unsupported legacy file version: {0}")]
    UnsupportedLegacyVersion(i32),

    #[error("unexpected end of data at offset {pos} (need {need} bytes)")]
    Eof { pos: usize, need: usize },

    #[error("{0}")]
    Invalid(String),

    #[error("{0}")]
    Unsupported(String),
}

impl From<std::io::Error> for ParseError {
    fn from(e: std::io::Error) -> Self {
        ParseError::Io(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, ParseError>;

pub fn invalid<T>(msg: impl Into<String>) -> Result<T> {
    Err(ParseError::Invalid(msg.into()))
}
