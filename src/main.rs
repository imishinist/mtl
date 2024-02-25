use std::path::{Path, PathBuf};
use std::{env, time};

use clap::{Args, CommandFactory, Parser, Subcommand};
use clap_complete::Shell;

use mtl::{commands, Context};

/// MTL is a tool that recursively computes hash values for files.
#[derive(Parser)]
#[command(author, version, about, long_about=None)]
#[command(propagate_version = true)]
struct MTLCommands {
    /// Working directory.
    #[clap(short, long, value_name = "directory", verbatim_doc_comment)]
    dir: Option<PathBuf>,

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

    /// Print object-id of reference or expression
    RevParse(commands::RevParseCommand),

    /// Diff two tree objects
    Diff(commands::DiffCommand),

    /// Run garbage collection
    GC(commands::GCCommand),

    /// Pack the objects
    Pack(commands::PackCommand),

    /// Print the tree of objects
    PrintTree(commands::PrintTreeCommand),

    /// Tool subcommands
    #[command(subcommand)]
    Tool(commands::ToolCommands),

    /// Generate shell completion script
    Completion(CompletionCommand),
}

#[derive(Args, Debug)]
struct CompletionCommand {
    shell: Shell,
}

impl CompletionCommand {
    pub fn run(&self) {
        let first_arg = env::args().next();
        let program_name = first_arg
            .as_ref()
            .map(Path::new)
            .and_then(|path| path.file_stem())
            .and_then(|file| file.to_str())
            .unwrap_or("mtl");
        let mut cmd = MTLCommands::command();
        clap_complete::generate(self.shell, &mut cmd, program_name, &mut std::io::stdout());
    }
}

fn setup_signal_handler() {
    #[cfg(not(target_os = "windows"))]
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
}

fn main() -> anyhow::Result<()> {
    env_logger::init();
    setup_signal_handler();
    let start = time::Instant::now();

    let mtl = MTLCommands::parse();

    let dir = mtl
        .dir
        .as_ref()
        .unwrap_or(&env::current_dir()?)
        .canonicalize()?;
    log::info!("dir: {}", dir.display());

    let ctx = Context::new(&dir)?;
    match &mtl.commands {
        Commands::Local(local) => local.run(ctx)?,
        Commands::Ref(ref_command) => ref_command.run(ctx)?,
        Commands::CatObject(cat_object) => cat_object.run(ctx)?,
        Commands::RevParse(rev_parse) => rev_parse.run(ctx)?,
        Commands::Diff(diff) => diff.run(ctx)?,
        Commands::GC(gc) => gc.run(ctx)?,
        Commands::Pack(pack) => pack.run(ctx)?,
        Commands::PrintTree(print_tree) => print_tree.run(ctx)?,
        Commands::Tool(tool) => tool.run(ctx)?,
        Commands::Completion(completion) => completion.run(),
    }

    log::info!("Elapsed time: {:?}", start.elapsed());

    Ok(())
}
