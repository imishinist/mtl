use clap::Parser;
use mtl::ObjectID;
use rayon::Scope;
use std::path::Path;
use std::sync::mpsc::Sender;
use std::{fs, io};

fn read<P: AsRef<Path>>(path: P) -> io::Result<ObjectID> {
    Ok(ObjectID::from_contents(&fs::read(path)?))
}

fn parallel_scan<'a, U: AsRef<Path>>(src: &U, tx: Sender<io::Result<ObjectID>>, scope: &Scope<'a>) {
    let dir = fs::read_dir(src).unwrap();
    dir.into_iter().for_each(|entry| {
        let info = entry.as_ref().unwrap();
        let path = info.path();

        if path.is_dir() {
            let tx = tx.clone();
            scope.spawn(move |s| parallel_scan(&path, tx, s)) // Recursive call here
        } else {
            let object_id = read(&path);
            tx.send(object_id).unwrap();
        }
        log::info!("parallel scanned {}", info.path().display());
    });
}

fn sequential_scan<P: AsRef<Path>>(src: P) -> io::Result<Vec<ObjectID>> {
    let mut ret = Vec::new();

    let dir = fs::read_dir(src)?;
    dir.into_iter().for_each(|entry| {
        let info = entry.as_ref().unwrap();
        let path = info.path();

        if path.is_dir() {
            let sub = sequential_scan(&path).unwrap();
            ret.extend(sub);
        } else {
            ret.push(read(&path).unwrap());
        }
        log::info!("sequentially scanned {}", path.display());
    });
    Ok(ret)
}

#[derive(Debug, Clone, clap::ValueEnum)]
enum Mode {
    Sequential,
    Parallel,
}

impl std::fmt::Display for Mode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Mode::Sequential => write!(f, "sequential"),
            Mode::Parallel => write!(f, "parallel"),
        }
    }
}

#[derive(Debug, Parser)]
#[command(author, version, about, long_about=None)]
#[command(propagate_version = true)]
struct Cli {
    #[clap(short, long)]
    mode: Mode,

    #[clap(short, long)]
    source: String,
}
fn main() {
    env_logger::init();

    let cli = Cli::parse();

    let mode = cli.mode;
    let source = cli.source;

    match mode {
        Mode::Sequential => {
            let ids = sequential_scan(&source).unwrap();
            for object_id in ids {
                println!("{}", object_id.to_string());
            }
        }
        Mode::Parallel => {
            let (tx, rx) = std::sync::mpsc::channel();
            rayon::scope(|s| parallel_scan(&source, tx, s));

            for object_id in rx {
                let object_id = object_id.unwrap();
                println!("{}", object_id.to_string());
            }
        }
    }
}
