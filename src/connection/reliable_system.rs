use std::net::SocketAddr;
use std::sync::Arc;
use crate::connection::ack_system::AckSystem;
use crate::connection::ordering_system::OrderingSystem;
use crate::messages::{AckMsg, NetMsg};
use crate::net::{MsgHeader, OrderNum};
use crate::{Guarantees, MType, MsgTable, NetConfig};
use std::time::Instant;


// TODO: move the ping system into here.
/// A system that handles the reliability and ordering of incoming messages based on their
/// [`Guarantees`].
///
/// Generic parameter `SD` is for "Send Data". It should be the data that you send to the
/// transport other than the header.
///
/// Generic parameter `RD` is for "Receive Data". It should be the data that you get from the
/// transport other than the header.
///
/// Since these differ between client and server (server needs to keep track of a from address),
/// these need to be generic parameters.
pub(crate) struct ReliableSystem<RD, C: NetMsg, A: NetMsg, R: NetMsg, D: NetMsg> {
    config: NetConfig,
    msg_table: MsgTable<C, A, R, D>,
    last_ack_msg: Instant,
    ack_sys: AckSystem,
    ordering_sys: OrderingSystem<RD>,
}

impl<RD, C: NetMsg, A: NetMsg, R: NetMsg, D: NetMsg> ReliableSystem<RD, C, A, R, D> {
    /// Creates a new [`ReliableSystem`].
    pub fn new(msg_table: MsgTable<C, A, R, D>, config: NetConfig) -> Self {
        let m_table_count = msg_table.mtype_count();
        ReliableSystem {
            config,
            msg_table,
            last_ack_msg: Instant::now(),
            ack_sys: AckSystem::new(config),
            ordering_sys: OrderingSystem::new(m_table_count),
        }
    }

    /// Handles the reliability aspects of receiving a message other than ordering.
    ///
    /// For ordering, use [`push_received`](Self::push_received) to push the message into the
    /// ordering buffer.
    pub fn msg_received(&mut self, header: MsgHeader) {
        self.ack_sys.msg_received(header.message_ack_num);
        self.ack_sys
            .mark_bitfield(header.acking_offset, header.ack_bits);
    }

    /// Handles an incoming [`AckMsg`].
    pub fn recv_ack_msg(&mut self, ack_msg: AckMsg) {
        self.ack_sys.handle_ack_msg(ack_msg)
    }

    /// Pushes the received message into the ordering buffer to be ordered according to
    /// it's guarantees.
    pub fn push_received(&mut self, header: MsgHeader, receive_data: RD) {
        let guarantees = self.msg_table.guarantees[header.m_type];
        self.ordering_sys.push(header, guarantees, receive_data);
    }

    /// Gets an [`AckMsg`] even if one is not due to be sent yet.
    pub fn force_get_ack_msg(&mut self) -> AckMsg {
        self.ack_sys.ack_msg_info()
    }

    /// Gets an [`AckMsg`] for acknowledging all received messages in the window if one needs to be
    /// sent.
    pub fn get_ack_msg(&mut self) -> Option<AckMsg> {
        if self.last_ack_msg.elapsed() < self.config.ack_msg_interval {
            return None;
        }
        self.last_ack_msg = Instant::now();
        Some(self.ack_sys.ack_msg_info())
    }

    pub fn get_received(&mut self) -> Option<(MsgHeader, RD)> {
        self.ordering_sys.next()
    }

    pub fn get_send_header(&mut self, m_type: MType) -> MsgHeader {
        let (offset, bitfield) = self.ack_sys.next_bitfield();
        MsgHeader {
            m_type,
            order_num: self.ordering_sys.next_outgoing(m_type),
            message_ack_num: self.ack_sys.next_ack_num(),
            acking_offset: offset,
            ack_bits: bitfield,
        }
    }

    /// Saves a message, if it is reliable, so that it can be resent if it is lost in transit.
    pub fn save(&mut self, guarantees: Guarantees, header: MsgHeader, to_address: SocketAddr, payload: Arc<Vec<u8>>) {
        self.ack_sys.save_msg(guarantees, header, to_address, payload);
    }

    /// Gets messages that are due for a resend.
    pub fn get_resend(&mut self, rtt: u32) -> Vec<(MsgHeader, SocketAddr, Arc<Vec<u8>>)> {
        self.ack_sys.get_resend(rtt)
    }

    /// Gets the current order number for incoming messages.
    #[inline]
    pub fn current_incoming_order_num(&self, m_type: MType) -> OrderNum {
        self.ordering_sys.get_incoming(m_type)
    }
}
