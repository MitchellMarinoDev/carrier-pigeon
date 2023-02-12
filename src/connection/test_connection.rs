use crate::messages::Response;
use crate::Server;
use crate::{Client, NetConfig, Guarantees, MsgTable, MsgTableBuilder};
use serde::{Deserialize, Serialize};
use std::process::Command;
use std::thread::sleep;
use std::time::Duration;

#[test]
#[cfg(target_os = "linux")]
fn test_reliability() {
    let _ = simple_logger::SimpleLogger::new()
        .with_level(log::LevelFilter::Trace)
        .init();

    let msg_table = get_msg_table();

    let server_addr = "127.0.0.1:7777".parse().unwrap();
    let client_addr = "127.0.0.1:0".parse().unwrap();

    let mut server: Server<Connection, Accepted, Rejected, Disconnect> =
        Server::new(NetConfig::default(), server_addr, msg_table.clone()).unwrap();
    let mut client: Client<Connection, Accepted, Rejected, Disconnect> =
        Client::new(NetConfig::default(), msg_table);

    client
        .connect(client_addr, server_addr, &Connection)
        .expect("Connection failed");

    sleep(Duration::from_millis(10));
    server.tick();
    let handled = server.handle_pending(|_cid, _addr, _msg: Connection| {
        Response::Accepted::<Accepted, Rejected>(Accepted)
    });
    assert_eq!(handled, 1);

    // simulate bad network conditions
    Command::new("bash")
        .arg("-c")
        .arg("sudo tc qdisc add dev lo root netem delay 10ms corrupt 5 duplicate 5 loss random 5 reorder 5")
        .output()
        .expect("failed to run `tc` to emulate an unstable network on the `lo` adapter");

    let msg = ReliableMsg::new("This is the message that is sent.");
    let mut results = vec![];

    // send 10 bursts of 10 messages.
    for _ in 0..10 {
        // make sure that the client is receiving the server's acks
        client.tick();
        for _ in 0..10 {
            client.send(&msg).expect("failed to send");
        }
        sleep(Duration::from_millis(150));
        server.tick();
        for msg in server.recv::<ReliableMsg>() {
            results.push(msg.m.clone());
        }
    }

    println!("All messages sent at least once. Looping 10 more times for reliability");
    // do some more receives to get the stragglers
    for _ in 0..10 {
        // make sure that the client is receiving the server's acks
        client.tick();
        sleep(Duration::from_millis(150));
        server.tick();
        for msg in server.recv::<ReliableMsg>() {
            results.push(msg.m.clone());
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
        assert_eq!(v, &msg, "message is not intact")
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
        ReliableMsg {
            msg: msg.to_string(),
        }
    }
}

#[derive(Clone, Eq, PartialEq, Serialize, Deserialize, Debug)]
/// An unreliable test message.
pub struct UnreliableMsg {
    pub msg: String,
}

#[derive(Clone, Eq, PartialEq, Serialize, Deserialize, Debug)]
/// A test connection message.
pub struct Connection;

#[derive(Clone, Eq, PartialEq, Serialize, Deserialize, Debug)]
/// A test disconnection message.
pub struct Disconnect;

/// The accepted message.
#[derive(Clone, Eq, PartialEq, Serialize, Deserialize, Debug)]
pub struct Accepted;
/// The rejected message.
#[derive(Clone, Eq, PartialEq, Serialize, Deserialize, Debug)]
pub struct Rejected;

/// Builds a table with all these test messages and returns it's parts.
pub fn get_msg_table() -> MsgTable<Connection, Accepted, Rejected, Disconnect> {
    let mut builder = MsgTableBuilder::new();
    // TODO: change this back to reliable
    // TODO: add test for dupe checking when reliable
    builder
        .register_ordered::<ReliableMsg>(Guarantees::ReliableOrdered)
        .unwrap();
    builder
        .register_ordered::<UnreliableMsg>(Guarantees::Unreliable)
        .unwrap();
    builder
        .build::<Connection, Accepted, Rejected, Disconnect>()
        .unwrap()
}
