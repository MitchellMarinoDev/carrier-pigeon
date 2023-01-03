use crate::message_table::{
    MsgTableParts, CONNECTION_TYPE_MID, DISCONNECT_TYPE_MID, RESPONSE_TYPE_MID,
};
use crate::net::{CId, CIdSpec, Config, DeserFn, ErasedNetMsg, NetMsg, Status, Transport};
use crate::udp::UdpCon;
use crate::MId;
use hashbrown::HashMap;
use log::{debug, error, trace};
use std::any::{type_name, Any, TypeId};
use std::collections::VecDeque;
use std::io;
use std::io::ErrorKind::{InvalidData, WouldBlock};
use std::io::{Error, ErrorKind};
use std::net::{SocketAddr, TcpListener, ToSocketAddrs};
use std::time::{Duration, Instant};

/// A server.
///
/// Listens on a address and port, allowing for clients to connect. Newly connected clients will
/// be given a client ID (CId) starting at `1` that is unique for the session.
///
/// This will manage multiple connections to clients. Each connection will have a TCP and UDP
/// connection on the same address and port.
#[cfg_attr(feature = "bevy", derive(bevy::prelude::Resource))]
pub struct Server {
    /// The current cid. incremented then assigned to new connections.
    current_cid: CId,
    /// The configuration of the server.
    config: Config,
    /// The received message buffer.
    ///
    /// Each [`MId`] has its own vector.
    msg_buff: Vec<Vec<ErasedNetMsg>>,

    /// Disconnected connections.
    disconnected: VecDeque<(CId, Status)>,
    /// The UDP connection for this client.
    udp: UdpCon,

    /// The map from CId to SocketAddr for the UDP messages to be sent to.
    ///
    /// This needs to be a mirror of `addr_cid`, and needs to be added and removed with the TCP
    /// connections. Because of these things, ***ALWAYS*** use the `add_tcp_con` and `rm_tcp_con`
    /// functions to mutate these maps.
    cid_addr: HashMap<CId, SocketAddr>,
    /// The map from SocketAddr to CId for the UDP messages to be sent to.
    ///
    /// This needs to be a mirror of `cid_addr`, and needs to be added and removed with the TCP
    /// connections. Because of these things, ***ALWAYS*** use the `add_tcp_con` and `rm_tcp_con`
    /// functions to mutate these maps.
    addr_cid: HashMap<SocketAddr, CId>,

    /// The [`MsgTableParts`] to use for sending messages.
    parts: MsgTableParts,
}

