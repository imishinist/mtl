use std::collections::HashMap;
use std::ffi::OsString;
use std::fs::File;
use std::io::{BufRead, Read};
use std::path::PathBuf;
use std::rc::Rc;
use std::{fs, io};

use anyhow::anyhow;
use clap::Args;
use ignore::{WalkBuilder, WalkState};
use itertools::Itertools;
use rayon::prelude::*;

use crate::filter::{Filter, MatchAllFilter};
use crate::progress::BuildProgressBar;
use crate::{filesystem, Context, Object, ObjectID, ObjectType, RelativePath};

#[derive(Debug, Clone, Eq, PartialEq)]
struct FileEntry {
    mode: ObjectType,
    path: RelativePath,
    depth: usize,

    #[cfg(unix)]
    inode: u64,
}

impl FileEntry {
    #[cfg(unix)]
    fn new(mode: ObjectType, path: RelativePath, depth: usize, inode: u64) -> Self {
        Self {
            mode,
            path,
            depth,
            inode,
        }
    }

    #[cfg(not(unix))]
    fn new(mode: ObjectType, path: RelativePath, depth: usize) -> Self {
        Self { mode, path, depth }
    }
}

impl PartialOrd for FileEntry {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for FileEntry {
    #[cfg(unix)]
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.inode.cmp(&other.inode)
    }

    #[cfg(not(unix))]
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.path.cmp(&other.path)
    }
}

#[derive(Debug)]
struct TargetEntries {
    max_depth: usize,
    files: Vec<FileEntry>,
    num_files: u64,
    num_dirs: u64,
}

impl TargetEntries {
    fn new() -> Self {
        Self {
            max_depth: 0,
            files: Vec::new(),
            num_files: 0,
            num_dirs: 0,
        }
    }

    fn push_file_entry(&mut self, entry: FileEntry) {
        self.max_depth = self.max_depth.max(entry.depth);
        match entry.mode {
            ObjectType::File => self.num_files += 1,
            ObjectType::Tree => self.num_dirs += 1,
        }
        self.files.push(entry);
    }
}

fn list_all_files(
    ctx: &Context,
    filter: impl Filter,
    hidden: bool,
) -> anyhow::Result<TargetEntries> {
    let (tx, rx) = crossbeam_channel::bounded::<FileEntry>(100);
    let output_thread = std::thread::spawn(move || {
        let mut entries = TargetEntries::new();
        for entry in rx {
            entries.push_file_entry(entry);
        }
        entries
    });

    let root_dir = ctx.root_dir();
    let walker = WalkBuilder::new(root_dir)
        .hidden(!hidden)
        .threads(num_cpus::get())
        .build_parallel();
    walker.run(|| {
        let tx = tx.clone();
        let filter = filter.clone();
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
            let relative_path = RelativePath::from(path);
            if !filter.path_matches(&relative_path) {
                return WalkState::Continue;
            }

            let entry = if path.as_os_str().is_empty() {
                FileEntry::new(
                    ObjectType::Tree,
                    RelativePath::Root,
                    depth,
                    #[cfg(unix)]
                    entry.ino().unwrap(),
                )
            } else if ft.is_dir() {
                FileEntry::new(
                    ObjectType::Tree,
                    relative_path,
                    depth,
                    #[cfg(unix)]
                    entry.ino().unwrap(),
                )
            } else if ft.is_file() {
                FileEntry::new(
                    ObjectType::File,
                    relative_path,
                    depth,
                    #[cfg(unix)]
                    entry.ino().unwrap(),
                )
            } else {
                log::warn!(
                    "ignored: not supported file type: {} \"{}\"",
                    format_filetype(&ft),
                    path.display()
                );
                return WalkState::Continue;
            };
            tx.send(entry).unwrap();
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
        acc.entry(k).or_default().extend(vs);
        acc
    })
}

fn process_tree_content(
    ctx: &Context,
    map: &HashMap<RelativePath, Vec<Object>>,
    entry: &FileEntry,
) -> io::Result<Option<Object>> {
    let objects = match map.get(&entry.path) {
        Some(objects) => objects.iter().sorted().collect::<Vec<_>>(),
        None => return Ok(None), // empty dir
    };

    match &entry.path {
        RelativePath::Path(path) => {
            let object_id = ctx.write_tree_contents(&objects)?;
            Ok(Some(Object::new_tree(object_id, path)))
        }
        RelativePath::Root => {
            let object_id = ctx.write_tree_contents(&objects)?;
            Ok(Some(Object::new_tree(object_id, PathBuf::from(""))))
        }
    }
}

fn process_file_content(ctx: &Context, entry: &FileEntry) -> io::Result<Object> {
    let path = ctx.root_dir().join(entry.path.as_path());

    let mut file = File::open(path)?;
    let mut contents = Vec::new();
    file.read_to_end(&mut contents)?;
    if ctx.drop_cache {
        filesystem::fadvise(&file, filesystem::Advise::DontNeed, None, None)?;
    }

    let object_id = ObjectID::from_contents(&contents);
    let file_name = PathBuf::from(entry.path.file_name().ok_or(io::Error::new(
        io::ErrorKind::NotFound,
        "failed to get file_name",
    ))?);

    Ok(Object::new_file(object_id, file_name))
}

