use std::io;
use std::net::SocketAddr;
use hashbrown::HashMap;
use log::{debug, trace, warn};
use crate::{CId, Guarantees, MsgTable};
use crate::connection::ack_system::AckSystem;
use crate::connection::ordering_system::OrderingSystem;
use crate::connection::SavedMsg;
use crate::net::{AckNum, HEADER_SIZE, MsgHeader};
use crate::transport::Transport;

struct Reliable<T: Transport> {
    ack_counter: u16,
    msg_table: MsgTable,
    non_acked: HashMap<AckNum, SavedMsg>,
    ack_sys: AckSystem,
    ordering_sys: OrderingSystem,
    transport: T,
}

impl<T: Transport> Reliable<T> {
    pub fn recv(&mut self) -> io::Result<(SocketAddr, MsgHeader, Vec<u8>)> {
        loop {
            let (from, payload) = self.transport.recv()?;

            let n = payload.len();
            if n < HEADER_SIZE {
                debug!(
                    "Received a packet of length {} which is not big enough \
                    to be a carrier pigeon message. Discarding",
                    n
                );
                continue;
            }
            let header = MsgHeader::from_be_bytes(&payload[..HEADER_SIZE]);
            self.msg_table.check_m_type(header.m_type)?;

            trace!(
                "Received message with MType: {}, len: {}.",
                header.m_type,
                n,
            );

            return Ok((from, header, payload));
        }
        todo!()
    }

    pub fn send(&mut self, guarantees: Guarantees, addr: SocketAddr, payload: Vec<u8>) {
        todo!()
    }
}
