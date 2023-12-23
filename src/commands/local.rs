use std::collections::HashMap;
use std::ffi::OsString;
use std::fs::File;
use std::io::{BufRead, Read};
use std::os::unix::fs::FileTypeExt;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::{env, fs, io};

use anyhow::anyhow;
use clap::Args;
use ignore::{WalkBuilder, WalkState};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use itertools::Itertools;
use rayon::prelude::*;

use crate::{Context, Object, ObjectID, ObjectType, MTL_DIR};

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

fn list_all_files(
    ctx: &Context,
    hidden: bool,
) -> anyhow::Result<(usize, Vec<FileEntry>, u64, u64)> {
    let num_cpu = 4;

    let (tx, rx) = crossbeam_channel::bounded::<FileEntry>(100);
    let output_thread = std::thread::spawn(move || {
        let mut result = Vec::new();
        let mut max_depth = 0;
        let mut files = 0;
        let mut dirs = 0;

        for entry in rx {
            max_depth = max_depth.max(entry.depth);
            match entry.mode {
                ObjectType::File => files += 1,
                ObjectType::Tree => dirs += 1,
            }
            result.push(entry);
        }
        (max_depth, result, files, dirs)
    });

    let root_dir = ctx.root_dir();
    let walker = WalkBuilder::new(&root_dir)
        .hidden(hidden)
        .threads(num_cpu)
        .build_parallel();
    walker.run(|| {
        let tx = tx.clone();
        Box::new(move |entry| {
            let entry = match entry {
                Ok(entry) => entry,
                Err(e) => {
                    log::warn!("ignored: {}", e);
                    return WalkState::Continue;
                }
            };

            let depth = entry.depth();
            let ft = match entry.file_type() {
                Some(ft) => ft,
                None => {
                    return WalkState::Continue;
                }
            };

            let path = entry.path().strip_prefix(root_dir).unwrap();
            if is_ignore_dir(&path) {
                return WalkState::Continue;
            }

            if ft.is_dir() {
                tx.send(FileEntry::new_dir(path, depth)).unwrap();
            } else if ft.is_file() {
                tx.send(FileEntry::new_file(path, depth)).unwrap();
            } else {
                log::warn!(
                    "ignored: not supported file type: {} \"{}\"",
                    format_filetype(&ft),
                    path.display()
                );
                return WalkState::Continue;
            }
            WalkState::Continue
        })
    });
    drop(tx);

    Ok(output_thread.join().unwrap())
}

fn merge_hashmap<K: std::hash::Hash + Eq + Clone, V: Clone>(
    map1: HashMap<K, Vec<V>>,
    map2: HashMap<K, Vec<V>>,
) -> HashMap<K, Vec<V>> {
    map2.into_iter().fold(map1, |mut acc, (k, vs)| {
        acc.entry(k).or_insert(vec![]).extend(vs);
        acc
    })
}

fn is_ignore_dir(path: &Path) -> bool {
    let path = path.to_str().unwrap();
    path.contains(MTL_DIR) || path.contains(".git")
}

fn process_tree_content(
    ctx: &Context,
    map: &HashMap<PathBuf, Vec<Object>>,
    entry: &FileEntry,
) -> io::Result<Option<Object>> {
    let objects = match map.get(&entry.path) {
        Some(objects) => objects.iter().sorted().collect::<Vec<_>>(),
        None => return Ok(None), // empty dir
    };

    let file_name = PathBuf::from(entry.path.file_name().ok_or(io::Error::new(
        io::ErrorKind::NotFound,
        "failed to get file_name",
    ))?);
    let object_id = ctx.write_tree_contents(&objects)?;
    Ok(Some(Object::new_tree(object_id, file_name)))
}

fn process_file_content(ctx: &Context, entry: &FileEntry) -> io::Result<Object> {
    let path = ctx.root_dir().join(&entry.path);
    let contents = fs::read(path)?;
    let object_id = ObjectID::from_contents(&contents);
    let file_name = PathBuf::from(entry.path.file_name().ok_or(io::Error::new(
        io::ErrorKind::NotFound,
        "failed to get file_name",
    ))?);

    Ok(Object::new_file(object_id, file_name))
}

fn parallel_walk(
    ctx: &Context,
    pb_files: &ProgressBar,
    pb_dirs: &ProgressBar,
    files: Vec<FileEntry>,
    depth: usize,
    map: HashMap<PathBuf, Vec<Object>>,
) -> io::Result<Object> {
    if depth == 0 {
        assert!(map.len() == 1);

        let path = PathBuf::from("");
        let mut objects = map
            .get(&path)
            .ok_or(io::Error::new(io::ErrorKind::NotFound, "not found"))?
            .clone();
        objects.par_sort_unstable();

        let object_id = ctx.write_tree_contents(&objects)?;

        let (file, dir) = objects.iter().partition::<Vec<_>, _>(|o| o.is_file());
        pb_files.inc(file.len() as u64);
        pb_dirs.inc(dir.len() as u64);

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
                    ObjectType::Tree => {
                        pb_dirs.inc(1);
                        match process_tree_content(ctx, &map, &entry).unwrap() {
                            Some(object) => object,
                            None => return m,
                        }
                    }
                    ObjectType::File => {
                        pb_files.inc(1);
                        process_file_content(ctx, &entry).unwrap()
                    }
                };

                let parent = entry.path.parent().unwrap();
                let parent = PathBuf::from(parent);
                m.entry(parent).or_insert(vec![]).push(object);
                m
            },
        )
        .reduce(HashMap::new, merge_hashmap);
    parallel_walk(ctx, pb_files, pb_dirs, rest, depth - 1, new_map)
}

