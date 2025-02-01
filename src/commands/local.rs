use std::collections::HashSet;
use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::mpsc::TryRecvError;
use std::time::Duration;

use clap::Args;
use notify::{Config, Event, RecommendedWatcher, Watcher};

use crate::builder::{Builder, FileTargetGenerator, ScanTargetGenerator, TargetGenerator};
use crate::filter::{Filter, MatchAllFilter, PathFilter};
use crate::{builder, Context, ObjectType, RelativePath};

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
}

impl Build {
    pub fn run(&self, ctx: Context) -> anyhow::Result<()> {
        let ctx = ctx;

        let root_dir = ctx.root_dir().to_path_buf();
        let generator = get_generator(root_dir, None, self.input.as_ref(), self.hidden);
        let builder = Builder::new(generator, self.progress);
        let object = builder.build(&ctx)?;
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

    path: Option<PathBuf>,
}

impl List {
    pub fn run(&self, ctx: Context) -> anyhow::Result<()> {
        let root_dir = ctx.root_dir().to_path_buf();
        let generator = get_generator(
            root_dir,
            self.path.as_ref(),
            self.input.as_ref(),
            self.hidden,
        );
        let target_entries = generator.generate(&ctx)?;
        for file in target_entries.iter() {
            if file.path.is_root() {
                println!("{} .", file.mode);
                continue;
            }
            println!("{} {}", file.mode, file.path);
        }
        Ok(())
    }
}

fn get_generator(
    root_dir: PathBuf,
    path: Option<&PathBuf>,
    input: Option<&OsString>,
    hidden: bool,
) -> Box<dyn TargetGenerator> {
    let filter: Box<dyn Filter> = match path {
        Some(path) => Box::new(PathFilter::new(root_dir, path)),
        None => Box::new(MatchAllFilter::new(root_dir)),
    };
    match input {
        Some(input) => Box::new(FileTargetGenerator::new(filter, input.to_os_string())),
        None => Box::new(ScanTargetGenerator::new(filter, hidden)),
    }
}

#[derive(Args, Debug)]
pub struct Watch {}

impl Watch {
    pub fn run(&self, ctx: Context) -> anyhow::Result<()> {
        let ctx = ctx;

        let (tx, rx) = mpsc::channel::<Event>();

        let root_dir = ctx.root_dir().to_path_buf();
        let mut watcher = RecommendedWatcher::new(
            move |event| match event {
                Ok(event) => tx.send(event).unwrap(),
                Err(err) => {
                    eprintln!("Error: {:?}", err);
                }
            },
            Config::default().with_poll_interval(Duration::from_secs(1)),
        )?;
        watcher.watch(&root_dir, notify::RecursiveMode::Recursive)?;
        println!("Start watching {} ... (Ctrl+C to exit)", root_dir.display());

        let filter = MatchAllFilter::new(root_dir.clone());
        loop {
            let event = rx.recv()?;
            let mut events = vec![event];

            loop {
                match rx.try_recv() {
                    Ok(e) => events.push(e),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        log::info!("Channel disconnected. Exiting...");
                        return Ok(());
                    }
                }
            }

            let mut unique_paths = HashSet::new();
            for ev in events {
                for path in ev.paths {
                    let path = match path.strip_prefix(&root_dir) {
                        Ok(path) => path.to_path_buf(),
                        Err(_) => {
                            log::debug!("Failed to strip prefix: {:?}", path);
                            continue;
                        }
                    };
                    let path = RelativePath::from(path);
                    if !filter.path_matches(&path) {
                        continue;
                    }
                    unique_paths.insert(path);
                }
            }

            for path in unique_paths {
                let file_entry = builder::FileEntry::new(ObjectType::File, path.clone(), 0);
                let object_id = match builder::hash_file_entry(&ctx, &file_entry) {
                    Ok(object_id) => object_id,
                    Err(e) => {
                        log::debug!("Failed to hash file entry: {} {:?}", path, e.kind());
                        continue;
                    }
                };
                println!("{} {}", object_id.object_id, path);
            }
        }
    }
}
