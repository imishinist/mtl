use std::collections::HashMap;
use std::ffi::OsString;
use std::fs::File;
use std::io::{BufRead, Read};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::{env, fs, io};

use clap::Args;
use itertools::Itertools;
use rayon::prelude::*;

use crate::{write_head, write_tree_contents, Object, ObjectID, ObjectType, MTL_DIR};

#[derive(Debug)]
struct FileEntry {
    mode: ObjectType,
    path: PathBuf,
    depth: usize,
}

impl FileEntry {
    fn new<P: Into<PathBuf>>(mode: ObjectType, path: P, depth: usize) -> Self {
        Self {
            mode,
            path: path.into(),
            depth,
        }
    }

    fn new_file<P: Into<PathBuf>>(path: P, depth: usize) -> Self {
        Self::new(ObjectType::File, path, depth)
    }

    fn new_dir<P: Into<PathBuf>>(path: P, depth: usize) -> Self {
        Self::new(ObjectType::Tree, path, depth)
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
pub struct Build {
    /// Working directory.
    #[clap(short, long, value_name = "directory", verbatim_doc_comment)]
    cwd: Option<OsString>,

    /// The input file containing a list of files to be scanned.
    /// By default, it scans all files in the current directory.
    /// If you want to receive from standard input, specify "-".
    #[clap(short, long, value_name = "input-file", verbatim_doc_comment)]
    input: Option<OsString>,

    /// If true, don't write the object ID of the root tree to HEAD.
    #[clap(short, long, default_value_t = false, verbatim_doc_comment)]
    no_write_head: bool,
}

impl Build {
    pub fn run(&self) -> io::Result<()> {
        let cwd = match &self.cwd {
            Some(cwd) => cwd.clone(),
            None => env::current_dir()?.into_os_string(),
        };
        env::set_current_dir(&cwd)?;

        log::info!("cwd: {:?}", cwd);
        let (max_depth, files) = self.target_files(&cwd)?;
        log::info!("max_depth: {}, files: {}", max_depth, files.len());

        let object = parallel_walk(&cwd, files, max_depth, HashMap::new())?;
        if self.no_write_head {
            println!("HEAD: {}", object.object_id);
        } else {
            write_head(&object.object_id)?;
            println!("Written HEAD: {}", object.object_id);
        }
        Ok(())
    }

    fn target_files<P: AsRef<Path>>(&self, cwd: P) -> io::Result<(usize, Vec<FileEntry>)> {
        let Some(input) = &self.input else {
            return list_all_files(cwd, 1);
        };
        let input = input.as_os_str();

        let input: BufReaderWrapper<Box<dyn BufRead>> = if input.eq("-") {
            let stdin = io::stdin().lock();
            let reader = io::BufReader::new(stdin);
            BufReaderWrapper::new(Box::new(reader))
        } else {
            let reader = io::BufReader::new(File::open(input)?);
            BufReaderWrapper::new(Box::new(reader))
        };

        let mut max_depth = 0;
        let mut ret = Vec::new();
        for line in input {
            let line = line?;
            let file_path = line.trim();

            let is_dir = file_path.ends_with('/');
            let file_path = file_path.trim_start_matches("./").trim_end_matches('/');
            let depth = file_path.split('/').count();
            max_depth = max_depth.max(depth);

            let mut path = PathBuf::from(file_path);
            if !filter(&path) {
                continue;
            }

            if path.is_relative() {
                path = cwd.as_ref().join(path);
            } else {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "absolute path is not supported",
                ));
            }

            if is_dir {
                ret.push(FileEntry::new_dir(&path, depth));
            } else {
                ret.push(FileEntry::new_file(&path, depth));
            }
        }

        Ok((max_depth, ret))
    }
}

pub struct BufReaderWrapper<R: BufRead> {
    reader: R,
    buf: Rc<String>,
}

fn new_buf() -> Rc<String> {
    Rc::new(String::with_capacity(1024)) // Tweakable capacity
}

impl<R: BufRead> BufReaderWrapper<R> {
    pub fn new(inner: R) -> Self {
        let buf = new_buf();
        Self { reader: inner, buf }
    }
}

impl<R: BufRead> Iterator for BufReaderWrapper<R> {
    type Item = io::Result<Rc<String>>;

    fn next(&mut self) -> Option<Self::Item> {
        let buf = match Rc::get_mut(&mut self.buf) {
            Some(buf) => {
                buf.clear();
                buf
            }
            None => {
                self.buf = new_buf();
                Rc::make_mut(&mut self.buf)
            }
        };

        self.reader
            .read_line(buf)
            .map(|u| {
                if u == 0 {
                    None
                } else {
                    Some(Rc::clone(&self.buf))
                }
            })
            .transpose()
    }
}

impl<R: BufRead> Read for BufReaderWrapper<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.reader.read(buf)
    }
}

impl<R: BufRead> BufRead for BufReaderWrapper<R> {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        self.reader.fill_buf()
    }

    fn consume(&mut self, amt: usize) {
        self.reader.consume(amt)
    }
}