#[derive(Args, Debug)]
pub struct Build {
    /// Working directory.
    #[clap(short, long, value_name = "directory", verbatim_doc_comment)]
    dir: Option<PathBuf>,

    /// The input file containing a list of files to be scanned.
    /// By default, it scans all files in the current directory.
    /// If you want to receive from standard input, specify "-".
    #[clap(short, long, value_name = "input-file", verbatim_doc_comment)]
    input: Option<OsString>,

    /// If true, don't write the object ID of the root tree to HEAD.
    #[clap(short, long, default_value_t = false, verbatim_doc_comment)]
    no_write_head: bool,

    /// If true, scan hidden files.
    #[clap(long, default_value_t = false, verbatim_doc_comment)]
    hidden: bool,
}

impl Build {
    pub fn run(&self) -> io::Result<()> {
        let dir = self
            .dir
            .as_ref()
            .unwrap_or(&env::current_dir()?)
            .canonicalize()?;
        log::info!("dir: {}", dir.display());

        let ctx = Context::new(&dir);

        let (max_depth, files, num_files, num_dirs) =
            self.target_files(&ctx).expect("failed to list all files");
        log::info!("max_depth: {}, files: {}", max_depth, files.len());

        let m = MultiProgress::new();
        let sty = ProgressStyle::with_template(
            "[{elapsed_precise}] {bar:40.cyan/blue} {pos:>7}/{len:7} {msg}",
        )
        .unwrap()
        .progress_chars("##-");

        let pb_files = m.add(ProgressBar::new(num_files));
        pb_files.set_style(sty.clone());
        pb_files.set_message("files");
        let pb_dirs = m.add(ProgressBar::new(num_dirs));
        pb_dirs.set_style(sty.clone());
        pb_dirs.set_message("dirs");

        let object = parallel_walk(&ctx, &pb_files, &pb_dirs, files, max_depth, HashMap::new())?;
        pb_files.finish_and_clear();
        pb_dirs.finish_and_clear();

        if self.no_write_head {
            println!("HEAD: {}", object.object_id);
        } else {
            ctx.write_head(&object.object_id)?;
            println!("Written HEAD: {}", object.object_id);
        }
        Ok(())
    }

    fn target_files(&self, ctx: &Context) -> anyhow::Result<(usize, Vec<FileEntry>, u64, u64)> {
        let Some(input) = &self.input else {
            return list_all_files(&ctx, self.hidden);
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

        let mut files = 0;
        let mut dirs = 0;
        let mut max_depth = 0;
        let mut ret = Vec::new();

        for line in input {
            let line = line?;
            let file_path = line.trim();

            let is_dir = file_path.ends_with('/');
            let file_path = file_path.trim_start_matches("./").trim_end_matches('/');
            let depth = file_path.split('/').count();
            max_depth = max_depth.max(depth);

            let path = PathBuf::from(file_path);
            if is_ignore_dir(&path) {
                continue;
            }
            if path.is_absolute() {
                return Err(anyhow!("absolute path is not supported"));
            }

            if is_dir {
                dirs += 1;
                ret.push(FileEntry::new_dir(&path, depth));
            } else {
                files += 1;
                ret.push(FileEntry::new_file(&path, depth));
            }
        }
        ret.push(FileEntry::new_dir(PathBuf::from("."), 0));

        Ok((max_depth, ret, files, dirs))
    }
}

#[derive(Args, Debug)]
pub struct List {
    /// Working directory.
    #[clap(short, long, value_name = "directory", verbatim_doc_comment)]
    dir: Option<PathBuf>,
}

fn format_filetype(mode: &fs::FileType) -> &'static str {
    if mode.is_dir() {
        "dir"
    } else if mode.is_file() {
        "file"
    } else if mode.is_symlink() {
        "symlink"
    } else if mode.is_block_device() {
        "block"
    } else if mode.is_char_device() {
        "char"
    } else if mode.is_fifo() {
        "fifo"
    } else if mode.is_socket() {
        "socket"
    } else {
        "unknown"
    }
}

impl List {
    pub fn run(&self) -> io::Result<()> {
        let dir = self
            .dir
            .as_ref()
            .unwrap_or(&env::current_dir()?)
            .canonicalize()?;
        log::info!("dir: {}", dir.display());

        let ctx = Context::new(&dir);
        let (max_depth, files, _, _) =
            list_all_files(&ctx, false).expect("failed to list all files");
        log::info!("max_depth: {}, files: {}", max_depth, files.len());
        for file in files {
            if file.path == PathBuf::from("") {
                println!("{} .", file.mode);
                continue;
            }
            println!("{} {}", file.mode, file.path.display());
        }
        Ok(())
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
