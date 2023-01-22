#![feature(test)]
extern crate test;
use test::Bencher;

use crate::helper::test_messages::UnreliableMsg;

mod helper;

#[bench]
fn single_udp_big(b: &mut Bencher) {
    let (mut client, mut server) = helper::create_client_server_pair();

    let string: String = vec!['A'; 504].into_iter().collect();
    let s_msg = UnreliableMsg::new(string);

    b.iter(|| {
        server.clear_msgs();
        client.send(&s_msg).unwrap();
        while server.get_msgs() == 0 {}
        let msg = server.recv::<UnreliableMsg>().next().unwrap();
        assert_eq!(msg.m, &s_msg);
    })
}

#[bench]
fn single_udp_small(b: &mut Bencher) {
    let (mut client, mut server) = helper::create_client_server_pair();

    let string: String = vec!['A'; 10].into_iter().collect();
    let s_msg = UnreliableMsg::new(string);

    b.iter(|| {
        server.clear_msgs();
        client.send(&s_msg).unwrap();
        while server.get_msgs() == 0 {}
        let msg = server.recv::<UnreliableMsg>().next().unwrap();
        assert_eq!(msg.m, &s_msg);
    })
}

#[bench]
fn many_udp_big(b: &mut Bencher) {
    let (mut client, mut server) = helper::create_client_server_pair();

    let string: String = vec!['A'; 504].into_iter().collect();
    let msg = UnreliableMsg::new(string);

    b.iter(|| {
        server.clear_msgs();
        for _ in 0..100 {
            client.send(&msg).unwrap();
        }
        let mut n = 0;
        while n < 100 {
            n += server.get_msgs();
        }
        assert_eq!(server.recv::<UnreliableMsg>().count(), 100);
    })
}

#[bench]
fn many_udp_small(b: &mut Bencher) {
    let (mut client, mut server) = helper::create_client_server_pair();

    let string: String = vec!['A'; 10].into_iter().collect();
    let msg = UnreliableMsg::new(string);

    b.iter(|| {
        server.clear_msgs();
        for _ in 0..100 {
            client.send(&msg).unwrap();
        }
        let mut n = 0;
        while n < 100 {
            n += server.get_msgs();
        }
        assert_eq!(server.recv::<UnreliableMsg>().count(), 100);
    })
}
