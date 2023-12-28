use std::fs::File;
use std::io::Write;

use clap::Parser;
use indicatif::ProgressBar;
use rand::prelude::{Distribution, thread_rng};
use rand_distr::Normal;
use rayon::prelude::*;

use mtl::hash::Hash;

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
    let need_bytes = normal.sample(&mut thread_rng()) as usize;
    (0..need_bytes).map(|_| rand::random::<u8>()).collect::<Vec<_>>()
}

#[derive(Debug, Parser)]
#[command(author, version, about, long_about=None)]
#[command(propagate_version = true)]
struct Cli {
    dir: String,
    nfile: usize,

    #[clap(long, default_value = "20")]
    num_kilobytes: usize,

    #[clap(long, default_value = "2")]
    num_kilobytes_stddev: usize,

    #[clap(short, long, default_value = "2", value_delimiter = ',')]
    prefix_bytes: Vec<usize>,
}

async fn async_main() -> std::io::Result<()> {
    let cli = Cli::parse();

    let dir = std::path::Path::new(&cli.dir);
    let pb = ProgressBar::new(cli.nfile as u64);
    (0..cli.nfile).into_par_iter().for_each(|_i| {
        pb.inc(1);

        let mut normal = Normal::new((cli.num_kilobytes * 1024) as f64, (cli.num_kilobytes_stddev * 1024) as f64).unwrap();

        let random_contents = generate_bytes(&mut normal);
        let hash = Hash::from_contents(&random_contents);
        let hash = hash.to_string();

        let (prefix, rest) = split_by_prefixes(&hash, &cli.prefix_bytes);

        let path = dir.join(prefix);
        std::fs::create_dir_all(&path).unwrap();

        let path = path.join(rest);
        let mut file = File::create(path).unwrap();
        file.write_all(&random_contents).unwrap();
    });
    pb.finish_and_clear();

    Ok(())
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    async_main().await?;
    Ok(())
}
