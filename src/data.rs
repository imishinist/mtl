use std::borrow::Borrow;
use std::cmp::Ordering;
use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;

use byteorder::ByteOrder;
use clap::ValueEnum;
use redb::{RedbKey, RedbValue, TypeName};

use crate::hash::Hash;
use crate::path::RelativePath;
use crate::ParseError;

#[derive(Debug, PartialEq, Eq, Clone, ValueEnum, std::hash::Hash)]
pub enum ObjectKind {
    Tree,
    File,
}

impl fmt::Display for ObjectKind {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        match self {
            ObjectKind::Tree => write!(f, "tree"),
            ObjectKind::File => write!(f, "file"),
        }
    }
}

impl FromStr for ObjectKind {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "tree" => Ok(ObjectKind::Tree),
            "file" => Ok(ObjectKind::File),
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

#[derive(Debug, PartialEq, Eq, Clone, std::hash::Hash)]
pub enum ObjectRef {
    Reference(String),
    ID(ObjectID),
}

impl ObjectRef {
    pub fn new_reference<S: Into<String>>(reference: S) -> Self {
        ObjectRef::Reference(reference.into())
    }

    pub fn new_id(object_id: ObjectID) -> Self {
        ObjectRef::ID(object_id)
    }
}

impl fmt::Display for ObjectRef {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        match self {
            ObjectRef::Reference(reference) => write!(f, "{}", reference),
            ObjectRef::ID(object_id) => write!(f, "{}", object_id),
        }
    }
}

impl FromStr for ObjectRef {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match ObjectID::from_hex(s) {
            Ok(object_id) => Ok(ObjectRef::new_id(object_id)),
            Err(_) => Ok(ObjectRef::new_reference(s)),
        }
    }
}

impl From<&str> for ObjectRef {
    fn from(value: &str) -> Self {
        ObjectRef::Reference(value.to_string())
    }
}

impl PartialOrd for ObjectRef {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ObjectRef {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (ObjectRef::Reference(a), ObjectRef::Reference(b)) => a.cmp(b),
            (ObjectRef::ID(a), ObjectRef::ID(b)) => a.cmp(b),
            (ObjectRef::Reference(_), ObjectRef::ID(_)) => Ordering::Less,
            (ObjectRef::ID(_), ObjectRef::Reference(_)) => Ordering::Greater,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, std::hash::Hash)]
pub struct Object {
    pub kind: ObjectKind,
    pub id: ObjectID,

    // only contains basename of file
    pub basename: RelativePath,
}

impl Object {
    pub fn new<P: Into<PathBuf>>(kind: ObjectKind, id: ObjectID, file_name: P) -> Self {
        Object {
            kind,
            id,
            basename: RelativePath::from(file_name),
        }
    }

    pub fn new_tree<P: Into<PathBuf>>(object_id: ObjectID, file_name: P) -> Self {
        Object::new(ObjectKind::Tree, object_id, file_name.into())
    }

    pub fn new_file<P: Into<PathBuf>>(object_id: ObjectID, file_name: P) -> Self {
        Object::new(ObjectKind::File, object_id, file_name.into())
    }

    pub fn is_tree(&self) -> bool {
        self.kind == ObjectKind::Tree
    }

    pub fn is_file(&self) -> bool {
        self.kind == ObjectKind::File
    }

    pub fn size(&self) -> usize {
        // "tree" "\t" "d447b1ea40e6988b" "\t" string "\n"
        // 4 + 1 + 16 + 1 + str_len + 1
        23 + self.basename.as_os_str().len()
    }

    pub fn as_object_ref(&self) -> ObjectRef {
        ObjectRef::new_id(self.id)
    }
}

impl AsRef<Object> for Object {
    fn as_ref(&self) -> &Object {
        self
    }
}

impl PartialOrd<Self> for Object {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Object {
    fn cmp(&self, other: &Self) -> Ordering {
        self.basename.cmp(&other.basename)
    }
}

impl fmt::Display for Object {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(
            f,
            "{}\t{}\t{}",
            self.kind,
            self.id,
            self.basename.file_name().unwrap_or_default().display()
        )
    }
}

#[cfg(test)]
mod tests {
    use crate::hash::Hash;
    use crate::{Object, ObjectID, ObjectKind, ObjectRef};
    use std::path::PathBuf;
    use std::str::FromStr;

    #[test]
    fn test_object_kind_display() {
        assert_eq!(ObjectKind::Tree.to_string(), "tree");
        assert_eq!(ObjectKind::File.to_string(), "file");
    }

    #[test]
    fn test_object_kind_from_str() {
        assert_eq!(ObjectKind::from_str("tree").unwrap(), ObjectKind::Tree);
        assert_eq!(ObjectKind::from_str("file").unwrap(), ObjectKind::File);
        assert!(ObjectKind::from_str("").is_err());
        assert!(ObjectKind::from_str("invalid").is_err());
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

    #[test]
    fn test_object_ref() {
        assert_eq!(
            "d447b1ea40e6988b".parse::<ObjectRef>().unwrap(),
            ObjectRef::new_id(ObjectID::from_hex("d447b1ea40e6988b").unwrap())
        );
        assert_eq!(
            "HEAD".parse::<ObjectRef>().unwrap(),
            ObjectRef::new_reference("HEAD")
        );
        assert_eq!(
            "invalid_hex".parse::<ObjectRef>().unwrap(),
            ObjectRef::new_reference("invalid_hex")
        );
    }

    #[test]
    fn test_object_order() {
        use crate::path::RelativePath;

        let object_id = ObjectID::from_hex("d447b1ea40e6988b").unwrap();
        let mut objects = vec![
            Object::new(ObjectKind::File, object_id.clone(), PathBuf::from("c")),
            Object::new(ObjectKind::File, object_id.clone(), PathBuf::from("d")),
            Object::new(ObjectKind::File, object_id.clone(), PathBuf::from("a")),
            Object::new(ObjectKind::File, object_id.clone(), PathBuf::from("b")),
        ];
        let mut compare_target = objects.clone();

        objects.sort();
        compare_target.sort_by(|a, b| a.basename.cmp(&b.basename));
        assert_eq!(objects, compare_target);
        assert_eq!(
            vec![
                RelativePath::from("a"),
                RelativePath::from("b"),
                RelativePath::from("c"),
                RelativePath::from("d")
            ],
            objects.into_iter().map(|o| o.basename).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_object_size() {
        let object_id = ObjectID::from_hex("d447b1ea40e6988b").unwrap();
        let objects = vec![
            Object::new(ObjectKind::File, object_id.clone(), PathBuf::from("a")),
            Object::new(ObjectKind::File, object_id.clone(), PathBuf::from("aa")),
            Object::new(ObjectKind::File, object_id.clone(), PathBuf::from("aあ")),
            Object::new(ObjectKind::File, object_id.clone(), PathBuf::from("あ")),
            Object::new(ObjectKind::File, object_id.clone(), PathBuf::from("ああ")),
        ];
        assert_eq!(objects[0].size(), 24);
        assert_eq!(objects[1].size(), 25);
        assert_eq!(objects[2].size(), 27);
        assert_eq!(objects[3].size(), 26);
        assert_eq!(objects[4].size(), 29);
    }

    #[test]
    fn test_object_display() {
        let object = Object::new_file(
            ObjectID::from_hex("d447b1ea40e6988b").unwrap(),
            PathBuf::from("foo/bar/baz"),
        );
        assert_eq!(format!("{}", object), "file\td447b1ea40e6988b\tbaz");
    }
}
