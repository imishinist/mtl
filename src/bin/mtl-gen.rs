use clap::Parser;
use indicatif::ProgressBar;
use std::io::BufRead;
use std::io::Write;

use rand::Rng;
use rayon::prelude::*;
use sha1::Digest;

struct Hash {
    inner: [u8; 32],
}

impl From<[u8; 32]> for Hash {
    fn from(inner: [u8; 32]) -> Self {
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
    let mut hasher = sha2::Sha256::new();
    hasher.write(content)?;
    let inner = hasher.finalize().into();
    Ok(Hash { inner })
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

#[derive(Debug, Parser)]
#[command(author, version, about, long_about=None)]
#[command(propagate_version = true)]
struct Cli {
    dir: String,
    nfile: usize,

    #[clap(long, default_value = "1234")]
    seed: u64,

    #[clap(long, default_value = "10000")]
    nlines: usize,

    #[clap(short, long, default_value = "2", value_delimiter = ',')]
    prefix_bytes: Vec<usize>,
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
        let n2 = rng.gen_range(n1..std::cmp::min(n1 + cli.nlines, len));

        let lines = (&lines[n1..n2]).join("\n");
        let lines = lines.as_bytes();
        let hash = hash_content(lines).unwrap();

        let hash = hash.to_string();

        let (prefix, rest) = split_by_prefixes(&hash, &cli.prefix_bytes);

        let path = dir.join(prefix);
        std::fs::create_dir_all(&path).unwrap();

        let path = path.join(rest);
        let mut file = std::fs::File::create(path).unwrap();
        file.write_all(&lines).unwrap();
    });
    pb.finish_and_clear();

    Ok(())
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    async_main().await?;
    Ok(())
}
