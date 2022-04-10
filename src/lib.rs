//! # carrier-pigeon
//! A rusty networking library for games.
//!
//! A simple networking library that handles all the serialization, sending, receiving,
//! and deserialization. This way you can worry about what to send, and pigeon will worry
//! about how to send it.
//!
//! ## Examples
//!
//! Complete examples are provided in the
//! [`examples/` directory](https://github.com/MitchellMarinoDev/carrier-pigeon/tree/master/examples)
//! on the GitHub repo.

pub mod net;
pub mod tcp;
pub mod udp;

mod client;
mod header;
mod message_table;
mod server;

pub use client::Client;
pub use header::Header;
pub use message_table::{MsgRegError, MsgTable, MsgTableParts, SortedMsgTable};
pub use net::{CId, MId, NetError, Transport};
pub use server::Server;
