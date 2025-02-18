use std::borrow::Borrow;
use std::cmp::Ordering;
use std::fmt;
use std::str::FromStr;

use byteorder::ByteOrder;
use clap::ValueEnum;
use redb::{RedbKey, RedbValue, TypeName};

use crate::hash::Hash;
use crate::ParseError;

#[derive(Debug, PartialEq, Eq, Clone, ValueEnum, std::hash::Hash)]
pub enum ObjectType {
    Tree,
    File,
}

impl fmt::Display for ObjectType {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        match self {
            ObjectType::Tree => write!(f, "tree"),
            ObjectType::File => write!(f, "file"),
        }
    }
}

impl FromStr for ObjectType {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "tree" => Ok(ObjectType::Tree),
            "file" => Ok(ObjectType::File),
            "" => Err(ParseError::EmptyToken),
            s => Err(ParseError::InvalidToken(s.to_string())),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Copy, Clone, std::hash::Hash, PartialOrd, Ord)]
pub struct ObjectID(Hash);

impl ObjectID {
    pub fn new(hash: Hash) -> Self {
        ObjectID(hash)
    }

    pub fn from_hex<S: AsRef<str>>(hex: S) -> Result<Self, ParseError> {
        Ok(ObjectID::new(Hash::from_hex(hex)?))
    }

    pub fn from_contents<T: AsRef<[u8]>>(contents: T) -> Self {
        ObjectID::new(Hash::from_contents(contents))
    }

    pub fn as_u64(&self) -> u64 {
        self.0.as_u64()
    }
}

impl fmt::Display for ObjectID {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        self.0.fmt(f)
    }
}

impl FromStr for ObjectID {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        ObjectID::from_hex(s)
    }
}

impl Borrow<u64> for ObjectID {
    fn borrow(&self) -> &u64 {
        self.0.borrow()
    }
}

impl RedbValue for ObjectID {
    type SelfType<'a> = ObjectID;
    type AsBytes<'a> = Vec<u8>;

    fn fixed_width() -> Option<usize> {
        Some(Hash::fixed_width())
    }

    fn from_bytes<'a>(data: &'a [u8]) -> ObjectID
    where
        Self: 'a,
    {
        ObjectID::new(Hash::new(byteorder::LittleEndian::read_u64(data)))
    }

    fn as_bytes<'a, 'b: 'a>(value: &'a ObjectID) -> Vec<u8>
    where
        Self: 'a,
    {
        value.0.to_bytes()
    }

    fn type_name() -> TypeName {
        TypeName::new("object-id")
    }
}

impl RedbKey for ObjectID {
    fn compare(data1: &[u8], data2: &[u8]) -> Ordering {
        byteorder::LittleEndian::read_u64(data1).cmp(&byteorder::LittleEndian::read_u64(data2))
    }
}

#[cfg(test)]
mod tests {
    use crate::hash::Hash;
    use crate::{ObjectID, ObjectType};
    use std::str::FromStr;

    #[test]
    fn test_object_type_display() {
        assert_eq!(ObjectType::Tree.to_string(), "tree");
        assert_eq!(ObjectType::File.to_string(), "file");
    }

    #[test]
    fn test_object_type_from_str() {
        assert_eq!(ObjectType::from_str("tree").unwrap(), ObjectType::Tree);
        assert_eq!(ObjectType::from_str("file").unwrap(), ObjectType::File);
        assert!(ObjectType::from_str("").is_err());
        assert!(ObjectType::from_str("invalid").is_err());
    }

    #[test]
    fn test_object_id_from_hex() {
        assert_eq!(
            ObjectID::from_hex("d447b1ea40e6988b").unwrap(),
            ObjectID::new(Hash::from_hex("d447b1ea40e6988b").unwrap())
        );

        assert_eq!(
            ObjectID::from_contents("hello world"),
            ObjectID::from_hex("d447b1ea40e6988b").unwrap()
        );

        assert_eq!(
            ObjectID::from_hex("d447b1ea40e6988b").unwrap().to_string(),
            "d447b1ea40e6988b".to_string()
        );
    }

    #[test]
    fn test_object_id_from_contents() {
        assert_eq!(
            ObjectID::from_contents("hello world"),
            ObjectID::from_hex("d447b1ea40e6988b").unwrap()
        );
    }

    #[test]
    fn test_object_id_from_str() {
        assert_eq!(
            ObjectID::from_str("d447b1ea40e6988b").unwrap(),
            ObjectID::new(Hash::from_hex("d447b1ea40e6988b").unwrap())
        );
    }
}
