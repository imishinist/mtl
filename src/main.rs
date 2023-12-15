use std::fmt::{Display, Error, Formatter};
use std::io;
use std::str::FromStr;

#[derive(Debug, PartialEq)]
enum ObjectType {
    Tree,
    File,
}

impl Display for ObjectType {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        match self {
            ObjectType::Tree => write!(f, "tree"),
            ObjectType::File => write!(f, "file"),
        }
    }
}

impl FromStr for ObjectType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "tree" => Ok(ObjectType::Tree),
            "file" => Ok(ObjectType::File),
            _ => Err(format!("{} is not a valid object type", s)),
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct ObjectID {
    inner: [u8; 16],
}

impl ObjectID {
    pub fn new(inner: [u8; 16]) -> Self {
        ObjectID { inner }
    }

    pub fn from_hex(hex: &str) -> io::Result<Self> {
        if hex.len() != 32 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Invalid hex length: {}", hex.len()),
            ));
        }

        let mut buf = [0; 16];
        for i in (0..32).step_by(2) {
            buf[i / 2] = u8::from_str_radix(&hex[i..i + 2], 16).map_err(|e| {
                io::Error::new(io::ErrorKind::InvalidData, format!("Invalid hex: {}", e))
            })?;
        }

        Ok(ObjectID::new(buf))
    }

    pub fn from_contents<T: AsRef<[u8]>>(contents: T) -> Self {
        let mut context = md5::Context::new();
        context.consume(contents);

        ObjectID::new(context.compute().into())
    }
}

impl Display for ObjectID {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        for byte in self.inner.iter() {
            write!(f, "{:02x}", byte)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_object_type_from_str() {
        assert_eq!("tree".parse::<ObjectType>().unwrap(), ObjectType::Tree);
        assert_eq!("file".parse::<ObjectType>().unwrap(), ObjectType::File);
        assert!("foo".parse::<ObjectType>().is_err());
    }

    #[test]
    fn test_object_type_display() {
        assert_eq!(format!("{}", ObjectType::Tree), "tree");
        assert_eq!(format!("{}", ObjectType::File), "file");
    }

    #[test]
    fn test_object_id_from_hex() {
        assert_eq!(
            ObjectID::from_hex("0123456789abcdef0123456789abcdef").unwrap(),
            ObjectID::new([
                0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, //
                0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef
            ])
        );
        assert!(ObjectID::from_hex("0123456789abcdef0123456789abcde").is_err());
        assert!(ObjectID::from_hex("0123456789abcdef0123456789abcdeg").is_err());
    }

    #[test]
    fn test_object_id_from_contents() {
        assert_eq!(
            ObjectID::from_contents("hello world"),
            ObjectID::from_hex("5eb63bbbe01eeed093cb22bb8f5acdc3").unwrap()
        );
    }
}

fn main() {}
