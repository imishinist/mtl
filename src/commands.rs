pub mod local;

use std::io::BufWriter;
use std::path::PathBuf;
use std::{env, fs, io};

use clap::{Args, Subcommand};

use crate::{read_tree_contents, ref_head_name, ObjectID, ObjectType};

#[derive(Subcommand)]
pub enum LocalCommand {
    /// Build a tree of objects
    Build(local::Build),
}

impl LocalCommand {
    pub fn run(&self) -> io::Result<()> {
        match self {
            LocalCommand::Build(cmd) => cmd.run(),
        }
    }
}

#[derive(Args, Debug)]
pub struct PrintTreeCommand {
    /// Directory where to run the command
    #[clap(long, short, value_name = "dir", value_hint = clap::ValueHint::DirPath)]
    dir: Option<PathBuf>,

    /// Root object ID where to start printing the tree
    #[clap(long, short, value_name = "object")]
    root: Option<ObjectID>,

    /// Type of objects to print
    #[clap(long, short, value_name = "type")]
    r#type: Option<ObjectType>,

    /// Maximum depth to print
    #[clap(long, value_name = "max-depth")]
    max_depth: Option<usize>,
}

impl PrintTreeCommand {
    pub fn run(&self) -> io::Result<()> {
        let _dir = match self.dir {
            Some(ref dir) => {
                env::set_current_dir(dir)?;
                dir.clone()
            }
            None => env::current_dir()?,
        };

        let object_id = match self.root {
            Some(ref object_id) => object_id.clone(),
            None => {
                let head = fs::read_to_string(ref_head_name())?;
                ObjectID::from_hex(&head)?
            }
        };
        let object_type = self.r#type.as_ref();

        println!("tree {}\t<root>", object_id);
        Self::print_tree("", &object_id, object_type, self.max_depth)?;

        Ok(())
    }

    fn print_tree<P: Into<PathBuf>>(
        parent: P,
        object_id: &ObjectID,
        object_type: Option<&ObjectType>,
        max_depth: Option<usize>,
    ) -> io::Result<()> {
        let stdout = io::stdout();
        let mut stdout = BufWriter::new(stdout.lock());
        Self::inner_print_tree(&mut stdout, parent, object_id, object_type, max_depth, 0)
    }

    fn inner_print_tree<W: io::Write, P: Into<PathBuf>>(
        stdout: &mut W,
        parent: P,
        object_id: &ObjectID,
        object_type: Option<&ObjectType>,
        max_depth: Option<usize>,
        depth: usize,
    ) -> io::Result<()> {
        if let Some(max_depth) = max_depth {
            if depth >= max_depth {
                return Ok(());
            }
        }

        let parent = parent.into();
        let objects = read_tree_contents(object_id).unwrap();
        for object in &objects {
            let file_name = parent.join(&object.file_name);

            match object.object_type {
                ObjectType::Tree => {
                    if object_type.is_none() || object_type == Some(&ObjectType::Tree) {
                        writeln!(
                            stdout,
                            "tree {}\t{}/",
                            object.object_id,
                            file_name.display(),
                        )?;
                    }
                    Self::inner_print_tree(
                        stdout,
                        &file_name,
                        &object.object_id,
                        object_type,
                        max_depth,
                        depth + 1,
                    )?;
                }
                ObjectType::File => {
                    if object_type.is_none() || object_type == Some(&ObjectType::File) {
                        writeln!(stdout, "file {}\t{}", object.object_id, file_name.display())?;
                    }
                }
            }
        }
        Ok(())
    }
}
