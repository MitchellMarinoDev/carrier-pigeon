//! Benchmarks for downcasting.

use criterion::{criterion_group, criterion_main, Bencher, Criterion};
use std::any::Any;

fn criterion_benchmark(c: &mut Criterion) {
    c.bench_function("downcast_unwrap", downcast_unwrap);
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);

fn downcast_unwrap(b: &mut Bencher) {
    let typed: (i32, &str, f64) = (1, "Hello", std::f64::consts::PI);
    b.iter(|| {
        let any = Box::new(typed) as Box<dyn Any>;
        let downcast = any.downcast::<(i32, &str, f64)>().unwrap();
        assert_eq!(*downcast, typed);
    });
}
