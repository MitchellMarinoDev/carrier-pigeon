use crate::connection::ping_system::ClientPingSystem;
use crate::connection::reliable::ReliableSystem;
use crate::message_table::PING_M_TYPE;
use crate::messages::{NetMsg, PingMsg, PingType};
use crate::net::{MsgHeader, HEADER_SIZE};
use crate::transport::ClientTransport;
use crate::MsgTable;
use log::{error, trace, warn};
use std::any::TypeId;
use std::collections::VecDeque;
use std::io;
use std::io::{Error, ErrorKind};
use std::net::SocketAddr;
use std::sync::Arc;

/// [`ReliableSystem`] with the generic parameters set for a server.
type ClientReliableSystem<C: NetMsg, A: NetMsg, R: NetMsg, D: NetMsg> = ReliableSystem<Arc<Vec<u8>>, Box<dyn NetMsg>, C, A, R, D>;

/// A wrapper around the the [`ClientTransport`] that adds the reliability and ordering.
pub(crate) struct ClientConnection<T: ClientTransport, C: NetMsg, A: NetMsg, R: NetMsg, D: NetMsg> {
    /// The [`MsgTable`] to use for sending and receiving messages.
    msg_table: MsgTable<C, A, R, D>,
    /// The error that caused the client to disconnect.
    disconnect_err: Option<Error>,
    /// The [`Transport`] to use to send and receive the messages, if the connection is open.
    transport: Option<T>,
    /// The system used to generate ping messages and estimate the RTT.
    ping_sys: ClientPingSystem,
    /// The [`ReliableSystem`] to add optional reliability to messages.
    reliable_sys: ClientReliableSystem<C, A, R, D>,
    /// A buffer for messages that are ready to be received.
    ready: VecDeque<(MsgHeader, Box<dyn NetMsg>)>,
}

impl<T: ClientTransport, C: NetMsg, A: NetMsg, R: NetMsg, D: NetMsg> ClientConnection<T, C, A, R, D> {
    pub fn new(msg_table: MsgTable<C, A, R, D>) -> Self {
        Self {
            msg_table: msg_table.clone(),
            disconnect_err: None,
            transport: None,
            ping_sys: ClientPingSystem::new(),
            reliable_sys: ReliableSystem::new(msg_table),
            ready: VecDeque::new(),
        }
    }

    pub fn connect(&mut self, local_addr: SocketAddr, peer_addr: SocketAddr) -> io::Result<()> {
        let transport = T::new(local_addr, peer_addr)?;
        trace!(
            "ClientConnection created from {} to {}",
            transport
                .local_addr()
                .map(|addr| addr.to_string())
                .unwrap_or("UNKNOWN".to_owned()),
            transport
                .peer_addr()
                .map(|addr| addr.to_string())
                .unwrap_or("UNKNOWN".to_owned()),
        );

        // clear the disconnect error when creating a new connection
        self.disconnect_err = None;
        // clean up from last connection
        self.ping_sys = ClientPingSystem::new();
        self.reliable_sys = ReliableSystem::new(self.msg_table.clone());
        self.ready.clear();

        self.transport = Some(transport);
        Ok(())
    }

    // TODO: rework to not fail due to the transport. Only due to passing in a wrong message type.
    //      Then a custom error type may be helpful.
    pub fn send<M: NetMsg>(&mut self, msg: &M) -> io::Result<()> {
        // TODO: convert to a custom error type?
        let transport = match &mut self.transport {
            Some(t) => t,
            None => {
                return Err(Error::new(
                    ErrorKind::NotConnected,
                    "Client is not connected",
                ))
            }
        };

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
        transport.send(m_type, payload)
    }

    /// Disconnects this connection due to the error.
    fn disconnect_err(&mut self, err: Error) {
        self.disconnect_err = Some(err);
        self.transport = None;
    }

    /// Disconnects this connection if there is an error.
    fn disconnect_result(&mut self, result: io::Result<()>) {
        if let Err(err) = result {
            self.disconnect_err(err);
        }
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

    pub fn recv(&mut self) -> Option<(MsgHeader, Box<dyn NetMsg>)> {
        self.ready.pop_front()
    }

    /// Gets all outstanding messages from the [`Transport`], and adds them to an internal buffer.
    ///
    /// To get the actual messages, use [`recv`](Self::recv).
    pub fn get_msgs(&mut self) {
        match self.get_msgs_err() {
            Ok(()) => {}
            Err(err) if err.kind() == ErrorKind::WouldBlock => {}
            Err(err) => {
                self.disconnect_err(err);
            }
        }
    }

    /// Gets all the outstanding messages from the [`Transport`] and adds them to the `self.ready`
    /// buffer. Any errors other than a [`WouldBlock`](ErrorKind::WouldBlock) are treated as
    /// unrecoverable errors and therefor close the connection.
    fn get_msgs_err(&mut self) -> io::Result<()> {
        // TODO: support blocking somehow.
        loop {
            let buf = match &mut self.transport {
                None => return Ok(()),
                Some(t) => t.recv()?,
            };

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
            if !self.msg_table.valid_m_type(header.m_type) {
                warn!(
                    "Client: Received a message with an invalid MType: {}, Maximum MType is {}",
                    header.m_type,
                    self.msg_table.mtype_count()
                );
            }

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
                    Ok(ping_msg) => match ping_msg.ping_type {
                        PingType::Req => {
                            if let Err(err) = self.send(&ping_msg.response()) {
                                warn!("Error in responding to a ping: {}", err);
                            }
                        }
                        PingType::Res => {
                            self.ping_sys.recv_ping_msg(ping_msg.ping_num);
                        }
                    },
                }
                continue;
            }

            let msg = match self.msg_table.deser[header.m_type](&buf[HEADER_SIZE..]) {
                Ok(msg) => msg,
                Err(err) => {
                    warn!("{}", err);
                    continue;
                }
            };

            // handle reliability and ordering
            self.reliable_sys.push_received(header, msg);
            // get all messages from the reliable system and push them on the "ready" que.
            while let Some((header, msg)) = self.reliable_sys.get_received() {
                self.ready.push_back((header, msg));
            }
        }
    }

    /// Takes the disconnection error, if any.
    pub fn take_err(&mut self) -> Option<Error> {
        self.disconnect_err.take()
    }

    /// Resends any messages that it needs to for the reliability system to work.
    pub fn resend_reliable(&mut self) {
        for (header, payload) in self.reliable_sys.get_resend() {
            if let Some(transport) = &self.transport {
                if let Err(err) = transport.send(header.m_type, payload.clone()) {
                    self.disconnect_err(err);
                }
            }
        }
    }

    fn check_transport(&mut self) -> io::Result<&mut T> {
        match &mut self.transport {
            Some(t) => Ok(t),
            None => Err(io::Error::new(
                ErrorKind::NotConnected,
                "Client is not connected",
            )),
        }
    }

    pub fn local_addr(&self) -> Option<SocketAddr> {
        self.transport.as_ref()?.local_addr().ok()
    }

    pub fn peer_addr(&self) -> Option<SocketAddr> {
        self.transport.as_ref()?.peer_addr().ok()
    }

    pub fn rtt(&self) -> u32 {
        self.ping_sys.rtt()
    }
}
