use std::collections::HashSet;
use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::mpsc::{Receiver, RecvError, TryRecvError};
use std::time::{Duration, Instant};

use clap::Args;
use notify::event::CreateKind;
use notify::{Config, Event, EventKind, RecommendedWatcher, Watcher};

use crate::builder::{Builder, FileTargetGenerator, ScanTargetGenerator, TargetGenerator};
use crate::filter::{Filter, IgnoreFilter, MatchAllFilter, PathFilter};
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
pub struct Watch {
    /// If true, scan hidden files.
    #[clap(long, default_value_t = false, verbatim_doc_comment)]
    hidden: bool,
}

impl Watch {
    fn handle_event(events: &mut HashSet<PathBuf>, event: Event) {
        match event.kind {
            EventKind::Create(create) if create == CreateKind::File => {
                events.extend(event.paths);
            }
            EventKind::Modify(_) => {
                events.extend(event.paths);
            }
            _ => log::debug!("Received event: {:?}", event),
        }
    }

    fn drain_messages(
        &self,
        receiver: &Receiver<Event>,
        debounce: Duration,
        max_entry: usize,
    ) -> HashSet<PathBuf> {
        let mut events = HashSet::new();

        match receiver.recv() {
            Ok(event) => Self::handle_event(&mut events, event),
            Err(RecvError) => {
                log::info!("Channel disconnected. Exiting...");
                return events;
            }
        }

        let start = Instant::now();
        while Instant::now().duration_since(start) < debounce && events.len() < max_entry {
            match receiver.try_recv() {
                Ok(event) => Self::handle_event(&mut events, event),
                Err(TryRecvError::Empty) => {
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(TryRecvError::Disconnected) => {
                    log::info!("Channel disconnected. Exiting...");
                    return events;
                }
            }
        }
        events
    }

    pub fn run(&self, ctx: Context) -> anyhow::Result<()> {
        let max_entry = 5000;
        let debounce = Duration::from_secs(1);
        let (tx, rx) = mpsc::channel::<Event>();

        let root_dir = ctx.root_dir().to_path_buf();

        let mut watcher = RecommendedWatcher::new(
            move |event| match event {
                Ok(event) => tx.send(event).unwrap(),
                Err(err) => eprintln!("Error: {:?}", err),
            },
            Config::default(),
        )?;
        watcher.watch(&root_dir, notify::RecursiveMode::Recursive)?;
        println!("Start watching {} ... (Ctrl+C to exit)", root_dir.display());

        let filter = IgnoreFilter::new(root_dir.clone(), self.hidden);
        loop {
            for path in self.drain_messages(&rx, debounce, max_entry) {
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

                let depth = path.depth();
                let file_entry = builder::FileEntry::new(ObjectType::File, path.clone(), depth);
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
