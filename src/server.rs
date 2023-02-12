use crate::connection::ping_system::ServerPingSystem;
use crate::connection::reliable::ReliableSystem;
use crate::connection::{ConnectionList, ConnectionListError, DisconnectionEvent};
use crate::message_table::{CONNECTION_M_TYPE, DISCONNECT_M_TYPE, PING_M_TYPE, RESPONSE_M_TYPE};
use crate::messages::{AckMsg, NetMsg, PingMsg, PingType, Response};
use crate::net::{AckNum, CIdSpec, ErasedNetMsg, Message, MsgHeader, HEADER_SIZE};
use crate::transport::server_std_udp::UdpServerTransport;
use crate::transport::ServerTransport;
use crate::{CId, MsgTable, ServerConfig};
use hashbrown::HashMap;
use log::{debug, error, info, trace, warn};
use std::any::{type_name, TypeId};
use std::collections::VecDeque;
use std::io;
use std::io::{Error, ErrorKind};
use std::net::SocketAddr;
use std::sync::Arc;

/// [`ReliableSystem`] with the generic parameters set for a server.
type ServerReliableSystem<C, A, R, D> =
    ReliableSystem<(SocketAddr, Arc<Vec<u8>>), (CId, Box<dyn NetMsg>), C, A, R, D>;

/// A server that manages connections to multiple clients.
///
/// Listens on a address and port, allowing for clients to connect. Newly connected clients will
/// be given a connection ID ([`CId`]) starting at `1` that is unique for the session.
#[cfg_attr(feature = "bevy", derive(bevy::prelude::Resource))]
pub struct Server<C: NetMsg, A: NetMsg, R: NetMsg, D: NetMsg> {
    /// The configuration of the server.
    config: ServerConfig,
    /// The [`MsgTable`] to use for sending and receiving messages.
    msg_table: MsgTable<C, A, R, D>,
    /// The transport to use to send and receive the messages.
    transport: UdpServerTransport,
    /// The system used to generate ping messages and estimate the RTT.
    ping_sys: ServerPingSystem,
    /// The [`ReliableSystem`]s to add optional reliability to messages for each connection.
    reliable_sys: HashMap<CId, ServerReliableSystem<C, A, R, D>>,
    /// The connection list for managing the connections to this [`ServerConnection`].
    connection_list: ConnectionList<C>,
    /// A que that keeps track of disconnection events.
    disconnection_events: VecDeque<DisconnectionEvent<D>>,
    /// A que for clients that are disconnecting
    disconnecting: Vec<(CId, AckNum, D)>,
    /// The received message buffer.
    ///
    /// Each [`MType`](crate::MType) has its own vector.
    msg_buf: Vec<Vec<ErasedNetMsg>>,
}

impl<C: NetMsg, A: NetMsg, R: NetMsg, D: NetMsg> Server<C, A, R, D> {
    pub fn new(
        config: ServerConfig,
        listen_addr: SocketAddr,
        msg_table: MsgTable<C, A, R, D>,
    ) -> io::Result<Self> {
        let connection_list = ConnectionList::new();
        let transport = UdpServerTransport::new(listen_addr)?;
        trace!(
            "{} listening on {}",
            type_name::<UdpServerTransport>(),
            transport
                .listen_addr()
                .map(|addr| addr.to_string())
                .unwrap_or("UNKNOWN".to_owned()),
        );

        Ok(Self {
            msg_buf: (0..msg_table.mtype_count()).map(|_| vec![]).collect(),
            config,
            msg_table,
            transport,
            ping_sys: ServerPingSystem::new(),
            reliable_sys: HashMap::new(),
            disconnection_events: VecDeque::new(),
            disconnecting: vec![],
            connection_list,
        })
    }

