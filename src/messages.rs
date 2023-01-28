//! A module for internal messages that are used by carrier pigeon.
//! This includes [`AckMsg`] and [`PingMsg`].

use std::io;
use std::io::ErrorKind;
use serde::{Serialize, Deserialize};
use crate::net::AckNum;

/// A packet for acknowledging all received messages in the window.
#[derive(Serialize, Deserialize)]
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) struct AckMsg {
    /// The offset of the acknowledgments.
    ack_offset: AckNum,
    /// The bitfields for the succeeding AckNums.
    bitfields: Vec<u32>,
}

impl AckMsg {
    /// Creates a new [`AckMsg`].
    pub(crate) fn new(ack_offset: AckNum, bitfields: Vec<u32>) -> Self {
        AckMsg { ack_offset, bitfields }
    }
}

/// A type for estimating the RTT of a connection.
#[derive(Serialize, Deserialize)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) enum PingMsg {
    /// A ping request.
    Req(u32),
    /// A ping response.
    Res(u32),
}

impl PingMsg {
    /// Deserializes the ping message using bincode.
    pub(crate) fn deser(bytes: &[u8]) -> io::Result<Self> {
        bincode::deserialize(bytes).map_err(|err| io::Error::new(ErrorKind::InvalidData, format!("deserialization error: {}", err)))
    }

    /// Serializes the ping message using bincode.
    pub(crate) fn ser(&self, buf: &mut Vec<u8>) -> io::Result<()> {
        bincode::serialize_into(buf, self).map_err(|err| io::Error::new(ErrorKind::InvalidData, format!("serialization error: {}", err)))
    }

    /// Gets the corresponding response message type.
    pub(crate) fn response(&self) -> Option<Self> {
        match self {
            PingMsg::Req(ping_num) => Some(PingMsg::Res(*ping_num)),
            PingMsg::Res(_) => None,
        }
    }
}
