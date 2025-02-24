use std::borrow::Borrow;
use std::fmt;

use byteorder::ByteOrder;

use crate::ParseHashError;

#[derive(Debug, PartialEq, Clone, Copy, Eq, std::hash::Hash, PartialOrd, Ord)]
pub struct Hash {
    xxh3: u64,
}

impl Hash {
    pub fn new(xxh3: u64) -> Self {
        Hash { xxh3 }
    }

    pub fn from_contents<T: AsRef<[u8]>>(contents: T) -> Self {
        Hash::new(xxh3_contents(contents))
    }

    pub fn from_hex<S: AsRef<str>>(hex: S) -> Result<Self, ParseHashError> {
        let hex = hex.as_ref();
        if hex.len() != 16 {
            return Err(ParseHashError::InvalidFormat);
        }
        let xxh3 = u64::from_str_radix(hex, 16)?;
        Ok(Hash::new(xxh3))
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = vec![0; 8];
        byteorder::LittleEndian::write_u64(&mut buf, self.xxh3);
        buf
    }

    pub fn fixed_width() -> usize {
        8
    }

    pub fn as_u64(&self) -> u64 {
        self.xxh3
    }
}

impl fmt::Display for Hash {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "{:016x}", self.xxh3)
    }
}

impl Borrow<u64> for Hash {
    fn borrow(&self) -> &u64 {
        &self.xxh3
    }
}

pub fn xxh3_contents<T: AsRef<[u8]>>(contents: T) -> u64 {
    xxhash_rust::xxh3::xxh3_64(contents.as_ref())
}

pub fn xxh64_contents<T: AsRef<[u8]>>(contents: T) -> u64 {
    xxhash_rust::xxh64::xxh64(contents.as_ref(), 0)
}

#[cfg(test)]
mod tests {
    use crate::hash::Hash;
    use byteorder::ByteOrder;

    #[test]
    fn test_hash() {
        let actual = super::Hash::new(0);
        let expected = "0000000000000000";
        assert_eq!(expected, format!("{}", actual));

        let actual = super::Hash::from_contents("hello world");
        assert_eq!(actual, super::Hash::from_hex("d447b1ea40e6988b").unwrap());
    }

    #[test]
    fn test_hash_error() {
        let actual = super::Hash::from_hex("g");
        assert!(actual.is_err());
        assert_eq!(format!("{}", actual.unwrap_err()), "invalid hash format");

        let actual = super::Hash::from_hex("ghijklmnopqrstuv");
        assert!(actual.is_err());
        assert_eq!(
            format!("{}", actual.unwrap_err()),
            "invalid digit found in string"
        );
    }

    #[test]
    fn hash_ref() {
        let a = vec![1, 2, 4, 8, 16, 32, 64, 128];
        let t = byteorder::LittleEndian::read_u64(&a);
        let hash = Hash::new(t);

        assert_eq!(a, hash.to_bytes());
    }
}
