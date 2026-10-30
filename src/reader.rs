//! Little-endian cursor over an in-memory byte buffer.

use crate::errors::{ParseError, Result};

#[derive(Clone)]
pub struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    pub fn data(&self) -> &'a [u8] {
        self.data
    }

    pub fn pos(&self) -> usize {
        self.pos
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    pub fn seek(&mut self, pos: usize) -> Result<()> {
        if pos > self.data.len() {
            return Err(ParseError::Eof { pos, need: 0 });
        }
        self.pos = pos;
        Ok(())
    }

    pub fn skip(&mut self, n: usize) -> Result<()> {
        self.bytes(n).map(|_| ())
    }

    pub fn bytes(&mut self, n: usize) -> Result<&'a [u8]> {
        if n > self.remaining() {
            return Err(ParseError::Eof { pos: self.pos, need: n });
        }
        let s = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }

    pub fn peek_bytes(&self, n: usize) -> Option<&'a [u8]> {
        if n > self.remaining() {
            None
        } else {
            Some(&self.data[self.pos..self.pos + n])
        }
    }

    fn arr<const N: usize>(&mut self) -> Result<[u8; N]> {
        let b = self.bytes(N)?;
        let mut a = [0u8; N];
        a.copy_from_slice(b);
        Ok(a)
    }

    pub fn u8(&mut self) -> Result<u8> {
        Ok(self.arr::<1>()?[0])
    }
    pub fn i8(&mut self) -> Result<i8> {
        Ok(self.u8()? as i8)
    }
    pub fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(self.arr()?))
    }
    pub fn i16(&mut self) -> Result<i16> {
        Ok(i16::from_le_bytes(self.arr()?))
    }
    pub fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.arr()?))
    }
    pub fn i32(&mut self) -> Result<i32> {
        Ok(i32::from_le_bytes(self.arr()?))
    }
    pub fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_le_bytes(self.arr()?))
    }
    pub fn i64(&mut self) -> Result<i64> {
        Ok(i64::from_le_bytes(self.arr()?))
    }
    pub fn f32(&mut self) -> Result<f32> {
        Ok(f32::from_le_bytes(self.arr()?))
    }
    pub fn f64(&mut self) -> Result<f64> {
        Ok(f64::from_le_bytes(self.arr()?))
    }

    /// UE `bool` serialized through FArchive (4 bytes).
    pub fn bool32(&mut self) -> Result<bool> {
        let v = self.u32()?;
        if v > 1 {
            return Err(ParseError::Invalid(format!(
                "invalid bool value {v} at {}",
                self.pos - 4
            )));
        }
        Ok(v != 0)
    }

    pub fn guid(&mut self) -> Result<Guid> {
        Ok(Guid(self.arr()?))
    }

    pub fn peek_i32(&self) -> Option<i32> {
        self.peek_bytes(4)
            .map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    /// Reads an array length and validates it against the remaining bytes.
    pub fn count(&mut self, min_elem_size: usize) -> Result<usize> {
        let n = self.i32()?;
        if n < 0 || (n as usize).saturating_mul(min_elem_size) > self.remaining() {
            return Err(ParseError::Invalid(format!(
                "invalid array count {n} at {}",
                self.pos - 4
            )));
        }
        Ok(n as usize)
    }

    /// FString: int32 length (negative => UTF-16), null terminated.
    pub fn fstring(&mut self) -> Result<String> {
        let start = self.pos;
        let len = self.i32()?;
        if len == 0 {
            return Ok(String::new());
        }
        if len == i32::MIN {
            return Err(ParseError::Invalid(format!("invalid string length at {start}")));
        }
        if len < 0 {
            let n = (-len) as usize;
            if n * 2 > self.remaining() {
                self.pos = start;
                return Err(ParseError::Invalid(format!(
                    "invalid string length {len} at {start}"
                )));
            }
            let b = self.bytes(n * 2)?;
            let mut u: Vec<u16> = b
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect();
            while u.last() == Some(&0) {
                u.pop();
            }
            Ok(String::from_utf16_lossy(&u))
        } else {
            let n = len as usize;
            if n > self.remaining() {
                self.pos = start;
                return Err(ParseError::Invalid(format!(
                    "invalid string length {len} at {start}"
                )));
            }
            let mut b = self.bytes(n)?;
            while let [rest @ .., 0] = b {
                b = rest;
            }
            Ok(match std::str::from_utf8(b) {
                Ok(s) => s.to_string(),
                // Latin-1 fallback
                Err(_) => b.iter().map(|&c| c as char).collect(),
            })
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Guid(pub [u8; 16]);

impl Guid {
    pub fn is_zero(&self) -> bool {
        self.0.iter().all(|&b| b == 0)
    }

    pub fn from_u32s(a: u32, b: u32, c: u32, d: u32) -> Self {
        let mut g = [0u8; 16];
        g[0..4].copy_from_slice(&a.to_le_bytes());
        g[4..8].copy_from_slice(&b.to_le_bytes());
        g[8..12].copy_from_slice(&c.to_le_bytes());
        g[12..16].copy_from_slice(&d.to_le_bytes());
        Guid(g)
    }

    fn words(&self) -> [u32; 4] {
        let w = |i: usize| u32::from_le_bytes([self.0[i], self.0[i + 1], self.0[i + 2], self.0[i + 3]]);
        [w(0), w(4), w(8), w(12)]
    }
}

impl std::fmt::Display for Guid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let w = self.words();
        write!(f, "{:08X}{:08X}{:08X}{:08X}", w[0], w[1], w[2], w[3])
    }
}

impl std::fmt::Debug for Guid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}

pub fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02X}"));
    }
    s
}
