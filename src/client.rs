use crate::message_table::{MsgTableParts, DISCONNECT_TYPE_MID, RESPONSE_TYPE_MID};
use crate::net::{Status, Transport};
use crate::tcp::TcpCon;
use crate::udp::UdpCon;
use crate::MId;
use crossbeam_channel::internal::SelectHandle;
use crossbeam_channel::Receiver;
use log::{debug, error, trace};
use std::any::{Any, TypeId};
use std::fmt::{Debug, Display, Formatter};
use std::io;
use std::io::{Error, ErrorKind};
use std::marker::PhantomData;
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

/// A Client connection.
///
/// This can only connect to 1 server.
///
/// Contains a TCP and UDP connection to the server.
pub struct Client<C, R, D>
where
    C: Any + Send + Sync,
    R: Any + Send + Sync,
    D: Any + Send + Sync,
{
    /// The status of the client
    status: Status<D>,
    /// The received message buffer.
    ///
    /// Each [`MId`] has its own vector.
    msg_buff: Vec<Vec<Box<dyn Any + Send + Sync>>>,

    /// The TCP connection for this client.
    tcp: TcpCon,
    /// The UDP connection for this client.
    udp: UdpCon,

    /// The [`MsgTableParts`] to use for sending messages.
    parts: MsgTableParts<C, R, D>,
    _pd: PhantomData<(C, R, D)>,
}

