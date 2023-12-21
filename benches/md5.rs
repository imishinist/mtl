use std::fs;

use criterion::{black_box, criterion_group, Criterion};

use mtl::hash;

#[cfg(feature = "dhat-heap")]
#[global_allocator]
static GLOBAL: dhat::Alloc = dhat::Alloc;

fn md5_contents(c: &mut Criterion) {
    let contents = fs::read("/usr/share/dict/words").unwrap();
    c.bench_function("md5_contents", |b| {
        b.iter(|| hash::md5_contents(black_box(&contents)))
    });
}

fn md5_file(c: &mut Criterion) {
    let path = "/usr/share/dict/words";
    // c.bench_function("md5_file", |b| b.iter(|| hash::md5_file(black_box(path))));
    c.bench_function("md5_file", |b| b.iter(|| hash::md5_file_partial(black_box(path), 1_000)));
}

// criterion_group!(md5, md5_contents, md5_file);
criterion_group!(md5, md5_file);
// criterion_main!(md5);

fn main() {
    #[cfg(feature = "dhat-heap")]
    let _profiler = dhat::Profiler::new_heap();

    md5();
    Criterion::default().configure_from_args().final_summary();
}
