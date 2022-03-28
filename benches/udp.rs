#![feature(test)]
extern crate test;
use test::Bencher;

use crate::helper::test_packets::UdpPacket;

mod helper;

#[bench]
fn send_single_udp_big(b: &mut Bencher) {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let rt = runtime.handle();

    let (client, mut server) = helper::create_client_server_pair(rt.clone());

    let string: String = vec!['A'; 504].into_iter().collect();
    let msg = UdpPacket::new(string);

    b.iter(|| {
        server.clear_msgs();
        client.send(&msg).unwrap();
        while server.recv_msgs() == 0 {}
        let packet: &UdpPacket = server.recv().unwrap().next().unwrap().1;
        assert_eq!(packet, &msg);
    })
}

#[bench]
fn send_single_udp_small(b: &mut Bencher) {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let rt = runtime.handle();

    let (client, mut server) = helper::create_client_server_pair(rt.clone());

    let string: String = vec!['A'; 10].into_iter().collect();
    let msg = UdpPacket::new(string);

    b.iter(|| {
        server.clear_msgs();
        client.send(&msg).unwrap();
        while server.recv_msgs() == 0 {}
        let packet: &UdpPacket = server.recv().unwrap().next().unwrap().1;
        assert_eq!(packet, &msg);
    })
}

#[bench]
fn send_multiple_udp_big(b: &mut Bencher) {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let rt = runtime.handle();

    let (client, mut server) = helper::create_client_server_pair(rt.clone());

    let string: String = vec!['A'; 504].into_iter().collect();
    let msg = UdpPacket::new(string);

    b.iter(|| {
        server.clear_msgs();
        for _ in 0..100 {
            client.send(&msg).unwrap();
        }
        let mut n = 0;
        while n < 100 { n += server.recv_msgs(); }
        assert_eq!(server.recv::<UdpPacket>().unwrap().count(), 100);
    })
}

#[bench]
fn send_multiple_udp_small(b: &mut Bencher) {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let rt = runtime.handle();

    let (client, mut server) = helper::create_client_server_pair(rt.clone());

    let string: String = vec!['A'; 10].into_iter().collect();
    let msg = UdpPacket::new(string);

    b.iter(|| {
        server.clear_msgs();
        for _ in 0..100 {
            client.send(&msg).unwrap();
        }
        let mut n = 0;
        while n < 100 { n += server.recv_msgs(); }
        assert_eq!(server.recv::<UdpPacket>().unwrap().count(), 100);
    })
}