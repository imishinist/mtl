use crate::{
    read_tree_contents, ref_head_name, write_head, write_tree_contents, Object, ObjectID,
    ObjectType, MTL_DIR,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::{env, fs, io};

use clap::{Args, Subcommand};
use itertools::Itertools;
use rayon::prelude::*;

struct FileEntry {
    mode: ObjectType,
    path: PathBuf,
    depth: usize,
}

impl FileEntry {
    fn new_file<P: AsRef<Path>>(path: P, depth: usize) -> Self {
        Self {
            mode: ObjectType::File,
            path: path.as_ref().to_path_buf(),
            depth,
        }
    }

    fn new_dir<P: AsRef<Path>>(path: P, depth: usize) -> Self {
        Self {
            mode: ObjectType::Tree,
            path: path.as_ref().to_path_buf(),
            depth,
        }
    }
}

fn list_all_files<P: AsRef<Path>>(
    relative_path: P,
    depth: usize,
) -> io::Result<(usize, Vec<FileEntry>)> {
    let mut ret = Vec::new();
    let dir = fs::read_dir(relative_path)?;

    let mut max = depth;
    for entry in dir {
        let entry = entry?;
        let path = entry.path();

        if !filter(&path) {
            continue;
        }

        let ft = entry.file_type()?;
        if ft.is_dir() {
            ret.push(FileEntry::new_dir(&path, depth));
            let (d, sub) = list_all_files(path, depth + 1)?;
            max = max.max(d);
            ret.extend(sub);
        } else {
            ret.push(FileEntry::new_file(&path, depth));
        }
    }
    Ok((max, ret))
}

fn merge_hashmap<K: std::hash::Hash + Eq + Clone, V: Clone>(
    map1: HashMap<K, Vec<V>>,
    map2: HashMap<K, Vec<V>>,
) -> HashMap<K, Vec<V>> {
    map2.iter().fold(map1, |mut acc, (k, vs)| {
        acc.entry(k.clone()).or_insert(vec![]).extend_from_slice(vs);
        acc
    })
}

fn filter(path: &Path) -> bool {
    let path = path.to_str().unwrap();
    !(path.contains(".git")
        || path.contains(MTL_DIR)
        || path.contains("target")
        || path.contains(".idea"))
}

fn process_tree_content(
    map: &HashMap<PathBuf, Vec<Object>>,
    entry: &FileEntry,
) -> io::Result<Option<Object>> {
    let objects = match map.get(&entry.path) {
        Some(objects) => objects.iter().sorted().collect::<Vec<_>>(),
        None => return Ok(None), // empty dir
    };

    let file_name = PathBuf::from(
        entry
            .path
            .file_name()
            .ok_or(io::Error::new(io::ErrorKind::NotFound, "not found"))?,
    );
    let object_id = write_tree_contents(&objects)?;
    Ok(Some(Object::new_tree(object_id, file_name)))
}

fn process_file_content(entry: &FileEntry) -> io::Result<Object> {
    let contents = fs::read(&entry.path)?;
    let object_id = ObjectID::from_contents(&contents);
    let file_name = PathBuf::from(
        entry
            .path
            .file_name()
            .ok_or(io::Error::new(io::ErrorKind::NotFound, "not found"))?,
    );

    Ok(Object::new_file(object_id, file_name))
}

fn parallel_walk<P: AsRef<Path>>(
    cwd: P,
    files: Vec<FileEntry>,
    depth: usize,
    map: HashMap<PathBuf, Vec<Object>>,
) -> io::Result<Object> {
    if depth == 0 {
        assert!(map.len() == 1);

        let path = cwd.as_ref().to_path_buf();
        let mut objects = map
            .get(&path)
            .ok_or(io::Error::new(io::ErrorKind::NotFound, "not found"))?
            .clone();
        objects.par_sort_unstable();

        let object_id = write_tree_contents(&objects)?;
        return Ok(Object::new_tree(object_id, path));
    }
    let (target, rest) = files
        .into_iter()
        .partition::<Vec<_>, _>(|entry| entry.depth == depth);

    let new_map = target
        .par_iter()
        .fold(
            HashMap::new,
            |mut m: HashMap<PathBuf, Vec<Object>>, entry| {
                let object = match entry.mode {
                    ObjectType::Tree => match process_tree_content(&map, &entry).unwrap() {
                        Some(object) => object,
                        None => return m,
                    },
                    ObjectType::File => process_file_content(&entry).unwrap(),
                };

                let parent = PathBuf::from(entry.path.parent().unwrap());
                m.entry(parent).or_insert(vec![]).push(object);
                m
            },
        )
        .reduce(HashMap::new, merge_hashmap);
    parallel_walk(cwd, rest, depth - 1, new_map)
}

#[derive(Args, Debug)]
pub struct LocalBuild {}

impl LocalBuild {
    pub fn run(&self) -> io::Result<()> {
        let cwd = env::current_dir()?;

        log::info!("cwd: {}", cwd.display());
        let (max_depth, files) = list_all_files(&cwd, 1)?;
        log::info!("max_depth: {}, files: {}", max_depth, files.len());

        let object = parallel_walk(&cwd, files, max_depth, HashMap::new())?;
        write_head(&object.object_id)?;
        println!("HEAD: {}", object.object_id);
        Ok(())
    }
}

#[derive(Subcommand)]
pub enum LocalCommand {
    Build(LocalBuild),
}

impl LocalCommand {
    pub fn run(&self) -> io::Result<()> {
        match self {
            LocalCommand::Build(cmd) => cmd.run(),
        }
    }
}

#[derive(Args, Debug)]
pub struct PrintTreeCommand {
    #[clap(long, short)]
    dir: Option<String>,

    #[clap(long, short)]
    object_id: Option<String>,

    #[clap(long, short)]
    r#type: Option<ObjectType>,

    #[clap(long, short)]
    max_depth: Option<usize>,
}

impl PrintTreeCommand {
    pub fn run(&self) -> io::Result<()> {
        let dir = self
            .dir
            .as_ref()
            .map(|s| PathBuf::from(s))
            .unwrap_or_else(|| env::current_dir().unwrap());
        env::set_current_dir(&dir)?;

        let object_id = self
            .object_id
            .as_ref()
            .map(|s| ObjectID::from_hex(s))
            .unwrap_or_else(|| {
                let head = fs::read_to_string(ref_head_name())?;
                Ok(ObjectID::from_hex(&head)?)
            })?;

        let object_type = self.r#type.as_ref();

        println!("tree {}\t<root>", object_id);
        Self::print_tree("", &object_id, object_type, 0, self.max_depth)?;
        Ok(())
    }

    fn print_tree<P: AsRef<Path>>(
        parent: P,
        object_id: &ObjectID,
        object_type: Option<&ObjectType>,
        depth: usize,
        max_depth: Option<usize>,
    ) -> io::Result<()> {
        if let Some(max_depth) = max_depth {
            if depth >= max_depth {
                return Ok(());
            }
        }

        let parent = parent.as_ref().to_path_buf();
        let objects = read_tree_contents(object_id)?;
        for object in &objects {
            match object.object_type {
                ObjectType::Tree => {
                    if object_type.is_none() || object_type == Some(&ObjectType::Tree) {
                        println!(
                            "tree {}\t{}/",
                            object.object_id,
                            parent.join(&object.file_name).display()
                        );
                    }
                    Self::print_tree(
                        &parent.join(&object.file_name),
                        &object.object_id,
                        object_type,
                        depth + 1,
                        max_depth,
                    )?;
                }
                ObjectType::File => {
                    if object_type.is_none() || object_type == Some(&ObjectType::File) {
                        println!(
                            "file {}\t{}",
                            object.object_id,
                            parent.join(&object.file_name).display()
                        );
                    }
                }
            }
        }

        Ok(())
    }
}
