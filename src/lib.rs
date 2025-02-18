pub(crate) mod builder;
pub(crate) mod cache;
pub mod commands;
pub mod data;
pub mod error;
pub(crate) mod filesystem;
mod filter;
pub mod hash;
pub mod path;
pub(crate) mod progress;

use std::cmp::Ordering;
use std::collections::HashSet;
use std::ffi::OsStr;
use std::fmt;
use std::fs;
use std::io::{self, Write};
use std::path::{Components, Path, PathBuf};
use std::str::FromStr;
use std::time::Duration;

use redb::{ReadableTable, TableDefinition};

use crate::cache::{Cache, CacheValue};
pub use crate::data::*;
pub use crate::error::*;
pub use crate::filesystem::*;
use crate::path::RelativePath;
#[cfg(feature = "jemalloc")]
use tikv_jemallocator::Jemalloc;

#[cfg(feature = "jemalloc")]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

#[derive(Debug, PartialEq, Eq, Clone, std::hash::Hash)]
pub struct ObjectExpr {
    object_ref: ObjectRef,
    path: Option<PathBuf>,
}

impl FromStr for ObjectExpr {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut it = s.splitn(2, ':');

        let r#ref = it.next().ok_or("format invalid".to_string())?;
        // set None if path is empty
        let path = it.next().filter(|p| !p.is_empty()).map(PathBuf::from);

        Ok(Self {
            object_ref: ObjectRef::from_str(r#ref)?,
            path,
        })
    }
}

impl fmt::Display for ObjectExpr {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        match &self.path {
            Some(path) => write!(f, "{}:{}", self.object_ref, path.display()),
            None => write!(f, "{}", self.object_ref),
        }
    }
}

impl ObjectExpr {
    pub fn resolve(&self, ctx: &Context) -> Result<ObjectID, ReadContentError> {
        match &self.path {
            Some(path) => ctx.search_object(&self.object_ref, path),
            None => Ok(ctx.deref_object_ref(&self.object_ref)?),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, std::hash::Hash)]
pub struct Object {
    object_type: ObjectType,
    object_id: ObjectID,

    // only contains basename of file
    file_path: RelativePath,
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
            file_path: RelativePath::from(file_name),
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
        23 + self.file_path.as_os_str().len()
    }

    pub fn as_object_ref(&self) -> ObjectRef {
        ObjectRef::new_id(self.object_id)
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
        self.file_path.cmp(&other.file_path)
    }
}

impl fmt::Display for Object {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(
            f,
            "{}\t{}\t{}",
            self.object_type,
            self.object_id,
            self.file_path.file_name().unwrap_or_default().display()
        )
    }
}

const MTL_DIR: &str = ".mtl";
pub(crate) const PACKED_OBJECTS_TABLE: TableDefinition<ObjectID, Vec<u8>> =
    TableDefinition::new("packed-objects");

pub struct Context {
    // root of the repository
    root_dir: PathBuf,

    packed_db: Option<redb::Database>,

    cache: Cache,
}

impl Context {
    pub fn new<P: Into<PathBuf>>(root_dir: P) -> anyhow::Result<Self> {
        let root_dir = root_dir.into();
        let packed_db_file = root_dir.join(MTL_DIR).join("pack").join("packed.redb");
        let cache_db_file = root_dir.join(MTL_DIR).join("cache").join("cache.redb");

        let packed_db = packed_db_file
            .exists()
            .then(|| redb::Database::open(&packed_db_file))
            .transpose()?;

        let cache = Cache::open(&cache_db_file, Duration::from_secs(1))?;
        Ok(Context {
            root_dir,
            packed_db,
            cache,
        })
    }

    #[inline]
    pub fn root_dir(&self) -> &Path {
        &self.root_dir
    }

    #[inline]
    pub fn objects_dir(&self) -> PathBuf {
        self.root_dir.as_path().join(MTL_DIR).join("objects")
    }

    #[inline]
    pub fn pack_dir(&self) -> PathBuf {
        self.root_dir.as_path().join(MTL_DIR).join("pack")
    }

    #[inline]
    pub fn cache_dir(&self) -> PathBuf {
        self.root_dir.as_path().join(MTL_DIR).join("cache")
    }

    pub fn pack_file(&self) -> PathBuf {
        self.pack_dir().join("packed.redb")
    }

    pub fn cache_db_file(&self) -> PathBuf {
        self.cache_dir().join("cache.redb")
    }

    #[inline]
    pub fn object_dir(&self, object_id: &ObjectID) -> PathBuf {
        self.objects_dir().join(&object_id.to_string()[0..2])
    }

    pub fn object_file(&self, object_id: &ObjectID) -> PathBuf {
        let object_string = object_id.to_string();
        self.objects_dir()
            .join(&object_string[0..2])
            .join(&object_string[2..])
    }

    pub fn read_object(&self, object_id: &ObjectID) -> anyhow::Result<Vec<u8>, ReadContentError> {
        let object_file = self.object_file(object_id);
        if let Ok(contents) = fs::read(object_file) {
            return Ok(contents);
        }

        let Some(packed_db) = &self.packed_db else {
            return Err(ReadContentError::ObjectNotFound);
        };

        let read_txn = packed_db.begin_read()?;
        let table = read_txn.open_table(PACKED_OBJECTS_TABLE)?;
        let ret = match table.get(object_id)? {
            Some(v) => Ok(v.value()),
            None => Err(ReadContentError::ObjectNotFound),
        };
        ret
    }

