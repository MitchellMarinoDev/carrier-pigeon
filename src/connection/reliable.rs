use std::fmt::Debug;
use crate::connection::ack_system::AckSystem;
use crate::connection::ordering_system::OrderingSystem;
use crate::net::MsgHeader;
use crate::{Guarantees, MType, MsgTable};
use crate::messages::AckMsg;

/// A system that handles the reliablility and ordering of incoming messages based on their
/// [`Guarantees`].
///
/// Generic parameter `O` is for "OtherData", which is the data that should be stored along side
/// the header. This is so, if this is used by a client, this can store the payload.
/// If this is used by a server, this can store the payload and from address.
pub(crate) struct ReliableSystem<SD: Debug, RD: Debug> {
    msg_table: MsgTable,
    ack_sys: AckSystem<SD>,
    ordering_sys: OrderingSystem<RD>,
}

impl<SD: Debug, RD: Debug> ReliableSystem<SD, RD> {
    /// Creates a new [`Reliable`] wrapper around a transport.
    pub fn new(msg_table: MsgTable) -> Self {
        let m_table_count = msg_table.mtype_count();
        ReliableSystem {
            msg_table,
            ack_sys: AckSystem::new(),
            ordering_sys: OrderingSystem::new(m_table_count),
        }
    }

    pub fn push_received(&mut self, header: MsgHeader, receive_data: RD) {
        self.ack_sys.msg_received(header.sender_ack_num);
        self.ack_sys.mark_bitfield(header.receiver_acking_offset, header.ack_bits);

        let guarantees = self.msg_table.guarantees[header.m_type];
        self.ordering_sys.push(header, guarantees, receive_data);
    }

    /// Gets an [`AckMsg`] for acknowledging all received messages in the window.
    pub fn get_ack_msg(&mut self) -> AckMsg {
        let (offset, flags) = self.ack_sys.ack_msg_info();
        AckMsg::new(offset, flags)
    }

    pub fn get_received(&mut self) -> Option<(MsgHeader, RD)> {
        self.ordering_sys.next()
    }

    pub fn get_send_header(&mut self, m_type: MType) -> MsgHeader {
        let (offset, bitfield) = self.ack_sys.next_header();
        MsgHeader {
            m_type,
            order_num: self.ordering_sys.next_outgoing(m_type),
            sender_ack_num: self.ack_sys.outgoing_ack_num(),
            receiver_acking_offset: offset,
            ack_bits: bitfield,
        }
    }

    /// Saves a message, if it is reliable, so that it can be resent if it is lost in transit.
    ///
    /// This also
    pub fn save(&mut self, header: MsgHeader, guarantees: Guarantees, send_data: SD) {
        self.ack_sys.save_msg(header, guarantees, send_data);
    }

    /// Gets messages that are due for a resend.
    pub fn get_resend(&mut self) -> impl Iterator<Item = (&MsgHeader, &SD)> {
        self.ack_sys.get_resend()
    }
}
