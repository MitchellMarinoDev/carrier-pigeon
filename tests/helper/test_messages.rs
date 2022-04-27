#![allow(unused)]
//! Test messages for use in tests.

use carrier_pigeon::{MsgTable, MsgTableParts, Transport};
use serde::{Deserialize, Serialize};

#[derive(Clone, Eq, PartialEq, Serialize, Deserialize, Debug)]
/// A test message for TCP.
pub struct TcpMsg {
    pub msg: String,
}
impl TcpMsg {
    pub fn new<A: Into<String>>(msg: A) -> Self {
        TcpMsg { msg: msg.into() }
    }
}

#[derive(Clone, Eq, PartialEq, Serialize, Deserialize, Debug)]
/// A test message for UDP.
pub struct UdpMsg {
    pub msg: String,
}
impl UdpMsg {
    pub fn new<A: Into<String>>(msg: A) -> Self {
        UdpMsg { msg: msg.into() }
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
pub fn get_table_parts() -> MsgTableParts {
    let mut table = MsgTable::new();
    table.register::<TcpMsg>(Transport::TCP).unwrap();
    table.register::<UdpMsg>(Transport::UDP).unwrap();
    table.build::<Connection, Response, Disconnect>().unwrap()
}
