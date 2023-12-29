use std::path::PathBuf;

use clap::Args;

use crate::{Context, ObjectRef};

#[derive(Args, Debug)]
pub struct List {
    /// Working directory.
    #[clap(short, long, value_name = "directory", verbatim_doc_comment)]
    dir: Option<PathBuf>,
}

impl List {
    pub fn run(&self, ctx: Context) -> anyhow::Result<()> {
        let refs = ctx.list_object_refs()?;
        for object_ref in refs {
            let object_id = ctx
                .deref_object_ref(&object_ref)
                .expect("invalid object id");
            println!("{}\t{}", object_ref, object_id);
        }
        Ok(())
    }
}

#[derive(Args, Debug)]
pub struct Save {
    /// Working directory.
    #[clap(short, long, value_name = "directory", verbatim_doc_comment)]
    dir: Option<PathBuf>,

    #[clap(value_name = "ref-name")]
    ref_name: String,

    #[clap(value_name = "object-id")]
    object_id: Option<ObjectRef>,
}

impl Save {
    pub fn run(&self, ctx: Context) -> anyhow::Result<()> {
        let object_id = match self.object_id {
            Some(ref object_id) => ctx.deref_object_ref(object_id).expect("invalid object id"),
            None => ctx.read_head()?,
        };

        ctx.write_object_ref(&self.ref_name, object_id)?;
        println!("Save \"{}\" to \"{}\"", object_id, self.ref_name);
        Ok(())
    }
}

#[derive(Args, Debug)]
pub struct Delete {
    /// Working directory.
    #[clap(short, long, value_name = "directory", verbatim_doc_comment)]
    dir: Option<PathBuf>,

    #[clap(value_name = "ref-name")]
    ref_name: String,
}

impl Delete {
    pub fn run(&self, ctx: Context) -> anyhow::Result<()> {
        ctx.delete_object_ref(&self.ref_name)?;
        println!("\"{}\" deleted", self.ref_name);
        Ok(())
    }
}