impl Server {
    /// Creates a new [`Server`].
    ///
    /// Creates a new [`Server`] listening on the address `listen_addr`.
    pub fn new<A: ToSocketAddrs>(
        listen_addr: A,
        parts: MsgTableParts,
        config: Config,
    ) -> io::Result<Self> {
        let listener = TcpListener::bind(listen_addr)?;
        let listen_addr = listener.local_addr().unwrap();
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
            disconnected: VecDeque::new(),
            listener,
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

    /// Disconnects from the given `cid`. You should always disconnect all clients before dropping
    /// the server to let the clients know that you intentionally disconnected. The `discon_msg`
    /// allows you to give a reason for the disconnect.
    pub fn disconnect<T: Any + Send + Sync>(&mut self, discon_msg: &T, cid: CId) -> io::Result<()> {
        if !self.alive(cid) {
            return Err(io::Error::new(ErrorKind::InvalidData, "Invalid CId."));
        }
        debug!("Disconnecting CId {}", cid);
        self.send_to(cid, discon_msg)?;
        self.disconnected.push_back((cid, Status::Closed));
        Ok(())
    }

    /// Handles all available new connection attempts in a loop, calling the given hook for each.
    ///
    /// The hook function should return `(should_accept, response_msg)`.
    ///
    /// Types `C` and `R` need to match the `C` and `R` types that you passed into
    /// [`MsgTable::build()`](MsgTable::build).
    ///
    /// Returns whether a connection was handled.
    pub fn handle_new_con<C: Any + Send + Sync, R: Any + Send + Sync>(
        &mut self,
        mut hook: impl FnMut(CId, C) -> (bool, R),
    ) -> bool {
        // TODO: fix removed: Start handling incoming connections.

        // If we have no active connections, we don't have to continue.
        // TODO: add the handeling of new connections.

        // Handle the new connections.
        let deser_fn = self.parts.deser[CONNECTION_TYPE_MID];

        // // List of accepted connections.
        // let mut accepted = vec![];
        // // List of rejected connections.
        // let mut rejected = vec![];
        // // List of connections that errored out.
        // let mut dead = vec![];

        // TODO: impl
        // for (idx, (con, cid, time)) in self.new_cons.iter_mut().enumerate() {
        //     match Self::handle_con_helper::<C>(deser_fn, con, self.config.timeout, time) {
        //         // Done connecting.
        //         Ok(c) => {
        //             // Call hook
        //             let (acc, resp) = hook(*cid, c);
        //             if acc {
        //                 accepted.push((idx, resp));
        //             } else {
        //                 rejected.push((idx, resp));
        //             }
        //             break; // Only handle 1 connection max.
        //         }
        //         // Not done yet.
        //         Err(e) if e.kind() == ErrorKind::WouldBlock => {}
        //         // Error in connecting.
        //         Err(e) => {
        //             error!(
        //                 "IO error occurred while handling a pending connection. {}",
        //                 e
        //             );
        //             dead.push(idx);
        //         }
        //     }
        // }

        // TODO; impl
        // // Dead connections do not count as handled; they do not call the hook.
        // let handled = !(accepted.is_empty() && rejected.is_empty());
        //
        // // Handle accepted.
        // for (idx, resp) in accepted {
        //     let (con, cid, _) = self.new_cons.remove(idx);
        //     self.accept_incoming(cid, con, &resp);
        // }
        //
        // // Handle rejected.
        // for (idx, resp) in rejected {
        //     let (con, cid, _) = self.new_cons.remove(idx);
        //     self.reject_incoming(cid, con, &resp);
        // }
        //
        // // Handle dead.
        // for idx in dead {
        //     self.new_cons.remove(idx);
        // }
        //
        // handled
        todo!()
    }

    /// Handles all available new connection attempts in a loop, calling the given hook for each.
    ///
    /// The hook function should return `(should_accept, response_msg)`.
    ///
    /// Types `C` and `R` need to match the `C` and `R` types that you passed into
    /// [`MsgTable::build()`](MsgTable::build).
    ///
    /// Returns the number of handled connections.
    pub fn handle_new_cons<C: Any + Send + Sync, R: Any + Send + Sync>(
        &mut self,
        mut hook: impl FnMut(CId, C) -> (bool, R),
    ) -> u32 {
        // Start handling incoming connections.
        self.start_incoming();

        // If we have no active connections, we don't have to continue.
        if self.new_cons.is_empty() {
            return 0;
        }

        // Handle the new connections.
        let deser_fn = self.parts.deser[CONNECTION_TYPE_MID];

        // List of accepted connections.
        let mut accepted = vec![];
        // List of rejected connections.
        let mut rejected = vec![];
        // List of connections that errored out.
        let mut dead = vec![];

        for (idx, (con, cid, time)) in self.new_cons.iter_mut().enumerate() {
            match Self::handle_con_helper::<C>(deser_fn, con, self.config.timeout, time) {
                // Done connecting.
                Ok(c) => {
                    // Call hook
                    let (acc, resp) = hook(*cid, c);
                    if acc {
                        accepted.push((idx, resp));
                    } else {
                        rejected.push((idx, resp));
                    }
                }
                // Not done yet.
                Err(e) if e.kind() == ErrorKind::WouldBlock => {}
                // Error in connecting.
                Err(e) => {
                    error!(
                        "IO error occurred while handling a pending connection. {}",
                        e
                    );
                    dead.push(idx);
                }
            }
        }

        // Dead connections do not count as handled; they do not call the hook.
        let handled = accepted.len() + rejected.len();

        // Handle accepted.
        for (idx, resp) in accepted {
            let (con, cid, _) = self.new_cons.remove(idx);
            self.accept_incoming(cid, con, &resp);
        }

        // Handle rejected.
        for (idx, resp) in rejected {
            let (con, cid, _) = self.new_cons.remove(idx);
            self.reject_incoming(cid, con, &resp);
        }

        // Handle dead.
        for idx in dead {
            self.new_cons.remove(idx);
        }

        handled as u32
    }

    // TODO: add logic for handeling new connections.

    /// Handles a single disconnect, if there is one available to handle.
    ///
    /// If there is no disconnects to handle, `hook` will not be called.
    ///
    /// Returns weather it handled a disconnect.
    pub fn handle_disconnect(&mut self, mut hook: impl FnMut(CId, Status)) -> bool {
        while let Some((cid, status)) = self.disconnected.pop_front() {
            // If the disconnect is a live connection
            if !self.alive(cid) {
                continue;
            }

            // call hook.
            hook(cid, status);
            debug!("Removing CId {}", cid);
            self.rm_tcp_con(cid).unwrap();
            return true;
        }
        false
    }

    /// Handles all remaining disconnects.
    ///
    /// Returns the number of disconnects handled.
    pub fn handle_disconnects(&mut self, mut hook: impl FnMut(CId, Status)) -> u32 {
        // disconnect counts.
        let mut i = 0;

        while let Some((cid, status)) = self.disconnected.pop_front() {
            // If the disconnect is a live connection
            if !self.alive(cid) {
                continue;
            }

            // call hook.
            hook(cid, status);
            debug!("Removing CId {}", cid);
            self.rm_tcp_con(cid).unwrap();
            i += 1;
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
    /// Any errors in receiving are returned. An error of type [`WouldBlock`] means no more
    /// messages can be yielded without blocking.
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
            msg,
        };

        Ok((mid, net_msg))
    }

    /// A function that encapsulates the receiving logic for the `UDP` transport.
    ///
    /// Any errors in receiving are returned. An error of type [`WouldBlock`] means no more
    /// messages can be yielded without blocking. [`InvalidData`] likely means carrier-pigeon
    /// got bad data.
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
            msg,
        };

