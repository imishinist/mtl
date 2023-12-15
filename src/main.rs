use std::env;
use std::fmt::{Display, Error, Formatter};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::str::FromStr;

const MTL_DIR: &str = ".mtl";

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

#[derive(Debug, PartialEq)]
struct Object {
    object_type: ObjectType,
    object_id: ObjectID,

    // only contains basename of file
    file_name: PathBuf,
}

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

fn write_tree_contents(entries: &[Object]) -> io::Result<ObjectID> {
    let tree_contents = serialize_entries(&entries)?;
    let object_id = ObjectID::from_contents(&tree_contents);

    let dir_name = object_dir_name(&object_id);
    let file_name = object_file_name(&object_id);

    fs::create_dir_all(&dir_name)?;
    fs::write(&file_name, tree_contents)?;

    Ok(object_id)
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
}

fn filter(path: &Path) -> bool {
    let path = path.to_str().unwrap();
    !(path.contains(".git")
        || path.contains(MTL_DIR)
        || path.contains("target")
        || path.contains(".idea"))
}

fn walk_dir(path: &Path) -> io::Result<Vec<Object>> {
    let mut objects = Vec::new();

    for entry in path.read_dir()? {
        let path = entry?.path();
        if !filter(&path) {
            continue;
        }

        let file_name = PathBuf::from(path.file_name().unwrap());
        if path.is_dir() {
            let object_type = ObjectType::Tree;
            let entries = walk_dir(&path)?;
            let object_id = write_tree_contents(&entries)?;

            log::info!("{}\t{}\t{}", object_type, object_id, file_name.display());
            objects.push(Object {
                object_type,
                object_id,
                file_name,
            });
        } else {
            let object_type = ObjectType::File;
            let object_id = ObjectID::from_contents(&fs::read(path)?);

            log::info!("{}\t{}\t{}", object_type, object_id, file_name.display());
            objects.push(Object {
                object_type,
                object_id,
                file_name,
            });
        }
    }

    Ok(objects)
}

fn main() -> io::Result<()> {
    env_logger::init();

    let cwd = env::current_dir()?;
    let objects = walk_dir(&cwd)?;
    for object in objects {
        println!(
            "{} {} {}",
            object.object_type,
            object.object_id,
            object.file_name.display()
        );
    }
    Ok(())
}
