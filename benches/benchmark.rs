use criterion::{criterion_group, criterion_main, Criterion};
use rand::Rng;

fn num_digits_1(n: usize) -> usize {
    let mut m = n;
    let mut i = 0;
    while m > 0 {
        m /= 10;
        i += 1;
    }
    i
}

fn digits_by_loop(c: &mut Criterion) {
    let mut rng = rand::thread_rng();
    c.bench_function("digits_by_loop", |b| b.iter(|| {
        num_digits_1(rng.gen())
    }));
}


fn digits_by_log10(c: &mut Criterion) {
    let mut rng = rand::thread_rng();
    c.bench_function("digits_by_log10", |b| b.iter(|| {
        let a = rng.gen::<usize>();
        f64::log10(a as f64) as usize
    }));
}


criterion_group!(benches, digits_by_loop, digits_by_log10);
criterion_main!(benches);
