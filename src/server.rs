use crate::message_table::{MsgTableParts, CONNECTION_TYPE_MID, DISCONNECT_TYPE_MID};
use crate::net::{
    CId, DeserFn, Header, NetError, Status, Transport, MAX_MESSAGE_SIZE,
};
use crate::MId;
use hashbrown::HashMap;
use log::{debug, error, trace};
use std::any::{Any, TypeId};
use std::io;
use std::io::ErrorKind::WouldBlock;
use std::io::{Error, ErrorKind, Read};
use std::marker::PhantomData;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::time::{Duration, Instant};
use crate::tcp::TcpCon;
use crate::udp::UdpCon;

const TIMEOUT: Duration = Duration::from_millis(10_000);

/// A server.
///
/// Listens on a address and port, allowing for clients to connect.
/// Newly connected clients will be given a client ID (CId) starting
/// at `1` that is unique for the session.
///
/// This will manage multiple connections to clients. Each connection
/// will have a TCP and UDP connection on the same address and port.
pub struct Server<C, R, D>
where
    C: Any + Send + Sync,
    R: Any + Send + Sync,
    D: Any + Send + Sync,
{
    /// The current cid. incremented then assigned to new connections.
    current_cid: CId,
    /// The buffer used for sending and receiving packets
    buff: [u8; MAX_MESSAGE_SIZE],
    /// The received message buffer.
    ///
    /// Each [`MId`] has its own vector.
    msg_buff: Vec<Vec<(CId, Box<dyn Any + Send + Sync>)>>,

    /// The pending connections (Connections that are established but have
    /// not sent a connection packet yet).
    new_cons: Vec<(TcpStream, Instant)>,
    /// Disconnected connections.
    disconnected: Vec<(CId, Status<D>)>,
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
    parts: MsgTableParts<C, R, D>,
    _pd: PhantomData<(C, R, D)>,
}

