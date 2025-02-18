mod ignore;
mod match_all;
mod path;

pub use ignore::IgnoreFilter;
pub use match_all::MatchAllFilter;
pub use path::PathFilter;

use std::path::Path;

use crate::RelativePath;

pub trait Filter: Send + Sync {
    fn root(&self) -> &Path;

    fn path_matches(&self, path: &RelativePath) -> bool;
}