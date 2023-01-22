use crate::connection::ack_system::AckSystem;
use crate::connection::{NonAckedMsgs, SavedMsg};
use crate::net::{AckNum, MsgHeader, OrderNum, HEADER_SIZE};
use crate::transport::ClientTransport;
use crate::{MType, MsgTable};
use log::{trace, warn};
use std::any::{Any, TypeId};
use std::io;
use std::io::{Error, ErrorKind};
use std::net::SocketAddr;
use std::sync::Arc;

/// A wrapper around the the [`ClientTransport`] that adds the reliability and ordering.
pub(crate) struct ClientConnection<T: ClientTransport> {
    msg_table: MsgTable,
    transport: T,

    /// A MType-independent counter for acknowledgment numbers.
    ack_num: AckNum,
    /// A per-MType counter used for ordering.
    order_num: Vec<OrderNum>,
    non_acked: Vec<NonAckedMsgs>,
    // TODO: replace with `Reliable`.
    remote_ack: AckSystem<Vec<u8>>,

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

        let len = msg_table.mtype_count();
        let msg_counter = vec![0; len];
        let non_acked = (0..len).map(|_| NonAckedMsgs::new()).collect();
        let missing_msg = (0..len).map(|_| Vec::with_capacity(0)).collect();
        Ok(Self {
            msg_table,
            transport,
            ack_num: 0,
            order_num: msg_counter,
            non_acked,
            remote_ack: AckSystem::new(),
            missing_msg,
        })
    }

    pub fn send<M: Any + Send + Sync>(&mut self, msg: &M) -> io::Result<()> {
        // verify type is valid
        self.msg_table.check_type::<M>()?;
        let tid = TypeId::of::<M>();

        // create the message header
        let m_type = self.msg_table.tid_map[&tid];
        let order_num = self.order_num[m_type];
        self.order_num[m_type] += 1;
        let sender_ack_num = self.ack_num;
        self.ack_num += 1;
        let (receiver_acking_num, ack_bits) = self.remote_ack.next_header();
        let msg_header = MsgHeader::new(
            m_type,
            order_num,
            sender_ack_num,
            receiver_acking_num,
            ack_bits,
        );

        // build the payload using the header and the message
        let mut payload = Vec::new();
        payload.extend(msg_header.to_be_bytes());

        let ser_fn = self.msg_table.ser[m_type];
        ser_fn(msg, &mut payload).map_err(|e| Error::new(ErrorKind::InvalidData, e))?;
        let payload = Arc::new(payload);

        // send the payload based on the guarantees
        let guarantees = self.msg_table.guarantees[m_type];
        if guarantees.reliable() {
            self.send_reliable(m_type, sender_ack_num, payload)
        } else {
            self.send_unreliable(m_type, payload)
        }
    }

    fn send_reliable(
        &mut self,
        m_type: MType,
        ack_num: AckNum,
        payload: Arc<Vec<u8>>,
    ) -> io::Result<()> {
        self.transport.send(m_type, payload.clone())?;
        // add the payload to the list of non-acked messages
        self.non_acked
            .get_mut(m_type)
            .expect("m_type should be valid")
            .insert(ack_num, SavedMsg::new(payload));
        Ok(())
    }

    fn send_unreliable(&self, m_type: MType, payload: Arc<Vec<u8>>) -> io::Result<()> {
        self.transport.send(m_type, payload)
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
            self.msg_table.check_m_type(header.m_type)?;

            trace!(
                "Client: received message with MType: {}, len: {}.",
                header.m_type,
                n,
            );

            let msg = self.msg_table.deser[header.m_type](&buf[HEADER_SIZE..])?;
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
