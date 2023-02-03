//! Code that is shared between the client and server examples.
//!
//! This is mostly just the Message type declarations.

use serde::{Deserialize, Serialize};

pub const CLIENT_ADDR_LOCAL: &str = "127.0.0.1:7776";
pub const SERVER_ADDR_LOCAL: &str = "127.0.0.1:7777";

#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
/// A message from a user
pub struct Msg {
    pub from: String,
    pub text: String,
}

#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
/// The connection message.
pub struct Connection {
    pub user: String,
}

#[derive(Clone, Eq, PartialEq, Serialize, Deserialize, Debug)]
/// The disconnection message.
pub struct Disconnect {
    pub reason: String,
}

#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
/// The response message.
pub enum Response {
    Accepted,
    Rejected(String),
}
