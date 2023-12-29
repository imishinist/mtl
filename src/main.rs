use std::time;

use clap::{Parser, Subcommand};

use mtl::commands;

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
    Local(commands::LocalCommand),

    /// Reference of object
    #[command(subcommand)]
    Ref(commands::RefCommand),

    /// Print the content of an object
    CatObject(commands::CatObjectCommand),

    /// Diff two tree objects
    Diff(commands::DiffCommand),

    /// Run garbage collection
    GC(commands::GCCommand),

    /// Print the tree of objects
    PrintTree(commands::PrintTreeCommand),
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

fn main() -> anyhow::Result<()> {
    #[cfg(feature = "dhat-heap")]
    let _profiler = dhat::Profiler::new_heap();

    env_logger::init();
    setup_signal_handler();
    let start = time::Instant::now();

    let mtl = MTLCommands::parse();
    match &mtl.commands {
        Commands::Local(local) => local.run()?,
        Commands::Ref(ref_command) => ref_command.run()?,
        Commands::CatObject(cat_object) => cat_object.run()?,
        Commands::Diff(diff) => diff.run()?,
        Commands::GC(gc) => gc.run()?,
        Commands::PrintTree(print_tree) => print_tree.run()?,
    }

    log::info!("Elapsed time: {:?}", start.elapsed());

    Ok(())
}
