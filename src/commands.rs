pub mod local;

use std::path::{Path, PathBuf};
use std::{env, fs, io};

use clap::{Args, Subcommand};

use crate::{read_tree_contents, ref_head_name, ObjectID, ObjectType};

#[derive(Subcommand)]
pub enum LocalCommand {
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
    #[clap(long, short)]
    dir: Option<String>,

    #[clap(long, short)]
    object_id: Option<String>,

    #[clap(long, short)]
    r#type: Option<ObjectType>,

    #[clap(long, short)]
    max_depth: Option<usize>,
}

impl PrintTreeCommand {
    pub fn run(&self) -> io::Result<()> {
        let dir = self
            .dir
            .as_ref()
            .map(|s| PathBuf::from(s))
            .unwrap_or_else(|| env::current_dir().unwrap());
        env::set_current_dir(&dir)?;

        let object_id = self
            .object_id
            .as_ref()
            .map(|s| ObjectID::from_hex(s))
            .unwrap_or_else(|| {
                let head = fs::read_to_string(ref_head_name())?;
                Ok(ObjectID::from_hex(&head)?)
            })?;

        let object_type = self.r#type.as_ref();

        println!("tree {}\t<root>", object_id);
        Self::print_tree("", &object_id, object_type, 0, self.max_depth)?;
        Ok(())
    }

    fn print_tree<P: AsRef<Path>>(
        parent: P,
        object_id: &ObjectID,
        object_type: Option<&ObjectType>,
        depth: usize,
        max_depth: Option<usize>,
    ) -> io::Result<()> {
        if let Some(max_depth) = max_depth {
            if depth >= max_depth {
                return Ok(());
            }
        }

        let parent = parent.as_ref().to_path_buf();
        let objects = read_tree_contents(object_id).unwrap();
        for object in &objects {
            match object.object_type {
                ObjectType::Tree => {
                    if object_type.is_none() || object_type == Some(&ObjectType::Tree) {
                        println!(
                            "tree {}\t{}/",
                            object.object_id,
                            parent.join(&object.file_name).display()
                        );
                    }
                    Self::print_tree(
                        &parent.join(&object.file_name),
                        &object.object_id,
                        object_type,
                        depth + 1,
                        max_depth,
                    )?;
                }
                ObjectType::File => {
                    if object_type.is_none() || object_type == Some(&ObjectType::File) {
                        println!(
                            "file {}\t{}",
                            object.object_id,
                            parent.join(&object.file_name).display()
                        );
                    }
                }
            }
        }

        Ok(())
    }
}
