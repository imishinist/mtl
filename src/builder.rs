mod parallel;

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, Read};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::SystemTime;
use std::{fs, io};

use ignore::{WalkBuilder, WalkState};
use itertools::Itertools;

use crate::cache::CacheValue;
use crate::filter::Filter;
use crate::progress::BuildProgressBar;
use crate::{Context, Object, ObjectID, ObjectType, ReadContentError, RelativePath};

pub trait TargetGenerator {
    fn generate(&self, ctx: &Context) -> anyhow::Result<TargetEntries, ReadContentError>;
}

pub struct Builder {
    generator: Box<dyn TargetGenerator>,
    progress: bool,
}

impl Builder {
    pub fn new(generator: Box<dyn TargetGenerator>, progress: bool) -> Self {
        Self {
            generator,
            progress,
        }
    }

    pub fn build(&self, ctx: &Context) -> anyhow::Result<Object, ReadContentError> {
        let target_entries = self.generator.generate(ctx)?;
        if target_entries.max_depth == 0 {
            return Err(ReadContentError::TargetEmpty);
        }

        let pb = BuildProgressBar::new(
            target_entries.num_files,
            target_entries.num_dirs,
            self.progress,
        );
        Ok(parallel::build(ctx, &pb, target_entries)?)
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct FileEntry {
    pub mode: ObjectType,
    pub path: RelativePath,
    pub depth: usize,
}

impl FileEntry {
    pub fn new(mode: ObjectType, path: RelativePath, depth: usize) -> Self {
        Self { mode, path, depth }
    }
}

impl PartialOrd for FileEntry {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for FileEntry {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.path.cmp(&other.path)
    }
}

pub fn process_tree_content(
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

pub fn hash_file_entry(ctx: &Context, entry: &FileEntry) -> io::Result<Object> {
    let path = ctx.root_dir().join(entry.path.as_path());
    let file_name = entry.path.file_name().ok_or(io::Error::new(
        io::ErrorKind::NotFound,
        "failed to get file_name",
    ))?;

    let mut file = File::open(path)?;

    let metadata = file.metadata()?;
    let mtime = metadata
        .modified()?
        .duration_since(SystemTime::UNIX_EPOCH)
        .ok()
        .unwrap_or(std::time::Duration::new(0, 0))
        .as_micros();
    let cache_key = entry.path.as_path();

    // cache hit!
    match ctx.read_cache(&cache_key) {
        Some(cache_value) if cache_value.mtime == mtime && cache_value.size == metadata.len() => {
            return Ok(Object::new_file(cache_value.object_id, file_name));
        }
        _ => {}
    }

    let mut contents = Vec::new();
    file.read_to_end(&mut contents)?;

    let object_id = ObjectID::from_contents(&contents);
    let cache_value = CacheValue {
        mtime,
        size: metadata.len(),
        object_id,
    };
    // save cache
    ctx.write_cache(&cache_key, cache_value);

    Ok(Object::new_file(object_id, file_name))
}

#[derive(Debug)]
pub(crate) struct TargetEntries {
    max_depth: usize,
    files: Vec<FileEntry>,
    num_files: u64,
    num_dirs: u64,
}

impl TargetEntries {
    pub fn new() -> Self {
        Self {
            max_depth: 0,
            files: Vec::new(),
            num_files: 0,
            num_dirs: 0,
        }
    }

    pub fn push_file_entry(&mut self, entry: FileEntry) {
        self.max_depth = self.max_depth.max(entry.depth);
        match entry.mode {
            ObjectType::File => self.num_files += 1,
            ObjectType::Tree => self.num_dirs += 1,
        }
        self.files.push(entry);
    }

    pub fn iter(&self) -> impl Iterator<Item = &FileEntry> {
        self.files.iter()
    }
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

pub struct ScanTargetGenerator {
    filter: Arc<Box<dyn Filter>>,
    hidden: bool,
}

impl ScanTargetGenerator {
    pub fn new(filter: Box<dyn Filter>, hidden: bool) -> Self {
        Self {
            filter: Arc::new(filter),
            hidden,
        }
    }
}

impl TargetGenerator for ScanTargetGenerator {
    fn generate(&self, ctx: &Context) -> anyhow::Result<TargetEntries, ReadContentError> {
        let (tx, rx) = crossbeam_channel::bounded::<FileEntry>(100);

        let output_thread = std::thread::spawn(move || {
            let mut entries = TargetEntries::new();
            entries.push_file_entry(FileEntry::new(ObjectType::Tree, RelativePath::Root, 0));
            for entry in rx {
                entries.push_file_entry(entry);
            }
            entries
        });

        let filter = self.filter.clone();
        let root_dir = ctx.root_dir();
        let walker = WalkBuilder::new(root_dir)
            .hidden(!self.hidden)
            .filter_entry(move |entry| {
                let Ok(path) = entry.path().strip_prefix(filter.root()) else {
                    return false;
                };
                let relative_path = RelativePath::from(path);
                filter.path_matches(&relative_path)
            })
            .threads(num_cpus::get())
            .build_parallel();
        walker.run(|| {
            let tx = tx.clone();
            Box::new(move |entry| {
                // get DirEntry error
                let Ok(entry) = entry.map_err(|e| log::warn!("ignored: {}", e)) else {
                    return WalkState::Continue;
                };

                // strip prefix error
                let Ok(path) = entry
                    .path()
                    .strip_prefix(root_dir)
                    .map_err(|e| log::error!("strip prefix error: {}", e))
                else {
                    return WalkState::Continue;
                };
                // root dir
                if path.as_os_str().is_empty() {
                    return WalkState::Continue;
                }

                // get file type error
                let Some(ft) = entry.file_type() else {
                    return WalkState::Continue;
                };

                // not supported file type
                if !ft.is_file() && !ft.is_dir() {
                    log::warn!(
                        "ignored: not supported file type: {} \"{}\"",
                        format_filetype(&ft),
                        path.display()
                    );
                    return WalkState::Continue;
                }

                let object_type = if ft.is_dir() {
                    ObjectType::Tree
                } else {
                    ObjectType::File
                };

                tx.send(FileEntry::new(
                    object_type,
                    RelativePath::from(path),
                    entry.depth(),
                ))
                .unwrap();
                WalkState::Continue
            })
        });
        drop(tx);

        Ok(output_thread.join().unwrap())
    }
}

pub struct FileTargetGenerator {
    filter: Box<dyn Filter>,
    input: PathBuf,
}

impl FileTargetGenerator {
    pub fn new(filter: Box<dyn Filter>, input: PathBuf) -> Self {
        Self { filter, input }
    }
}

impl TargetGenerator for FileTargetGenerator {
    fn generate(&self, _ctx: &Context) -> anyhow::Result<TargetEntries, ReadContentError> {
        let input: BufReaderWrapper<Box<dyn BufRead>> = if self.input.eq(Path::new("-")) {
            let stdin = io::stdin().lock();
            let reader = io::BufReader::new(stdin);
            BufReaderWrapper::new(Box::new(reader))
        } else {
            let reader = io::BufReader::new(File::open(&self.input)?);
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
                return Err(ReadContentError::AbsolutePathNotSupported);
            }

            let relative_path = RelativePath::from(relative_path);
            if !self.filter.path_matches(&relative_path) {
                continue;
            }

            let object_type = if is_dir {
                ObjectType::Tree
            } else {
                ObjectType::File
            };
            entries.push_file_entry(FileEntry::new(object_type, relative_path, depth));
        }
        entries.push_file_entry(FileEntry::new(ObjectType::Tree, RelativePath::Root, 0));
        Ok(entries)
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
