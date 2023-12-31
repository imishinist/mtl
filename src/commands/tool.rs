use std::io::Read;
use std::path::PathBuf;

use std::fs::File;
use std::io::Write;

use clap::Args;
use indicatif::ProgressBar;
use rand::prelude::{thread_rng, Distribution};
use rand_distr::Normal;
use rayon::prelude::*;

#[derive(Debug, Args)]
pub struct Hash {
    input: Vec<PathBuf>,
}

impl Hash {
    pub fn run(&self) -> anyhow::Result<()> {
        if self.input.is_empty() {
            let contents = Self::contents_from_stdin()?;
            println!("{} -", crate::hash::Hash::from_contents(&contents));
        } else {
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
        }
        Ok(())
    }

    fn contents_from_stdin() -> anyhow::Result<Vec<u8>> {
        let mut contents = Vec::new();
        std::io::stdin().read_to_end(&mut contents)?;
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
        let need_bytes = normal.sample(&mut thread_rng()) as usize;
        (0..need_bytes)
            .map(|_| rand::random::<u8>())
            .collect::<Vec<_>>()
    }
}
