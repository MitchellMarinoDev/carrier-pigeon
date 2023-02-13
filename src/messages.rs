//! A module for internal messages that are used by carrier pigeon.
//! This includes [`AckMsg`] and [`PingMsg`].

use crate::net::AckNum;
use downcast_rs::{impl_downcast, Downcast};
use serde::{Deserialize, Serialize};
use std::fmt::Debug;

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
    pub ack_offset: AckNum,
    /// The bitfields for the succeeding AckNums.
    pub bitfields: Vec<u32>,
    /// Extra [`AckNum`]s that didnt fit in the bitfield.
    pub residuals: Vec<AckNum>,
}

impl AckMsg {
    /// Creates a new [`AckMsg`].
    pub(crate) fn new(ack_offset: AckNum, bitfields: Vec<u32>, residuals: Vec<AckNum>) -> Self {
        AckMsg {
            ack_offset,
            bitfields,
            residuals,
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
    /// Gets the corresponding response message type.
    pub(crate) fn response(&self) -> Self {
        PingMsg {
            ping_type: PingType::Res,
            ping_num: self.ping_num,
        }
    }
}
