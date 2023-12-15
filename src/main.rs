use std::io;

use clap::{Parser, Subcommand};

use mtl::{LocalCommand, PrintTreeCommand};

#[derive(Parser)]
#[command(author, version, about, long_about=None)]
#[command(propagate_version = true)]
struct MTLCommands {
    #[command(subcommand)]
    commands: Commands,
}

#[derive(Subcommand)]
enum Commands {
    #[command(subcommand)]
    Local(LocalCommand),
    PrintTree(PrintTreeCommand),
}

fn main() -> io::Result<()> {
    env_logger::init();

    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }

    let mtl = MTLCommands::parse();
    match &mtl.commands {
        Commands::Local(local) => local.run()?,
        Commands::PrintTree(print_tree) => print_tree.run()?,
    }

    Ok(())
}
