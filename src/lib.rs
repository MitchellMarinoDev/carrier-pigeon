//! # carrier-pigeon
//! A rusty networking library for games.
//!
//! A simple networking library that handles all the serialization, sending, receiving, and
//! deserialization. This way you can worry about what to send, and pigeon will worry about how
//! to send it.
//!
//! ### Add carrier-pigeon to your `Cargo.toml`:
//!
//! `carrier-pigeon = "0.3.0"`
//!
//! ## Examples
//!
//! Complete examples are provided in the
//! [`examples/` directory](https://github.com/MitchellMarinoDev/carrier-pigeon/blob/main/examples)
//! on the GitHub repo.

pub mod net;

mod client;
mod connection;
mod message_table;
mod server;
mod time;
mod transport;
mod util;

pub use client::{Client, OptionPendingClient, PendingClient};
pub use message_table::{MsgRegError, MsgTable, MsgTableBuilder};
pub use net::{CId, MId, Transport};
pub use server::Server;
