use std::borrow::Cow;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

#[cfg(not(windows))]
pub fn path_from_bytes(bytes: &[u8]) -> PathBuf {
    use std::os::unix::ffi::OsStrExt;

    let os_str = OsStr::from_bytes(bytes);
    PathBuf::from(os_str)
}

#[cfg(windows)]
fn path_from_bytes(bytes: &[u8]) -> PathBuf {
    let s = std::str::from_utf8(bytes).expect("Invalid UTF-8");
    std::path::PathBuf::from(s)
}

#[cfg(any(unix, target_os = "redox"))]
pub fn osstr_to_bytes(input: &OsStr) -> Cow<[u8]> {
    use std::os::unix::ffi::OsStrExt;
    Cow::Borrowed(input.as_bytes())
}

#[cfg(windows)]
pub fn osstr_to_bytes(input: &OsStr) -> Cow<[u8]> {
    let string = input.to_string_lossy();

    match string {
        Cow::Owned(string) => Cow::Owned(string.into_bytes()),
        Cow::Borrowed(string) => Cow::Borrowed(string.as_bytes()),
    }
}

#[cfg(unix)]
pub fn file_size(metadata: &fs::Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;
    metadata.size()
}

#[cfg(windows)]
pub fn file_size(metadata: &fs::Metadata) -> u64 {
    use std::os::windows::fs::MetadataExt;
    metadata.file_size()
}

pub fn strip_current_dir(path: &Path) -> &Path {
    path.strip_prefix(".").unwrap_or(path)
}

#[cfg(test)]
mod tests {
    use super::strip_current_dir;
    use std::path::Path;

    #[test]
    fn strip_current_dir_basic() {
        assert_eq!(strip_current_dir(Path::new("./foo")), Path::new("foo"));
        assert_eq!(strip_current_dir(Path::new("foo")), Path::new("foo"));
        assert_eq!(
            strip_current_dir(Path::new("./foo/bar/baz")),
            Path::new("foo/bar/baz")
        );
        assert_eq!(
            strip_current_dir(Path::new("foo/bar/baz")),
            Path::new("foo/bar/baz")
        );
    }
}
