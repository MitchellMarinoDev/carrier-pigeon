use crate::connection::client::ClientConnection;
use crate::connection::server::ServerConnection;
use crate::transport::client_std_udp::UdpClientTransport;
use crate::transport::server_std_udp::UdpServerTransport;
use crate::{Guarantees, MsgTable, MsgTableBuilder};
use serde::{Deserialize, Serialize};
use std::io::ErrorKind;
use std::process::Command;
use std::thread::sleep;
use std::time::Duration;
use log::LevelFilter;
use simple_logger::SimpleLogger;

#[test]
#[ignore]
fn test_reliability() {
    SimpleLogger::new()
        .with_level(LevelFilter::Trace)
        .init()
        .unwrap();

    let msg_table = get_msg_table();

    let mut server_connection: ServerConnection<UdpServerTransport> =
        ServerConnection::new(msg_table.clone(), "127.0.0.1:7777".parse().unwrap()).unwrap();
    let mut client_connection: ClientConnection<UdpClientTransport> = ClientConnection::new(
        msg_table,
        "127.0.0.1:7776".parse().unwrap(),
        "127.0.0.1:7777".parse().unwrap(),
    )
    .unwrap();
    client_connection.send(&Connection).unwrap();

    sleep(Duration::from_millis(10));
    // recv_from needs to be called in order for the connection to read the client's message.
    // Since the message is a connection type message, it will not be returned from the function.
    assert_eq!(
        server_connection.recv_from().unwrap_err().kind(),
        ErrorKind::WouldBlock
    );
    let handled = server_connection
        .handle_pending(|_cid, _addr, _msg: Connection| (true, Response::Accepted))
        .unwrap();
    assert_eq!(handled, 1);

    // simulate bad network conditions
    Command::new("bash")
        .arg("-c")
        .arg("sudo tc qdisc add dev lo root netem delay 10ms corrupt 5 duplicate 5 loss random 5 reorder 5")
        .output()
        .expect("failed to run `tc` to emulate an unstable network on the `lo` adapter");

    let msg = ReliableMsg {
        msg: "This is the message that is sent.".to_owned(),
    };
    let mut results = vec![];

    // send 10 bursts of 10 messages.
    for _ in 0..10 {
        // make sure that the client is receiving the server's acks
        let _ = client_connection.recv();
        client_connection.resend_reliable();
        for _ in 0..10 {
            client_connection.send(&msg).expect("failed to send");
        }
        sleep(Duration::from_millis(150));
        loop {
            match server_connection.recv_from() {
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                Err(err) => panic!("unexpected error: {}", err),
                Ok((_cid, header, msg)) => {
                    // TODO: This will also not be necessary once ignore Connection/Response msgs
                    //       after connected.
                    if header.m_type == 5 {
                        results.push(msg);
                    }
                }
            }
        }
        // send ack messages for the reliability system.
        server_connection.send_ack_msgs();
    }

    println!("All messages sent at least once. Looping 10 more times for reliability");
    // do some more receives to get the stragglers
    for _ in 0..10 {
        // make sure that the client is receiving the server's acks
        let _ = client_connection.recv();
        client_connection.resend_reliable();
        sleep(Duration::from_millis(150));
        loop {
            match server_connection.recv_from() {
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                Err(err) => panic!("unexpected error: {}", err),
                Ok((_cid, header, msg)) => {
                    // TODO: This will also not be necessary once ignore Connection/Response msgs
                    //       after connected.
                    if header.m_type == 5 {
                        results.push(msg);
                    }
                }
            }
        }
        // send ack messages for the reliability system.
        server_connection.send_ack_msgs();
    }
    // remove the simulated conditions
    Command::new("bash")
        .arg("-c")
        .arg("sudo tc qdisc del dev lo root netem")
        .output()
        .expect("failed to run `tc` to remove the emulated network conditions on the `lo` adapter");

    for v in results.iter() {
        println!("{:?}", v);
    }

    // ensure all messages arrive uncorrupted
    for v in results.iter() {
        assert_eq!(
            v.downcast_ref::<ReliableMsg>().unwrap(),
            &msg,
            "message is not intact"
        )
    }

    // ensure all messages arrive
    assert_eq!(results.len(), 10 * 10, "not all messages arrived");
}

#[derive(Clone, Eq, PartialEq, Serialize, Deserialize, Debug)]
/// A reliable test message.
pub struct ReliableMsg {
    pub msg: String,
}
impl ReliableMsg {
    pub fn new(msg: impl ToString) -> Self {
        ReliableMsg { msg: msg.to_string() }
    }
}

#[derive(Clone, Eq, PartialEq, Serialize, Deserialize, Debug)]
/// An unreliable test message.
pub struct UnreliableMsg {
    pub msg: String,
}
impl UnreliableMsg {
    pub fn new<A: Into<String>>(msg: A) -> Self {
        UnreliableMsg { msg: msg.into() }
    }
}

#[derive(Clone, Eq, PartialEq, Serialize, Deserialize, Debug)]
/// A test connection message.
pub struct Connection;

#[derive(Clone, Eq, PartialEq, Serialize, Deserialize, Debug)]
/// A test disconnection message.
pub struct Disconnect;

#[derive(Clone, Eq, PartialEq, Serialize, Deserialize, Debug)]
/// A test response message.
pub enum Response {
    Accepted,
    Rejected,
}

/// Builds a table with all these test messages and returns it's parts.
pub fn get_msg_table() -> MsgTable {
    let mut builder = MsgTableBuilder::new();
    builder
        .register_ordered::<ReliableMsg>(Guarantees::ReliableOrdered)
        .unwrap();
    builder
        .register_ordered::<UnreliableMsg>(Guarantees::Unreliable)
        .unwrap();
    builder.build::<Connection, Response, Disconnect>().unwrap()
}
