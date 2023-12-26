use std::{io, time};

use clap::{Parser, Subcommand};

use mtl::{CatObjectCommand, DiffCommand, GCCommand, LocalCommand, PrintTreeCommand};

/// MTL is a tool that recursively computes hash values for files.
#[derive(Parser)]
#[command(author, version, about, long_about=None)]
#[command(propagate_version = true)]
struct MTLCommands {
    #[command(subcommand)]
    commands: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run a command locally
    #[command(subcommand)]
    Local(LocalCommand),

    /// Print the content of an object
    CatObject(CatObjectCommand),

    /// Diff two tree objects
    Diff(DiffCommand),

    /// Run garbage collection
    GC(GCCommand),

    /// Print the tree of objects
    PrintTree(PrintTreeCommand),
}

#[cfg(feature = "dhat-heap")]
#[global_allocator]
static GLOBAL: dhat::Alloc = dhat::Alloc;

fn setup_signal_handler() {
    #[cfg(not(target_os = "windows"))]
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
}

fn main() -> io::Result<()> {
    #[cfg(feature = "dhat-heap")]
    let _profiler = dhat::Profiler::new_heap();

    env_logger::init();
    setup_signal_handler();
    let start = time::Instant::now();

    let mtl = MTLCommands::parse();
    match &mtl.commands {
        Commands::Local(local) => local.run()?,
        Commands::CatObject(cat_object) => cat_object.run()?,
        Commands::Diff(diff) => diff.run()?,
        Commands::GC(gc) => gc.run()?,
        Commands::PrintTree(print_tree) => print_tree.run()?,
    }

    log::info!("Elapsed time: {:?}", start.elapsed());

    Ok(())
}