impl<C, R, D> Server<C, R, D>
where
    C: Any + Send + Sync,
    R: Any + Send + Sync,
    D: Any + Send + Sync,
{
    /// Creates a new [`Server`].
    ///
    /// Creates a new [`Server`] asynchronously, passing back a oneshot receiver.
    /// This oneshot receiver allows you to wait on the connection however you like.
    pub fn new(mut listen_addr: SocketAddr, parts: MsgTableParts<C, R, D>) -> io::Result<Self> {
        let listener = TcpListener::bind(listen_addr)?;
        listen_addr = listener.local_addr().unwrap();
        listener.set_nonblocking(true)?;
        let udp = UdpCon::new(listen_addr, None)?;

        debug!("New server created at {}.", listen_addr);

        let mid_count = parts.tid_map.len();
        let mut msg_buff = Vec::with_capacity(mid_count);
        for _i in 0..mid_count {
            msg_buff.push(vec![]);
        }

        Ok(Server {
            current_cid: 0,
            buff: [0; MAX_MESSAGE_SIZE],
            msg_buff,
            new_cons: vec![],
            disconnected: vec![],
            listener,
            tcp: HashMap::new(),
            udp,
            cid_addr: Default::default(),
            addr_cid: Default::default(),
            parts,
            _pd: PhantomData,
        })
    }

    /// A function that encapsulates the sending logic for the TCP transport.
    fn send_tcp(&mut self, cid: CId, mid: MId, payload: Vec<u8>) -> io::Result<()> {
        let tcp = match self.tcp.get_mut(&cid) {
            Some(tcp) => tcp,
            None => return Err(Error::new(ErrorKind::InvalidData, "Invalid CId.")),
        };

        tcp.send(mid, payload)
    }

    /// A function that encapsulates the sending logic for the UDP transport.
    fn send_udp(&mut self, cid: CId, mid: MId, payload: Vec<u8>) -> io::Result<()> {
        let addr = match self.cid_addr.get(&cid) {
            Some(addr) => *addr,
            None => return Err(Error::new(ErrorKind::InvalidData, "Invalid CId.")),
        };

        self.udp.send(mid, addr, payload)
    }

    /// A function that encapsulates the receiving logic for the TCP transport.
    ///
    /// Any errors in receiving are returned. An error of type [`WouldBlock`] means
    /// no more packets can be yielded without blocking.
    fn recv_tcp(&mut self, cid: CId) -> io::Result<(CId, MId, Box<dyn Any + Send + Sync>)> {
        let tcp = match self.tcp.get_mut(&cid) {
            Some(tcp) => tcp,
            None => return Err(Error::new(ErrorKind::InvalidData, "Invalid CId.")),
        };

        let (mid, bytes) = tcp.recv()?;

        if !self.parts.valid_mid(mid) {
            let e_msg = format!(
                "TCP: Got a packet specifying MId {}, but the maximum MId is {}.",
                mid, self.parts.mid_count()
            );
            return Err(Error::new(ErrorKind::InvalidData, e_msg));
        }

        let deser_fn = self.parts.deser[mid];
        let msg = deser_fn(bytes)?;

        // TODO: move
        if mid == DISCONNECT_TYPE_MID {
            // Remote connection disconnected.
            debug!("TCP: Connection {} sent disconnect packet.", cid);
        }

        Ok((cid, mid, msg))
    }

    /// A function that encapsulates the receiving logic for the UDP transport.
    ///
    /// Any errors in receiving are returned. An error of type [`WouldBlock`] means
    /// no more packets can be yielded without blocking. [`InvalidData`] likely means
    /// carrier-pigeon detected bad data.
    pub fn recv_udp(&mut self) -> io::Result<(CId, MId, Box<dyn Any + Send + Sync>)> {
        let (from, mid, bytes) = self.udp.recv()?;

        if !self.parts.valid_mid(mid) {
            let e_msg = format!(
                "TCP: Got a packet specifying MId {}, but the maximum MId is {}.",
                mid, self.parts.mid_count()
            );
            return Err(Error::new(ErrorKind::InvalidData, e_msg));
        }

        let deser_fn = self.parts.deser[mid];
        let msg = deser_fn(bytes)?;

        let cid = match self.addr_cid.get(&from) {
            Some(&cid) if self.alive(cid) => cid,
            _ => return Err(Error::new(
                ErrorKind::Other,
                "Received data from a address that is not connected.",
            )),
        };

        Ok((cid, mid, msg))
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
        self.send_to(cid, discon_msg)?;
        // Close the TcpCon
        self.tcp.get_mut(&cid).unwrap().close()?;
        // No shutdown method on udp.
        self.disconnected.push((cid, Status::Closed));
        Ok(())
    }

    /// Handle the new connection attempts by calling the given hook.
    pub fn handle_new_cons(&mut self, hook: &mut dyn FnMut(C) -> (bool, R)) -> u32 {
        // Start waiting on the connection packets for new connections.
        // TODO: add cap to connections that we are handeling.
        // TODO: add a timeout to the functions.
        while let Ok((new_con, _addr)) = self.listener.accept() {
            debug!("New connection attempt.");
            new_con.set_nonblocking(true).unwrap();
            self.new_cons.push((new_con, Instant::now()));
        }

        let mut accept_count = 0;
        // Handle the new connections.
        let mut remove = vec![];
        let deser_fn = self.parts.deser[CONNECTION_TYPE_MID];
        for (idx, (con, time)) in self.new_cons.iter_mut().enumerate() {
            match Self::handle_new_con(&mut self.buff, deser_fn, con, time) {
                Ok(Some(c)) => {
                    let (acc, r) = hook(c);
                    if acc {
                        debug!("Accepted new connection at {}.", con.peer_addr().unwrap());
                        accept_count += 1;
                    }
                    remove.push((idx, Some(r)));
                }
                Ok(None) => {}
                Err(e) if e.kind() == ErrorKind::WouldBlock => {}
                Err(e) => {
                    error!("Error in handling a pending connection. {}", e);
                    remove.push((idx, None));
                }
            }
        }
        for (idx, resp) in remove {
            let (stream, _) = self.new_cons.remove(idx);
            // If `resp` is Some, the connection was accepted and
            // we need to send the response packet.
            // TODO: this process might be able to be inlined if `send()` calls take immutable refs to self
            if let Some(r) = resp {
                let con = TcpCon::from_stream(stream);
                let cid = self.add_tcp_con(con);
                let _ = self.send_to(cid, &r);
            }
        }

        accept_count
    }

    /// Handles a new connection by trying to read the connection packet.
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
    fn handle_new_con(
        buff: &mut [u8],
        deser_fn: DeserFn,
        con: &mut TcpStream,
        time: &Instant,
    ) -> io::Result<Option<C>> {
        // TODO: make the timeout configurable.
        if time.elapsed() > TIMEOUT {
            return Err(Error::new(
                ErrorKind::TimedOut,
                "The new connection did not send a connection packet in time.",
            ));
        }

        // Peak the first 4 bytes for the header.
        if con.peek(&mut buff[..4])? != 4 {
            return Ok(None);
        }
        let h = Header::from_be_bytes(&buff[..4]);

        if h.mid != CONNECTION_TYPE_MID {
            let msg = format!("Expected MId {}, got MId {}.", CONNECTION_TYPE_MID, h.mid);
            return Err(Error::new(ErrorKind::InvalidData, msg));
        }

        con.read_exact(&mut buff[..h.len + 4])?;

        let con_msg = deser_fn(&buff[4..h.len + 4]).map_err(|o| {
            Error::new(
                ErrorKind::InvalidData,
                format!(
                    "Encountered a deserialization error when handling a new connection. {}",
                    o
                ),
            )
        })?;

        let con_msg = *con_msg.downcast::<C>().unwrap();
        Ok(Some(con_msg))
    }

    /// Handles the disconnect events.
    pub fn handle_disconnects(&mut self, hook: &mut dyn FnMut(CId, Status<D>)) -> u32 {
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

    /// Sends a message to the [`CId`] `cid`.
    ///
    /// ## Errors
    /// If the client isn't connected to another computer,
    /// This will return [`Error::NotConnected`].
    /// If the message type isn't registered, this will return
    /// [`Error::TypeNotRegistered`]. If the msg fails to be
    /// serialized this will return [`Error::SerdeError`].
    pub fn send_to<T: Any + Send + Sync>(&mut self, cid: CId, msg: &T) -> io::Result<()> {
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

        debug!(
            "Sending message of MId {}, len {}, to CId {}",
            mid,
            b.len(),
            cid
        );
        match transport {
            Transport::TCP => self.send_tcp(cid, mid, b),
            Transport::UDP => self.send_udp(cid, mid, b),
        }?;

        Ok(())
    }

    /// Broadcasts a message to all connected clients.
    pub fn broadcast<T: Any + Send + Sync>(&mut self, msg: &T) -> io::Result<()> {
        for cid in self.cids().collect::<Vec<_>>() {
            self.send_to(cid, msg)?;
        }
        Ok(())
    }

    /// Broadcasts a message to all connected clients except the [`CId`] `cid`.
    pub fn broadcast_except<T: Any + Send + Sync>(&mut self, msg: &T, cid: CId) -> io::Result<()> {
        for cid in self
            .cid_addr
            .iter()
            .map(|t| *t.0)
            .filter(|o_cid| *o_cid != cid)
            .collect::<Vec<_>>()
        {
            self.send_to(cid, msg)?;
        }
        Ok(())
    }

    /// Gets an iterator for the messages of type T.
    /// Make sure to call [`recv_msgs()`](Self::recv_msgs)
    ///
    /// Returns None if the type T was not registered.
    pub fn recv<T: Any + Send + Sync>(&self) -> Option<impl Iterator<Item = (CId, &T)>> {
        let tid = TypeId::of::<T>();
        let mid = *self.parts.tid_map.get(&tid)?;

        Some(
            self.msg_buff[mid]
                .iter()
                .map(|(cid, m)| (*cid, (*m).downcast_ref::<T>().unwrap())),
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
        // TODO: dont collect after interior mutability send impl.
        for cid in self.cids().collect::<Vec<_>>() {
            loop {
                println!("looping");

                let msg = self.recv_tcp(cid);
                if self.handle_tcp(&mut i, cid, msg) {
                    // Done yielding messages.
                    break;
                }

            }
        }

        // UDP
        loop {
            let msg = self.recv_udp();
            if self.handle_udp(&mut i, msg) {
                // Done yielding messages.
                break;
            }
        }
        i




        // // TCP
        // 'cid_loop: for cid in self.cids().collect::<Vec<_>>() {
        //     // Loop through all the messages in the tcp connection.
        //     loop {
        //         let recv = self.recv_tcp(cid);
        //         match recv {
        //             // End of data.
        //             Err(e) if e.kind() == WouldBlock => break,
        //             // Other error occurred.
        //             Err(e) => {
        //                 error!("TCP: An error occurred while receiving a message. {}", e);
        //                 self.disconnected.push((cid, Status::Dropped(e)));
        //                 break;
        //             }
        //             // Got a message.
        //             Ok((mid, msg)) => {
        //                 i += 1;
        //                 if mid == DISCONNECT_TYPE_MID {
        //                     debug!("Disconnecting peer {}", cid);
        //                     let discon = msg.downcast::<D>().unwrap();
        //                     self.disconnected.push((cid, Status::Disconnected(*discon)));
        //                     continue 'cid_loop;
        //                 }
        //
        //                 self.msg_buff[mid].push((cid, msg));
        //             }
        //         }
        //     }
        // }
        //
        // // UDP
        // loop {
        //     let recv = self.recv_udp();
        //     match recv {
        //         // End of data.
        //         Err(e) if e.kind() == WouldBlock => break,
        //         // Other error occurred.
        //         Err(e) => {
        //             error!("UDP: An error occurred while receiving a message. {}", e);
        //             break;
        //         }
        //         // Got a message.
        //         Ok((cid, mid, msg)) => {
        //             self.msg_buff[mid].push((cid, msg));
        //             i += 1;
        //         }
        //     }
        // }



        // let deser_fn = match self.parts.deser.get(header.mid) {
        //             Some(d) => *d,
        //             None => {
        //                 let msg = format!(
        //                     "Invalid MId {} read from peer. Max MId: {}.",
        //                     header.mid,
        //                     self.parts.mid_count() - 1
        //                 );
        //                 return Err(Error::new(ErrorKind::InvalidData, msg));
        //             }
        //         };
        //
        //         let msg = deser_fn(&self.buff[..header.len]).map_err(|_| {
        //             Error::new(
        //                 ErrorKind::InvalidData,
        //                 "Got error when deserializing data from peer.",
        //             )
        //         })?;
        //
        //         if header.mid == DISCONNECT_TYPE_MID {
        //             // Remote connection disconnected.
        //             debug!("TCP: Remote computer sent disconnect packet.");
        //         }
    }

    /// Logic for handling a new TCP message.
    ///
    /// Increments `count` when it successfully got a packet including a disconnect packet.
    ///
    /// When getting an error, this will disconnect the peer.
    /// When getting a disconnection packet, this will add it to the disconnection que.
    /// Otherwise it adds it to the msg buffer.
    ///
    /// returns weather the tcp connection is done yielding messages.
    fn handle_tcp(&mut self, count: &mut u32, cid: CId, msg: io::Result<(CId, MId, Box<dyn Any + Send + Sync>)>) -> bool {
        match msg {
            Err(e) if e.kind() == WouldBlock => true,
            // Other error occurred.
            Err(e) => {
                error!("TCP: An error occurred while receiving a message. {}", e);
                self.disconnected.push((cid, Status::Dropped(e)));
                true
            }
            // Got a message.
            Ok((cid, mid, msg)) => {
                *count += 1;
                if mid == DISCONNECT_TYPE_MID {
                    debug!("Disconnecting peer {}", cid);
                    let discon = msg.downcast::<D>().unwrap();
                    self.disconnected.push((cid, Status::Disconnected(*discon)));
                    return true;
                }

                self.msg_buff[mid].push((cid, msg));
                false
            }
        }
    }

    /// Logic for handling a new UDP message.
    ///
    /// Increments `count` when it successfully got a packet
    ///
    /// When getting an error, this will log and ignore it.
    /// Otherwise it adds it to the msg buffer.
    ///
    /// returns weather the udp connection is done yielding messages.
    fn handle_udp(&mut self, count: &mut u32, msg: io::Result<(CId, MId, Box<dyn Any + Send + Sync>)>) -> bool {
        match msg {
            Err(e) if e.kind() == WouldBlock => true,
            // Other error occurred.
            Err(e) => {
                error!("UDP: An error occurred while receiving a message. {}", e);
                true
            }
            // Got a message.
            Ok((cid, mid, msg)) => {
                *count += 1;
                self.msg_buff[mid].push((cid, msg));
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

    pub fn valid_tid(&self, tid: TypeId) -> bool {
        self.parts.tid_map.contains_key(&tid)
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
    /// Adds a TCP connection.
    fn add_tcp_con(&mut self, con: TcpCon) -> CId {
        self.current_cid += 1;
        let cid = self.current_cid;
        let peer_addr = con.peer_addr();
        self.tcp.insert(cid, con);
        self.addr_cid.insert(peer_addr, cid);
        self.cid_addr.insert(cid, peer_addr);
        self.current_cid
    }

    /// Removes a TCP connection.
    fn rm_tcp_con(&mut self, cid: CId) -> Result<(), NetError> {
        self.tcp.remove(&cid).ok_or(NetError::InvalidCId)?;
        let addr = self.cid_addr.remove(&cid).unwrap();
        self.addr_cid.remove(&addr);
        Ok(())
    }
}
