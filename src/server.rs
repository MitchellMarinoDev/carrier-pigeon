use crate::message_table::{MsgTableParts, CONNECTION_TYPE_MID, RESPONSE_TYPE_MID};
use crate::net::{CId, DeserFn, Header, MAX_PACKET_SIZE, NetError, SerFn, Status, Transport};
use crate::tcp::TcpCon;
use crate::MId;
use hashbrown::HashMap;
use log::{debug, error, trace};
use std::any::{Any, TypeId};
use std::fmt::{Display, Formatter};
use std::io;
use std::io::{ErrorKind, Read};
use std::marker::PhantomData;
use std::net::{SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::time::Duration;
use crossbeam_channel::internal::SelectHandle;
use crossbeam_channel::{Receiver, Sender};

const TIMEOUT: Duration = Duration::from_millis(10_000);

/// A server.
///
/// Listens on a address and port, allowing for clients to connect.
/// Newly connected clients will be given a client ID (CId) starting
/// at `1` that is unique for the session.
///
/// This will manage multiple connections to clients. Each connection
/// will have a TCP and UDP connection on the same address and port.
///
///
pub struct Server<C, R, D>
where
    C: Any + Send + Sync,
    R: Any + Send + Sync,
    D: Any + Send + Sync,
{
    /// The received message buffer.
    ///
    /// Each [`MId`] has its own vector.
    msg_buff: Vec<Vec<(CId, Box<dyn Any + Send + Sync>)>>,

    /// The sender that corresponds with the `new_cons` receiver.
    new_cons_sender: Sender<(TcpStream, C)>,
    /// The receiver of new connections with their connection packets.
    new_cons: Receiver<(TcpStream, C)>,
    /// The listener for new connections.
    listener: TcpListener,
    /// The TCP connection for this client.
    tcp: HashMap<CId, TcpStream>,
    /// The UDP connection for this client.
    udp: UdpSocket,

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
    pub fn new(
        mut listen_addr: SocketAddr,
        parts: MsgTableParts<C, R, D>,
    ) -> io::Result<Self> {
        let listener = TcpListener::bind(listen_addr)?;
        listener.set_nonblocking(true)?;
        listen_addr = listener.local_addr().unwrap();
        let udp = UdpSocket::bind(listen_addr).unwrap();

        debug!("New server created at {}.", listen_addr);

        let mid_count = parts.tid_map.len();
        let mut msg_buff = Vec::with_capacity(mid_count);
        for _i in 0..mid_count {
            msg_buff.push(vec![]);
        }

        let new_cons_channel = crossbeam_channel::bounded(4);

        Ok(Server {
            msg_buff,
            new_cons_sender: new_cons_channel.0,
            new_cons: new_cons_channel.1,
            listener,
            tcp: HashMap::new(),
            udp,
            cid_addr: Default::default(),
            addr_cid: Default::default(),
            parts,
            _pd: PhantomData,
        })
    }

    /// Disconnects from the given `cid`. You should always disconnect
    /// all clients before dropping the server to let the clients know
    /// that you intentionally disconnected. The `discon_msg` allows you
    /// to give a reason for the disconnect.
    pub fn disconnect<T: Any + Send + Sync>(&mut self, discon_msg: &T, cid: CId) -> Result<(), NetError> {
        debug!("Disconnecting CId {}", cid);
        self.send_to(cid, discon_msg)?;
        self.rm_tcp_con(cid)?;
        Ok(())
    }

    /// Handle the new connection attempts by calling the given hook.
    pub fn handle_new_cons(&mut self, hook: &mut dyn FnMut(C) -> (bool, R)) -> u32 {
        // Start waiting on the connection packets for new connections.
        while let Ok((mut new_con, addr)) = self.listener.accept() {
            // Spawn a thread to handle receiving the connection packet of the connection.
            let sender = self.new_cons_sender.clone();
            let deser_fn = self.parts.deser[CONNECTION_TYPE_MID];
            std::thread::spawn(move || {

                let mut buff = [0; MAX_PACKET_SIZE];
                new_con.set_read_timeout(Some(TIMEOUT))?;
                new_con.read_exact(&mut buff[..4])?;
                let header = Header::from_be_bytes(&buff[..4]);
                if header.mid != CONNECTION_TYPE_MID {
                    let msg =
                        format!(
                            "Expected the first packet to be of type Connection Packet (MId: {}) got MId: {}",
                            CONNECTION_TYPE_MID, header.mid
                        );
                    return Err(io::Error::new(ErrorKind::InvalidData, msg));
                }
                new_con.read_exact(&mut buff[..header.len])?;
                new_con.set_nonblocking(true)?;

                let con_msg = deser_fn(&buff[..header.len])?.downcast::<C>().unwrap();

                let _ = sender.send((new_con, *con_msg));
                Ok(())
            });
        }

        let mut i = 0;
        while let Ok((new_con, c)) = self.new_cons.try_recv() {

        }
        i
    }

    /// Handles the disconnect events.
    ///
    /// The `discon` hook is fired for the clients that gracefully disconnect.
    ///
    /// The `drop` hook is called for the clients that are dropped (disconnects
    /// without sending a disconnect packet), or in any other circumstance where
    /// an IO error occurs on the send or receive task.
    pub fn handle_disconnects(
        &mut self,
        discon: &mut dyn FnMut(CId, &D),
        drop: &mut dyn FnMut(CId, &io::Error),
    ) -> (u32, u32) {
        let mut cids_to_rm = vec![];

        // disconnect, drop counts.
        let mut i = (0, 0);

        for (cid, con) in self.tcp.iter_mut() {
            con.update_status();

            if con.open() {
                continue;
            }
            println!("CId: {} dead", cid);

            if let Some(result) = con.get_recv_status() {
                // Not open due to a receive error.
                match result {
                    Ok(discon_msg) => {
                        i.0 += 1; // inc discon count
                        discon(*cid, &*discon_msg)
                    }
                    Err(e) => {
                        i.1 += 1; // inc drop count
                        drop(*cid, e)
                    }
                }
            } else if let Some(Err(e)) = con.get_send_status() {
                // Not open due to a send error.
                i.1 += 1; // inc drop count
                drop(*cid, e);
            } else {
                error!("The connection closed because disconnect() was called.");
            }

            cids_to_rm.push(*cid);
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
    pub fn send_to<T: Any + Send + Sync>(&self, cid: CId, msg: &T) -> Result<(), NetError> {
        let addr = match self.cid_addr.get(&cid) {
            Some(addr) => *addr,
            None => return Err(NetError::InvalidCId),
        };

        let tid = TypeId::of::<T>();
        if !self.tid_map.contains_key(&tid) {
            return Err(NetError::TypeNotRegistered);
        }
        let mid = self.tid_map[&tid];
        let transport = self.transports[mid];

        match transport {
            Transport::TCP => self.tcp[&cid].send(mid, msg),
            Transport::UDP => self.udp.send_to(mid, msg, addr),
        }?;

        Ok(())
    }

    /// Broadcasts a message to all connected clients.
    pub fn broadcast<T: Any + Send + Sync>(&self, msg: &T) -> Result<(), NetError> {
        for cid in self.cid_addr.iter().map(|t| t.0) {
            self.send_to(*cid, msg)?;
        }
        Ok(())
    }

    /// Broadcasts a message to all connected clients except the [`CId`] `cid`.
    pub fn broadcast_except<T: Any + Send + Sync>(&self, msg: &T, cid: CId) -> Result<(), NetError> {
        for cid in self
            .cid_addr
            .iter()
            .map(|t| *t.0)
            .filter(|o_cid| *o_cid != cid)
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
        let mid = *self.tid_map.get(&tid)?;

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
        for (cid, tcp) in self.tcp.iter_mut() {
            while let Some((mid, msg)) = tcp.try_recv() {
                trace!("getting tcp msg with mid: {}", mid);
                self.msg_buff[mid].push((*cid, msg));
                i += 1;
            }
        }

        while let Some((mid, msg, addr)) = self.udp.try_recv() {
            if let Some(cid) = self.addr_cid.get(&addr) {
                i += 1;
                trace!("Getting udp msg with mid: {}", mid);
                self.msg_buff[mid].push((*cid, msg));
            } else {
                debug!("Getting udp msg from invalid addr {}", addr);
            }
        }
        i
    }

    /// Clears messages from the buffer.
    pub fn clear_msgs(&mut self) {
        for buff in self.msg_buff.iter_mut() {
            buff.clear();
        }
    }

    /// Gets the address that the server is listening on.
    pub fn listen_addr(&self) -> SocketAddr {
        self.listen_addr
    }

    /// An iterator of the [`CId`]s.
    pub fn cids(&self) -> impl Iterator<Item = CId> + '_ {
        self.cid_addr.keys().map(|cid| *cid)
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
    fn add_tcp_con(&mut self, con: TcpCon<D>) {
        let cid = con.cid();
        let peer_addr = con.peer_addr();
        self.tcp.insert(cid, con);
        self.addr_cid.insert(peer_addr, cid);
        self.cid_addr.insert(cid, peer_addr);
    }

    /// Removes a TCP connection.
    fn rm_tcp_con(&mut self, cid: CId) -> Result<(), NetError> {
        self.tcp.remove(&cid).ok_or(NetError::InvalidCId)?;
        let addr = self.cid_addr.remove(&cid).unwrap();
        self.addr_cid.remove(&addr);
        Ok(())
    }
}

/// Note: Will **not** stop automatically and needs to be aborted.
async fn listen<D: Any + Send + Sync>(
    listener: TcpListener,
    new_cons: Sender<(TcpCon<D>, Box<dyn Any + Send + Sync>)>,
    deser: Vec<DeserFn>,
    ser: Vec<SerFn>,
) -> io::Result<()> {
    let mut current_cid: CId = 1;

    loop {
        let (stream, _addr) = listener.accept().await?;

        // Get a new CId for the new connection
        let cid = current_cid;
        current_cid += 1;

        // Turn the socket into a [`TcpCon`]
        let con = TcpCon::from_stream(cid, stream, deser.clone(), ser.clone(), rt.clone());

        // Spawn a new task for handling the new connection
        rt.spawn(establish_con(con, new_cons.clone()));
    }
}

/// A branch task, spawned for each new connection from the listening task.
async fn establish_con<D: Any + Send + Sync>(
    mut con: TcpCon<D>,
    new_cons: Sender<(TcpCon<D>, Box<dyn Any + Send + Sync>)>,
) {
    let cid = con.cid();

    let (mid, msg) = match con.recv().await {
        Some(msg) => msg,
        None => {
            error!("TCP_C({}): New connection didn't get a single packet.", cid,);
            return;
        }
    };

    if mid != CONNECTION_TYPE_MID {
        error!(
            "TCP_C({}): First packet did not have MId {}; It was {}. Dropping connection.",
            cid, CONNECTION_TYPE_MID, mid
        );
        return;
    }

    // Pass back the connection attempt.
    if let Err(_e) = new_cons.send((con, msg)).await {
        error!(
            "TCP_C({}): Error while sending a new connection back to the server. \
            This could be due to a long lived connection attempt, or more likely, \
            The listening task did not get aborted when the server closed and is \
            listening longer than It should.",
            cid
        );
        return;
    }
    debug!("TCP_C({}): New connection established.", cid);
}

impl<C, R, D> Drop for Server<C, R, D>
where
    C: Any + Send + Sync,
    R: Any + Send + Sync,
    D: Any + Send + Sync,
{
    fn drop(&mut self) {
        self.listen_task.abort();
    }
}

#[derive(Debug)]
pub struct PendingServer<C, R, D>
    where
        C: Any + Send + Sync,
        R: Any + Send + Sync,
        D: Any + Send + Sync,
{
    channel: Receiver<io::Result<Server<C, R, D>>>,
}

// Impl display so that `get()` can be unwrapped.
impl<C, R, D> Display for PendingServer<C, R, D>
    where
        C: Any + Send + Sync,
        R: Any + Send + Sync,
        D: Any + Send + Sync,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Pending client connection ({}).", if self.done() { "done" } else { "not done" })
    }
}

impl<C, R, D> PendingServer<C, R, D>
    where
        C: Any + Send + Sync,
        R: Any + Send + Sync,
        D: Any + Send + Sync,
{
    /// Returns whether the client is finished connecting.
    pub fn done(&self) -> bool {
        self.channel.is_ready()
    }

    /// Gets the client. This will yield a value if [`done()`](Self::done)
    /// returned `true`.
    pub fn get(self) -> Result<io::Result<(Server<C, R, D>, R)>, Self> {
        if self.done() {
            Ok(self.channel.recv().unwrap())
        } else {
            Err(self)
        }
    }

    /// Blocks until the client is ready.
    pub fn block(self) -> io::Result<(Server<C, R, D>, R)> {
        self.channel.recv().unwrap()
    }
}
