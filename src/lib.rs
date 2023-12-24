pub mod commands;
pub mod error;
pub(crate) mod filesystem;
pub mod hash;

pub use commands::*;
pub use error::*;
pub use filesystem::*;

use std::cmp::Ordering;
use std::fmt;
use std::fs;
use std::io;
use std::num::ParseIntError;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use clap::ValueEnum;

use crate::hash::Hash;
#[cfg(feature = "jemalloc")]
use tikv_jemallocator::Jemalloc;

#[cfg(feature = "jemalloc")]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

#[derive(Debug, PartialEq, Eq, Clone, ValueEnum)]
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
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "tree" => Ok(ObjectType::Tree),
            "file" => Ok(ObjectType::File),
            _ => Err(format!("{} is not a valid object type", s)),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct ObjectID {
    inner: Hash,
}

impl ObjectID {
    pub fn new(hash: Hash) -> Self {
        ObjectID { inner: hash }
    }

    pub fn from_hex<S: AsRef<str>>(hex: S) -> Result<Self, ParseIntError> {
        Ok(ObjectID::new(Hash::from_hex(hex)?))
    }

    pub fn from_contents<T: AsRef<[u8]>>(contents: T) -> Self {
        ObjectID::new(Hash::from_contents(contents))
    }
}

impl fmt::Display for ObjectID {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        self.inner.fmt(f)
    }
}

impl FromStr for ObjectID {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        ObjectID::from_hex(s).map_err(|e| e.to_string())
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Object {
    object_type: ObjectType,
    object_id: ObjectID,

    // only contains basename of file
    file_name: PathBuf,
}

impl Object {
    pub fn new(object_type: ObjectType, object_id: ObjectID, file_name: PathBuf) -> Self {
        Object {
            object_type,
            object_id,
            file_name,
        }
    }

    pub fn new_tree(object_id: ObjectID, file_name: PathBuf) -> Self {
        Object::new(ObjectType::Tree, object_id, file_name)
    }

    pub fn new_file(object_id: ObjectID, file_name: PathBuf) -> Self {
        Object::new(ObjectType::File, object_id, file_name)
    }

    pub fn is_tree(&self) -> bool {
        self.object_type == ObjectType::Tree
    }

    pub fn is_file(&self) -> bool {
        self.object_type == ObjectType::File
    }
}

impl AsRef<Object> for Object {
    fn as_ref(&self) -> &Object {
        self
    }
}

impl PartialOrd<Self> for Object {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        return self.file_name.partial_cmp(&other.file_name);
    }
}

impl Ord for Object {
    fn cmp(&self, other: &Self) -> Ordering {
        return self.file_name.cmp(&other.file_name);
    }
}

const MTL_DIR: &str = ".mtl";

#[derive(Debug)]
pub struct Context {
    // root of the repository
    root_dir: PathBuf,
}

impl Context {
    pub fn new<P: Into<PathBuf>>(root_dir: P) -> Self {
        Context {
            root_dir: root_dir.into(),
        }
    }

    pub fn root_dir(&self) -> &Path {
        &self.root_dir
    }

    pub fn object_dir(&self, object_id: &ObjectID) -> PathBuf {
        let dir_name = self.root_dir.as_path();
        dir_name
            .join(MTL_DIR)
            .join("objects")
            .join(&object_id.to_string()[0..2])
    }

    pub fn object_file(&self, object_id: &ObjectID) -> PathBuf {
        let object_string = object_id.to_string();

        let file_name = self.root_dir.as_path();
        file_name
            .join(MTL_DIR)
            .join("objects")
            .join(&object_string[0..2])
            .join(&object_string[2..])
    }

    pub fn object_files(&self) -> io::Result<Vec<PathBuf>> {
        let dir_name = self.root_dir.as_path();
        let object_dir = dir_name.join(MTL_DIR).join("objects");

        let mut object_files = Vec::new();
        for entry in fs::read_dir(object_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_file() {
                log::warn!("Unexpected file in object directory: {}", path.display());
            }
            if path.is_dir() {
                for entry in fs::read_dir(path)? {
                    let entry = entry?;
                    let path = entry.path();

                    if path.is_dir() {
                        log::warn!("Unexpected directory in object directory: {}", path.display());
                    }
                    if path.is_file() {
                        object_files.push(path);
                    }
                }
            }
        }

        Ok(object_files)
    }