    /// Disconnects from the given `cid`. You should always disconnect all clients before dropping
    /// the server to let the clients know that you intentionally disconnected. The `discon_msg`
    /// allows you to give a reason for the disconnect.
    pub fn disconnect(&mut self, discon_msg: D, cid: CId) -> io::Result<()> {
        // TODO: change to custom error type.
        if !self.cid_connected(cid) {
            return Err(Error::new(
                ErrorKind::NotConnected,
                format!("CId {} is not connected.", cid),
            ));
        }
        debug!("Disconnecting CId {}", cid);
        let discon_ack = self.send_to(cid, &discon_msg)?;
        self.disconnecting.push((cid, discon_ack, discon_msg));
        Ok(())
    }

    // TODO: rework to not fail due to the transport. Only due to passing in a wrong message type.
    //      Then a custom error type may be helpful.
    pub fn send_to<M: NetMsg>(&mut self, cid: CId, msg: &M) -> io::Result<AckNum> {
        // verify type is valid
        self.msg_table.check_type::<M>()?;
        let addr = self
            .connection_list
            .addr_of(cid)
            .ok_or_else(|| Error::new(ErrorKind::InvalidData, format!("Invalid CId: {}", cid)))?;

        let tid = TypeId::of::<M>();

        // create the message header
        let m_type = self.msg_table.tid_map[&tid];
        let reliable_sys = self
            .reliable_sys
            .get_mut(&cid)
            .expect("cid is already checked");
        let header = reliable_sys.get_send_header(m_type);

        // build the payload using the header and the message
        let mut payload = header.to_be_bytes().to_vec();

        let ser_fn = self.msg_table.ser[m_type];
        ser_fn(msg, &mut payload).map_err(|e| Error::new(ErrorKind::InvalidData, e))?;
        let payload = Arc::new(payload);

        // send the payload based on the guarantees
        let guarantees = self.msg_table.guarantees[m_type];
        reliable_sys.save(header, guarantees, (addr, payload.clone()));
        let result = self.transport.send_to(addr, m_type, payload);
        self.handle_send_result(cid, result);
        Ok(header.sender_ack_num)
    }

    /// Broadcasts a message to all connected clients.
    pub fn broadcast<T: NetMsg>(&mut self, msg: &T) -> io::Result<()> {
        for cid in self.cids().collect::<Vec<_>>() {
            self.send_to(cid, msg)?;
        }
        Ok(())
    }

    /// Sends a message to all [`CId`]s that match `spec`.
    pub fn send_spec<T: NetMsg>(&mut self, spec: CIdSpec, msg: &T) -> io::Result<()> {
        for cid in self
            .cids()
            .filter(|cid| spec.matches(*cid))
            .collect::<Vec<_>>()
        {
            self.send_to(cid, msg)?;
        }
        Ok(())
    }

    /// Gets an iterator for the messages of type `M` that have been received from [`CId`]s that
    /// match `spec`.
    ///
    /// Make sure to call [`get_msgs()`](Self::get_msgs)
    ///
    /// ### Panics
    /// Panics if the type `M` was not registered.
    /// For a non-panicking version, see [try_recv_spec()](Self::try_recv_spec).
    pub fn recv_spec<M: NetMsg>(&self, spec: CIdSpec) -> impl Iterator<Item = Message<M>> + '_ {
        self.msg_table.check_type::<M>().expect(
            "`recv_spec` panics if generic type `M` is not registered in the MsgTable. \
            For a non panicking version, use `try_recv_spec`",
        );
        let tid = TypeId::of::<M>();
        let m_type = self.msg_table.tid_map[&tid];

