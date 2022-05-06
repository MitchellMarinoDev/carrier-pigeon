use crate::message_table::{MsgTableParts, CONNECTION_TYPE_MID, DISCONNECT_TYPE_MID};
use crate::net::{CId, DeserFn, Status, Transport, CIdSpec, ErasedNetMsg, NetMsg, Config};
use crate::tcp::TcpCon;
use crate::udp::UdpCon;
use crate::MId;
use hashbrown::HashMap;
use log::{debug, error, trace};
use std::any::{Any, type_name, TypeId};
use std::io;
use std::io::ErrorKind::{InvalidData, WouldBlock};
use std::io::{Error, ErrorKind};
use std::net::{SocketAddr, TcpListener};
use std::time::{Duration, Instant};

/// A server.
///
/// Listens on a address and port, allowing for clients to connect.
/// Newly connected clients will be given a client ID (CId) starting
/// at `1` that is unique for the session.
///
/// This will manage multiple connections to clients. Each connection
/// will have a TCP and UDP connection on the same address and port.
pub struct Server {
    /// The current cid. incremented then assigned to new connections.
    current_cid: CId,
    /// The configuration of the server.
    config: Config,
    /// The received message buffer.
    ///
    /// Each [`MId`] has its own vector.
    msg_buff: Vec<Vec<ErasedNetMsg>>,

    /// The pending connections (Connections that are established but have
    /// not sent a connection message yet).
    new_cons: Vec<(TcpCon, CId, Instant)>,
    /// Disconnected connections.
    disconnected: Vec<(CId, Status)>,
    /// The listener for new connections.
    listener: TcpListener,
    /// The TCP connection for this client.
    tcp: HashMap<CId, TcpCon>,
    /// The UDP connection for this client.
    udp: UdpCon,

    /// The map from CId to SocketAddr for the UDP messages to be sent to.
    ///
    /// This needs to be a mirror of `addr_cid`, and needs to be added and
    /// removed with the TCP connections.
    /// Because of these things, ***ALWAYS*** use the `add_tcp_con` and
    /// `rm_tcp_con` functions to mutate these maps.
    cid_addr: HashMap<CId, SocketAddr>,
    /// The map from SocketAddr to CId for the UDP messages to be sent to.
    ///
    /// This needs to be a mirror of `cid_addr`, and needs to be added and
    /// removed with the TCP connections.
    /// Because of these things, ***ALWAYS*** use the `add_tcp_con` and
    /// `rm_tcp_con` functions to mutate these maps.
    addr_cid: HashMap<SocketAddr, CId>,

    /// The [`MsgTableParts`] to use for sending messages.
    parts: MsgTableParts,
}

impl Server {
    /// Creates a new [`Server`].
    ///
    /// Creates a new [`Server`] listening on the address `listen_addr`.
    pub fn new(mut listen_addr: SocketAddr, parts: MsgTableParts, config: Config) -> io::Result<Self> {
        let listener = TcpListener::bind(listen_addr)?;
        listen_addr = listener.local_addr().unwrap();
        listener.set_nonblocking(true)?;
        let udp = UdpCon::new(listen_addr, None, config.max_msg_size)?;

        debug!("New server created at {}.", listen_addr);

        let mid_count = parts.tid_map.len();
        let mut msg_buff = Vec::with_capacity(mid_count);
        for _i in 0..mid_count {
            msg_buff.push(vec![]);
        }

        Ok(Server {
            current_cid: 0,
            config,
            msg_buff,
            new_cons: vec![],
            disconnected: vec![],
            listener,
            tcp: HashMap::new(),
            udp,
            cid_addr: Default::default(),
            addr_cid: Default::default(),
            parts,
        })
    }

    /// Gets the config of the server.
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Disconnects from the given `cid`. You should always disconnect
    /// all clients before dropping the server to let the clients know
    /// that you intentionally disconnected. The `discon_msg` allows you
    /// to give a reason for the disconnect.
    pub fn disconnect<T: Any + Send + Sync>(&mut self, discon_msg: &T, cid: CId) -> io::Result<()> {
        if !self.alive(cid) {
            return Err(io::Error::new(ErrorKind::InvalidData, "Invalid CId."));
        }
        debug!("Disconnecting CId {}", cid);
        self.send_to(discon_msg, cid)?;
        // Close the TcpCon
        self.tcp.get_mut(&cid).unwrap().close()?;
        // No shutdown method on udp.
        self.disconnected.push((cid, Status::Closed));
        Ok(())
    }

