#![feature(test)]
extern crate test;

use test::Bencher;

use crate::std_helper::create_udp_pair;

mod std_helper;

#[bench]
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

#[bench]
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

#[bench]
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

#[bench]
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
