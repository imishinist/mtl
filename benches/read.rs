use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::env;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::{fs, io};

struct TestDir {
    path: PathBuf,

    files: Vec<PathBuf>,
}

impl TestDir {
    fn new<P: AsRef<Path>>(name: P) -> io::Result<Self> {
        let path = env::temp_dir().join(name);
        fs::create_dir_all(&path)?;
        Ok(Self {
            path,
            files: vec![],
        })
    }

    fn create_file(&mut self, name: &str, contents: &[u8]) -> io::Result<PathBuf> {
        let path = self.path.join(name);
        let mut file = fs::File::create(&path)?;
        file.write_all(contents)?;
        self.files.push(path.clone());
        Ok(path)
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        for file in &self.files {
            fs::remove_file(file).unwrap();
        }
        fs::remove_dir(&self.path).unwrap();
    }
}

fn generate_random(need_bytes: usize) -> Vec<u8> {
    (0..need_bytes)
        .map(|_| rand::random::<u8>())
        .collect::<Vec<_>>()
}

fn setup(dir: &mut TestDir) -> io::Result<PathBuf> {
    let random_file_name = rand::random::<u64>().to_string();
    let random_contents = generate_random(20000);

    dir.create_file(&random_file_name, &random_contents)
}

fn read_outside_buffer(c: &mut Criterion) {
    let mut dir = TestDir::new("read_outside_buffer").unwrap();

    c.bench_function("read_outside_buffer", |b| {
        let mut buf = Vec::new();
        b.iter(|| {
            let file_name = setup(&mut dir).unwrap();
            let mut file = fs::File::open(file_name).unwrap();
            file.read_to_end(black_box(&mut buf)).unwrap();
        })
    });
}

fn read_inside_buffer(c: &mut Criterion) {
    let mut dir = TestDir::new("read_inside_buffer").unwrap();

    c.bench_function("read_inside_buffer", |b| {
        b.iter(|| {
            let mut buf = Vec::new();
            let file_name = setup(&mut dir).unwrap();
            let mut file = fs::File::open(file_name).unwrap();
            file.read_to_end(black_box(&mut buf)).unwrap();
        })
    });
}

criterion_group!(read, read_outside_buffer, read_inside_buffer);
criterion_main!(read);
