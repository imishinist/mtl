use std::borrow::Cow;
use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::Path;

use clap::ValueEnum;

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum Advise {
    Normal,
    Sequential,
    Random,
    WillNeed,
    DontNeed,
    NoReuse,
}

#[cfg(target_os="linux")]
pub fn fadvise(
    file: &fs::File,
    advice: Advise,
    offset: Option<u64>,
    len: Option<usize>,
) -> io::Result<()> {
    use std::os::fd::AsRawFd;
    let advice = match advice {
        Advise::Normal => libc::POSIX_FADV_NORMAL,
        Advise::Sequential => libc::POSIX_FADV_SEQUENTIAL,
        Advise::Random => libc::POSIX_FADV_RANDOM,
        Advise::WillNeed => libc::POSIX_FADV_WILLNEED,
        Advise::DontNeed => libc::POSIX_FADV_DONTNEED,
        Advise::NoReuse => libc::POSIX_FADV_NOREUSE,
    };
    let offset = offset.unwrap_or(0);
    let len = len.unwrap_or_else(|| file.metadata().map(|m| m.len()).unwrap_or(0) as usize);

    let ret = unsafe { libc::posix_fadvise(file.as_raw_fd(), offset as _, len as _, advice) };
    if ret == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(not(target_os="linux"))]
#[inline]
pub fn fadvise(
    _file: &fs::File,
    _advice: Advise,
    _offset: Option<u64>,
    _len: Option<usize>,
) -> io::Result<()> {
    Ok(())
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
