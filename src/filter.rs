use crate::RelativePath;

pub trait Filter {
    fn path_matches(&self, path: &RelativePath) -> bool;
}

pub struct PathFilter {
    target: RelativePath,
}

impl PathFilter {
    #[allow(dead_code)]
    pub fn new(target: impl Into<RelativePath>) -> Self {
        Self {
            target: target.into(),
        }
    }
}

impl Filter for PathFilter {
    fn path_matches(&self, path: &RelativePath) -> bool {
        if self.target.is_root() {
            return true;
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
        ];
        for test in table.iter() {
            let filter = PathFilter::new(test.target.clone());
            assert_eq!(
                filter.path_matches(&test.args),
                test.expected,
                "test case: {}",
                test.name
            );
        }
    }
}
