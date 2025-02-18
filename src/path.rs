use std::cmp::Ordering;
use std::fmt;
use std::ops::Deref;
use std::path::{Path, PathBuf};

#[derive(Debug, PartialEq, Eq, Clone, std::hash::Hash)]
pub enum RelativePath {
    Root,
    Path(PathBuf),
}

impl RelativePath {
    pub fn is_root(&self) -> bool {
        matches!(self, RelativePath::Root)
    }

    pub fn is_path(&self) -> bool {
        matches!(self, RelativePath::Path(_))
    }

    pub fn parent(&self) -> Self {
        match self {
            RelativePath::Root => RelativePath::Root,
            RelativePath::Path(path) => match path.parent() {
                None => RelativePath::Root,
                Some(parent) if parent.as_os_str().eq("") => RelativePath::Root,
                Some(parent) => RelativePath::Path(parent.to_path_buf()),
            },
        }
    }

    pub fn file_name(&self) -> Option<PathBuf> {
        match self {
            RelativePath::Root => None,
            RelativePath::Path(path) => path.file_name().map(PathBuf::from),
        }
    }

    pub fn as_path(&self) -> &Path {
        match self {
            RelativePath::Root => Path::new(""),
            RelativePath::Path(path) => path.as_path(),
        }
    }

    pub fn join<P: AsRef<Path>>(&self, name: P) -> Self {
        match self {
            RelativePath::Root => RelativePath::Path(PathBuf::from(name.as_ref())),
            RelativePath::Path(path) => RelativePath::Path(path.join(name)),
        }
    }

    pub fn depth(&self) -> usize {
        match self {
            RelativePath::Root => 0,
            RelativePath::Path(path) => path.components().count(),
        }
    }
}

impl<P: Into<PathBuf>> From<P> for RelativePath {
    fn from(path: P) -> Self {
        RelativePath::Path(path.into())
    }
}

impl AsRef<Path> for RelativePath {
    fn as_ref(&self) -> &Path {
        self.as_path()
    }
}

impl Deref for RelativePath {
    type Target = Path;

    fn deref(&self) -> &Self::Target {
        match self {
            RelativePath::Root => Path::new(""),
            RelativePath::Path(path) => path.as_path(),
        }
    }
}

impl fmt::Display for RelativePath {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        match self {
            RelativePath::Root => write!(f, ""),
            RelativePath::Path(path) => write!(f, "{}", path.display()),
        }
    }
}

impl PartialOrd<Self> for RelativePath {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for RelativePath {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (RelativePath::Root, RelativePath::Root) => Ordering::Equal,
            (RelativePath::Root, RelativePath::Path(_)) => Ordering::Less,
            (RelativePath::Path(_), RelativePath::Root) => Ordering::Greater,
            (RelativePath::Path(a), RelativePath::Path(b)) => a.cmp(b),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::path::RelativePath;
    use std::path::{Path, PathBuf};

    #[test]
    fn test_is() {
        assert_eq!(RelativePath::Root.is_root(), true);
        assert_eq!(RelativePath::Root.is_path(), false);

        assert_eq!(RelativePath::from(Path::new("foo")).is_root(), false);
        assert_eq!(RelativePath::from(Path::new("foo")).is_path(), true);
    }

    #[test]
    fn test_parent() {
        let root = RelativePath::Root;
        assert_eq!(root.parent(), RelativePath::Root);

        let path = RelativePath::from(Path::new("foo"));
        assert_eq!(path.parent(), RelativePath::Root);

        let path = RelativePath::from(Path::new("foo/bar"));
        assert_eq!(path.parent(), RelativePath::from(Path::new("foo")));

        let path = RelativePath::from(Path::new("foo/bar/baz"));
        assert_eq!(path.parent(), RelativePath::from(Path::new("foo/bar")));
    }

    #[test]
    fn test_file_name() {
        let root = RelativePath::Root;
        assert_eq!(root.file_name(), None);

        let path = RelativePath::from(Path::new("foo"));
        assert_eq!(path.file_name(), Some(PathBuf::from("foo")));

        let path = RelativePath::from(Path::new("foo/bar"));
        assert_eq!(path.file_name(), Some(PathBuf::from("bar")));

        let path = RelativePath::from(Path::new("foo/bar/baz"));
        assert_eq!(path.file_name(), Some(PathBuf::from("baz")));
    }

    #[test]
    fn test_as_path() {
        let root = RelativePath::Root;
        assert_eq!(root.as_path(), Path::new(""));

        let path = RelativePath::from(Path::new("foo"));
        assert_eq!(path.as_path(), Path::new("foo"));

        let path = RelativePath::from(Path::new("foo/bar"));
        assert_eq!(path.as_path(), Path::new("foo/bar"));
    }

    #[test]
    fn test_join() {
        let root = RelativePath::Root;
        assert_eq!(root.join("foo"), RelativePath::from(Path::new("foo")));

        let path = RelativePath::from(Path::new("foo"));
        assert_eq!(path.join("bar"), RelativePath::from(Path::new("foo/bar")));

        let path = RelativePath::from(Path::new("foo/bar"));
        assert_eq!(
            path.join("baz"),
            RelativePath::from(Path::new("foo/bar/baz"))
        );
    }
}