fn process_target_entries(
    ctx: &Context,
    pb: &BuildProgressBar,
    files: Vec<FileEntry>,
    max_depth: usize,
) -> io::Result<Object> {
    let mut files = files;
    #[cfg(unix)]
    files.par_sort();

    let (files, mut dirs) = files
        .into_iter()
        .partition::<Vec<_>, _>(|entry| matches!(entry.mode, ObjectType::File));

    let mut objects_per_dir = files
        .into_par_iter()
        .fold(
            HashMap::new,
            |mut acc: HashMap<RelativePath, Vec<_>>, entry| {
                let parent = entry.path.parent();
                let object =
                    process_file_content(ctx, &entry).expect("failed to process file content");
                acc.entry(parent).or_default().push(object);
                pb.inc_file(1);
                acc
            },
        )
        .reduce(HashMap::new, merge_hashmap);

    for i in (1..max_depth).rev() {
        let (target, rest) = dirs
            .into_iter()
            .partition::<Vec<_>, _>(|entry| entry.depth == i);
        dirs = rest;

        let tmp = target
            .into_par_iter()
            .fold(
                HashMap::new,
                |mut acc: HashMap<RelativePath, Vec<_>>, entry| {
                    let parent = entry.path.parent();
                    if let Some(object) =
                        process_tree_content(ctx, &objects_per_dir, &entry).unwrap()
                    {
                        acc.entry(parent).or_default().push(object);
                    }
                    pb.inc_dir(1);

                    acc
                },
            )
            .reduce(HashMap::new, merge_hashmap);
        objects_per_dir = merge_hashmap(objects_per_dir, tmp);
    }

    let root = RelativePath::Root;
    let mut objects = objects_per_dir.remove(&root).unwrap();
    objects.par_sort_unstable();

    let object_id = ctx.write_tree_contents(&objects)?;
    Ok(Object::new_tree(object_id, PathBuf::from("")))
}

#[derive(Args, Debug)]
pub struct Build {
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

    /// If true, show progress bar.
    #[clap(long, default_value_t = false, verbatim_doc_comment)]
    progress: bool,

    /// If true, drop cache after reading files.
    #[clap(long, default_value_t = false, verbatim_doc_comment)]
    drop_cache: bool,
}

impl Build {
    pub fn run(&self, ctx: Context) -> anyhow::Result<()> {
        let mut ctx = ctx;
        ctx.set_drop_cache(self.drop_cache);

        let target_entries = target_entries(&ctx, MatchAllFilter, &self.input, self.hidden)?;
        let max_depth = target_entries.max_depth;
        log::info!(
            "max_depth: {}, files: {}",
            max_depth,
            target_entries.files.len()
        );

        let object = {
            let pb = BuildProgressBar::new(
                target_entries.num_files,
                target_entries.num_dirs,
                self.progress,
            );
            process_target_entries(&ctx, &pb, target_entries.files, max_depth)?
        };

        match self.no_write_head {
            true => println!("HEAD: {}", object.object_id),
            false => {
                ctx.write_head(&object.object_id)?;
                println!("Written HEAD: {}", object.object_id);
            }
        }
        Ok(())
    }
}

#[derive(Args, Debug)]
pub struct List {
    /// The input file containing a list of files to be scanned.
    /// By default, it scans all files in the current directory.
    /// If you want to receive from standard input, specify "-".
    #[clap(short, long, value_name = "input-file", verbatim_doc_comment)]
    input: Option<OsString>,

    /// If true, scan hidden files.
    #[clap(long, default_value_t = false, verbatim_doc_comment)]
    hidden: bool,
}

fn format_filetype(mode: &fs::FileType) -> &'static str {
    #[cfg(unix)]
    return unix_format_filetype(mode);

    #[cfg(windows)]
    return windows_format_filetype(mode);
}

#[cfg(unix)]
fn unix_format_filetype(mode: &fs::FileType) -> &'static str {
    use std::os::unix::fs::FileTypeExt;
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

#[cfg(windows)]
fn windows_format_filetype(mode: &fs::FileType) -> &'static str {
    #[allow(unused_imports)]
    use std::os::windows::fs::FileTypeExt;
    if mode.is_dir() {
        "dir"
    } else if mode.is_file() {
        "file"
    } else if mode.is_symlink() {
        "symlink"
    } else {
        "unknown"
    }
}

impl List {
    pub fn run(&self, ctx: Context) -> anyhow::Result<()> {
        let target_entries = target_entries(&ctx, MatchAllFilter, &self.input, self.hidden)?;
        log::info!(
            "max_depth: {}, files: {}",
            target_entries.max_depth,
            target_entries.files.len()
        );
        for file in target_entries.files {
            if file.path.is_root() {
                println!("{} .", file.mode);
                continue;
            }
            println!("{} {}", file.mode, file.path);
        }
        Ok(())
    }
}

fn target_entries(
    ctx: &Context,
    filter: impl Filter,
    input: &Option<OsString>,
    hidden: bool,
) -> anyhow::Result<TargetEntries> {
    let Some(input) = &input else {
        return list_all_files(ctx, filter, hidden);
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

    let mut entries = TargetEntries::new();
    for line in input {
        let line = line?;
        let relative_path = line.trim();

        let is_dir = relative_path.ends_with('/');
        let relative_path = relative_path.trim_start_matches("./").trim_end_matches('/');
        let depth = relative_path.split('/').count();

        let relative_path = PathBuf::from(relative_path);
        if relative_path.is_absolute() {
            return Err(anyhow!("absolute path is not supported"));
        }

        let relative_path = RelativePath::from(relative_path);
        if !filter.path_matches(&relative_path) {
            continue;
        }

        let object_type = if is_dir {
            ObjectType::Tree
        } else {
            ObjectType::File
        };

        #[cfg(unix)]
        entries.push_file_entry(FileEntry::new(object_type, relative_path, depth, 0));
        #[cfg(not(unix))]
        entries.push_file_entry(FileEntry::new(object_type, relative_path, depth));
    }
    entries.push_file_entry(FileEntry::new(ObjectType::Tree, RelativePath::Root, 0, 0));
    Ok(entries)
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
