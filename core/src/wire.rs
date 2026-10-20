//! Little-endian binary encoding helpers used by [`crate::protocol`].

use std::fmt;

/// The input was shorter than a message claimed, or a field was invalid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecodeError;

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("malformed message")
    }
}

impl std::error::Error for DecodeError {}

pub type DecodeResult<T> = Result<T, DecodeError>;

/// Appends fields to a growable buffer.
#[derive(Default)]
pub struct Writer {
    buf: Vec<u8>,
}

impl Writer {
    pub fn with_tag(tag: u8) -> Self {
        let mut buf = Vec::with_capacity(32);
        buf.push(tag);
        Writer { buf }
    }

    pub fn u8(&mut self, v: u8) -> &mut Self {
        self.buf.push(v);
        self
    }

    pub fn bool(&mut self, v: bool) -> &mut Self {
        self.u8(v as u8)
    }

    pub fn u16(&mut self, v: u16) -> &mut Self {
        self.buf.extend_from_slice(&v.to_le_bytes());
        self
    }

    pub fn i16(&mut self, v: i16) -> &mut Self {
        self.buf.extend_from_slice(&v.to_le_bytes());
        self
    }

    pub fn u64(&mut self, v: u64) -> &mut Self {
        self.buf.extend_from_slice(&v.to_le_bytes());
        self
    }

    pub fn f32(&mut self, v: f32) -> &mut Self {
        self.buf.extend_from_slice(&v.to_le_bytes());
        self
    }

    /// Length-prefixed (u16) byte string.
    pub fn bytes(&mut self, v: &[u8]) -> &mut Self {
        let len = v.len().min(u16::MAX as usize);
        self.u16(len as u16);
        self.buf.extend_from_slice(&v[..len]);
        self
    }

    pub fn array32(&mut self, v: &[u8; 32]) -> &mut Self {
        self.buf.extend_from_slice(v);
        self
    }

    /// Length-prefixed UTF-8 string, truncated on a character boundary.
    pub fn str(&mut self, v: &str) -> &mut Self {
        let mut end = v.len().min(1024);
        while !v.is_char_boundary(end) {
            end -= 1;
        }
        self.bytes(&v.as_bytes()[..end])
    }

    pub fn finish(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.buf)
    }
}

/// Reads fields from a byte slice. Trailing bytes are tolerated so newer
/// peers can append fields without breaking older ones.
pub struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        Reader { buf, pos: 0 }
    }

    fn take(&mut self, n: usize) -> DecodeResult<&'a [u8]> {
        let end = self.pos.checked_add(n).ok_or(DecodeError)?;
        let slice = self.buf.get(self.pos..end).ok_or(DecodeError)?;
        self.pos = end;
        Ok(slice)
    }

    pub fn u8(&mut self) -> DecodeResult<u8> {
        Ok(self.take(1)?[0])
    }

    pub fn bool(&mut self) -> DecodeResult<bool> {
        Ok(self.u8()? != 0)
    }

    pub fn u16(&mut self) -> DecodeResult<u16> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }

    pub fn i16(&mut self) -> DecodeResult<i16> {
        Ok(i16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }

    pub fn u64(&mut self) -> DecodeResult<u64> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }

    /// Reads an `f32`, rejecting NaN and infinities so geometry stays sane.
    pub fn f32(&mut self) -> DecodeResult<f32> {
        let v = f32::from_le_bytes(self.take(4)?.try_into().unwrap());
        if v.is_finite() {
            Ok(v)
        } else {
            Err(DecodeError)
        }
    }

    pub fn bytes(&mut self) -> DecodeResult<Vec<u8>> {
        let len = self.u16()? as usize;
        Ok(self.take(len)?.to_vec())
    }

    pub fn array32(&mut self) -> DecodeResult<[u8; 32]> {
        Ok(self.take(32)?.try_into().unwrap())
    }

    pub fn str(&mut self) -> DecodeResult<String> {
        String::from_utf8(self.bytes()?).map_err(|_| DecodeError)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_every_field_type() {
        let bytes = Writer::with_tag(7)
            .u8(1)
            .bool(true)
            .u16(513)
            .i16(-2)
            .u64(u64::MAX)
            .f32(1.5)
            .bytes(&[9, 9])
            .str("Mac mini")
            .array32(&[3; 32])
            .finish();
        let mut r = Reader::new(&bytes);
        assert_eq!(r.u8().unwrap(), 7);
        assert_eq!(r.u8().unwrap(), 1);
        assert!(r.bool().unwrap());
        assert_eq!(r.u16().unwrap(), 513);
        assert_eq!(r.i16().unwrap(), -2);
        assert_eq!(r.u64().unwrap(), u64::MAX);
        assert_eq!(r.f32().unwrap(), 1.5);
        assert_eq!(r.bytes().unwrap(), vec![9, 9]);
        assert_eq!(r.str().unwrap(), "Mac mini");
        assert_eq!(r.array32().unwrap(), [3; 32]);
        assert!(r.u8().is_err());
    }

    #[test]
    fn rejects_truncated_and_non_finite_input() {
        assert!(Reader::new(&[1]).u16().is_err());
        assert!(Reader::new(&[5, 0, 1]).bytes().is_err());
        assert!(Reader::new(&f32::NAN.to_le_bytes()).f32().is_err());
    }

    #[test]
    fn truncates_long_strings_on_char_boundary() {
        let long = "ব".repeat(600);
        let bytes = Writer::default().str(&long).finish();
        let s = Reader::new(&bytes).str().unwrap();
        assert!(s.len() <= 1024);
        assert!(s.chars().all(|c| c == 'ব'));
    }
}
