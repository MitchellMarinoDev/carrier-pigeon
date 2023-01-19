use crate::connection::{NonAckedMsgs, SavedMsg};
use crate::net::{MsgHeader, HEADER_SIZE};
use crate::transport::ClientTransport;
use crate::{MId, MsgTable};
use log::{trace, warn};
use std::any::{Any, TypeId};
use std::io;
use std::io::{Error, ErrorKind};
use std::net::SocketAddr;
use std::sync::Arc;

/// A wrapper around the the [`ClientTransport`] that adds the reliability and ordering.
pub struct ClientConnection<T: ClientTransport> {
    msg_table: MsgTable,
    transport: T,

    msg_counter: Vec<u32>,
    non_acked: Vec<NonAckedMsgs>,

    missing_msg: Vec<Vec<u32>>,
}

impl<T: ClientTransport> ClientConnection<T> {
    pub fn new(msg_table: MsgTable, local: SocketAddr, peer: SocketAddr) -> io::Result<Self> {
        let transport = T::new(local, peer)?;
        trace!(
            "UdpClientTransport connected from {} to {}",
            transport
                .local_addr()
                .map(|addr| addr.to_string())
                .unwrap_or("UNKNOWN".to_owned()),
            transport
                .peer_addr()
                .map(|addr| addr.to_string())
                .unwrap_or("UNKNOWN".to_owned()),
        );

        let len = msg_table.mid_count();
        let msg_counter = vec![0; len];
        let non_acked = (0..len).map(|_| NonAckedMsgs::new()).collect();
        let missing_msg = (0..len).map(|_| Vec::with_capacity(0)).collect();
        Ok(Self {
            msg_table,
            transport,
            msg_counter,
            non_acked,
            missing_msg,
        })
    }

    pub fn send<M: Any + Send + Sync>(&mut self, msg: &M) -> io::Result<()> {
        // verify type is valid
        self.msg_table.check_type::<M>()?;
        let tid = TypeId::of::<M>();

        // create the message header
        let mid = self.msg_table.tid_map[&tid];
        let ack_num = self.msg_counter[mid];
        self.msg_counter[mid] += 1;
        let msg_header = MsgHeader::new(mid, ack_num);

        // build the payload using the header and the message
        let mut payload = Vec::new();
        payload.extend(msg_header.to_be_bytes());

        let ser_fn = self.msg_table.ser[mid];
        ser_fn(msg, &mut payload).map_err(|e| Error::new(ErrorKind::InvalidData, e))?;
        let payload = Arc::new(payload);

        // send the payload based on the guarantees
        let guarantees = self.msg_table.guarantees[mid];
        if guarantees.reliable() {
            self.send_reliable(mid, ack_num, payload)
        } else {
            self.send_unreliable(mid, payload)
        }
    }

    fn send_reliable(&mut self, mid: MId, ack_num: u32, payload: Arc<Vec<u8>>) -> io::Result<()> {
        self.transport.send(mid, payload.clone())?;
        // add the payload to the list of non-acked messages
        self.non_acked
            .get_mut(mid)
            .expect("mid should exist")
            .insert(ack_num, SavedMsg::new(payload));
        Ok(())
    }

    fn send_unreliable(&self, mid: MId, payload: Arc<Vec<u8>>) -> io::Result<()> {
        self.transport.send(mid, payload)
    }

    pub fn recv(&mut self) -> io::Result<(MsgHeader, Box<dyn Any + Send + Sync>)> {
        self.recv_shared(false)
    }

    pub fn recv_blocking(&mut self) -> io::Result<(MsgHeader, Box<dyn Any + Send + Sync>)> {
        self.recv_shared(true)
    }

    fn recv_shared(
        &mut self,
        blocking: bool,
    ) -> io::Result<(MsgHeader, Box<dyn Any + Send + Sync>)> {
        loop {
            let buf = if blocking {
                self.transport.recv_blocking()
            } else {
                self.transport.recv()
            }?;

            let n = buf.len();
            if n < HEADER_SIZE {
                warn!(
                    "Client: Received a packet of length {} which is not big enough \
                    to be a carrier pigeon message. Discarding",
                    n
                );
                continue;
            }
            let header = MsgHeader::from_be_bytes(&buf[..HEADER_SIZE]);
            self.msg_table.check_mid(header.mid)?;

            trace!(
                "Client: received message with MId: {}, len: {}.",
                header.mid,
                n,
            );

            let msg = self.msg_table.deser[header.mid](&buf)?;
            // TODO: handle any reliability stuff here.

            return Ok((header, msg));
        }
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.transport.local_addr()
    }

    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        self.transport.peer_addr()
    }
}
