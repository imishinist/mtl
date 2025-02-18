use crate::ParseError;
use clap::ValueEnum;
use std::fmt;
use std::str::FromStr;

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

#[cfg(test)]
mod tests {
    use crate::ObjectType;
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
}
