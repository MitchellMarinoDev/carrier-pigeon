use crate::helper::create_udp_pair;
use criterion::{criterion_group, criterion_main, Bencher, Criterion};

fn criterion_benchmark(c: &mut Criterion) {
    c.bench_function("single_std_udp_big", single_std_udp_big);
    c.bench_function("single_std_udp_small", single_std_udp_small);
    c.bench_function("many_std_udp_big", many_std_udp_big);
    c.bench_function("many_std_udp_small", many_std_udp_small);
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);

mod helper;

fn single_std_udp_big(b: &mut Bencher) {
    let (s1, s2) = create_udp_pair();

    let msg = ['A' as u8; 504];
    let mut buff = [0; 504];

    b.iter(|| {
        s1.send(&msg).unwrap();
        s2.recv(&mut buff).unwrap();
        assert_eq!(buff, buff);
    })
}

fn single_std_udp_small(b: &mut Bencher) {
    let (s1, s2) = create_udp_pair();

    let msg = ['A' as u8; 10];
    let mut buff = [0; 10];

    b.iter(|| {
        s1.send(&msg).unwrap();
        s2.recv(&mut buff).unwrap();
        assert_eq!(buff, buff);
    })
}

fn many_std_udp_big(b: &mut Bencher) {
    let (s1, s2) = create_udp_pair();

    let msg = ['A' as u8; 504];
    let mut buff = [0; 504];

    b.iter(|| {
        for _ in 0..100 {
            s1.send(&msg).unwrap();
        }

        for _ in 0..100 {
            s2.recv(&mut buff).unwrap();
        }
        assert_eq!(msg, buff);
    })
}

fn many_std_udp_small(b: &mut Bencher) {
    let (s1, s2) = create_udp_pair();

    let msg = ['A' as u8; 10];
    let mut buff = [0; 10];

    b.iter(|| {
        for _ in 0..100 {
            s1.send(&msg).unwrap();
        }

        for _ in 0..100 {
            s2.recv(&mut buff).unwrap();
        }
        assert_eq!(msg, buff);
    })
}
