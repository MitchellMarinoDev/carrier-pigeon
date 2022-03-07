//! Code that is shared between the client and server examples.
//!
//! This is mostly just the Message type declarations.

use carrier_pigeon::NetMsg;
use serde::{Deserialize, Serialize};

#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
/// A packet for a message from a user
pub struct Msg {
    pub from: String,
    pub text: String,
}
impl NetMsg for Msg {}

#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
/// The connection packet.
pub struct Connection {
    pub user: String,
}
impl NetMsg for Connection {}

#[derive(Clone, Eq, PartialEq, Serialize, Deserialize, Debug)]
/// The disconnection packet.
pub struct Disconnect {
    pub reason: String,
}
impl NetMsg for Disconnect {}

#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
/// The response packet.
pub enum Response {
    Accepted,
    Rejected(String),
}
impl NetMsg for Response {}
