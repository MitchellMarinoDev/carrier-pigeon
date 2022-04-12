//! Benchmarks for (de)serialization.
#![feature(test)]
#![feature(bench_black_box)]

extern crate test;
mod helper;

use std::hint::black_box;
use test::Bencher;
use crate::helper::test_packets::UdpPacket;

#[bench]
fn ser(b: &mut Bencher) {
    let udp = black_box(UdpPacket::new("Short Message"));
    b.iter(|| {
        bincode::serialize(&udp)
    })
}

#[bench]
fn ser_size(b: &mut Bencher) {
    let udp = black_box(UdpPacket::new("Short Message"));
    b.iter(|| {
        black_box(bincode::serialized_size(&udp).unwrap());
    })
}

#[bench]
fn deser(b: &mut Bencher) {
    let udp = black_box(UdpPacket::new("Short Message"));
    let bytes = bincode::serialize(&udp).unwrap();
    b.iter(|| {
        let udp_deser = bincode::deserialize::<UdpPacket>(&bytes[..]).unwrap();
        assert_eq!(udp_deser, udp);
    })
}