        self.msg_buf[m_type]
            .iter()
            .filter(move |net_msg| spec.matches(net_msg.cid))
            .map(|net_msg| net_msg.get_typed().unwrap())
    }

    /// Gets an iterator for the messages of type `M` that have been received from [`CId`]s that
    /// match `spec`.
    ///
    /// Make sure to call [`get_msgs()`](Self::get_msgs)
    ///
    /// Returns `None` if the type `M` was not registered.
    pub fn try_recv_spec<M: NetMsg>(
        &self,
        spec: CIdSpec,
    ) -> Option<impl Iterator<Item = Message<M>> + '_> {
        let tid = TypeId::of::<M>();
        let m_type = *self.msg_table.tid_map.get(&tid)?;

        Some(
            self.msg_buf[m_type]
                .iter()
                .filter(move |net_msg| spec.matches(net_msg.cid))
                .map(|net_msg| net_msg.get_typed().unwrap()),
        )
    }

    /// Gets an iterator for the messages of type `M`.
    ///
    /// Make sure to call [`get_msgs()`](Self::get_msgs) before calling this.
    ///
    /// ### Panics
    /// Panics if the type `M` was not registered.
    /// For a non-panicking version, see [try_recv()](Self::try_recv).
    pub fn recv<M: NetMsg>(&self) -> impl Iterator<Item = Message<M>> {
        self.msg_table.check_type::<M>().expect(
            "`recv` panics if generic type `M` is not registered in the MsgTable. \
            For a non panicking version, use `try_recv`",
        );
        let tid = TypeId::of::<M>();
        let m_type = self.msg_table.tid_map[&tid];

        self.msg_buf[m_type].iter().map(|m| m.get_typed().unwrap())
    }

    /// Gets an iterator for the messages of type `M`.
    ///
    /// Make sure to call [`get_msgs()`](Self::get_msgs) before calling this.
    ///
    /// Returns `None` if the type `M` was not registered.
    pub fn try_recv<M: NetMsg>(&self) -> Option<impl Iterator<Item = Message<M>>> {
        let tid = TypeId::of::<M>();
        let m_type = *self.msg_table.tid_map.get(&tid)?;

        Some(self.msg_buf[m_type].iter().map(|m| m.get_typed().unwrap()))
    }

    /// This handles everything that the server needs to do each frame.
    ///
    /// This includes:
    ///
    ///  - Clearing the message buffer. This gets rid of all the messages from last frame.
    ///  - Getting the messages for this frame.
    ///  - Resending messages that are needed for the reliability layer.
    ///  - Updating statuses.
    pub fn tick(&mut self) {
        self.clear_msgs();
        self.send_ack_msgs();
        self.send_pings();
        self.resend_reliable();
        self.get_msgs();
        self.update_statuses();
    }

    /// Clears messages from the buffer.
    fn clear_msgs(&mut self) {
        for buff in self.msg_buf.iter_mut() {
            buff.clear();
        }
    }

    /// Sends an [`AckMsg`] to all clients in order to acknowledge all received messages.
    fn send_ack_msgs(&mut self) {
        let ack_msgs: Vec<(CId, AckMsg)> = self
            .reliable_sys
            .iter_mut()
            .filter_map(|(cid, reliable_sys)| reliable_sys.get_ack_msg().map(|msg| (*cid, msg)))
            .collect();

        for (cid, ack_msg) in ack_msgs {
            if let Err(err) = self.send_to(cid, &ack_msg) {
                error!("Error sending AckMsg: {}", err);
            }
        }
    }

    /// Sends a ping messages to the clients if necessary.
    fn send_pings(&mut self) {
        if let Some(msg) = self.ping_sys.get_ping_msg() {
            for cid in self.connection_list.cids().collect::<Vec<_>>() {
                if let Err(err) = self.send_to(cid, &msg) {
                    error!("Failed to send ping message to {}: {}", cid, err);
                }
            }
        }
    }

    /// Resends any messages that it needs to for the reliability system to work.
    fn resend_reliable(&mut self) {
        for cid in self.connection_list.cids() {
            let reliable_sys = self
                .reliable_sys
                .get_mut(&cid)
                .expect("cid should be valid");
            for (header, (addr, payload)) in reliable_sys.get_resend() {
                debug!("Resending msg {}", header.sender_ack_num);
                if let Err(err) = self.transport.send_to(addr, header.m_type, payload) {
                    error!("Error resending msg {}: {}", header.sender_ack_num, err);
                }
            }
        }
    }

    /// Gets all outstanding messages from the [`Transport`], and adds them to an internal buffer.
    ///
    /// To get the actual messages, use [`recv`](Self::recv).
    fn get_msgs(&mut self) {
        match self.get_msgs_err() {
            Ok(()) => {}
            Err(err) if err.kind() == ErrorKind::WouldBlock => {}
            Err(err) => {
                warn!("Server: Error while receiving messages: {}", err);
            }
        }
    }

    /// Receives a message from the transport.
    ///
    /// This will get the next message that is ready to be yielded (if all ordering conditions are
    /// satisfied).
    ///
    /// A Error of type WouldBlock means no more messages can be returned at this time. Other
    /// errors are errors in receiving or validating the data.
    // TODO: refactor to match client.
    fn get_msgs_err(&mut self) -> io::Result<()> {
        loop {
            let (from, buf) = self.transport.recv_from()?;
            let n = buf.len();
            if n < HEADER_SIZE {
                warn!(
                    "Server: Received a packet of length {} from {} which is not big enough \
                    to be a carrier pigeon message. Discarding",
                    n, from
                );
                continue;
            }
            let header = MsgHeader::from_be_bytes(&buf[..HEADER_SIZE]);
            if !self.msg_table.valid_m_type(header.m_type) {
                warn!("Server received message with invalid MType ({}). Maximum is {}", header.m_type, self.msg_table.mtype_count() - 1);
                continue;
            }
            trace!(
                "Server: received message (MType: {}, len: {}, AckNum: {}, from: {})",
                header.m_type,
                n,
                header.sender_ack_num,
                from,
            );

            let cid = match self.connection_list.cid_of(from) {
                // the message received was not from a connected client
                None => {
                    // ignore messages from not connected clients,
                    // unless it is a connection type message
                    if header.m_type != CONNECTION_M_TYPE {
                        debug!(
                            "Server: Discarding a message that not a connection \
                        message from a non-client ({})",
                            from
                        );
                        continue;
                    }

                    debug!("Server: Connection message from {}", from);
                    let msg = match self.msg_table.deser[header.m_type](&buf[HEADER_SIZE..]) {
                        Ok(msg) => *msg.downcast().expect("since the MType is `CONNECTION_M_TYPE`, the message should be the connection type"),
                        Err(err) => {
                            warn!("Error in deserializing a connection message: {}", err);
                            continue;
                        },
                    };
                    // create a new connection
                    self.connection_list
                        .new_pending(from, msg)
                        .expect("address already checked to not be connected");
                    continue;
                }
                Some(cid) => cid,
            };

            let msg = match self.msg_table.deser[header.m_type](&buf[HEADER_SIZE..]) {
                Ok(i) => i,
                Err(err) => {
                    warn!("Server: Error deserializing message: {}", err);
                    continue;
                }
            };

            match header.m_type {
                // TODO: Add other special types.
                PING_M_TYPE => {
                    let msg: PingMsg = *msg.downcast().expect(
                        "since the MType is `PING_M_TYPE`, the message should be the PingMsg type",
                    );
                    match msg.ping_type {
                        PingType::Req => {
                            if let Err(err) = self.send_to(cid, &msg.response()) {
                                warn!("Error in responding to a ping: {}", err);
                            }
                        }
                        PingType::Res => {
                            self.ping_sys.recv_ping_msg(cid, msg.ping_num);
                        }
                    }
                }
                DISCONNECT_M_TYPE => {
                    // TODO: impl for server
                    if !self.cid_connected(cid) {
                        // TODO: send an ack_msg.
                        continue;
                    }
                    let disconnect_msg: D = *msg.downcast().expect("since the MType is `DISCONNECT_M_TYPE`, the message should be the disconnection type");
                    self.disconnection_events
                        .push_back(DisconnectionEvent::disconnected(cid, disconnect_msg));
                    // TODO: add this next to all disconnection_events.push calls; Or, even better, make it a function
                    self.remove_connection(cid)
                        .expect("cid should be from a connected client");
                }
                RESPONSE_M_TYPE => {
                    warn!("Server: Got a response type message. Ignoring.");
                }
                _ => {
                    // handle reliability and ordering
                    let reliable_sys = self
                        .reliable_sys
                        .get_mut(&cid)
                        .expect("cid already checked");
                    reliable_sys.push_received(header, (cid, msg));
                    // get all messages from the reliable system and push them on the "ready" que.
                    while let Some((header, (cid, msg))) = reliable_sys.get_received() {
                        self.msg_buf[header.m_type].push(ErasedNetMsg::new(
                            cid,
                            header.sender_ack_num,
                            header.order_num,
                            msg,
                        ));
                    }
                }
            }
        }
    }

    // TODO: handle things in the disconnecting buffer.
    fn update_statuses(&mut self) {
        let mut remove = vec![];
        for (idx, (cid, ack_num, _d)) in self.disconnecting.iter().enumerate() {
            if !self.reliable_sys[cid].is_not_acked(*ack_num) {
                remove.push(idx);
            }
        }

        for idx in remove.into_iter().rev() {
            let (cid, _, disconnect_msg) = self.disconnecting.swap_remove(idx);
            self.server_disconnected_event(cid, disconnect_msg);
        }
    }

    /// Updates the status of the connection based on a send error.
    ///
    /// Since receiving is not connection specific, it should be handled differently.
    fn handle_send_err(&mut self, cid: CId, err: Error) {
        warn!("Got error while sending data to {}. Considering connection dropped. {}", cid, err);
        self.connection_dropped_event(cid, err);
    }

    /// Updates the status of the connection if there is a send error.
    ///
    /// Since receiving is not connection specific, it should be handled differently.
    fn handle_send_result<T>(&mut self, cid: CId, result: io::Result<T>) {
        if let Err(err) = result {
            self.handle_send_err(cid, err);
        }
    }

    /// Handles all outstanding pending connections
    /// by calling `hook` with the `CId`, `SocketAddr` and the connection message.
    ///
    /// ### Guarentees
    /// The caller must guarentee that generic parameters `C` `A` and `R` are the same generic
    /// parameters that were passed into [`MsgTableBuilder::build`](crate::MsgTableBuilder::build).
    pub fn handle_pending(
        &mut self,
        mut hook: impl FnMut(CId, SocketAddr, C) -> Response<A, R>,
    ) -> u32 {
        let mut count = 0;
        while let Some((cid, addr, msg)) = self.connection_list.get_pending() {
            if self.connection_list.addr_connected(addr) {
                // address is already connected; ignore the connection request
                continue;
            }

            count += 1;
            let response = hook(cid, addr, msg);
            if let Response::Accepted(_) = &response {
                info!("Accepting client {}", cid);
                self.new_connection(cid, addr).expect(
                    "cid and address should be valid, as they came from the connection list",
                )
            } else {
                debug!("Rejecting client {}", cid);
            }
            if let Err(err) = self.send_to(cid, &response) {
                warn!(
                    "failed to send response message to {} (cid: {}): {}",
                    addr, cid, err
                );
            }
        }
        count
    }

    // TODO: change to handle events
    /// Handles all remaining disconnects.
    ///
    /// Returns the number of disconnects handled.
    pub fn handle_disconnect(&mut self) -> Option<DisconnectionEvent<D>> {
        self.disconnection_events.pop_front()
    }

    /// Handles an incoming ping message.
    ///
    /// If this is a request type, it will respond to it.
    /// If it is a response, it is handled accordingly.
    fn recv_ping(&mut self, cid: CId, ping_msg: PingMsg) {
        match ping_msg.ping_type {
            PingType::Req => {
                if let Err(err) = self.send_to(cid, &ping_msg.response()) {
                    warn!("Error in responding to a ping (CId: {}): {}", cid, err);
                }
            }
            PingType::Res => self.ping_sys.recv_ping_msg(cid, ping_msg.ping_num),
        }
    }

    /// Add a new connection with `cid` and `addr`.
    fn new_connection(&mut self, cid: CId, addr: SocketAddr) -> Result<(), ConnectionListError> {
        self.connection_list.new_connection(cid, addr)?;
        self.ping_sys.add_cid(cid);
        self.reliable_sys
            .insert(cid, ReliableSystem::new(self.msg_table.clone()));
        Ok(())
    }

    /// Removes a connection `cid`.
    fn remove_connection(&mut self, cid: CId) -> Result<(), ConnectionListError> {
        self.connection_list.remove_connection(cid)?;
        let old_reliable = self.reliable_sys.remove(&cid);
        debug_assert!(old_reliable.is_some(), "since self.connection_list.remove_connection() didn't fail, there should be a corresponding entry for that cid in self.reliable_sys");
        let removed = self.ping_sys.remove_cid(cid);
        debug_assert!(removed, "since self.connection_list.remove_connection() didn't fail, self.ping_sys.remove_cid() should return true");
        Ok(())
    }

    /// Creates a [`DisconnectionEvent`] of type `Dropped`,
    /// and and removes the connection.
    pub fn connection_dropped_event(&mut self, cid: CId, err: Error) {
        if !self.cid_connected(cid) {
            return;
        }
        self.disconnection_events
            .push_back(DisconnectionEvent::dropped(cid, err));
        self.remove_connection(cid)
            .expect("cid should be from a connected client");
        debug!("CId {} dropped.", cid);
    }

    /// Creates a [`DisconnectionEvent`] of type `Disconnected`,
    /// and and removes the connection.
    pub fn connection_disconnected_event(&mut self, cid: CId, disconnect_msg: D) {
        if !self.cid_connected(cid) {
            return;
        }
        self.disconnection_events
            .push_back(DisconnectionEvent::disconnected(cid, disconnect_msg));
        self.remove_connection(cid)
            .expect("cid should be from a connected client");
        debug!("CId {} disconnected.", cid);
    }

    /// Creates a [`DisconnectionEvent`] of type `ServerDisconnected`,
    /// and and removes the connection.
    pub fn server_disconnected_event(&mut self, cid: CId, disconnect_msg: D) {
        if !self.cid_connected(cid) {
            return;
        }
        self.disconnection_events
            .push_back(DisconnectionEvent::server_disconnected(cid, disconnect_msg));
        self.remove_connection(cid)
            .expect("cid should be from a connected client");
        debug!("CId {} got disconnected by the server.", cid);
    }

    /// Gets the [`NetConfig`] of the client.
    pub fn config(&self) -> &ServerConfig {
        &self.config
    }

    /// Gets the [`MsgTable`] of the client.
    pub fn msg_table(&self) -> &MsgTable<C, A, R, D> {
        &self.msg_table
    }

    pub fn listen_addr(&self) -> io::Result<SocketAddr> {
        self.transport.listen_addr()
    }

    pub fn cids(&self) -> impl Iterator<Item = CId> + '_ {
        self.connection_list.cids()
    }

    pub fn cid_of(&self, addr: SocketAddr) -> Option<CId> {
        self.connection_list.cid_of(addr)
    }

    pub fn cid_connected(&self, cid: CId) -> bool {
        self.connection_list.cid_connected(cid)
    }

    pub fn cid_disconnecting(&self, cid: CId) -> bool {
        // self.disconnecting
        // TODO: impl
        todo!()
    }

    pub fn addr_of(&self, cid: CId) -> Option<SocketAddr> {
        self.connection_list.addr_of(cid)
    }

    pub fn connection_count(&self) -> usize {
        self.connection_list.connection_count()
    }

    pub fn rtt(&self, cid: CId) -> Option<u32> {
        self.ping_sys.rtt(cid)
    }
}
