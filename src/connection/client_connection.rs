use crate::connection::ping_system::ClientPingSystem;
use crate::connection::reliable::ReliableSystem;
use crate::message_table::PING_M_TYPE;
use crate::messages::PingMsg;
use crate::net::{MsgHeader, HEADER_SIZE};
use crate::transport::ClientTransport;
use crate::MsgTable;
use log::{error, trace, warn};
use std::any::{Any, TypeId};
use std::collections::VecDeque;
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;

/// A wrapper around the the [`ClientTransport`] that adds the reliability and ordering.
pub(crate) struct ClientConnection<T: ClientTransport> {
    /// The [`MsgTable`] to use for sending and receiving messages.
    msg_table: MsgTable,
    /// The transport to use to send and receive the messages.
    transport: T,
    /// The system used to generate ping messages and estimate the RTT.
    ping_sys: ClientPingSystem,
    /// The [`ReliableSystem`] to add optional reliability to messages.
    reliable_sys: ReliableSystem<Arc<Vec<u8>>, Box<dyn Any + Send + Sync>>,
    /// A buffer for messages that are ready to be received.
    ready: VecDeque<(MsgHeader, Box<dyn Any + Send + Sync>)>,
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
            ping_sys: ClientPingSystem::new(),
            reliable_sys: ReliableSystem::new(msg_table),
            ready: VecDeque::new(),
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
        let mut payload = header.to_be_bytes().to_vec();

        let ser_fn = self.msg_table.ser[m_type];
        ser_fn(msg, &mut payload)?;
        let payload = Arc::new(payload);

        // send the payload based on the guarantees
        let guarantees = self.msg_table.guarantees[m_type];
        self.reliable_sys.save(header, guarantees, payload.clone());
        self.transport.send(m_type, payload)
    }

    /// Sends an [`AckMsg`] to acknowledge all received messages.
    pub fn send_ack_msg(&mut self) {
        let ack_msg = match self.reliable_sys.get_ack_msg() {
            None => return,
            Some(ack_msg) => ack_msg,
        };

        if let Err(err) = self.send(&ack_msg) {
            error!("Error sending AckMsg: {}", err);
        }
    }

    /// Sends a ping message to the server if necessary.
    pub fn send_ping(&mut self) {
        if let Some(msg) = self.ping_sys.get_ping_msg() {
            if let Err(err) = self.send(&msg) {
                error!("Failed to send ping message: {}", err);
            }
        }
    }

    /// Sends a ping messages to the clients if necessary.
    fn recv_ping(&mut self, ping_msg: PingMsg) {
        match ping_msg {
            PingMsg::Req(ping_num) => {
                if let Err(err) = self.send(&PingMsg::Res(ping_num)) {
                    warn!("Error in responding to a ping: {}", err);
                }
            }
            PingMsg::Res(_) => self.ping_sys.recv_ping_msg(ping_msg),
        }
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
            // if there is a message that is ready, return it
            if let Some(ready) = self.ready.pop_front() {
                return Ok(ready);
            }
            // otherwise, try to get a new one

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
                "Client: received message (MType: {}, len: {}, AckNum: {})",
                header.m_type,
                n,
                header.sender_ack_num,
            );

            if header.m_type == PING_M_TYPE {
                // handle ping type messages separately
                match PingMsg::deser(&buf[HEADER_SIZE..]) {
                    Err(err) => {
                        warn!("Client: failed to deserialize ping message: {}", err);
                    }
                    Ok(ping_msg) => {
                        self.recv_ping(ping_msg);
                    }
                }
            }

            let msg = self.msg_table.deser[header.m_type](&buf[HEADER_SIZE..])?;

            // handle reliability and ordering
            self.reliable_sys.push_received(header, msg);
            // get all messages from the reliable system and push them on the "ready" que.
            while let Some((header, msg)) = self.reliable_sys.get_received() {
                self.ready.push_back((header, msg));
            }
        }
    }

    /// Resends any messages that it needs to for the reliability system to work.
    pub fn resend_reliable(&mut self) {
        for (header, payload) in self.reliable_sys.get_resend() {
            if let Err(err) = self.transport.send(header.m_type, payload.clone()) {
                error!("Error resending msg {}: {}", header.sender_ack_num, err);
            }
        }
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.transport.local_addr()
    }

    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        self.transport.peer_addr()
    }

    pub fn rtt(&self) -> u32 {
        self.ping_sys.rtt()
    }
}