        Ok((mid, net_msg))
    }

    /// Sends a message to the [`CId`] `cid`.
    pub fn send_to<T: Any + Send + Sync>(&self, cid: CId, msg: &T) -> io::Result<()> {
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
        let b = ser_fn(msg)?;

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
            self.send_to(cid, msg)?;
        }
        Ok(())
    }

    /// Sends a message to all [`CId`]s that match `spec`.
    pub fn send_spec<T: Any + Send + Sync>(&self, spec: CIdSpec, msg: &T) -> io::Result<()> {
        for cid in self.cids().filter(|cid| spec.matches(*cid)) {
            self.send_to(cid, msg)?;
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
    pub fn recv<T: Any + Send + Sync>(&self) -> impl Iterator<Item = NetMsg<T>> {
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
    pub fn try_recv<T: Any + Send + Sync>(&self) -> Option<impl Iterator<Item = NetMsg<T>>> {
        let tid = TypeId::of::<T>();
        let mid = *self.parts.tid_map.get(&tid)?;

        Some(
            self.msg_buff[mid]
                .iter()
                .map(|m| m.to_typed::<T>().unwrap()),
        )
    }

    /// Gets an iterator for the messages of type `T` that have been received from [`CId`]s that
    /// match `spec`.
    ///
    /// Make sure to call [`recv_msgs()`](Self::recv_msgs)
    ///
    /// ### Panics
    /// Panics if the type `T` was not registered.
    /// For a non-panicking version, see [try_recv_spec()](Self::try_recv_spec).
    pub fn recv_spec<T: Any + Send + Sync>(
        &self,
        spec: CIdSpec,
    ) -> impl Iterator<Item = NetMsg<T>> + '_ {
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

    /// Gets an iterator for the messages of type `T` that have been received from [`CId`]s that
    /// match `spec`.
    ///
    /// Make sure to call [`recv_msgs()`](Self::recv_msgs)
    ///
    /// Returns `None` if the type `T` was not registered.
    pub fn try_recv_spec<T: Any + Send + Sync>(
        &self,
        spec: CIdSpec,
    ) -> Option<impl Iterator<Item = NetMsg<T>> + '_> {
        let tid = TypeId::of::<T>();
        let mid = *self.parts.tid_map.get(&tid)?;

        Some(
            self.msg_buff[mid]
                .iter()
                .filter(move |net_msg| spec.matches(net_msg.cid))
                .map(|net_msg| net_msg.to_typed().unwrap()),
        )
    }

    /// Receives the messages from the connections. This should be done before calling `recv<T>()`.
    ///
    /// When done in a game loop, you should call `clear_msgs()`, then `recv_msgs()` before default
    /// time. This will clear the messages between frames.
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

    /// Logic for handling a new `TCP` message.
    ///
    /// Increments `count` when it successfully got a message including a disconnect message.
    ///
    /// When getting an error, this will disconnect the peer. When getting a disconnection message,
    /// this will add it to the disconnection que. Otherwise it adds it to the msg buffer.
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
                error!(
                    "TCP({}): IO error occurred while receiving data. {}",
                    cid, e
                );
                self.disconnected.push_back((cid, Status::Dropped(e)));
                true
            }
            // Got a message.
            Ok((mid, net_msg)) => {
                *count += 1;
                if mid == DISCONNECT_TYPE_MID {
                    debug!("Disconnecting peer {}", cid);
                    self.disconnected
                        .push_back((cid, Status::Disconnected(net_msg.msg)));
                    return true;
                }

                self.msg_buff[mid].push(net_msg);
                false
            }
        }
    }

    /// Logic for handling a new `UDP` message.
    ///
    /// Increments `count` when it successfully got a message
    ///
    /// When getting an error, this will log and ignore it. Otherwise it adds it to the msg buffer.
    ///
    /// returns weather the udp connection is done yielding messages.
    fn handle_udp_msg(&mut self, count: &mut u32, msg: io::Result<(MId, ErasedNetMsg)>) -> bool {
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
        self.cid_addr.keys().copied()
    }

    /// Returns whether the connection of the given [`CId`] is alive.
    pub fn alive(&self, cid: CId) -> bool {
        self.cid_addr.contains_key(&cid)
    }

    /// Returns whether a message of type `tid` can be sent.
    pub fn valid_tid(&self, tid: TypeId) -> bool {
        self.parts.valid_tid(tid)
    }

    /// The number of active connections. To ensure an accurate count, it is best to call this
    /// after calling [`handle_disconnects()`](Self::handle_disconnects).
    pub fn connection_count(&self) -> usize {
        self.cid_addr.len()
    }

    /// Gets the address of the given [`CId`].
    pub fn addr_of(&self, cid: CId) -> Option<SocketAddr> {
        self.cid_addr.get(&cid).copied()
    }

    /// Gets the address of the given [`CId`].
    pub fn cid_of(&self, addr: SocketAddr) -> Option<CId> {
        self.addr_cid.get(&addr).copied()
    }

    // Private:
    fn new_cid(&mut self) -> CId {
        self.current_cid += 1;
        self.current_cid
    }
}
