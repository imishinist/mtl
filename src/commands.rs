pub mod local;
mod r#ref;
mod tool;

use std::collections::HashMap;
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::{fs, io};

use clap::{Args, Subcommand};
use console::{style, Style};
use itertools::Itertools;
use redb::Database;
use similar::{self, Algorithm, ChangeTag, DiffOp};

use crate::{
    file_size, Context, Object, ObjectID, ObjectRef, ObjectType, ReadContentError, RelativePath,
    PACKED_OBJECTS_TABLE,
};

#[derive(Subcommand)]
pub enum LocalCommand {
    /// Build a tree of objects
    Build(local::Build),

    /// List target files
    List(local::List),
}

impl LocalCommand {
    pub fn run(&self, ctx: Context) -> anyhow::Result<()> {
        match self {
            LocalCommand::Build(cmd) => cmd.run(ctx),
            LocalCommand::List(cmd) => cmd.run(ctx),
        }
    }
}

#[derive(Subcommand)]
pub enum RefCommand {
    /// List references
    List(r#ref::List),

    /// Save a reference
    Save(r#ref::Save),

    /// Delete a reference
    Delete(r#ref::Delete),
}

impl RefCommand {
    pub fn run(&self, ctx: Context) -> anyhow::Result<()> {
        match self {
            RefCommand::List(cmd) => cmd.run(ctx),
            RefCommand::Save(cmd) => cmd.run(ctx),
            RefCommand::Delete(cmd) => cmd.run(ctx),
        }
    }
}

#[derive(Args, Debug)]
pub struct CatObjectCommand {
    /// Object ID to print
    #[clap(value_name = "object-id")]
    object_id: ObjectRef,
}

impl CatObjectCommand {
    pub fn run(&self, ctx: Context) -> anyhow::Result<()> {
        let object_id = ctx.deref_object_ref(&self.object_id)?;
        let object_file = ctx.object_file(&object_id);
        let contents = fs::read_to_string(object_file)?;
        print!("{}", contents);

        Ok(())
    }
}

#[derive(Args, Debug)]
pub struct DiffCommand {
    #[clap(value_name = "object-id")]
    pub object_a: ObjectRef,

    #[clap(value_name = "object-id")]
    pub object_b: ObjectRef,

    /// Maximum depth to print
    #[clap(long, value_name = "max-depth")]
    max_depth: Option<usize>,
}

impl DiffCommand {
    pub fn run(&self, ctx: Context) -> anyhow::Result<()> {
        let object_a = ctx.deref_object_ref(&self.object_a)?;
        let object_b = ctx.deref_object_ref(&self.object_b)?;
        Self::print_diff(&ctx, &object_a, &object_b, self.max_depth)?;

        Ok(())
    }

    fn print_diff(
        ctx: &Context,
        object_a_id: &ObjectID,
        object_b_id: &ObjectID,
        max_depth: Option<usize>,
    ) -> anyhow::Result<()> {
        let object_a = Object::new_tree(*object_a_id, ".");
        let object_b = Object::new_tree(*object_b_id, ".");
        Self::print_difference(&RelativePath::Root, Some(&object_a), Some(&object_b))?;
        Self::inner_print_diff(
            ctx,
            &RelativePath::Root,
            object_a_id,
            object_b_id,
            max_depth,
            0,
        )
    }

    fn inner_print_diff<P: AsRef<Path>>(
        ctx: &Context,
        parent: P,
        object_a: &ObjectID,
        object_b: &ObjectID,
        max_depth: Option<usize>,
        depth: usize,
    ) -> anyhow::Result<()> {
        if object_a == object_b {
            return Ok(());
        }
        if let Some(max_depth) = max_depth {
            if depth >= max_depth {
                return Ok(());
            }
        }

        let parent = parent.as_ref();
        let tree_a = ctx.read_tree_contents(object_a)?;
        let tree_b = ctx.read_tree_contents(object_b)?;

        let diff = similar::capture_diff_slices(Algorithm::Myers, &tree_a, &tree_b);
        for op in diff {
            match op {
                DiffOp::Equal { .. } => continue,
                DiffOp::Delete { .. } => {
                    for change in op.iter_changes(&tree_a, &tree_b) {
                        Self::print_difference(parent, Some(change.value_ref()), None)?;
                    }
                }
                DiffOp::Insert { .. } => {
                    for change in op.iter_changes(&tree_a, &tree_b) {
                        Self::print_difference(parent, None, Some(change.value_ref()))?;
                    }
                }

                DiffOp::Replace { .. } => {
                    let file_names = op
                        .iter_changes(&tree_a, &tree_b)
                        .fold(
                            HashMap::new(),
                            |mut file_names: HashMap<RelativePath, Vec<_>>, change| {
                                let object = change.value();
                                file_names
                                    .entry(object.file_name.clone())
                                    .or_default()
                                    .push((change, object));
                                file_names
                            },
                        )
                        .into_iter()
                        .sorted_by(|(file_name_a, _), (file_name_b, _)| {
                            file_name_a.cmp(file_name_b)
                        })
                        .collect_vec();
                    for (file_name, changes) in file_names {
                        let mut object_a = None;
                        let mut object_b = None;
                        for (change, object) in changes {
                            match change.tag() {
                                ChangeTag::Delete => object_a = Some(object),
                                ChangeTag::Insert => object_b = Some(object),
                                ChangeTag::Equal => {}
                            }
                        }

                        Self::print_difference(parent, object_a.as_ref(), object_b.as_ref())?;
                        match (object_a, object_b) {
                            (Some(object_a), Some(object_b))
                                if object_a.is_tree() && object_b.is_tree() =>
                            {
                                Self::inner_print_diff(
                                    ctx,
                                    parent.join(&file_name),
                                    &object_a.object_id,
                                    &object_b.object_id,
                                    max_depth,
                                    depth + 1,
                                )?;
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        Ok(())
    }

    fn print_difference<P: AsRef<Path>>(
        path: P,
        object_a: Option<&Object>,
        object_b: Option<&Object>,
    ) -> io::Result<()> {
        let path = path.as_ref();
        match (object_a, object_b) {
            (Some(object_a), Some(object_b)) => {
                let (object_type_style_a, object_type_style_b) =
                    if object_a.object_type == object_b.object_type {
                        (Style::new(), Style::new())
                    } else {
                        (Style::new().red(), Style::new().green())
                    };
                let (object_id_style_a, object_id_style_b) =
                    if object_a.object_id == object_b.object_id {
                        (Style::new(), Style::new())
                    } else {
                        (Style::new().red(), Style::new().green())
                    };
                let path = path.join(&object_a.file_name);
                println!(
                    "{}/{} {}/{}\t{}/{}\t{}",
                    style("-").red(),
                    style("+").green(),
                    object_type_style_a.apply_to(&object_a.object_type),
                    object_type_style_b.apply_to(&object_b.object_type),
                    object_id_style_a.apply_to(&object_a.object_id),
                    object_id_style_b.apply_to(&object_b.object_id),
                    path.display(),
                );
            }
            (Some(object_a), None) => {
                let path = path.join(&object_a.file_name);
                println!(
                    "{}/  {}/{}\t{}/{}\t{}",
                    style("-").red(),
                    style(&object_a.object_type).red(),
                    " ".repeat(4),
                    style(&object_a.object_id).red(),
                    " ".repeat(16),
                    style(path.display()).red(),
                );
            }
            (None, Some(object_b)) => {
                let path = path.join(&object_b.file_name);
                println!(
                    " /{} {}/{}\t{}/{}\t{}",
                    style("+").green(),
                    " ".repeat(4),
                    style(&object_b.object_type).green(),
                    " ".repeat(16),
                    style(&object_b.object_id).green(),
                    style(path.display()).green(),
                );
            }
            (None, None) => {}
        }
        Ok(())
    }
}

#[derive(Args, Debug)]
pub struct PackCommand {}

impl PackCommand {
    pub fn run(&self, ctx: Context) -> anyhow::Result<()> {
        let pack_dir = ctx.pack_dir();
        fs::create_dir_all(&pack_dir)?;

        let db = Database::create(ctx.pack_file())?;
        let write_txn = db.begin_write()?;
        {
            let mut table = write_txn.open_table(PACKED_OBJECTS_TABLE)?;

            for object_id in ctx.list_object_ids()? {
                let object_path = ctx.object_file(&object_id);
                let content = fs::read_to_string(&object_path)?;
                table.insert(object_id, content.as_str())?;

                fs::remove_file(&object_path)?;
            }
        }
        write_txn.commit()?;

        let dirs = fs::read_dir(ctx.objects_dir())?;
        for dir in dirs {
            let dir = dir?;
            if !dir.file_type()?.is_dir() {
                continue;
            }

            fs::remove_dir(dir.path())?;
        }

        Ok(())
    }
}

#[derive(Args, Debug)]
pub struct PrintTreeCommand {
    /// Root object ID where to start printing the tree
    #[clap(long, short, value_name = "object")]
    root: Option<ObjectRef>,

    /// Type of objects to print
    #[clap(long, short, value_name = "type")]
    r#type: Option<ObjectType>,

    /// Maximum depth to print
    #[clap(long, value_name = "max-depth")]
    max_depth: Option<usize>,
}

impl PrintTreeCommand {
    pub fn run(&self, ctx: Context) -> anyhow::Result<()> {
        let object_id = match self.root {
            Some(ref object_id) => ctx.deref_object_ref(object_id)?,
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
    ) -> anyhow::Result<()> {
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
    ) -> anyhow::Result<()> {
        if let Some(max_depth) = max_depth {
            if depth >= max_depth {
                return Ok(());
            }
        }

        let parent = parent.into();
        let objects = ctx.read_tree_contents(object_id)?;
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
    /// Dry run
    #[clap(long = "dry", short = 'n', default_value_t = false)]
    dry_run: bool,
}

impl GCCommand {
    pub fn run(&self, ctx: Context) -> anyhow::Result<()> {
        let head_object = ctx.read_head()?;
        let mut objects = ctx
            .object_files()?
            .iter()
            .map(|x| (x.to_path_buf(), false))
            .collect::<HashMap<_, _>>();
        Self::mark_used_object(&ctx, &head_object, &mut objects)?;

        let object_refs = ctx.list_object_refs()?;
        for object_ref in object_refs {
            let object_id = ctx.deref_object_ref(&object_ref)?;
            Self::mark_used_object(&ctx, &object_id, &mut objects)?;
        }

        let mut deleted_objects = 0u64;
        let mut deleted_bytes = 0u64;
        for (path, used) in objects {
            if !used {
                let metadata = fs::metadata(&path)?;
                deleted_objects += 1;
                deleted_bytes += file_size(&metadata);

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
        ctx: &Context,
        root_object: &ObjectID,
        objects: &mut HashMap<PathBuf, bool>,
    ) -> anyhow::Result<(), ReadContentError> {
        let root_path = ctx.object_file(root_object);
        objects.insert(root_path, true);

        let tree = ctx.read_tree_contents(root_object)?;
        for object in tree {
            if object.is_tree() {
                let object_path = ctx.object_file(&object.object_id);

                objects.insert(object_path, true);
                Self::mark_used_object(ctx, &object.object_id, objects)?;
            }
        }

        Ok(())
    }
}

#[derive(Subcommand)]
pub enum ToolCommands {
    /// generate test data
    Generate(tool::Generate),

    /// calculate xxHash
    Hash(tool::Hash),

    /// fincore
    Fincore(tool::Fincore),

    /// fadvise
    Fadvise(tool::Fadvise),

    /// redb commands
    Redb(tool::ReDB),
}

impl ToolCommands {
    pub fn run(&self, _ctx: Context) -> anyhow::Result<()> {
        match self {
            ToolCommands::Generate(cmd) => cmd.run(),
            ToolCommands::Hash(cmd) => cmd.run(),
            ToolCommands::Fincore(cmd) => cmd.run(),
            ToolCommands::Fadvise(cmd) => cmd.run(),
            ToolCommands::Redb(cmd) => cmd.run(),
        }
    }
}
