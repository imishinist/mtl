pub mod commands;

use std::cmp::Ordering;
pub use commands::*;

use std::fmt;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::str::FromStr;

#[cfg(feature="jemalloc")]
use tikv_jemallocator::Jemalloc;

#[cfg(feature="jemalloc")]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

#[derive(Debug, PartialEq, Eq, Clone)]
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
    inner: [u8; 16],
}

impl ObjectID {
    pub fn new(inner: [u8; 16]) -> Self {
        ObjectID { inner }
    }

    pub fn from_hex<S: AsRef<str>>(hex: S) -> io::Result<Self> {
        let hex = hex.as_ref();
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

impl fmt::Display for ObjectID {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        for byte in self.inner.iter() {
            write!(f, "{:02x}", byte)?;
        }
        Ok(())
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

fn object_dir_name(object_id: &ObjectID) -> PathBuf {
    let object_id = object_id.to_string();

    let mut dir_name = PathBuf::new();
    dir_name.push(MTL_DIR);
    dir_name.push("objects");
    dir_name.push(&object_id[0..2]);

    dir_name
}

fn object_file_name(object_id: &ObjectID) -> PathBuf {
    let object_id = object_id.to_string();

    let mut file_name = PathBuf::new();
    file_name.push(MTL_DIR);
    file_name.push("objects");
    file_name.push(&object_id[0..2]);
    file_name.push(&object_id[2..]);

    file_name
}

fn ref_head_name() -> PathBuf {
    let mut head_name = PathBuf::new();
    head_name.push(MTL_DIR);
    head_name.push("HEAD");

    head_name
}

// serialize entries should be called with sorted entries
fn serialize_entries(entries: &[Object]) -> io::Result<Vec<u8>> {
    // Note: should decrease allocation?
    let mut buf = Vec::new();

    for entry in entries {
        buf.extend_from_slice(format!("{}\t{}\t", entry.object_type, entry.object_id).as_bytes());
        buf.extend_from_slice(entry.file_name.to_str().unwrap().as_bytes());
        buf.push(b'\n');
    }

    Ok(buf)
}

pub fn write_tree_contents(entries: &[Object]) -> io::Result<ObjectID> {
    let tree_contents = serialize_entries(&entries)?;
    let object_id = ObjectID::from_contents(&tree_contents);

    let dir_name = object_dir_name(&object_id);
    let file_name = object_file_name(&object_id);

    fs::create_dir_all(&dir_name)?;
    fs::write(&file_name, tree_contents)?;

    Ok(object_id)
}

fn write_head(object_id: &ObjectID) -> io::Result<()> {
    let head_name = ref_head_name();
    fs::write(head_name, object_id.to_string())?;

    Ok(())
}

fn read_tree_contents(object_id: &ObjectID) -> io::Result<Vec<Object>> {
    let file_name = object_file_name(object_id);
    let tree_contents = fs::read_to_string(file_name)?;

    let mut objects = Vec::new();
    for line in tree_contents.lines() {
        let mut parts = line.split('\t');
        let object_type = parts.next().unwrap().parse::<ObjectType>().unwrap();
        let object_id = parts.next().unwrap().parse::<ObjectID>().unwrap();
        let file_name = PathBuf::from(parts.next().unwrap());

        objects.push(Object {
            object_type,
            object_id,
            file_name,
        });
    }

    Ok(objects)
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

    #[test]
    fn test_object_dir_name() {
        assert_eq!(
            object_dir_name(&ObjectID::from_hex("0123456789abcdef0123456789abcdef").unwrap()),
            Path::new(".mtl/objects/01")
        );
    }

    #[test]
    fn test_object_file_name() {
        assert_eq!(
            object_file_name(&ObjectID::from_hex("0123456789abcdef0123456789abcdef").unwrap()),
            Path::new(".mtl/objects/01/23456789abcdef0123456789abcdef")
        );
    }

    #[test]
    fn test_object_order() {
        let object_id = ObjectID::from_hex("0123456789abcdef0123456789abcdef").unwrap();
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
            vec![PathBuf::from("a"), PathBuf::from("b"), PathBuf::from("c"), PathBuf::from("d")],
            objects.into_iter().map(|o| o.file_name).collect::<Vec<_>>()
        );
    }
}
