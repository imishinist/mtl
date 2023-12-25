pub mod local;

use std::collections::HashMap;
use std::io::BufWriter;
use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;
use std::{env, fs, io};

use clap::{Args, Subcommand};
use similar::{self, Algorithm};

use crate::{Context, ObjectID, ObjectType, ParseError};

#[derive(Subcommand)]
pub enum LocalCommand {
    /// Build a tree of objects
    Build(local::Build),

    /// List target files
    List(local::List),
}

impl LocalCommand {
    pub fn run(&self) -> io::Result<()> {
        match self {
            LocalCommand::Build(cmd) => cmd.run(),
            LocalCommand::List(cmd) => cmd.run(),
        }
    }
}

#[derive(Args, Debug)]
pub struct CatObjectCommand {
    /// Directory where to run the command
    #[clap(long, short, value_name = "dir", value_hint = clap::ValueHint::DirPath)]
    dir: Option<PathBuf>,

    /// Object ID to print
    #[clap(value_name = "object-id")]
    object_id: ObjectID,
}

impl CatObjectCommand {
    pub fn run(&self) -> io::Result<()> {
        let dir = self
            .dir
            .as_ref()
            .unwrap_or(&env::current_dir()?)
            .canonicalize()?;
        log::info!("dir: {}", dir.display());

        let ctx = Context::new(&dir);

        let file_name = ctx.object_file(&self.object_id);
        let contents = fs::read_to_string(file_name)?;
        print!("{}", contents);

        Ok(())
    }
}

#[derive(Args, Debug)]
pub struct DiffCommand {
    /// Directory where to run the command
    #[clap(long, short, value_name = "dir", value_hint = clap::ValueHint::DirPath)]
    dir: Option<PathBuf>,

    #[clap(value_name = "object-id")]
    pub object_a: ObjectID,

    #[clap(value_name = "object-id")]
    pub object_b: ObjectID,
}

impl DiffCommand {
    pub fn run(&self) -> io::Result<()> {
        let dir = self
            .dir
            .as_ref()
            .unwrap_or(&env::current_dir()?)
            .canonicalize()?;
        log::info!("dir: {}", dir.display());

        let ctx = Context::new(&dir);

        let tree_a = ctx.read_tree_contents(&self.object_a).expect("tree_a");
        let tree_b = ctx.read_tree_contents(&self.object_b).expect("tree_b");
        let group = similar::group_diff_ops(
            similar::capture_diff_slices(Algorithm::Myers, &tree_a, &tree_b),
            10,
        );

        println!("--- {}\n+++ {}", self.object_a, self.object_b);
        for (_idx, group) in group.iter().enumerate() {
            for op in group {
                let old = op.old_range();
                let new = op.new_range();
                println!(
                    "@@ -{},{} +{},{} @@",
                    old.start + 1,
                    old.len(),
                    new.start + 1,
                    new.len());
                for change in op.iter_changes(&tree_a, &tree_b) {
                    match change.tag() {
                        similar::ChangeTag::Delete => {
                            println!("- {}", change.value());
                        }
                        similar::ChangeTag::Insert => {
                            println!("+ {}", change.value());
                        }
                        similar::ChangeTag::Equal => {
                            println!("  {}", change.value());
                        }
                    }
                }
            }
        }

        Ok(())
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
        let dir = self
            .dir
            .as_ref()
            .unwrap_or(&env::current_dir()?)
            .canonicalize()?;
        log::info!("dir: {}", dir.display());
        let ctx = Context::new(&dir);

        let object_id = match self.root {
            Some(ref object_id) => object_id.clone(),
            None => ctx.read_head()?,
        };
        let object_type = self.r#type.as_ref();

        println!("tree {}\t.", object_id);
        Self::print_tree(&ctx, &object_id, object_type, self.max_depth)?;

        Ok(())
    }

    fn print_tree(
        ctx: &Context,
        object_id: &ObjectID,
        object_type: Option<&ObjectType>,
        max_depth: Option<usize>,
    ) -> io::Result<()> {
        let stdout = io::stdout();
        let mut stdout = BufWriter::new(stdout.lock());
        Self::inner_print_tree(ctx, &mut stdout, "", object_id, object_type, max_depth, 0)
    }

    fn inner_print_tree<W: io::Write, P: Into<PathBuf>>(
        ctx: &Context,
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
        let objects = ctx.read_tree_contents(object_id).unwrap();
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
                        ctx,
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

#[derive(Debug, Args)]
pub struct GCCommand {
    /// Directory where to run the command
    #[clap(long, short, value_name = "dir", value_hint = clap::ValueHint::DirPath)]
    dir: Option<PathBuf>,

    /// Dry run
    #[clap(long = "dry", short = 'n', default_value_t = false)]
    dry_run: bool,
}

impl GCCommand {
    pub fn run(&self) -> io::Result<()> {
        let dir = self
            .dir
            .as_ref()
            .unwrap_or(&env::current_dir()?)
            .canonicalize()?;
        log::info!("dir: {}", dir.display());
        let ctx = Context::new(&dir);

        let head_object = ctx.read_head()?;
        let mut objects = ctx
            .object_files()?
            .iter()
            .map(|x| (x.to_path_buf(), false))
            .collect::<HashMap<_, _>>();
        self.mark_used_object(&ctx, &head_object, &mut objects)
            .expect("mark_used_object");

        let mut deleted_objects = 0u64;
        let mut deleted_bytes = 0u64;
        for (path, used) in objects {
            if !used {
                let metadata = fs::metadata(&path)?;
                deleted_objects += 1;
                deleted_bytes += metadata.size();
                if self.dry_run {
                    println!("[dry-run] Removing {}", path.display());
                } else {
                    println!("Removing {}", path.display());
                    fs::remove_file(path)?;
                }
            }
        }
        if self.dry_run {
            println!(
                "[dry-run] Deleted {} objects ({} bytes)",
                deleted_objects, deleted_bytes
            );
        } else {
            println!(
                "Deleted {} objects ({} bytes)",
                deleted_objects, deleted_bytes
            );
        }

        Ok(())
    }

    pub fn mark_used_object(
        &self,
        ctx: &Context,
        root_object: &ObjectID,
        objects: &mut HashMap<PathBuf, bool>,
    ) -> Result<(), ParseError> {
        let root_path = ctx.object_file(root_object);
        objects.insert(root_path, true);

        let tree = ctx.read_tree_contents(root_object)?;
        for object in tree {
            if object.is_tree() {
                let object_path = ctx.object_file(&object.object_id);

                objects.insert(object_path, true);
                self.mark_used_object(ctx, &object.object_id, objects)?;
            }
        }

        Ok(())
    }
}
