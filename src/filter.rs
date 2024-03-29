use std::path::{Path, PathBuf};

use crate::{RelativePath, MTL_DIR};

pub trait Filter: Send + Sync {
    fn root(&self) -> &Path;

    fn path_matches(&self, path: &RelativePath) -> bool;
}

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

#[allow(dead_code)]
pub fn path_clean(path: &std::path::Path) -> std::path::PathBuf {
    let mut ret = std::path::PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::Prefix(prefix) => {
                ret.push(prefix.as_os_str());
            }
            std::path::Component::Normal(normal) => {
                ret.push(normal);
            }

            std::path::Component::RootDir => {
                ret.push("/");
            }
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                ret.pop();
            }
        }
    }
    ret
}

#[cfg(test)]
mod tests {
    use crate::filter::{path_clean, Filter, PathFilter};
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

    #[test]
    fn path_clean_test() {
        let path = path_clean(std::path::Path::new("/foo/bar/baz/.././foo"));
        assert_eq!(path, std::path::Path::new("/foo/bar/foo"));
    }
}
