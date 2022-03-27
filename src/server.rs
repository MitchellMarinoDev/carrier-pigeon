use crate::message_table::{MsgTableParts, CONNECTION_TYPE_MID, RESPONSE_TYPE_MID};
use crate::net::{CId, DeserFn, NetError, SerFn, Transport};
use crate::tcp::TcpCon;
use crate::udp::UdpCon;
use crate::{MId, NetMsg};
use hashbrown::HashMap;
use log::{debug, error, trace};
use std::any::TypeId;
use std::io;
use std::marker::PhantomData;
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tokio::runtime::Handle;
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

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
    C: NetMsg,
    R: NetMsg,
    D: NetMsg,
{
    /// The received message buffer.
    ///
    /// Each [`MId`] has its own vector.
    msg_buff: Vec<Vec<(CId, Box<dyn NetMsg>)>>,

    /// New connection attempts.
    new_cons: Receiver<(TcpCon<D>, Box<dyn NetMsg>)>,

    /// The address that is being listened on.
    listen_addr: SocketAddr,
    /// The listening task.
    listen_task: JoinHandle<io::Result<()>>,
    /// The TCP connection for this client.
    tcp: HashMap<CId, TcpCon<D>>,
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

    /// The map from [`TypeId`] to [`MId`].
    tid_map: HashMap<TypeId, MId>,
    /// The transport that is associated with each MId.
    transports: Vec<Transport>,
    _pd: PhantomData<(C, R, D)>,
}

impl<C, R, D> Server<C, R, D>
where
    C: NetMsg,
    R: NetMsg,
    D: NetMsg,
{
    /// Creates a new [`Server`].
    ///
    /// Creates a new [`Server`] asynchronously, passing back a oneshot receiver.
    /// This oneshot receiver allows you to wait on the connection however you like.
    pub fn new(
        listen_addr: SocketAddr,
        parts: MsgTableParts<C, R, D>,
        rt: Handle,
    ) -> oneshot::Receiver<io::Result<Self>> {
        let (server_tx, server_rx) = oneshot::channel();

        let rt0 = rt.clone();
        rt0.spawn(async move {
            let _ = server_tx.send(Self::new_async(listen_addr, parts, rt).await);
        });

        server_rx
    }

    /// Creates a new [`Server`] asynchronously.
    pub async fn new_async(
        mut listen_addr: SocketAddr,
        parts: MsgTableParts<C, R, D>,
        rt: Handle,
    ) -> io::Result<Self> {
        let (new_cons_tx, new_cons_rx) = channel(2);

        let listener = TcpListener::bind(listen_addr).await?;
        listen_addr = listener.local_addr().unwrap();

        let listen_task = rt.spawn(listen(
            listener,
            new_cons_tx,
            parts.deser.clone(),
            parts.ser.clone(),
            rt.clone(),
        ));

        let udp = UdpCon::new(0, listen_addr, None, parts.deser, parts.ser, rt).await?;

        debug!("New server created at {}.", listen_addr);

        let mid_count = parts.tid_map.len();
        let mut msg_buff = Vec::with_capacity(mid_count);
        for _i in 0..mid_count {
            msg_buff.push(vec![]);
        }

        Ok(Server {
            msg_buff,
            new_cons: new_cons_rx,
            listen_addr,
            listen_task,
            tcp: HashMap::new(),
            udp,
            cid_addr: Default::default(),
            addr_cid: Default::default(),
            tid_map: parts.tid_map,
            transports: parts.transports,
            _pd: PhantomData,
        })
    }

    /// Disconnects from the given `cid`. You should always disconnect
    /// all clients before dropping the server to let the clients know
    /// that you intentionally disconnected. The `discon_msg` allows you
    /// to give a reason for the disconnect.
    pub fn disconnect<T: NetMsg>(&mut self, discon_msg: &T, cid: CId) -> Result<(), NetError> {
        debug!("Disconnecting CId {}", cid);
        self.send_to(cid, discon_msg)?;
        self.rm_tcp_con(cid)?;
        Ok(())
    }

    /// Handle the new connection attempts by calling the given hook.
    pub fn handle_new_cons(&mut self, hook: &mut dyn FnMut(C) -> (bool, R)) -> u32 {
        let mut i = 0;
        while let Ok((new_con, msg)) = self.new_cons.try_recv() {
            // The type of `msg` is already checked and safe to downcast.
            let msg = msg.downcast().map_err(|_| "").unwrap();
            trace!("Handling new connection {}.", new_con.cid());
            let (accept, response_msg) = (*hook)(*msg);
            new_con.send(RESPONSE_TYPE_MID, &response_msg).unwrap();
            if accept {
                trace!("Connection {} accepted.", new_con.cid());
                self.add_tcp_con(new_con);
            } else {
                trace!("Connection {} rejected.", new_con.cid());
            }
            i += 1;
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
    pub fn send_to<T: NetMsg>(&self, cid: CId, msg: &T) -> Result<(), NetError> {
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
    pub fn broadcast<T: NetMsg>(&self, msg: &T) -> Result<(), NetError> {
        for cid in self.cid_addr.iter().map(|t| t.0) {
            self.send_to(*cid, msg)?;
        }
        Ok(())
    }

    /// Broadcasts a message to all connected clients except the [`CId`] `cid`.
    pub fn broadcast_except<T: NetMsg>(&self, msg: &T, cid: CId) -> Result<(), NetError> {
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
    pub fn recv<T: NetMsg>(&self) -> Option<impl Iterator<Item = (CId, &T)>> {
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
                i += 1;
                trace!("getting tcp msg with mid: {}", mid);
                self.msg_buff[mid].push((*cid, msg));
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

    /// Gets the status of the send task of the given
    /// `transport`.
    ///
    /// [`None`] indicates that the send task has not
    /// completed yet. Otherwise a reference is given
    /// to the [`Result`] of the task.
    pub fn get_send_status(&mut self, transport: Transport, cid: CId) -> Option<&io::Result<()>> {
        match transport {
            Transport::UDP => self.udp.get_send_status(),
            Transport::TCP => self.tcp.get_mut(&cid)?.get_send_status(),
        }
    }

    /// Gets the status of the receive task of the given
    /// `transport`.
    ///
    /// [`None`] indicates that the receive task has not
    /// completed yet. Otherwise a reference is given
    /// to the [`Result`] of the task.
    pub fn get_udp_recv_status(&mut self) -> Option<&io::Result<()>> {
        self.udp.get_recv_status()
    }

    /// Gets the status of the receive task of the given
    /// `transport`.
    ///
    /// [`None`] indicates that the receive task has not
    /// completed yet. Otherwise a reference is given
    /// to the [`Result`] of the task.
    pub fn get_tcp_recv_status(&mut self, cid: CId) -> Option<&io::Result<D>> {
        self.tcp.get_mut(&cid)?.get_recv_status()
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
async fn listen<D: NetMsg>(
    listener: TcpListener,
    new_cons: Sender<(TcpCon<D>, Box<dyn NetMsg>)>,
    deser: Vec<DeserFn>,
    ser: Vec<SerFn>,
    rt: Handle,
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
async fn establish_con<D: NetMsg>(
    mut con: TcpCon<D>,
    new_cons: Sender<(TcpCon<D>, Box<dyn NetMsg>)>,
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
    C: NetMsg,
    R: NetMsg,
    D: NetMsg,
{
    fn drop(&mut self) {
        self.listen_task.abort();
    }
}
