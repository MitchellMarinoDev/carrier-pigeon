use crate::net::{MsgHeader, HEADER_SIZE};
use crate::transport::ClientTransport;
use crate::MsgTable;
use log::{trace, warn};
use std::any::{Any, TypeId};
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use crate::connection::reliable::ReliableSystem;

/// A wrapper around the the [`ClientTransport`] that adds the reliability and ordering.
pub(crate) struct ClientConnection<T: ClientTransport> {
    /// The [`MsgTable`] to use for sending and receiving messages.
    msg_table: MsgTable,
    /// The transport to use to send and receive the messages.
    transport: T,

    /// The [`ReliableSystem`] to add optional reliability to messages.
    reliable_sys: ReliableSystem<Arc<Vec<u8>>, Box<dyn Any + Send + Sync>>,
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

        Ok(Self {
            msg_table: msg_table.clone(),
            transport,
            reliable_sys: ReliableSystem::new(msg_table)
        })
    }

    pub fn send<M: Any + Send + Sync>(&mut self, msg: &M) -> io::Result<()> {
        // verify type is valid
        self.msg_table.check_type::<M>()?;
        let tid = TypeId::of::<M>();

        // create the message header
        let m_type = self.msg_table.tid_map[&tid];
        let header = self.reliable_sys.get_send_header(m_type);

        // build the payload using the header and the message
        let mut payload = Vec::new();
        payload.extend(header.to_be_bytes());

        let ser_fn = self.msg_table.ser[m_type];
        ser_fn(msg, &mut payload)?;
        let payload = Arc::new(payload);

        // send the payload based on the guarantees
        let guarantees = self.msg_table.guarantees[m_type];
        self.reliable_sys.save(header, guarantees, payload.clone());
        self.transport.send(m_type, payload)
    }

    pub fn recv(&mut self) -> io::Result<(MsgHeader, Box<dyn Any + Send + Sync>)> {
        self.recv_shared(false)
    }

    // TODO: move the blocking call into the client transport new function.
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