impl<C, R, D> Client<C, R, D>
where
    C: Any + Send + Sync,
    R: Any + Send + Sync,
    D: Any + Send + Sync,
{
    /// Creates a new [`Client`].
    ///
    /// Creates a new [`Client`] on another thread, passing back a [`PendingClient`].
    /// This [`PendingClient`] allows you to wait for the client to send the connection
    /// packet, and the server to send back the response packet.
    pub fn new(
        peer: SocketAddr,
        parts: MsgTableParts<C, R, D>,
        con_msg: C,
    ) -> PendingClient<C, R, D> {
        let (client_tx, client_rx) = crossbeam_channel::bounded(1);

        std::thread::spawn(move || client_tx.send(Self::new_blocking(peer, parts, con_msg)));

        PendingClient { channel: client_rx }
    }

    /// Creates a new [`Client`] blocking.
    fn new_blocking(
        peer: SocketAddr,
        parts: MsgTableParts<C, R, D>,
        con_msg: C,
    ) -> io::Result<(Self, R)> {
        debug!("Attempting to create a client connection to {}", peer);
        // TCP & UDP Connections.
        let tcp = TcpStream::connect(peer)?;
        tcp.set_read_timeout(Some(Duration::from_millis(10_000)))?;
        let tcp = TcpCon::from_stream(tcp);
        let local_addr = tcp.local_addr().unwrap();
        trace!(
            "TcpStream established from {} to {}",
            local_addr,
            tcp.peer_addr().unwrap()
        );
        let udp = UdpCon::new(local_addr, Some(peer))?;
        trace!(
            "UdpSocket connected from {} to {}",
            udp.local_addr().unwrap(),
            udp.peer_addr().unwrap()
        );

        let mut msg_buff = Vec::with_capacity(parts.mid_count());
        for _ in 0..parts.mid_count() {
            msg_buff.push(vec![]);
        }

        let mut client = Client {
            status: Status::Connected,
            msg_buff,
            tcp,
            udp,
            parts,
            _pd: PhantomData,
        };

        // Send connection packet
        client.send(&con_msg)?;
        trace!("Client connection message sent. Awaiting response...");

        // Get response packet.
        let (r_mid, response) = client.recv_tcp()?;
        trace!("Got response packet from the server.");

        if r_mid != RESPONSE_TYPE_MID {
            let msg = format!(
                "Client: First received packet was MId: {} not MId: {} (Response packet)",
                r_mid, RESPONSE_TYPE_MID
            );
            error!("{}", msg);
            return Err(Error::new(ErrorKind::InvalidData, msg));
        }
        let response = *response.downcast::<R>().map_err(|_| "").unwrap();

        debug!(
            "New Client created at {}, to {}.",
            client.tcp.local_addr().unwrap(),
            client.tcp.peer_addr().unwrap(),
        );

        client.tcp.set_nonblocking(true)?;
        client.udp.set_nonblocking(true)?;

        Ok((client, response))
    }

    /// A function that encapsulates the sending logic for the TCP transport.
    fn send_tcp(&mut self, mid: MId, payload: &[u8]) -> io::Result<()> {
        self.tcp.send(mid, payload)
    }

    /// A function that encapsulates the sending logic for the UDP transport.
    fn send_udp(&mut self, mid: MId, payload: &[u8]) -> io::Result<()> {
        self.udp.send(mid, payload)
    }

    /// A function that encapsulates the receiving logic for the TCP transport.
    ///
    /// Any errors in receiving are returned. An error of type [`WouldBlock`] means
    /// no more packets can be yielded without blocking.
    fn recv_tcp(&mut self) -> io::Result<(MId, Box<dyn Any + Send + Sync>)> {
        let (mid, bytes) = self.tcp.recv()?;

        if !self.parts.valid_mid(mid) {
            let e_msg = format!(
                "TCP: Got a packet specifying MId {}, but the maximum MId is {}.",
                mid,
                self.parts.mid_count()
            );
            return Err(Error::new(ErrorKind::InvalidData, e_msg));
        }

        let deser_fn = self.parts.deser[mid];
        let msg = deser_fn(bytes)?;

        Ok((mid, msg))
    }

    /// A function that encapsulates the receiving logic for the UDP transport.
    ///
    /// Any errors in receiving are returned. An error of type [`WouldBlock`] means
    /// no more packets can be yielded without blocking. [`InvalidData`] likely means
    /// carrier-pigeon detected bad data.
    pub fn recv_udp(&mut self) -> io::Result<(MId, Box<dyn Any + Send + Sync>)> {
        let (mid, bytes) = self.udp.recv()?;

        if !self.parts.valid_mid(mid) {
            let e_msg = format!(
                "TCP: Got a packet specifying MId {}, but the maximum MId is {}.",
                mid,
                self.parts.mid_count()
            );
            return Err(Error::new(ErrorKind::InvalidData, e_msg));
        }

        let deser_fn = self.parts.deser[mid];
        let msg = deser_fn(bytes)?;

        Ok((mid, msg))
    }

    /// Disconnects from the server. You should ***always*** call this
    /// method before dropping the client to let the server know that
    /// you intentionally disconnected. The `discon_msg` allows you to
    /// give a reason for the disconnect.
    pub fn disconnect(&mut self, discon_msg: &D) -> io::Result<()> {
        debug!("Disconnecting client.");
        self.send(discon_msg)?;
        self.tcp.close()?;
        // No shutdown method on udp.
        self.status = Status::Closed;
        Ok(())
    }

    /// Gets the status of the connection.
    pub fn status(&self) -> &Status<D> {
        &self.status
    }

    /// Returns whether the connection is open.
    pub fn open(&self) -> bool {
        self.status().connected()
    }

    /// Sends a message to the connected computer.
    /// ### Errors
    /// If the client isn't connected to another computer,
    /// This will return [`NetError::NotConnected`].
    /// If the message type isn't registered, this will return
    /// [`NetError::TypeNotRegistered`]. If the msg fails to be
    /// serialized this will return [`NetError::SerdeError`].
    pub fn send<T: Any + Send + Sync>(&mut self, msg: &T) -> io::Result<()> {
        let tid = TypeId::of::<T>();
        if !self.parts.valid_tid(tid) {
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

        match transport {
            Transport::TCP => self.send_tcp(mid, &b),
            Transport::UDP => self.send_udp(mid, &b),
        }
    }

    /// Gets an iterator for the messages of type T.
    ///
    /// Returns None if the type T was not registered.
    pub fn recv<T: Any + Send + Sync>(&self) -> Option<impl Iterator<Item = &T>> {
        let tid = TypeId::of::<T>();
        let mid = *self.parts.tid_map.get(&tid)?;

        Some(
            self.msg_buff[mid]
                .iter()
                .map(|m| (*m).downcast_ref::<T>().unwrap()),
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
        loop {
            if !self.status.connected() {
                break;
            }

            let recv = self.recv_tcp();
            match recv {
                // No more data.
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                // IO Error occurred.
                Err(e) => {
                    error!("IO Error occurred while receiving data. {}", e);
                    self.status = Status::Dropped(e);
                }
                // Successfully got a message.
                Ok((mid, msg)) => {
                    i += 1;
                    if mid == DISCONNECT_TYPE_MID {
                        self.status = Status::Disconnected(*msg.downcast().unwrap());
                    } else {
                        self.msg_buff[mid].push(msg);
                    }
                }
            }
        }

        while let Ok((mid, msg)) = self.recv_udp() {
            i += 1;
            self.msg_buff[mid].push(msg);
        }
        i
    }

    /// Clears messages from the buffer.
    pub fn clear_msgs(&mut self) {
        for buff in self.msg_buff.iter_mut() {
            buff.clear();
        }
    }

    /// Gets the local -ess.
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.tcp.local_addr()
    }

    /// Gets the address of the peer.
    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        self.tcp.peer_addr()
    }
}

