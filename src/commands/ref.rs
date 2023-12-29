use crate::{Context, ObjectRef};
use clap::Args;
use std::path::PathBuf;
use std::{env, io};

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
    pub fn run(&self) -> io::Result<()> {
        let dir = self
            .dir
            .as_ref()
            .unwrap_or(&env::current_dir()?)
            .canonicalize()?;
        log::info!("dir: {}", dir.display());

        let ctx = Context::new(&dir);
        let object_id = match self.object_id {
            Some(ref object_id) => ctx.deref_object_ref(object_id).expect("invalid object id"),
            None => ctx.read_head()?,
        };

        ctx.write_object_ref(&self.ref_name, object_id)?;
        println!("Save \"{}\" to \"{}\"", object_id, self.ref_name);
        Ok(())
    }
}
