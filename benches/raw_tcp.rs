//! Benches the raw [`TcpCon`] and [`UdpCon`] abstractions.

#![feature(test)]
extern crate test;

use test::Bencher;
use carrier_pigeon::tcp::TcpCon;
use crate::helper::create_tcp_pair;

mod helper;

#[bench]
fn single_raw_tcp_big(b: &mut Bencher) {
    let (s1, s2) = create_tcp_pair();
    let mut s1 = TcpCon::from_stream(s1);
    s1.set_nonblocking(true).unwrap();
    let mut s2 = TcpCon::from_stream(s2);

    let msg = ['A' as u8; 504];

    b.iter(|| {
        s1.send(0, &msg[..]).unwrap();
        let recv = s2.recv().unwrap();
        assert_eq!(recv, (0, &msg[..]));
    })
}

// #[bench]
// fn single_std_tcp_small(b: &mut Bencher) {
//     let (mut s1, mut s2) = create_tcp_pair();
//
//     let msg = ['A' as u8; 10];
//     let mut buff = [0; 10];
//
//     b.iter(|| {
//         s1.write(&msg).unwrap();
//         s2.read(&mut buff).unwrap();
//         assert_eq!(buff, buff);
//     })
// }
//
// #[bench]
// fn many_std_tcp_big(b: &mut Bencher) {
//     let (mut s1, mut s2) = create_tcp_pair();
//
//     let msg = ['A' as u8; 504];
//     let mut buff = [0; 504];
//
//     b.iter(|| {
//         for _ in 0..100 {
//             s1.write(&msg).unwrap();
//         }
//
//         for _ in 0..100 {
//             s2.read(&mut buff).unwrap();
//         }
//         assert_eq!(msg, buff);
//     })
// }
//
// #[bench]
// fn many_std_tcp_small(b: &mut Bencher) {
//     let (mut s1, mut s2) = create_tcp_pair();
//
//     let msg = ['A' as u8; 10];
//     let mut buff = [0; 10];
//
//     b.iter(|| {
//         for _ in 0..100 {
//             s1.write(&msg).unwrap();
//         }
//
//         for _ in 0..100 {
//             s2.read(&mut buff).unwrap();
//         }
//         assert_eq!(msg, buff);
//     })
// }
