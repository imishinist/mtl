use clap::Parser;
use std::io::BufRead;
use std::io::Write;
use std::sync::atomic::AtomicUsize;
use indicatif::{MultiProgress, ProgressBar};

use rand::{Rng, SeedableRng};
use sha1::Digest;
use rayon::prelude::*;

struct Hash {
    inner: [u8; 20],
}

impl From<[u8; 20]> for Hash {
    fn from(inner: [u8; 20]) -> Self {
        Self { inner }
    }
}

impl std::fmt::Display for Hash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for byte in self.inner.iter() {
            write!(f, "{:02x}", byte)?;
        }
        Ok(())
    }
}

fn hash_content(content: &[u8]) -> std::io::Result<Hash> {
    let mut hasher = sha1::Sha1::new();
    hasher.write(content)?;
    let sha1 = hasher.finalize().into();
    Ok(Hash { inner: sha1 })
}

fn read_file<P: AsRef<std::path::Path>>(path: P) -> std::io::Result<Vec<String>> {
    let file = std::fs::File::open(path)?;
    let reader = std::io::BufReader::new(file);

    // read by lines
    let mut lines = Vec::new();
    for line in reader.lines() {
        lines.push(line?);
    }

    Ok(lines)
}

#[derive(Debug, Parser)]
#[command(author, version, about, long_about=None)]
#[command(propagate_version = true)]
struct Cli {
    dir: String,
    nfile: usize,

    #[clap(long, default_value = "1234")]
    seed: u64,

    #[clap(long, default_value = "2")]
    prefix_byte: usize,
}

async fn async_main() -> std::io::Result<()> {
    let cli = Cli::parse();

    let dir = std::path::Path::new(&cli.dir);
    let lines = read_file("/usr/share/dict/words")?;
    let len = lines.len();

    let pb = ProgressBar::new(cli.nfile as u64);
    (0..cli.nfile).into_par_iter().for_each(|_i| {
        pb.inc(1);

        let mut rng = rand::thread_rng();

        let n1 = rng.gen_range(0..len);
        let n2 = rng.gen_range(n1..std::cmp::min(n1 + 10000, len));

        let lines = (&lines[n1..n2]).join("\n");
        let lines = lines.as_bytes();
        let hash = hash_content(lines).unwrap();

        let hash = hash.to_string();

        let (prefix, rest) = hash.split_at(cli.prefix_byte);
        let path = dir.join(prefix);
        std::fs::create_dir_all(&path).unwrap();

        let path = path.join(rest);
        let mut file = std::fs::File::create(path).unwrap();
        file.write_all(&lines).unwrap();
    });
    pb.finish();

    Ok(())
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    async_main().await?;
    Ok(())
}