    /// Handle the new connection attempts by calling the given hook.
    pub fn handle_new_cons<C: Any + Send + Sync, R: Any + Send + Sync>(&mut self, hook: &mut dyn FnMut(CId, C) -> (bool, R)) -> u32 {
        // Start handling new connections.
        while self.new_cons.len() < self.config.max_con_handle {
            if let Ok((stream, _addr)) = self.listener.accept() {
                debug!("New connection attempt.");
                stream.set_nonblocking(true).unwrap();
                let tcp_con = TcpCon::from_stream(stream, self.config.max_msg_size);
                let cid = self.new_cid();
                self.new_cons.push((tcp_con, cid, Instant::now()));
            } else {
                break;
            }
        }

        let mut accept_count = 0;
        // Handle the new connections.
        let mut remove = vec![];
        let deser_fn = self.parts.deser[CONNECTION_TYPE_MID];
        for (idx, (con, cid, time)) in self.new_cons.iter_mut().enumerate() {
            match Self::handle_new_con::<C>(deser_fn, con, self.config.timeout, time) {
                Ok(c) => {
                    let (acc, r): (bool, R) = hook(*cid, c);
                    if acc {
                        debug!("Accepted new connection at {}.", con.peer_addr().unwrap());
                        accept_count += 1;
                    }
                    remove.push((idx, Some(r)));
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => {}
                Err(e) => {
                    error!("IO error occurred while handling a pending connection. {}", e);
                    remove.push((idx, None));
                }
            }
        }
        for (idx, resp) in remove {
            let (con, cid, _) = self.new_cons.remove(idx);
            // If `resp` is Some, the connection was accepted and
            // we need to send the response message.
            if let Some(r) = resp {
                self.add_tcp_con_cid(cid, con);
                if let Err(e) = self.send_to(&r, cid) {
                    error!("IO error occurred while responding to a pending connection. {}", e);
                }
            }
        }

        accept_count
    }

    /// Handles a new connection by trying to read the connection message.
    ///
    /// If there is an error in connection (including timeout) this will
    /// return `Err(e)`. If the connection is not finished it will return
    /// `Ok(None)`. If the connection opened successfully, it will return
    /// `Ok(Some(c))`.
    ///
    /// If this returns an error other than a `WouldBlock` error, it should
    /// be removed from the list of pending connections. If it returns
    /// Ok(Some(c)) it should also be removed, as it has finished connecting
    /// successfully. It should not be removed from this list if it returns
    /// Ok(None), as that means the connection is still pending.
    fn handle_new_con<C: Any + Send + Sync>(
        deser_fn: DeserFn,
        con: &mut TcpCon,
        timeout: Duration,
        time: &Instant,
    ) -> io::Result<C> {
        if time.elapsed() > timeout {
            return Err(Error::new(
                ErrorKind::TimedOut,
                "The new connection did not send a connection message in time.",
            ));
        }

        let (mid, msg) = con.recv()?;

        if mid != CONNECTION_TYPE_MID {
            let e_msg = format!("Expected MId {}, got MId {}.", CONNECTION_TYPE_MID, mid);
            return Err(Error::new(ErrorKind::InvalidData, e_msg));
        }

        let con_msg = deser_fn(msg).map_err(|o| {
            let e_msg = format!("Encountered a deserialization error when handling a new connection. {o}");
            Error::new(ErrorKind::InvalidData, e_msg)
        })?;

        let con_msg = *con_msg.downcast::<C>().unwrap();
        Ok(con_msg)
    }

    /// Handles the disconnect events.
    pub fn handle_disconnects(&mut self, hook: &mut dyn FnMut(CId, Status)) -> u32 {
        let mut cids_to_rm = vec![];

        // disconnect counts.
        let mut i = 0;

        for (cid, status) in self.disconnected.drain(..) {
            hook(cid, status);
            cids_to_rm.push(cid);
            i += 1;
        }

        // Remove the CIds
        for cid in cids_to_rm {
            debug!("Removing CId {}", cid);
            self.rm_tcp_con(cid).unwrap();
        }

        i
    }

    /// A function that encapsulates the sending logic for the TCP transport.
    fn send_tcp(&self, cid: CId, mid: MId, payload: &[u8]) -> io::Result<()> {
        let tcp = match self.tcp.get(&cid) {
            Some(tcp) => tcp,
            None => return Err(Error::new(ErrorKind::InvalidData, "Invalid CId.")),
        };

        tcp.send(mid, payload)
    }

    /// A function that encapsulates the sending logic for the UDP transport.
    fn send_udp(&self, cid: CId, mid: MId, payload: &[u8]) -> io::Result<()> {
        let addr = match self.cid_addr.get(&cid) {
            Some(addr) => *addr,
            None => return Err(Error::new(ErrorKind::InvalidData, "Invalid CId.")),
        };

        self.udp.send_to(addr, mid, payload)
    }

    /// A function that encapsulates the receiving logic for the TCP transport.
    ///
    /// Any errors in receiving are returned. An error of type [`WouldBlock`] means
    /// no more messages can be yielded without blocking.
    fn recv_tcp(&mut self, cid: CId) -> io::Result<(MId, ErasedNetMsg)> {
        let tcp = match self.tcp.get_mut(&cid) {
            Some(tcp) => tcp,
            None => return Err(Error::new(ErrorKind::InvalidData, "Invalid CId.")),
        };

        let (mid, bytes) = tcp.recv()?;

        if !self.parts.valid_mid(mid) {
            let e_msg = format!(
                "TCP: Got a message specifying MId {}, but the maximum MId is {}.",
                mid,
                self.parts.mid_count()
            );
            return Err(Error::new(ErrorKind::InvalidData, e_msg));
        }

        let deser_fn = self.parts.deser[mid];
        let msg = deser_fn(bytes)?;

        let net_msg = ErasedNetMsg {
            cid,
            time: None,
            msg
        };

        Ok((mid, net_msg))
    }

    /// A function that encapsulates the receiving logic for the UDP transport.
    ///
    /// Any errors in receiving are returned. An error of type [`WouldBlock`] means
    /// no more messages can be yielded without blocking. [`InvalidData`] likely means
    /// carrier-pigeon detected bad data.
    fn recv_udp(&mut self) -> io::Result<(MId, ErasedNetMsg)> {
        let (from, mid, time, bytes) = self.udp.recv_from()?;

        if !self.parts.valid_mid(mid) {
            let e_msg = format!(
                "TCP: Got a message specifying MId {}, but the maximum MId is {}.",
                mid,
                self.parts.mid_count()
            );
            return Err(Error::new(ErrorKind::InvalidData, e_msg));
        }

        let deser_fn = self.parts.deser[mid];
        let msg = deser_fn(bytes)?;

        let cid = match self.addr_cid.get(&from) {
            Some(&cid) if self.alive(cid) => cid,
            _ => {
                return Err(Error::new(
                    ErrorKind::Other,
                    "Received data from a address that is not connected.",
                ))
            }
        };

        let net_msg = ErasedNetMsg {
            cid,
            time: Some(time),
            msg
        };

        Ok((mid, net_msg))
    }

    /// Sends a message to the [`CId`] `cid`.
    ///
    /// ## Errors
    /// If the client isn't connected to another computer,
    /// This will return a not connected error.
    /// If the message type isn't registered, this will return
    /// a type not registered error. If the msg fails to be
    /// serialized this will return serialization error.
    pub fn send_to<T: Any + Send + Sync>(&self, msg: &T, cid: CId) -> io::Result<()> {
        let tid = TypeId::of::<T>();
        if !self.valid_tid(tid) {
            return Err(io::Error::new(
                ErrorKind::InvalidData,
                "Type not registered.",
            ));
        }
        let mid = self.parts.tid_map[&tid];
        let transport = self.parts.transports[mid];
        let ser_fn = self.parts.ser[mid];
        let b = ser_fn(msg)
            .map_err(|_| io::Error::new(ErrorKind::InvalidData, "Serialization Error."))?;

        trace!(
            "Sending message of MId {}, len {}, to CId {}",
            mid,
            b.len(),
            cid
        );
        match transport {
            Transport::TCP => self.send_tcp(cid, mid, &b[..]),
            Transport::UDP => self.send_udp(cid, mid, &b[..]),
        }?;

        Ok(())
    }

    /// Broadcasts a message to all connected clients.
    pub fn broadcast<T: Any + Send + Sync>(&self, msg: &T) -> io::Result<()> {
        for cid in self.cids() {
            self.send_to(msg, cid)?;
        }
        Ok(())
    }

        /// Sends a message to all [`CId`]s that match `spec`.
    pub fn send_spec<T: Any + Send + Sync>(&self, msg: &T, spec: CIdSpec) -> io::Result<()> {
        for cid in self
            .cids()
            .filter(|cid| spec.matches(*cid))
        {
            self.send_to(msg, cid)?;
        }
        Ok(())
    }

    /// Gets an iterator for the messages of type `T`.
    ///
    /// Make sure to call [`recv_msgs()`](Self::recv_msgs) before calling this.
    ///
    /// ### Panics
    /// Panics if the type `T` was not registered.
    /// For a non-panicking version, see [try_recv()](Self::try_recv).
    pub fn recv<'s, T: Any + Send + Sync>(&'s self) -> impl Iterator<Item=NetMsg<T>> + 's {
        let tid = TypeId::of::<T>();
        if !self.parts.valid_tid(tid) {
            panic!("Type ({}) not registered.", type_name::<T>());
        }
        let mid = self.parts.tid_map[&tid];

