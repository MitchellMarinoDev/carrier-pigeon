//! Benchmarks for (de)serialization.
use criterion::{criterion_group, criterion_main, Bencher, Criterion};

fn criterion_benchmark(c: &mut Criterion) {
    c.bench_function("ser", ser);
    c.bench_function("ser_size", ser_size);
    c.bench_function("deser", deser);
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);

mod helper;

use crate::helper::test_messages::UnreliableMsg;
use std::hint::black_box;

fn ser(b: &mut Bencher) {
    let udp = black_box(UnreliableMsg::new("Short Message"));
    b.iter(|| bincode::serialize(&udp))
}

fn ser_size(b: &mut Bencher) {
    let udp = black_box(UnreliableMsg::new("Short Message"));
    b.iter(|| {
        black_box(bincode::serialized_size(&udp).unwrap());
    })
}

fn deser(b: &mut Bencher) {
    let udp = black_box(UnreliableMsg::new("Short Message"));
    let bytes = bincode::serialize(&udp).unwrap();
    b.iter(|| {
        let udp_deser = bincode::deserialize::<UnreliableMsg>(&bytes[..]).unwrap();
        assert_eq!(udp_deser, udp);
    })
}