    pub fn head_file(&self) -> PathBuf {
        let head_name = self.root_dir.as_path();
        head_name.join(MTL_DIR).join("HEAD")
    }

    pub fn write_tree_contents<T: AsRef<Object>>(&self, entries: &[T]) -> io::Result<ObjectID> {
        let tree_contents = serialize_entries(&entries)?;
        let object_id = ObjectID::from_contents(&tree_contents);

        let dir_name = self.object_dir(&object_id);
        let file_name = self.object_file(&object_id);

        fs::create_dir_all(&dir_name)?;
        fs::write(&file_name, tree_contents)?;

        Ok(object_id)
    }

    pub fn read_tree_contents(&self, object_id: &ObjectID) -> Result<Vec<Object>, ParseError> {
        let file_name = self.object_file(object_id);
        let tree_contents = fs::read_to_string(file_name)?;

        let mut objects = Vec::new();
        for line in tree_contents.lines() {
            let mut parts = line.split('\t');
            let object_type: ObjectType = parts
                .next()
                .ok_or(ParseError::InvalidFormat)?
                .parse()
                .map_err(ParseError::InvalidToken)?;
            let object_id: ObjectID = parts
                .next()
                .ok_or(ParseError::InvalidFormat)?
                .parse()
                .map_err(ParseError::InvalidToken)?;
            let file_name = PathBuf::from(parts.next().ok_or(ParseError::InvalidFormat)?);

            objects.push(Object::new(object_type, object_id, file_name));
        }

        Ok(objects)
    }

    pub fn write_head(&self, object_id: &ObjectID) -> io::Result<()> {
        let head_name = self.head_file();
        fs::write(head_name, object_id.to_string())?;

        Ok(())
    }

    pub fn read_head(&self) -> io::Result<ObjectID> {
        let head = fs::read_to_string(self.head_file())?;
        let head = head.trim();

        head.parse()
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }
}

// serialize entries should be called with sorted entries
fn serialize_entries<T: AsRef<Object>>(entries: &[T]) -> io::Result<Vec<u8>> {
    // Note: should decrease allocation?
    let mut buf = Vec::new();

    for entry in entries {
        let entry = entry.as_ref();
        buf.extend_from_slice(format!("{}\t{}\t", entry.object_type, entry.object_id).as_bytes());
        buf.extend_from_slice(entry.file_name.to_str().unwrap().as_bytes());
        buf.push(b'\n');
    }

    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

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
    fn test_object_id() {
        assert_eq!(
            ObjectID::from_hex("d447b1ea40e6988b").unwrap(),
            ObjectID::new(Hash::from_hex("d447b1ea40e6988b").unwrap())
        );

        assert_eq!(
            ObjectID::from_contents("hello world"),
            ObjectID::from_hex("d447b1ea40e6988b").unwrap()
        );
    }

    #[test]
    fn test_context() {
        let ctx = Context::new("/tmp");
        assert_eq!(ctx.root_dir(), Path::new("/tmp"));
        assert_eq!(
            ctx.object_dir(&ObjectID::from_hex("d447b1ea40e6988b").unwrap()),
            Path::new("/tmp/.mtl/objects/d4")
        );
        assert_eq!(
            ctx.object_file(&ObjectID::from_hex("d447b1ea40e6988b").unwrap()),
            Path::new("/tmp/.mtl/objects/d4/47b1ea40e6988b")
        );
        assert_eq!(ctx.head_file(), Path::new("/tmp/.mtl/HEAD"));
    }

    #[test]
    fn test_object_order() {
        let object_id = ObjectID::from_hex("d447b1ea40e6988b").unwrap();
        let mut objects = vec![
            Object::new(ObjectType::File, object_id.clone(), PathBuf::from("c")),
            Object::new(ObjectType::File, object_id.clone(), PathBuf::from("d")),
            Object::new(ObjectType::File, object_id.clone(), PathBuf::from("a")),
            Object::new(ObjectType::File, object_id.clone(), PathBuf::from("b")),
        ];
        let mut compare_target = objects.clone();

        objects.sort();
        compare_target.sort_by(|a, b| a.file_name.cmp(&b.file_name));
        assert_eq!(objects, compare_target);
        assert_eq!(
            vec![
                PathBuf::from("a"),
                PathBuf::from("b"),
                PathBuf::from("c"),
                PathBuf::from("d")
            ],
            objects.into_iter().map(|o| o.file_name).collect::<Vec<_>>()
        );
    }
}
