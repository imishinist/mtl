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
    use crate::filter::path_clean;

    #[test]
    fn path_clean_test() {
        let path = path_clean(std::path::Path::new("/foo/bar/baz/.././foo"));
        assert_eq!(path, std::path::Path::new("/foo/bar/foo"));
    }
}
