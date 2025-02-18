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
use scopeguard::defer;
use similar::{self, Algorithm, ChangeTag, DiffOp};

use crate::{
    file_size, path::RelativePath, Context, Object, ObjectExpr, ObjectID, ObjectKind,
    ReadContentError, PACKED_OBJECTS_TABLE,
};

#[derive(Subcommand)]
pub enum LocalCommand {
    /// Build a tree of objects
    Build(local::Build),

    /// List target files
    List(local::List),

    /// Watch target files
    Watch(local::Watch),
}

impl LocalCommand {
    pub fn run(&self, ctx: Context) -> anyhow::Result<()> {
        match self {
            LocalCommand::Build(cmd) => cmd.run(ctx),
            LocalCommand::List(cmd) => cmd.run(ctx),
            LocalCommand::Watch(cmd) => cmd.run(ctx),
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
    object_id: ObjectExpr,
}

impl CatObjectCommand {
    pub fn run(&self, ctx: Context) -> anyhow::Result<()> {
        let object_id = self.object_id.resolve(&ctx)?;

        let contents = ctx.read_object(&object_id)?;
        let contents = String::from_utf8_lossy(&contents);
        print!("{}", contents);

        Ok(())
    }
}

#[derive(Args, Debug)]
pub struct RevParseCommand {
    /// Object expr to dereference
    #[clap(value_name = "object-id")]
    object_id: ObjectExpr,
}

impl RevParseCommand {
    pub fn run(&self, ctx: Context) -> anyhow::Result<()> {
        let object_id = self.object_id.resolve(&ctx)?;
        println!("{}", object_id);
        Ok(())
    }
}

#[derive(Args, Debug)]
pub struct DiffCommand {
    #[clap(value_name = "object-id")]
    pub object_a: ObjectExpr,

    #[clap(value_name = "object-id")]
    pub object_b: ObjectExpr,

    /// Maximum depth to print
    #[clap(long, value_name = "max-depth")]
    max_depth: Option<usize>,
}

impl DiffCommand {
    pub fn run(&self, ctx: Context) -> anyhow::Result<()> {
        let object_a = self.object_a.resolve(&ctx)?;
        let object_b = self.object_b.resolve(&ctx)?;

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
                                    .entry(object.basename.clone())
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
                                    &object_a.id,
                                    &object_b.id,
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
                let (kind_style_a, kind_style_b) = if object_a.kind == object_b.kind {
                    (Style::new(), Style::new())
                } else {
                    (Style::new().red(), Style::new().green())
                };
                let (id_style_a, id_style_b) = if object_a.id == object_b.id {
                    (Style::new(), Style::new())
                } else {
                    (Style::new().red(), Style::new().green())
                };
                let path = path.join(&object_a.basename);
                println!(
                    "{}/{} {}/{}\t{}/{}\t{}",
                    style("-").red(),
                    style("+").green(),
                    kind_style_a.apply_to(&object_a.kind),
                    kind_style_b.apply_to(&object_b.kind),
                    id_style_a.apply_to(&object_a.id),
                    id_style_b.apply_to(&object_b.id),
                    path.display(),
                );
            }
            (Some(object_a), None) => {
                let path = path.join(&object_a.basename);
                println!(
                    "{}/  {}/{}\t{}/{}\t{}",
                    style("-").red(),
                    style(&object_a.kind).red(),
                    " ".repeat(4),
                    style(&object_a.id).red(),
                    " ".repeat(16),
                    style(path.display()).red(),
                );
            }
            (None, Some(object_b)) => {
                let path = path.join(&object_b.basename);
                println!(
                    " /{} {}/{}\t{}/{}\t{}",
                    style("+").green(),
                    " ".repeat(4),
                    style(&object_b.kind).green(),
                    " ".repeat(16),
                    style(&object_b.id).green(),
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

        let tmp_file = pack_dir.join("tmp");
        defer! {
            if tmp_file.exists() {
                fs::remove_file(&tmp_file).unwrap();
            }
        }

        let db = Database::create(tmp_file.clone())?;
        let write_txn = db.begin_write()?;
        {
            let mut table = write_txn.open_table(PACKED_OBJECTS_TABLE)?;

            for object_id in ctx.list_object_ids()? {
                let content = ctx.read_object(&object_id)?;
                table.insert(object_id, content)?;

                let object_path = ctx.object_file(&object_id);
                if object_path.exists() {
                    fs::remove_file(&object_path)?;
                }
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
        let pack_file = ctx.pack_file();
        drop(ctx);

        fs::rename(&tmp_file, pack_file)?;

        Ok(())
    }
}

#[derive(Args, Debug)]
pub struct PrintTreeCommand {
    /// Root object ID where to start printing the tree
    #[clap(long, short, value_name = "object")]
    root: Option<ObjectExpr>,

    /// Type of objects to print
    #[clap(long, short, value_name = "type")]
    r#type: Option<ObjectKind>,

    /// Maximum depth to print
    #[clap(long, value_name = "max-depth")]
    max_depth: Option<usize>,
}

impl PrintTreeCommand {
    pub fn run(&self, ctx: Context) -> anyhow::Result<()> {
        let object_id = match self.root {
            Some(ref object_id) => object_id.resolve(&ctx)?,
            None => ctx.read_head()?,
        };
        let kind = self.r#type.as_ref();

        println!("tree {}\t.", object_id);
        Self::print_tree(&ctx, &object_id, kind, self.max_depth)?;

        Ok(())
    }

    fn print_tree(
        ctx: &Context,
        id: &ObjectID,
        kind: Option<&ObjectKind>,
        max_depth: Option<usize>,
    ) -> anyhow::Result<()> {
        let stdout = io::stdout();
        let mut stdout = BufWriter::new(stdout.lock());
        Self::inner_print_tree(ctx, &mut stdout, "", id, kind, max_depth, 0)
    }

    fn inner_print_tree<W: io::Write, P: Into<PathBuf>>(
        ctx: &Context,
        stdout: &mut W,
        parent: P,
        id: &ObjectID,
        kind: Option<&ObjectKind>,
        max_depth: Option<usize>,
        depth: usize,
    ) -> anyhow::Result<()> {
        if let Some(max_depth) = max_depth {
            if depth >= max_depth {
                return Ok(());
            }
        }

        let parent = parent.into();
        let objects = ctx.read_tree_contents(id)?;
        for object in &objects {
            let file_name = parent.join(&object.basename);

            match object.kind {
                ObjectKind::Tree => {
                    if kind.is_none() || kind == Some(&ObjectKind::Tree) {
                        writeln!(stdout, "tree {}\t{}/", object.id, file_name.display(),)?;
                    }
                    Self::inner_print_tree(
                        ctx,
                        stdout,
                        &file_name,
                        &object.id,
                        kind,
                        max_depth,
                        depth + 1,
                    )?;
                }
                ObjectKind::File => {
                    if kind.is_none() || kind == Some(&ObjectKind::File) {
                        writeln!(stdout, "file {}\t{}", object.id, file_name.display())?;
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
            .list_object_ids()?
            .iter()
            .map(|x| (*x, false))
            .collect::<HashMap<_, _>>();
        Self::mark_used_object(&ctx, &head_object, &mut objects)?;

        let object_refs = ctx.list_object_refs()?;
        for object_ref in object_refs {
            let object_id = ctx.deref_object_ref(&object_ref)?;
            Self::mark_used_object(&ctx, &object_id, &mut objects)?;
        }

        let mut deleted_objects = 0u64;
        let mut deleted_bytes = 0u64;
        for (object_id, used) in objects {
            if !used {
                let path = ctx.object_file(&object_id);
                let path_exists = path.exists();
                // Note: not remove from packed db
                if path_exists {
                    let metadata = fs::metadata(&path)?;
                    deleted_bytes += file_size(&metadata);
                }
                deleted_objects += 1;

                if path_exists {
                    if self.dry_run {
                        println!("[dry-run] Removing {}", path.display());
                    } else {
                        println!("Removing {}", path.display());
                        fs::remove_file(path)?;
                    }
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
        objects: &mut HashMap<ObjectID, bool>,
    ) -> anyhow::Result<(), ReadContentError> {
        objects.insert(*root_object, true);

        let tree = ctx.read_tree_contents(root_object)?;
        for object in tree {
            if object.is_tree() {
                objects.insert(object.id, true);
                Self::mark_used_object(ctx, &object.id, objects)?;
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

    /// redb commands
    Redb(tool::ReDB),
}

impl ToolCommands {
    pub fn run(&self, ctx: Context) -> anyhow::Result<()> {
        match self {
            ToolCommands::Generate(cmd) => cmd.run(),
            ToolCommands::Hash(cmd) => cmd.run(),
            ToolCommands::Redb(cmd) => cmd.run(ctx),
        }
    }
}
