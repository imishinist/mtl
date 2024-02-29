use std::ffi::OsString;
use std::path::PathBuf;

use clap::Args;

use crate::builder::{Builder, FileTargetGenerator, ScanTargetGenerator, TargetGenerator};
use crate::filter::{Filter, MatchAllFilter, PathFilter};
use crate::Context;

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
pub struct Update {
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

    path: PathBuf,
}

impl Update {
    pub fn run(&self, ctx: Context) -> anyhow::Result<()> {
        let mut ctx = ctx;
        ctx.set_drop_cache(self.drop_cache);

        let root_dir = ctx.root_dir().to_path_buf();
        let generator = get_generator(root_dir, Some(&self.path), None, self.hidden);
        let builder = Builder::new(generator, self.progress);
        let root = builder.update(&ctx, &self.path)?;
        match self.no_write_head {
            true => println!("HEAD: {}", root.object_id),
            false => {
                ctx.write_head(&root.object_id)?;
                println!("Written HEAD: {}", root.object_id);
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
