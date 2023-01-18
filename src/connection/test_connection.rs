use crate::connection::client::ClientConnection;
use crate::{Guarantees, MsgTable, MsgTableBuilder};
use crate::connection::server::ServerConnection;
use serde::{Serialize, Deserialize};

#[test]
fn test_reliablility() {
    let msg_table = get_msg_table();

    let server_connection = ServerConnection::new(msg_table.clone(), "127.0.0.1:7777").unwrap();
    let connection = ClientConnection::new(msg_table, "127.0.0.1:7776", "127.0.0.1:7777").unwrap();

    // TODO: test reliability.

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
pub struct Connection {
    pub usr: String,
}
impl Connection {
    pub fn new<A: Into<String>>(usr: A) -> Self {
        Connection { usr: usr.into() }
    }
}

#[derive(Clone, Eq, PartialEq, Serialize, Deserialize, Debug)]
/// A test disconnection message.
pub struct Disconnect {
    pub reason: String,
}
impl Disconnect {
    pub fn new<A: Into<String>>(reason: A) -> Self {
        Disconnect {
            reason: reason.into(),
        }
    }
}

#[derive(Clone, Eq, PartialEq, Serialize, Deserialize, Debug)]
/// A test response message.
pub enum Response {
    Accepted,
    Rejected(String),
}
impl Response {
    pub fn rejected<A: Into<String>>(reason: A) -> Self {
        Response::Rejected(reason.into())
    }
    pub fn accepted() -> Self {
        Response::Accepted
    }
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

