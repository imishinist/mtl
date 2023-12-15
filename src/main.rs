use std::io;

use clap::{Parser, Subcommand};

use mtl::LocalCommand;

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
}

fn main() -> io::Result<()> {
    env_logger::init();

    let mtl = MTLCommands::parse();
    match &mtl.commands {
        Commands::Local(local) => local.run()?,
    }

    Ok(())
}
