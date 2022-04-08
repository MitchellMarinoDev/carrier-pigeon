//! Simple send/receive tests.
use std::time::Duration;
use simple_logger::SimpleLogger;
use crate::helper::create_client_server_pair;
use crate::helper::test_packets::{TcpPacket, UdpPacket};

mod helper;

#[test]
fn send_recv() {
    // Create a simple logger
    let _ = SimpleLogger::new()
        .with_level(log::LevelFilter::Debug)
        .init();

    // CLIENT TO SERVER
    let (mut client, mut server) = create_client_server_pair();

    // Send 10 tcp packets.
    for i in 0..10 {
        client.send(&TcpPacket::new(format!("Test TCP Packet {}", i))).unwrap();
    }

    // Send 10 udp packets.
    for i in 0..10 {
        client.send(&UdpPacket::new(format!("Test UDP Packet {}", i))).unwrap();
    }

    // Give the client enough time to send the packets.
    std::thread::sleep(Duration::from_millis(100));

    assert_eq!(server.recv_msgs(), 20);

    let tcp_packets: Vec<_> = server.recv::<TcpPacket>().unwrap().collect();
    assert_eq!(tcp_packets.len(), 10); // Make sure all 10 tcp packets went through.

    for (i, p) in tcp_packets.into_iter().enumerate() {
        // TCP is reliable ordered. Assert that all packets arrive in the correct order.
        assert_eq!(p.1.msg, format!("Test TCP Packet {}", i));
    }

    // Despite UDP being unreliable, we are sending the packets through localhost
    // so none should get lost.
    let udp_packets: Vec<_> = server.recv::<UdpPacket>().unwrap().map(|m| m.1).collect();
    assert_eq!(udp_packets.len(), 10); // Make sure all 10 udp packets went through.

    // Udp is unreliable unordered. Assert that all packets arrive.
    for i in 0..10 {
        let msg = format!("Test UDP Packet {}", i);
        assert!(udp_packets.contains(&&UdpPacket::new(msg)));
    }


    // SERVER TO CLIENT
    println!("Cids: {:?}", server.cids().collect::<Vec<_>>());

    // Send 10 tcp packets.
    for i in 0..10 {
        server.send_to(1, &TcpPacket::new(format!("Test TCP Packet {}", i))).unwrap();
    }

    // Send 10 udp packets.
    for i in 0..10 {
        server.send_to(1, &UdpPacket::new(format!("Test UDP Packet {}", i))).unwrap();
    }

    // Give the client enough time to send the packets.
    std::thread::sleep(Duration::from_millis(100));

    assert_eq!(client.recv_msgs(), 20);

    let tcp_packets: Vec<_> = client.recv::<TcpPacket>().unwrap().collect();
    assert_eq!(tcp_packets.len(), 10); // Make sure all 10 tcp packets went through.

    for (i, p) in tcp_packets.into_iter().enumerate() {
        // TCP is reliable ordered. Assert that all packets arrive in the correct order.
        assert_eq!(p.msg, format!("Test TCP Packet {}", i));
    }

    // Despite UDP being unreliable, we are sending the packets through localhost
    // so none should get lost.
    let udp_packets: Vec<_> = client.recv::<UdpPacket>().unwrap().collect();
    assert_eq!(udp_packets.len(), 10); // Make sure all 10 udp packets went through.

    // Udp is unreliable unordered. Assert that all packets arrive.
    for i in 0..10 {
        let msg = format!("Test UDP Packet {}", i);
        assert!(udp_packets.contains(&&UdpPacket::new(msg)));
    }
}