        self.msg_buff[mid]
            .iter()
            .map(|m| m.to_typed::<T>().unwrap())
    }

    /// Gets an iterator for the messages of type `T`.
    ///
    /// Make sure to call [`recv_msgs()`](Self::recv_msgs) before calling this.
    ///
    /// Returns `None` if the type `T` was not registered.
    pub fn try_recv<'s, T: Any + Send + Sync>(&'s self) -> Option<impl Iterator<Item=NetMsg<T>> + 's> {
        let tid = TypeId::of::<T>();
        let mid = *self.parts.tid_map.get(&tid)?;

        Some(
            self.msg_buff[mid]
                .iter()
                .map(|m| m.to_typed::<T>().unwrap())
        )
    }

    /// Gets an iterator for the messages of type `T` that have
    /// been received from [`CId`]s that match `spec`.
    ///
    /// Make sure to call [`recv_msgs()`](Self::recv_msgs)
    ///
    /// ### Panics
    /// Panics if the type `T` was not registered.
    /// For a non-panicking version, see [try_recv_spec()](Self::try_recv_spec).
    pub fn recv_spec<T: Any + Send + Sync>(&self, spec: CIdSpec) ->  impl Iterator<Item=NetMsg<T>> + '_ {
        let tid = TypeId::of::<T>();
        if !self.parts.valid_tid(tid) {
            panic!("Type ({}) not registered.", type_name::<T>());
        }
        let mid = self.parts.tid_map[&tid];

        self.msg_buff[mid]
            .iter()
            .filter(move |net_msg| spec.matches(net_msg.cid))
            .map(|net_msg| net_msg.to_typed().unwrap())
    }

    /// Gets an iterator for the messages of type `T` that have
    /// been received from [`CId`]s that match `spec`.
    ///
    /// Make sure to call [`recv_msgs()`](Self::recv_msgs)
    ///
    /// Returns `None` if the type `T` was not registered.
    pub fn try_recv_spec<T: Any + Send + Sync>(&self, spec: CIdSpec) ->  Option<impl Iterator<Item=NetMsg<T>> + '_> {
        let tid = TypeId::of::<T>();
        let mid = *self.parts.tid_map.get(&tid)?;

        Some(
            self.msg_buff[mid]
                .iter()
                .filter(move |net_msg| spec.matches(net_msg.cid))
                .map(|net_msg| net_msg.to_typed().unwrap())
        )
    }

    /// Receives the messages from the connections.
    /// This should be done before calling `recv<T>()`.
    ///
    /// When done in a game loop, you should call `clear_msgs()`, then `recv_msgs()`
    /// before default time. This will clear the messages between frames.
    pub fn recv_msgs(&mut self) -> u32 {
        let mut i = 0;

        // TCP
        for cid in self.cids().collect::<Vec<_>>() {
            loop {
                let msg = self.recv_tcp(cid);
                if self.handle_tcp_msg(&mut i, cid, msg) {
                    // Done yielding messages.
                    break;
                }
            }
        }

        // UDP
        loop {
            let msg = self.recv_udp();
            if self.handle_udp_msg(&mut i, msg) {
                // Done yielding messages.
                break;
            }
        }
        i
    }

    /// Logic for handling a new TCP message.
    ///
    /// Increments `count` when it successfully got a message including a disconnect message.
    ///
    /// When getting an error, this will disconnect the peer.
    /// When getting a disconnection message, this will add it to the disconnection que.
    /// Otherwise it adds it to the msg buffer.
    ///
    /// returns weather the tcp connection is done yielding messages.
    fn handle_tcp_msg(
        &mut self,
        count: &mut u32,
        cid: CId,
        msg: io::Result<(MId, ErasedNetMsg)>,
    ) -> bool {
        match msg {
            Err(e) if e.kind() == WouldBlock => true,
            // Other error occurred.
            Err(e) => {
                error!("TCP({}): IO error occurred while receiving data. {}", cid, e);
                self.disconnected.push((cid, Status::Dropped(e)));
                true
            }
            // Got a message.
            Ok((mid, net_msg)) => {
                *count += 1;
                if mid == DISCONNECT_TYPE_MID {
                    debug!("Disconnecting peer {}", cid);
                    self.disconnected.push((cid, Status::Disconnected(net_msg.msg)));
                    return true;
                }

                self.msg_buff[mid].push(net_msg);
                false
            }
        }
    }

    /// Logic for handling a new UDP message.
    ///
    /// Increments `count` when it successfully got a message
    ///
    /// When getting an error, this will log and ignore it.
    /// Otherwise it adds it to the msg buffer.
    ///
    /// returns weather the udp connection is done yielding messages.
    fn handle_udp_msg(
        &mut self,
        count: &mut u32,
        msg: io::Result<(MId, ErasedNetMsg)>,
    ) -> bool {
        match msg {
            Err(e) if e.kind() == WouldBlock => true,
            // Other error occurred.
            Err(e) => {
                error!("UDP: IO error occurred while receiving data. {}", e);
                true
            }
            // Got a message.
            Ok((mid, net_msg)) => {
                *count += 1;
                self.msg_buff[mid].push(net_msg);
                false
            }
        }
    }

    /// Clears messages from the buffer.
    pub fn clear_msgs(&mut self) {
        for buff in self.msg_buff.iter_mut() {
            buff.clear();
        }
    }

    /// Gets the address that the server is listening on.
    pub fn listen_addr(&self) -> SocketAddr {
        self.listener.local_addr().unwrap()
    }

    /// An iterator of the [`CId`]s.
    pub fn cids(&self) -> impl Iterator<Item = CId> + '_ {
        self.cid_addr.keys().map(|cid| *cid)
    }

    /// Returns whether the connection of the given CId is alive.
    pub fn alive(&self, cid: CId) -> bool {
        self.cid_addr.contains_key(&cid)
    }

    /// Returns whether a message of type `tid` can be sent.
    pub fn valid_tid(&self, tid: TypeId) -> bool {
        self.parts.valid_tid(tid)
    }

    /// The number of active connections.
    /// To ensure an accurate count, it is best to call this after calling
    /// [`handle_disconnects()`](Self::handle_disconnects).
    pub fn connection_count(&self) -> usize {
        self.cid_addr.len()
    }

    /// Gets the address of the given CId.
    pub fn addr_of(&self, cid: CId) -> Option<SocketAddr> {
        self.cid_addr.get(&cid).map(|o| *o)
    }

    /// Gets the address of the given CId.
    pub fn cid_of(&self, addr: SocketAddr) -> Option<CId> {
        self.addr_cid.get(&addr).map(|o| *o)
    }

    // Private:
    fn new_cid(&mut self) -> CId {
        self.current_cid += 1;
        self.current_cid
    }

    /// Adds a TCP connection with the [`CId`] `cid`. The cid needs to be unique, generate one with `new_cid()`.
    fn add_tcp_con_cid(&mut self, cid: CId, con: TcpCon) {
        let peer_addr = con.peer_addr().unwrap();
        self.tcp.insert(cid, con);
        self.addr_cid.insert(peer_addr, cid);
        self.cid_addr.insert(cid, peer_addr);
    }

    /// Removes a TCP connection.
    fn rm_tcp_con(&mut self, cid: CId) -> io::Result<()> {
        self.tcp.remove(&cid).ok_or(Error::new(InvalidData, "Invalid CId."))?;
        let addr = self.cid_addr.remove(&cid).unwrap();
        self.addr_cid.remove(&addr);
        Ok(())
    }
}