impl<C, R, D> Debug for Client<C, R, D>
where
    C: Any + Send + Sync,
    R: Any + Send + Sync,
    D: Any + Send + Sync + Debug,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client")
            .field("status", self.status())
            .field("local", &self.local_addr())
            .field("peer", &self.peer_addr())
            .finish()
    }
}

#[derive(Debug)]
/// A client that has started connecting, but might not have finished connecting.
///
/// When creating a client, a new thread is spawned for the client connection cycle.
/// When the client is done being created, it will send it back through this pending client.
pub struct PendingClient<C, R, D>
where
    C: Any + Send + Sync,
    R: Any + Send + Sync,
    D: Any + Send + Sync,
{
    channel: Receiver<io::Result<(Client<C, R, D>, R)>>,
}

// Impl display so that `get()` can be unwrapped.
impl<C, R, D> Display for PendingClient<C, R, D>
where
    C: Any + Send + Sync,
    R: Any + Send + Sync,
    D: Any + Send + Sync,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Pending client connection ({}).",
            if self.done() { "done" } else { "not done" }
        )
    }
}

impl<C, R, D> PendingClient<C, R, D>
where
    C: Any + Send + Sync,
    R: Any + Send + Sync,
    D: Any + Send + Sync,
{
    /// Returns whether the client is finished connecting.
    pub fn done(&self) -> bool {
        self.channel.is_ready()
    }

    /// Takes the [`io::Result<Client>`] from the [`PendingClient`].
    /// This **will** yield a value if [`done()`](Self::done) returned `true`.
    pub fn take(self) -> Result<io::Result<(Client<C, R, D>, R)>, Self> {
        if self.done() {
            Ok(self.channel.recv().unwrap())
        } else {
            Err(self)
        }
    }

    /// Blocks until the client is ready.
    pub fn block(self) -> io::Result<(Client<C, R, D>, R)> {
        self.channel.recv().unwrap()
    }
}


#[derive(Debug)]
/// An optional version of the [`PendingClient`].
///
/// Represents a pending client that could have been received already.
/// This type is useful when you can only get a mutable reference to this (not own it).
///
/// The most notable difference is the `take` method only takes `&mut self`, instead of `self`,
/// and the returns from most methods are wrapped in an option.
pub struct OptionPendingClient<C, R, D>
    where
        C: Any + Send + Sync,
        R: Any + Send + Sync,
        D: Any + Send + Sync,
{
    channel: Option<Receiver<io::Result<(Client<C, R, D>, R)>>>,
}

// Impl display so that `get()` can be unwrapped.
impl<C, R, D> Display for OptionPendingClient<C, R, D>
    where
        C: Any + Send + Sync,
        R: Any + Send + Sync,
        D: Any + Send + Sync,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Optional pending client connection ({}).",
            match self.done() {
                None => "Already taken",
                Some(true) => "done",
                Some(false) => "not done",
            }
        )
    }
}

impl<C, R, D> OptionPendingClient<C, R, D>
    where
        C: Any + Send + Sync,
        R: Any + Send + Sync,
        D: Any + Send + Sync,
{
    /// Returns whether the client is finished connecting.
    pub fn done(&self) -> Option<bool> {
        Some(self.channel.as_ref()?.is_ready())
    }

    /// Takes the [`io::Result<Client>`] from the [`PendingClient`].
    /// This **will** yield a value if [`done()`](Self::done) returned `true`.
    pub fn take(&mut self) -> Option<Result<io::Result<(Client<C, R, D>, R)>, ()>> {
        if self.done()? {
            Some(Ok(self.channel.as_ref().unwrap().recv().unwrap()))
        } else {
            Some(Err(()))
        }
    }

    /// Blocks until the client is ready.
    pub fn block(self) -> Option<io::Result<(Client<C, R, D>, R)>> {
        Some(self.channel?.recv().unwrap())
    }
}

