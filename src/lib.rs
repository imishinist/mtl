pub mod commands;
pub mod error;
pub(crate) mod filesystem;
pub mod hash;
pub(crate) mod progress;

pub use error::*;
pub use filesystem::*;

use std::cmp::Ordering;
use std::ffi::OsStr;
use std::fmt;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::str::FromStr;

use clap::ValueEnum;

use crate::hash::Hash;
#[cfg(feature = "jemalloc")]
use tikv_jemallocator::Jemalloc;

#[cfg(feature = "jemalloc")]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

#[derive(Debug, PartialEq, Eq, Clone, std::hash::Hash)]
pub enum RelativePath {
    Root,
    Path(PathBuf),
}

impl RelativePath {
    pub fn is_root(&self) -> bool {
        matches!(self, RelativePath::Root)
    }

    pub fn parent(&self) -> Self {
        match self {
            RelativePath::Root => RelativePath::Root,
            RelativePath::Path(path) => match path.parent() {
                None => RelativePath::Root,
                Some(parent) if parent.as_os_str().eq("") => RelativePath::Root,
                Some(parent) => RelativePath::Path(parent.to_path_buf()),
            },
        }
    }

    pub fn file_name(&self) -> Option<&OsStr> {
        match self {
            RelativePath::Root => None,
            RelativePath::Path(path) => path.file_name(),
        }
    }

    pub fn as_path(&self) -> &Path {
        match self {
            RelativePath::Root => Path::new(""),
            RelativePath::Path(path) => path.as_path(),
        }
    }

    pub fn join<P: AsRef<Path>>(&self, name: P) -> Self {
        match self {
            RelativePath::Root => RelativePath::Path(PathBuf::from(name.as_ref())),
            RelativePath::Path(path) => RelativePath::Path(path.join(name)),
        }
    }
}

impl fmt::Display for RelativePath {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        match self {
            RelativePath::Root => write!(f, ""),
            RelativePath::Path(path) => write!(f, "{}", path.display()),
        }
    }
}

impl PartialOrd<Self> for RelativePath {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for RelativePath {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (RelativePath::Root, RelativePath::Root) => Ordering::Equal,
            (RelativePath::Root, RelativePath::Path(_)) => Ordering::Less,
            (RelativePath::Path(_), RelativePath::Root) => Ordering::Greater,
            (RelativePath::Path(a), RelativePath::Path(b)) => a.cmp(b),
        }
    }
}

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
    object_type: ObjectType,
    object_id: ObjectID,

    // only contains basename of file
    file_name: PathBuf,
}

impl Object {
    pub fn new<P: Into<PathBuf>>(
        object_type: ObjectType,
        object_id: ObjectID,
        file_name: P,
    ) -> Self {
        Object {
            object_type,
            object_id,
            file_name: file_name.into(),
        }
    }

    pub fn new_tree<P: Into<PathBuf>>(object_id: ObjectID, file_name: P) -> Self {
        Object::new(ObjectType::Tree, object_id, file_name.into())
    }

    pub fn new_file<P: Into<PathBuf>>(object_id: ObjectID, file_name: P) -> Self {
        Object::new(ObjectType::File, object_id, file_name.into())
    }

    pub fn is_tree(&self) -> bool {
        self.object_type == ObjectType::Tree
    }

    pub fn is_file(&self) -> bool {
        self.object_type == ObjectType::File
    }

    pub fn size(&self) -> usize {
        // "tree" "\t" "d447b1ea40e6988b" "\t" string "\n"
        // 4 + 1 + 16 + 1 + str_len + 1
        23 + self.file_name.as_os_str().len()
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
        self.file_name.cmp(&other.file_name)
    }
}

impl fmt::Display for Object {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(
            f,
            "{}\t{}\t{}",
            self.object_type,
            self.object_id,
            self.file_name.display()
        )
    }
}

const MTL_DIR: &str = ".mtl";

#[derive(Debug)]
pub struct Context {
    // root of the repository
    root_dir: PathBuf,

    drop_cache: bool,
}

impl Context {
    pub fn new<P: Into<PathBuf>>(root_dir: P) -> Self {
        Context {
            root_dir: root_dir.into(),
            drop_cache: false,
        }
    }

    pub fn root_dir(&self) -> &Path {
        &self.root_dir
    }

