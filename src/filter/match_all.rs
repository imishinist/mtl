use std::path::{Path, PathBuf};

use crate::filter::Filter;
use crate::{path::RelativePath, MTL_DIR};

#[derive(Clone)]
pub struct MatchAllFilter(PathBuf);

impl MatchAllFilter {
    pub fn new(root: PathBuf) -> Self {
        Self(root)
    }
}

impl Filter for MatchAllFilter {
    fn root(&self) -> &Path {
        &self.0
    }

    fn path_matches(&self, path: &RelativePath) -> bool {
        let path = path.as_os_str().as_encoded_bytes();
        !path.starts_with(MTL_DIR.as_bytes()) && !path.starts_with(b".git")
    }
}
