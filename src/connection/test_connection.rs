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

#[test]
#[ignore]
fn test_reliability() {
    // println!("testing...");
    let msg_table = get_msg_table();

    let mut server_connection: ServerConnection<UdpServerTransport> =
        ServerConnection::new(msg_table.clone(), "127.0.0.1:7777").unwrap();
    let mut client_connection: ClientConnection<UdpClientTransport> =
        ClientConnection::new(msg_table, "127.0.0.1:7776", "127.0.0.1:7777").unwrap();
    client_connection.send(&Connection).unwrap();

    sleep(Duration::from_millis(10));
    // recv_from needs to be called in order for the connection to read the client's message.
    // Since the message is a connection type message, it will not be returned from the function.
    let _ = server_connection.recv_from();
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

    // send 10 bursts of 100 messages.
    for _ in 0..10 {
        for _ in 0..100 {
            client_connection.send(&msg).expect("failed to send");
        }
        // this delay will allow for about 8-9 resends
        sleep(Duration::from_millis(150));
        loop {
            let msg = server_connection.recv_from();
            if matches!(&msg, Err(e) if e.kind() == ErrorKind::WouldBlock) {
                break;
            }

            results.push(msg);
        }
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
            v.as_ref().unwrap().2.downcast_ref::<ReliableMsg>().unwrap(),
            &msg,
            "message is not intact"
        )
    }

    // ensure all messages arrive
    assert_eq!(results.len(), 10 * 100, "not all messages arrived");
}

#[derive(Clone, Eq, PartialEq, Serialize, Deserialize, Debug)]
/// A reliable test message.
pub struct ReliableMsg {
    pub msg: String,
}
impl ReliableMsg {
    pub fn new<A: Into<String>>(msg: A) -> Self {
        ReliableMsg { msg: msg.into() }
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
        .register_ordered::<ReliableMsg>(Guarantees::Reliable)
        .unwrap();
    builder
        .register_ordered::<UnreliableMsg>(Guarantees::Unreliable)
        .unwrap();
    builder.build::<Connection, Response, Disconnect>().unwrap()
}