    pub fn set_drop_cache(&mut self, drop_cache: bool) {
        self.drop_cache = drop_cache;
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

    pub fn object_files(&self) -> anyhow::Result<Vec<PathBuf>, ReadContentError> {
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
                        log::warn!(
                            "Unexpected directory in object directory: {}",
                            path.display()
                        );
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

    pub fn reference_dir(&self) -> PathBuf {
        self.root_dir.as_path().join(MTL_DIR).join("refs")
    }

    pub fn reference_file(&self, reference: &str) -> PathBuf {
        self.reference_dir().join(reference)
    }

    pub fn deref_object_ref(&self, object_ref: &ObjectRef) -> Result<ObjectID, ReadContentError> {
        match object_ref {
            ObjectRef::Reference(reference) if reference == "HEAD" => self.read_head(),
            ObjectRef::Reference(reference) => {
                let ref_file = self.reference_file(reference);
                let contents = fs::read_to_string(ref_file)?;
                let contents = contents.trim();

                Ok(contents.parse()?)
            }
            ObjectRef::ID(object_id) => Ok(*object_id),
        }
    }

    pub fn list_object_refs(&self) -> anyhow::Result<Vec<ObjectRef>, ReadContentError> {
        let dir_name = self.reference_dir();
        fs::create_dir_all(&dir_name)?;

        let mut object_refs = Vec::new();
        for entry in fs::read_dir(dir_name)? {
            let entry = entry?;

            let ft = entry.file_type()?;
            if ft.is_file() {
                // Parse as ObjectRef always succeeds
                let reference = entry
                    .file_name()
                    .to_str()
                    .ok_or(ParseError::EmptyToken)?
                    .parse()
                    .unwrap();
                object_refs.push(reference);
            } else {
                log::warn!(
                    "Unexpected directory in refs directory: {}",
                    entry.path().display()
                );
            }
        }
        object_refs.sort();
        Ok(object_refs)
    }

    pub fn write_object_ref<S: AsRef<str>>(
        &self,
        ref_name: S,
        object_id: ObjectID,
    ) -> io::Result<()> {
        let ref_dir = self.reference_dir();
        fs::create_dir_all(ref_dir)?;

        let ref_file = self.reference_file(ref_name.as_ref());
        fs::write(ref_file, object_id.to_string())?;
        Ok(())
    }

    pub fn delete_object_ref<S: AsRef<str>>(&self, ref_name: S) -> io::Result<()> {
        let reference_file = self.reference_file(ref_name.as_ref());
        fs::remove_file(reference_file)?;
        Ok(())
    }

    pub fn write_tree_contents<T: AsRef<Object>>(&self, entries: &[T]) -> io::Result<ObjectID> {
        let tree_contents = serialize_entries(entries)?;
        let object_id = ObjectID::from_contents(&tree_contents);

        let dir_name = self.object_dir(&object_id);
        let file_name = self.object_file(&object_id);

        fs::create_dir_all(dir_name)?;
        fs::write(file_name, tree_contents)?;

        Ok(object_id)
    }

    pub fn read_tree_contents(
        &self,
        object_id: &ObjectID,
    ) -> Result<Vec<Object>, ReadContentError> {
        let file_name = self.object_file(object_id);
        let tree_contents = fs::read_to_string(file_name)?;

        let mut objects = Vec::new();
        for line in tree_contents.lines() {
            let mut parts = line.split('\t');
            let object_type: ObjectType = parts.next().ok_or(ParseError::EmptyToken)?.parse()?;
            let object_id: ObjectID = parts.next().ok_or(ParseError::EmptyToken)?.parse()?;
            let file_name = PathBuf::from(parts.next().ok_or(ParseError::EmptyToken)?);

            objects.push(Object::new(object_type, object_id, file_name));
        }

        Ok(objects)
    }

    pub fn write_head(&self, object_id: &ObjectID) -> io::Result<()> {
        let head_name = self.head_file();
        fs::write(head_name, object_id.to_string())?;

        Ok(())
    }

    pub fn read_head(&self) -> anyhow::Result<ObjectID, ReadContentError> {
        let head = fs::read_to_string(self.head_file())?;
        let head = head.trim();

        Ok(head.parse()?)
    }
}

// serialize entries should be called with sorted entries
fn serialize_entries<T: AsRef<Object>>(entries: &[T]) -> io::Result<Vec<u8>> {
    let size = entries.iter().map(|e| e.as_ref().size()).sum();

    let mut buf = Vec::with_capacity(size);
    for entry in entries {
        let entry = entry.as_ref();
        writeln!(&mut buf, "{}", entry)?;
    }

    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::ffi::OsString;
    use std::path::Path;

    #[test]
    fn test_relative_path() {
        let root = RelativePath::Root;
        assert_eq!(root.is_root(), true);
        assert_eq!(root.file_name(), None);

        let mut m = HashMap::new();
        m.insert(root.clone(), 1);
        assert_eq!(m.get(&root), Some(&1));
        assert_eq!(m.get(&RelativePath::Root), Some(&1));

        let os_string_path = OsString::from("foo");
        let path = RelativePath::Path(PathBuf::from(os_string_path.clone()));
        assert_eq!(path.is_root(), false);
        assert_eq!(path.parent(), RelativePath::Root);
        assert_eq!(path.file_name(), Some(os_string_path.as_os_str()));

        let path = RelativePath::Path(PathBuf::from("foo/bar"));
        assert_eq!(path.is_root(), false);
        assert_eq!(path.parent(), RelativePath::Path(PathBuf::from("foo")));

        assert_eq!(path.parent().parent().is_root(), true);
    }

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

    #[test]
    fn test_object_size() {
        let object_id = ObjectID::from_hex("d447b1ea40e6988b").unwrap();
        let objects = vec![
            Object::new(ObjectType::File, object_id.clone(), PathBuf::from("a")),
            Object::new(ObjectType::File, object_id.clone(), PathBuf::from("aa")),
            Object::new(ObjectType::File, object_id.clone(), PathBuf::from("aあ")),
            Object::new(ObjectType::File, object_id.clone(), PathBuf::from("あ")),
            Object::new(ObjectType::File, object_id.clone(), PathBuf::from("ああ")),
        ];
        assert_eq!(objects[0].size(), 24);
        assert_eq!(objects[1].size(), 25);
        assert_eq!(objects[2].size(), 27);
        assert_eq!(objects[3].size(), 26);
        assert_eq!(objects[4].size(), 29);
    }

    #[test]
    fn test_object_ref() {
        assert_eq!(
            "d447b1ea40e6988b".parse::<ObjectRef>().unwrap(),
            ObjectRef::new_id(ObjectID::from_hex("d447b1ea40e6988b").unwrap())
        );
        assert_eq!(
            "d447b1ea40e6988".parse::<ObjectRef>().unwrap(),
            ObjectRef::new_reference("d447b1ea40e6988")
        );
        assert_eq!(
            "invalid_hex".parse::<ObjectRef>().unwrap(),
            ObjectRef::new_reference("invalid_hex")
        );
    }
}
