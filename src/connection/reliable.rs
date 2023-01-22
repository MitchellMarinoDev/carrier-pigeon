use crate::connection::ack_system::AckSystem;
use crate::connection::ordering_system::OrderingSystem;
use crate::connection::SavedMsg;
use crate::net::{AckNum, MsgHeader, HEADER_SIZE};
use crate::transport::Transport;
use crate::{Guarantees, MsgTable};
use hashbrown::HashMap;
use log::{debug, trace};
use std::io;
use std::net::SocketAddr;

/// A reliable and ordered wrapper around a [`Transport`].
struct Reliable<T: Transport> {
    ack_counter: u16,
    msg_table: MsgTable,
    non_acked: HashMap<AckNum, SavedMsg>,
    ack_sys: AckSystem,
    ordering_sys: OrderingSystem,
    transport: T,
}

impl<T: Transport> Reliable<T> {
    /// Creates a new [`Reliable`] wrapper around a transport.
    pub fn new(transport: T, msg_table: MsgTable) -> Self {
        let m_table_count = msg_table.mtype_count();
        Reliable {
            ack_counter: 0,
            msg_table,
            non_acked: HashMap::new(),
            ack_sys: AckSystem::new(),
            ordering_sys: OrderingSystem::new(m_table_count),
            transport,
        }
    }

    pub fn recv(&mut self) -> io::Result<(SocketAddr, MsgHeader, Vec<u8>)> {
        loop {
            // if there is something in the ordering buffer, return it now
            if let Some(data) = self.ordering_sys.next() {
                return Ok(data);
            }

            let (from, header, payload) = self.recv_next()?;

            self.ack_sys.mark(header.sender_ack_num);

            // handle ordering guarantees
            let guarantees = self.msg_table.guarantees[header.m_type];
            if guarantees.newest() {
                if self.ordering_sys.check_newest(&header) {
                    return Ok((from, header, payload));
                }
            } else if guarantees.ordered() {
                self.ordering_sys.push(from, header, payload);
            }
        }
    }

    /// Gets the next message (or error) from the transport.
    ///
    /// Does some error checking and logging.
    /// This validates the [`MType`].
    fn recv_next(&mut self) -> io::Result<(SocketAddr, MsgHeader, Vec<u8>)> {
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
            if let Err(err) = self.msg_table.check_m_type(header.m_type) {
                debug!("Received a message with an invalid MType: {}", err);
                continue;
            }

            trace!(
                "Received message with MType: {}, len: {}.",
                header.m_type,
                n,
            );

            return Ok((from, header, payload));
        }
    }

    pub fn send(&mut self, guarantees: Guarantees, addr: SocketAddr, payload: Vec<u8>) {
        todo!()
    }

    pub fn resend_reliable(&mut self) {
        todo!()
    }
}
