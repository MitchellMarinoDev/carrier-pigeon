#![feature(test)]
extern crate test;
use test::Bencher;

use crate::helper::test_messages::ReliableMsg;

mod helper;

#[bench]
fn single_tcp_big(b: &mut Bencher) {
    let (mut client, mut server) = helper::create_client_server_pair();

    let string: String = vec!['A'; 504].into_iter().collect();
    let s_msg = ReliableMsg::new(string);

    b.iter(|| {
        server.clear_msgs();
        client.send(&s_msg).unwrap();
        while server.get_msgs() == 0 {}
        let msg = server.recv::<ReliableMsg>().next().unwrap();
        assert_eq!(msg.m, &s_msg);
    })
}

#[bench]
fn single_tcp_small(b: &mut Bencher) {
    let (mut client, mut server) = helper::create_client_server_pair();

    let string: String = vec!['A'; 10].into_iter().collect();
    let s_msg = ReliableMsg::new(string);

    b.iter(|| {
        server.clear_msgs();
        client.send(&s_msg).unwrap();
        while server.get_msgs() == 0 {}
        let msg = server.recv::<ReliableMsg>().next().unwrap();
        assert_eq!(msg.m, &s_msg);
    })
}

#[bench]
fn many_tcp_big(b: &mut Bencher) {
    let (mut client, mut server) = helper::create_client_server_pair();

    let string: String = vec!['A'; 504].into_iter().collect();
    let s_msg = ReliableMsg::new(string);

    b.iter(|| {
        server.clear_msgs();
        for _ in 0..100 {
            client.send(&s_msg).unwrap();
        }
        let mut n = 0;
        while n < 100 {
            n += server.get_msgs();
        }
        assert_eq!(server.recv::<ReliableMsg>().count(), 100);
    })
}

#[bench]
fn many_tcp_small(b: &mut Bencher) {
    let (mut client, mut server) = helper::create_client_server_pair();

    let string: String = vec!['A'; 10].into_iter().collect();
    let s_msg = ReliableMsg::new(string);

    b.iter(|| {
        server.clear_msgs();
        for _ in 0..100 {
            client.send(&s_msg).unwrap();
        }
        let mut n = 0;
        while n < 100 {
            n += server.get_msgs();
        }
        assert_eq!(server.recv::<ReliableMsg>().count(), 100);
    })
}
