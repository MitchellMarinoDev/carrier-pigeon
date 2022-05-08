#![feature(test)]
extern crate test;

use crate::helper::create_tcp_pair;
use std::io::{Read, Write};
use test::Bencher;

mod helper;

#[bench]
fn single_std_tcp_big(b: &mut Bencher) {
    let (mut s1, mut s2) = create_tcp_pair();

    let msg = ['A' as u8; 504];
    let mut buff = [0; 504];

    b.iter(|| {
        s1.write(&msg).unwrap();
        s2.read(&mut buff).unwrap();
        assert_eq!(buff, buff);
    })
}

#[bench]
fn single_std_tcp_small(b: &mut Bencher) {
    let (mut s1, mut s2) = create_tcp_pair();

    let msg = ['A' as u8; 10];
    let mut buff = [0; 10];

    b.iter(|| {
        s1.write(&msg).unwrap();
        s2.read(&mut buff).unwrap();
        assert_eq!(buff, buff);
    })
}

#[bench]
fn many_std_tcp_big(b: &mut Bencher) {
    let (mut s1, mut s2) = create_tcp_pair();

    let msg = ['A' as u8; 504];
    let mut buff = [0; 504];

    b.iter(|| {
        for _ in 0..100 {
            s1.write(&msg).unwrap();
        }

        for _ in 0..100 {
            s2.read(&mut buff).unwrap();
        }
        assert_eq!(msg, buff);
    })
}

#[bench]
fn many_std_tcp_small(b: &mut Bencher) {
    let (mut s1, mut s2) = create_tcp_pair();

    let msg = ['A' as u8; 10];
    let mut buff = [0; 10];

    b.iter(|| {
        for _ in 0..100 {
            s1.write(&msg).unwrap();
        }

        for _ in 0..100 {
            s2.read(&mut buff).unwrap();
        }
        assert_eq!(msg, buff);
    })
}
