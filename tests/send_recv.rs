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
    SimpleLogger::new()
        .with_level(log::LevelFilter::Trace)
        .init()
        .unwrap();

    // CLIENT TO SERVER
    let (mut client, mut server) = create_client_server_pair();
    info!("connection made");

    // Send 10 reliable messages.
    for i in 0..10 {
        client
            .send(&ReliableMsg::new(format!("Test Reliable Msg {}", i)))
            .unwrap();
    }

    // Send 10 unreliable messages.
    for i in 0..10 {
        client
            .send(&UnreliableMsg::new(format!("Test Unreliable Msg {}", i)))
            .unwrap();
    }

    info!("messages sent");
    // Give the client enough time to send the messages.
    std::thread::sleep(Duration::from_millis(100));

    // since we are going through localhost, we are going to assume all reliable and unreliable messages make it.
    server.tick();
    assert_eq!(server.recv::<UnreliableMsg>().count(), 10);
    assert_eq!(server.recv::<ReliableMsg>().count(), 10);
    info!("messages received. Verifying...");

    let reliable_msgs: Vec<_> = server.recv::<ReliableMsg>().collect();
    assert_eq!(reliable_msgs.len(), 10); // Make sure all 10 reliable messages went through.

    for (i, msg) in reliable_msgs.into_iter().enumerate() {
        // verify message content
        assert_eq!(msg.msg, format!("Test Reliable Msg {}", i));
    }

    let unreliable_msgs: Vec<_> = server.recv::<UnreliableMsg>().map(|m| m.m).collect();
    assert_eq!(unreliable_msgs.len(), 10); // Make sure all 10 udp messages went through.

    // Udp is unreliable unordered. Assert that all messages arrive.
    for i in 0..10 {
        let msg = format!("Test Unreliable Msg {}", i);
        assert!(unreliable_msgs.contains(&&UnreliableMsg::new(msg)));
    }
    info!("Client to Server success!");

    // SERVER TO CLIENT
    println!("Cids: {:?}", server.cids().collect::<Vec<_>>());

    // Send 10 tcp messages.
    for i in 0..10 {
        server
            .send_to(1, &ReliableMsg::new(format!("Test Reliable message {}", i)))
            .unwrap();
    }

    // Send 10 udp messages.
    for i in 0..10 {
        server
            .send_to(
                1,
                &UnreliableMsg::new(format!("Test Unreliable message {}", i)),
            )
            .unwrap();
    }

    // Give the client enough time to send the messages.
    std::thread::sleep(Duration::from_millis(100));

    assert_eq!(client.get_msgs(), 20);

    let reliable_msgs: Vec<_> = client.recv::<ReliableMsg>().collect();
    assert_eq!(reliable_msgs.len(), 10); // Make sure all 10 tcp messages went through.

    for (i, p) in reliable_msgs.into_iter().enumerate() {
        // TCP is reliable ordered. Assert that all messages arrive in the correct order.
        assert_eq!(p.msg, format!("Test Reliable message {}", i));
    }

    let unreliable_msgs: Vec<_> = client.recv::<UnreliableMsg>().map(|msg| msg.m).collect();
    assert_eq!(unreliable_msgs.len(), 10); // Make sure all 10 udp messages went through.

    // Udp is unreliable unordered. Assert that all messages arrive.
    for i in 0..10 {
        let msg = format!("Test Unreliable message {}", i);
        assert!(unreliable_msgs.contains(&&UnreliableMsg::new(msg)));
    }
}