    pub fn object_files(&self) -> anyhow::Result<Vec<PathBuf>, ReadContentError> {
        let entries = fs::read_dir(self.objects_dir())?;

        let mut object_files = Vec::new();
        for entry in entries {
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

    pub fn list_object_ids(&self) -> anyhow::Result<Vec<ObjectID>, ReadContentError> {
        let mut object_ids = HashSet::new();
        for entry in self.object_files()? {
            let dir_name = entry
                .parent()
                .and_then(Path::file_name)
                .and_then(OsStr::to_str)
                .ok_or(ParseError::EmptyToken)?;
            let file_name = entry
                .file_name()
                .and_then(OsStr::to_str)
                .ok_or(ParseError::EmptyToken)?;

            let mut buf = String::with_capacity(dir_name.len() + file_name.len());
            buf.push_str(dir_name);
            buf.push_str(file_name);

            let object_id: ObjectID = buf.parse()?;

            object_ids.insert(object_id);
            assert_eq!(object_id.to_string(), buf)
        }

        if let Some(packed_db) = &self.packed_db {
            let read_txn = packed_db.begin_read()?;

            let table = read_txn.open_table(PACKED_OBJECTS_TABLE)?;
            for range in table.iter()? {
                let (object_id, _) = range?;
                object_ids.insert(object_id.value());
            }
        }

        Ok(object_ids.into_iter().collect())
    }

    pub fn head_file(&self) -> PathBuf {
        let head_name = self.root_dir.as_path();
        head_name.join(MTL_DIR).join("HEAD")
    }

    pub fn reference_dir(&self) -> PathBuf {
        self.root_dir.as_path().join(MTL_DIR).join("refs")
    }

    pub fn reference_file<P: AsRef<Path>>(&self, reference: P) -> PathBuf {
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

    pub fn search_object<P: AsRef<Path>>(
        &self,
        base: &ObjectRef,
        path: P,
    ) -> Result<ObjectID, ReadContentError> {
        let components = path.as_ref().components();
        let routes = self.inner_search_object_with_routes(base, components)?;
        if routes.is_empty() {
            return Err(ReadContentError::ObjectNotFound);
        }
        Ok(routes[0])
    }

    pub fn search_object_with_routes<P: AsRef<Path>>(
        &self,
        base: &ObjectRef,
        path: P,
    ) -> Result<Vec<ObjectID>, ReadContentError> {
        let components = path.as_ref().components();
        self.inner_search_object_with_routes(base, components)
    }

    fn inner_search_object_with_routes(
        &self,
        object_ref: &ObjectRef,
        mut components: Components,
    ) -> Result<Vec<ObjectID>, ReadContentError> {
        let Some(file_path) = components.next() else {
            return Ok(vec![]);
        };

        let object = self.deref_object_ref(object_ref)?;
        let contents = self.read_tree_contents(&object)?;
        for content in contents {
            let Some(file_name) = content.file_path.file_name() else {
                continue;
            };

            if file_name.as_os_str() == file_path.as_os_str() {
                let object_ref = ObjectRef::new_id(content.object_id);
                let mut results = self.inner_search_object_with_routes(&object_ref, components)?;
                results.push(content.object_id);
                return Ok(results);
            }
        }
        Err(ReadContentError::ObjectNotFound)
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
        let tree_contents = self.read_object(object_id)?;
        let tree_contents = String::from_utf8(tree_contents)?;

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

    pub fn read_cache<P: AsRef<Path>>(&self, key: P) -> Option<CacheValue> {
        match self.cache.get(key) {
            Ok(cache_value) => {
                if let Some(cache_value) = cache_value {
                    return Some(cache_value);
                }
            }
            Err(e) => log::warn!("failed to read cache: {}", e),
        }
        None
    }

    pub fn write_cache<P: AsRef<Path>>(&self, key: P, value: CacheValue) {
        match self.cache.insert(key, value) {
            Ok(_) => {}
            Err(e) => log::warn!("failed to write cache: {}", e),
        }
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
    use std::path::Path;

    #[test]
    fn test_context() {
        let ctx = Context::new("/tmp").unwrap();
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
        compare_target.sort_by(|a, b| a.file_path.cmp(&b.file_path));
        assert_eq!(objects, compare_target);
        assert_eq!(
            vec![
                RelativePath::from("a"),
                RelativePath::from("b"),
                RelativePath::from("c"),
                RelativePath::from("d")
            ],
            objects.into_iter().map(|o| o.file_path).collect::<Vec<_>>()
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
    fn test_object_display() {
        let object = Object::new_file(
            ObjectID::from_hex("d447b1ea40e6988b").unwrap(),
            PathBuf::from("foo/bar/baz"),
        );
        assert_eq!(format!("{}", object), "file\td447b1ea40e6988b\tbaz");
    }

    #[test]
    fn test_object_expr() {
        let object_expr = "d447b1ea40e6988b:foo/bar/baz"
            .parse::<ObjectExpr>()
            .unwrap();
        assert_eq!(
            object_expr.object_ref,
            ObjectRef::new_id(ObjectID::from_hex("d447b1ea40e6988b").unwrap())
        );
        assert_eq!(object_expr.path, Some(PathBuf::from("foo/bar/baz")));

        let object_expr = "d447b1ea40e6988b".parse::<ObjectExpr>().unwrap();
        assert_eq!(
            object_expr.object_ref,
            ObjectRef::new_id(ObjectID::from_hex("d447b1ea40e6988b").unwrap())
        );
        assert_eq!(object_expr.path, None);

        let object_expr = "d447b1ea40e6988b:".parse::<ObjectExpr>().unwrap();
        assert_eq!(
            object_expr.object_ref,
            ObjectRef::new_id(ObjectID::from_hex("d447b1ea40e6988b").unwrap())
        );
        assert_eq!(object_expr.path, None);
    }
}
