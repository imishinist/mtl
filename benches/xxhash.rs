use std::fs;

use criterion::{black_box, criterion_main, criterion_group, Criterion};

use mtl::hash;

fn xxh3_contents(c: &mut Criterion) {
    let contents = fs::read("/usr/share/dict/words").unwrap();
    c.bench_function("xxh3_contents", |b| {
        b.iter(|| hash::xxh3_contents(black_box(&contents)))
    });
}

fn xxh64_contents(c: &mut Criterion) {
    let contents = fs::read("/usr/share/dict/words").unwrap();
    c.bench_function("xxh64_contents", |b| {
        b.iter(|| hash::xxh64_contents(black_box(&contents)))
    });
}


criterion_group!(xxhash, xxh3_contents, xxh64_contents);
criterion_main!(xxhash);
