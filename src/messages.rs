//! A module for internal messages that are used by carrier pigeon.
//! This includes [`AckMsg`] and [`PingMsg`].

use std::fmt::Debug;
use crate::net::AckNum;
use serde::{Deserialize, Serialize};
use std::io;
use std::io::ErrorKind;
use downcast_rs::{Downcast, impl_downcast};

pub trait NetMsg: Downcast + Send + Sync + Debug {}
impl<T: Downcast + Send + Sync + Debug> NetMsg for T {}
impl_downcast!(NetMsg);

/// An enum representing the possible responses to a connection request.
///
/// Generic types `A` and `R` allow you to give more information
/// upon being accepted or rejected respectively.
/// This could be server info or a reason for rejecting.
#[derive(Serialize, Deserialize, Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum Response<A: NetMsg, R: NetMsg> {
    /// The connection request is accepted.
    Accepted(A),
    /// The connection request is rejected.
    Rejected(R),
}

/// A packet for acknowledging all received messages in the window.
#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) struct AckMsg {
    /// The offset of the acknowledgments.
    ack_offset: AckNum,
    /// The bitfields for the succeeding AckNums.
    bitfields: Vec<u32>,
}

impl AckMsg {
    /// Creates a new [`AckMsg`].
    pub(crate) fn new(ack_offset: AckNum, bitfields: Vec<u32>) -> Self {
        AckMsg {
            ack_offset,
            bitfields,
        }
    }
}

#[derive(Serialize, Deserialize, Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) enum PingType {
    /// A request.
    Req,
    /// A response.
    Res,
}

/// A type for estimating the RTT of a connection.
#[derive(Serialize, Deserialize, Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) struct PingMsg {
    /// The type of ping.
    pub ping_type: PingType,
    /// The ping number identifier.
    pub ping_num: u32,
}

impl PingMsg {
    /// Deserializes the ping message using bincode.
    pub(crate) fn deser(bytes: &[u8]) -> io::Result<Self> {
        bincode::deserialize(bytes).map_err(|err| {
            io::Error::new(
                ErrorKind::InvalidData,
                format!("deserialization error: {}", err),
            )
        })
    }

    /// Serializes the ping message using bincode.
    pub(crate) fn ser(&self, buf: &mut Vec<u8>) -> io::Result<()> {
        bincode::serialize_into(buf, self).map_err(|err| {
            io::Error::new(
                ErrorKind::InvalidData,
                format!("serialization error: {}", err),
            )
        })
    }

    /// Gets the corresponding response message type.
    pub(crate) fn response(&self) -> Self {
        PingMsg {
            ping_type: PingType::Res,
            ping_num: self.ping_num,
        }
    }
}
