use std::fs::File;
use std::io;
use std::io::Read;
use std::io::Write;
#[cfg(not(windows))]
use std::path::Path;
use std::path::PathBuf;

use clap::Args;
use indicatif::ProgressBar;
use rand::prelude::Distribution;
use rand_distr::Normal;
use rayon::prelude::*;
use redb::ReadableTable;

use crate::{filesystem, Context, ObjectID, PACKED_OBJECTS_TABLE};

#[derive(Debug, Args)]
pub struct Hash {
    input: Vec<PathBuf>,
}

impl Hash {
    pub fn run(&self) -> anyhow::Result<()> {
        if self.input.is_empty() {
            let contents = Self::contents_from_stdin()?;
            println!("{} -", crate::hash::Hash::from_contents(contents));
            return Ok(());
        }
        for path in &self.input {
            if path.is_dir() {
                println!("{} {}", " ".repeat(16), path.display());
                continue;
            }

            let contents = std::fs::read(path)?;
            println!(
                "{} {}",
                crate::hash::Hash::from_contents(&contents),
                path.display()
            );
        }

        Ok(())
    }

    fn contents_from_stdin() -> anyhow::Result<Vec<u8>> {
        let mut contents = Vec::new();
        io::stdin().read_to_end(&mut contents)?;
        Ok(contents)
    }
}

#[derive(Debug, Args)]
pub struct Generate {
    dir: String,
    nfile: usize,

    #[clap(long, default_value = "20")]
    num_kilobytes: usize,

    #[clap(long, default_value = "2")]
    num_kilobytes_stddev: usize,

    #[clap(short, long, default_value = "2", value_delimiter = ',')]
    prefix_bytes: Vec<usize>,
}

impl Generate {
    pub async fn run_async(&self) -> anyhow::Result<()> {
        let dir = std::path::Path::new(&self.dir);
        let pb = ProgressBar::new(self.nfile as u64);
        (0..self.nfile).into_par_iter().for_each(|_i| {
            pb.inc(1);

            let mut normal = Normal::new(
                (self.num_kilobytes * 1024) as f64,
                (self.num_kilobytes_stddev * 1024) as f64,
            )
            .unwrap();

            let random_contents = Self::generate_bytes(&mut normal);
            let hash = crate::hash::Hash::from_contents(&random_contents);
            let hash = hash.to_string();

            let (prefix, rest) = Self::split_by_prefixes(&hash, &self.prefix_bytes);

            let path = dir.join(prefix);
            std::fs::create_dir_all(&path).unwrap();

            let path = path.join(rest);
            let mut file = File::create(path).unwrap();
            file.write_all(&random_contents).unwrap();
        });

        Ok(())
    }

    pub fn run(&self) -> anyhow::Result<()> {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()?;
        rt.block_on(self.run_async())?;
        Ok(())
    }

    fn split_by_prefixes<'a>(x: &'a str, prefix_bytes: &[usize]) -> (std::path::PathBuf, &'a str) {
        let mut path = std::path::PathBuf::new();

        let mut start = 0;
        for &byte in prefix_bytes {
            let end = start + byte;
            path = path.join(&x[start..end]);
            start = end;
        }
        (path, &x[start..])
    }

    fn generate_bytes(normal: &mut Normal<f64>) -> Vec<u8> {
        let need_bytes = normal.sample(&mut rand::rng()) as usize;
        (0..need_bytes)
            .map(|_| rand::random::<u8>())
            .collect::<Vec<_>>()
    }
}

#[cfg(not(windows))]
#[derive(Debug, Clone, Copy)]
struct CacheState {
    total_size: u64,
    total_pages: u64,
    cached_pages: u64,
    cached_size: usize,
    cached_percentage: f64,
}

#[cfg(not(windows))]
#[derive(Debug, Args)]
pub struct Fincore {
    input: Vec<PathBuf>,
}

#[cfg(not(windows))]
impl Fincore {
    fn fincore<P: AsRef<Path>>(path: P) -> io::Result<CacheState> {
        let file = File::open(path)?;
        let metadata = file.metadata()?;

        let len = metadata.len();
        let pagesize = page_size::get() as u64;
        let npages = (len + pagesize - 1) / pagesize;

        let mut vec = vec![0u8; npages as usize];
        let m = unsafe {
            memmap::MmapOptions::new()
                .offset(0)
                .len(len as usize)
                .map(&file)?
        };

        let ret = unsafe { libc::mincore(m.as_ptr() as _, len as _, vec.as_mut_ptr() as _) };
        if ret != 0 {
            return Err(io::Error::last_os_error());
        }

        let cached = vec.iter().filter(|&&x| x & 1 == 1).count();
        Ok(CacheState {
            total_size: len,
            total_pages: npages,
            cached_pages: cached as u64,
            cached_size: cached * pagesize as usize,
            cached_percentage: cached as f64 / npages as f64,
        })
    }

    pub fn run(&self) -> anyhow::Result<()> {
        println!("file_name total_size total_pages cached_pages cached_size cached_percentage");
        for path in &self.input {
            if path.is_dir() {
                continue;
            }

            let cache_state = Self::fincore(path)?;
            println!(
                "{} {} {} {} {} {:.2}%",
                path.display(),
                cache_state.total_size,
                cache_state.total_pages,
                cache_state.cached_pages,
                cache_state.cached_size,
                cache_state.cached_percentage * 100.0,
            );
        }
        Ok(())
    }
}

#[derive(Debug, Args)]
pub struct Fadvise {
    file: PathBuf,
    advise: filesystem::Advise,

    offset: Option<u64>,
    len: Option<usize>,
}

impl Fadvise {
    pub fn run(&self) -> anyhow::Result<()> {
        let file = File::open(&self.file)?;
        filesystem::fadvise(&file, self.advise, self.offset, self.len)?;
        Ok(())
    }
}

#[derive(Debug, Args)]
pub struct ReDB {
    /// The object to look up
    key: Option<ObjectID>,
}

impl ReDB {
    pub fn run(&self, ctx: Context) -> anyhow::Result<()> {
        let Some(db) = ctx.packed_db else {
            println!("no packed db");
            return Ok(());
        };

        let read_txn = db.begin_read()?;
        let Some(object_id) = &self.key else {
            let table = read_txn.open_table(PACKED_OBJECTS_TABLE)?;
            for range in table.iter()? {
                let (object_id, _) = range?;
                println!("{}", object_id.value());
            }
            return Ok(());
        };

        let table = read_txn.open_table(PACKED_OBJECTS_TABLE)?;
        match table.get(object_id)? {
            Some(val) => {
                let content = val.value();
                let s = String::from_utf8_lossy(&content);
                println!("{}", s);
            }
            None => println!("not found"),
        };
        Ok(())
    }
}
