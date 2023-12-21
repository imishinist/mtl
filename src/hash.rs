use std::path::Path;
use std::{fs, io};
use std::fs::File;
use std::io::{BufRead, BufReader};

pub fn md5_contents<T: AsRef<[u8]>>(contents: T) -> [u8; 16] {
    let mut context = md5::Context::new();
    context.consume(contents);

    context.compute().into()
}

pub fn md5_file<P: AsRef<Path>>(path: P) -> io::Result<[u8; 16]> {
    let contents = fs::read(path)?;
    Ok(md5_contents(contents))
}

pub fn md5_file_partial<P: AsRef<Path>>(path: P, buf_size: usize) -> io::Result<[u8; 16]> {
    let file = File::open(path)?;
    let len = file.metadata()?.len() as usize;

    let buf_len = len.min(buf_size);
    let mut buf = BufReader::with_capacity(buf_len, file);

    let mut context = md5::Context::new();
    loop {
        let part = buf.fill_buf()?;
        if part.is_empty() {
            break;
        }

        context.consume(part);

        let part_len = part.len();
        buf.consume(part_len);
    }
    Ok(context.compute().into())
}

pub fn md5_string(h: [u8; 16]) -> String {
    let mut s = String::new();
    for byte in h.iter() {
        s.push_str(&format!("{:02x}", byte));
    }
    s
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::path::{Path, PathBuf};
    use std::{fs, io};
    use std::fs::File;

    struct TempFile {
        path: PathBuf,
        file: fs::File,
    }

    impl TempFile {
        fn open<P: AsRef<Path>>(path: P) -> io::Result<Self> {
            let path = path.as_ref().to_path_buf();
            let file = fs::File::create(&path)?;
            Ok(Self { path, file })
        }

        fn write<T: AsRef<[u8]>>(&mut self, contents: T) -> io::Result<()> {
            self.file.write_all(contents.as_ref())
        }
    }

    impl Drop for TempFile {
        fn drop(&mut self) {
            fs::remove_file(&self.path).unwrap();
        }
    }

    #[test]
    fn test_md5_contents() {
        let contents = "Hello, world!";

        let expected = "6cd3556deb0da54bca060b4c39479839";
        let actual = super::md5_string(super::md5_contents(contents));
        assert_eq!(expected, actual);
    }

    #[test]
    fn test_md5_file() {
        let contents = "Hello, world!";
        let file = PathBuf::from("/tmp/test_md5_file");
        let mut temp_file = TempFile::open(&file).unwrap();
        temp_file.write(contents).unwrap();

        let expected = "6cd3556deb0da54bca060b4c39479839";
        let actual = super::md5_string(super::md5_file(&file).unwrap());
        assert_eq!(expected, actual);
    }

    #[test]
    fn test_md5_file_partial() {
        let contents = "Hello, world!";
        let file = PathBuf::from("/tmp/test_md5_file_partial");
        let mut temp_file = TempFile::open(&file).unwrap();
        temp_file.write(contents).unwrap();

        let expected = "6cd3556deb0da54bca060b4c39479839";
        let actual = super::md5_string(super::md5_file(&file).unwrap());
        assert_eq!(expected, actual);
    }

    #[test]
    fn compare_md5_file() {
        let file = PathBuf::from("/usr/share/dict/words");

        let expected = super::md5_file(&file).unwrap();
        let actual = super::md5_file_partial(&file).unwrap();
        assert_eq!(expected, actual);
    }

}
