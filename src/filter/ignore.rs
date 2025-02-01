use ignore::gitignore::{Gitignore, GitignoreBuilder};
use std::path::{Path, PathBuf};

use crate::filter::Filter;
use crate::RelativePath;

#[derive(Clone)]
pub struct IgnoreFilter {
    root: PathBuf,
    ignore: Gitignore,

    hidden: bool,
}

impl IgnoreFilter {
    pub fn new<P: AsRef<Path>>(root: P, hidden: bool) -> Self {
        let root = root.as_ref();
        let mut builder = GitignoreBuilder::new(&root);
        builder.add(&root.join(".gitignore"));
        builder.add(&root.join(".ignore"));
        let _ = builder.add_line(None, ".git/");
        let _ = builder.add_line(None, ".mtl/");

        let ignore = builder.build().unwrap_or(Gitignore::empty());
        Self {
            root: root.to_path_buf(),
            ignore,
            hidden,
        }
    }
}

impl Filter for IgnoreFilter {
    fn root(&self) -> &Path {
        &self.root
    }

    fn path_matches(&self, path: &RelativePath) -> bool {
        let path = path.as_path();
        if !self.hidden && is_hidden(path) {
            return false;
        }
        !self
            .ignore
            .matched_path_or_any_parents(path, true)
            .is_ignore()
    }
}

fn is_hidden<P: AsRef<Path>>(path: P) -> bool {
    let components = path.as_ref().components();
    for component in components {
        if let std::path::Component::Normal(normal) = component {
            if normal.to_string_lossy().starts_with('.') {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use crate::filter::{Filter, IgnoreFilter};
    use crate::RelativePath;

    #[test]
    fn test_filter() {
        let ignore = r#"
target/
"#;
        let tmp = tempfile::tempdir().unwrap();
        let ignorefile = tmp.path().join(".gitignore");
        std::fs::write(&ignorefile, ignore).unwrap();

        let root = tmp.path();
        let filter = IgnoreFilter::new(root, false);

        assert_eq!(filter.path_matches(&RelativePath::from("foo")), true);
        assert_eq!(filter.path_matches(&RelativePath::from(".foo")), false);
        assert_eq!(filter.path_matches(&RelativePath::from("foo/bar")), true);
        assert_eq!(filter.path_matches(&RelativePath::from("foo/.bar")), false);
        assert_eq!(
            filter.path_matches(&RelativePath::from("foo/bar/.baz")),
            false
        );
        assert_eq!(
            filter.path_matches(&RelativePath::from("foo/bar/baz")),
            true
        );
        assert_eq!(
            filter.path_matches(&RelativePath::from("target/debug/mtl")),
            false
        );
    }
}
