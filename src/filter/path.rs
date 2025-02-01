use std::path::{Path, PathBuf};

use crate::filter::Filter;
use crate::RelativePath;

#[derive(Clone)]
pub struct PathFilter {
    root: PathBuf,
    target: RelativePath,
    parents: Vec<RelativePath>,
}

impl PathFilter {
    #[allow(dead_code)]
    pub fn new(root: PathBuf, target: impl Into<RelativePath>) -> Self {
        let target = target.into();
        let mut parents = Vec::new();

        let mut tmp = PathBuf::new();
        for component in target.components() {
            tmp.push(component);
            parents.push(RelativePath::from(tmp.as_path()));
        }
        Self {
            root,
            target,
            parents,
        }
    }
}

impl Filter for PathFilter {
    fn root(&self) -> &Path {
        &self.root
    }

    fn path_matches(&self, path: &RelativePath) -> bool {
        if self.target.is_root() {
            return true;
        }
        for parent in &self.parents {
            if parent.as_os_str() == path.as_os_str() {
                return true;
            }
        }

        let target = self.target.as_path();
        let path = path.as_path();
        path.starts_with(target)
    }
}

#[cfg(test)]
mod tests {
    use crate::filter::{Filter, PathFilter};
    use crate::RelativePath;
    use std::path::PathBuf;

    #[test]
    fn test_filter() {
        struct TestCase {
            name: &'static str,
            target: RelativePath,
            args: RelativePath,
            expected: bool,
        }

        let table = [
            TestCase {
                name: "basic",
                target: RelativePath::from("foo/bar"),
                args: RelativePath::from("foo/bar/baz"),
                expected: true,
            },
            TestCase {
                name: "basic",
                target: RelativePath::from("foo/bar"),
                args: RelativePath::from("foo/baz"),
                expected: false,
            },
            TestCase {
                name: "root",
                target: RelativePath::Root,
                args: RelativePath::from("foo/bar/baz"),
                expected: true,
            },
            TestCase {
                name: "sub",
                target: RelativePath::from("foo/bar"),
                args: RelativePath::from("foo"),
                expected: true,
            },
            TestCase {
                name: "sub",
                target: RelativePath::from("foo/bar/baz"),
                args: RelativePath::from("foo/bar"),
                expected: true,
            },
        ];
        for test in table.iter() {
            let root = PathBuf::new();
            let filter = PathFilter::new(root, test.target.clone());
            assert_eq!(
                filter.path_matches(&test.args),
                test.expected,
                "test case: {}",
                test.name
            );
        }
    }
}
