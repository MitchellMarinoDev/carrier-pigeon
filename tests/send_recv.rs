//! Simple send/receive tests.
use crate::helper::create_client_server_pair;
use crate::helper::test_messages::{ReliableMsg, UnreliableMsg};
use log::info;
use simple_logger::SimpleLogger;
use std::time::Duration;

mod helper;

#[test]
fn send_recv() {
    // Create a simple logger
    let _ = SimpleLogger::new()
        .with_level(log::LevelFilter::Trace)
        .init();

    // CLIENT TO SERVER
    let (mut client, mut server) = create_client_server_pair();
    info!("connection made");

    // Send 10 tcp messages.
    for i in 0..10 {
        client
            .send(&ReliableMsg::new(format!("Test TCP Msg {}", i)))
            .unwrap();
    }

    // Send 10 udp messages.
    for i in 0..10 {
        client
            .send(&UnreliableMsg::new(format!("Test UDP Msg {}", i)))
            .unwrap();
    }

    // Give the client enough time to send the messages.
    std::thread::sleep(Duration::from_millis(100));

    assert_eq!(server.get_msgs(), 20);

    let tcp_msgs: Vec<_> = server.recv::<ReliableMsg>().collect();
    assert_eq!(tcp_msgs.len(), 10); // Make sure all 10 tcp messages went through.

    for (i, msg) in tcp_msgs.into_iter().enumerate() {
        // TCP is reliable ordered. Assert that all messages arrive in the correct order.
        assert_eq!(msg.msg, format!("Test TCP Msg {}", i));
    }

    // Despite UDP being unreliable, we are sending the messages through localhost
    // so none should get lost.
    let udp_msgs: Vec<_> = server.recv::<UnreliableMsg>().map(|m| m.m).collect();
    assert_eq!(udp_msgs.len(), 10); // Make sure all 10 udp messages went through.

    // Udp is unreliable unordered. Assert that all messages arrive.
    for i in 0..10 {
        let msg = format!("Test UDP Msg {}", i);
        assert!(udp_msgs.contains(&&UnreliableMsg::new(msg)));
    }

    // SERVER TO CLIENT
    println!("Cids: {:?}", server.cids().collect::<Vec<_>>());

    // Send 10 tcp messages.
    for i in 0..10 {
        server
            .send_to(1, &ReliableMsg::new(format!("Test TCP message {}", i)))
            .unwrap();
    }

    // Send 10 udp messages.
    for i in 0..10 {
        server
            .send_to(1, &UnreliableMsg::new(format!("Test UDP message {}", i)))
            .unwrap();
    }

    // Give the client enough time to send the messages.
    std::thread::sleep(Duration::from_millis(100));

    assert_eq!(client.get_msgs(), 20);

    let tcp_msgs: Vec<_> = client.recv::<ReliableMsg>().collect();
    assert_eq!(tcp_msgs.len(), 10); // Make sure all 10 tcp messages went through.

    for (i, p) in tcp_msgs.into_iter().enumerate() {
        // TCP is reliable ordered. Assert that all messages arrive in the correct order.
        assert_eq!(p.msg, format!("Test TCP message {}", i));
    }

    // Despite UDP being unreliable, we are sending the messages through localhost
    // so none should get lost.
    let udp_msgs: Vec<_> = client.recv::<UnreliableMsg>().map(|msg| msg.m).collect();
    assert_eq!(udp_msgs.len(), 10); // Make sure all 10 udp messages went through.

    // Udp is unreliable unordered. Assert that all messages arrive.
    for i in 0..10 {
        let msg = format!("Test UDP message {}", i);
        assert!(udp_msgs.contains(&&UnreliableMsg::new(msg)));
    }
}
