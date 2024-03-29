use crate::message_table::{MsgTableParts, DISCONNECT_TYPE_MID, RESPONSE_TYPE_MID};
use crate::net::{Config, ErasedNetMsg, NetMsg, Status, Transport};
use crate::tcp::TcpCon;
use crate::udp::UdpCon;
use crate::MId;
use crossbeam_channel::internal::SelectHandle;
use crossbeam_channel::Receiver;
use log::{debug, error, trace};
use std::any::{type_name, Any, TypeId};
use std::fmt::{Debug, Display, Formatter};
use std::io;
use std::io::ErrorKind::InvalidData;
use std::io::{Error, ErrorKind};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};

/// A Client connection.
///
/// This can only connect to 1 server.
///
/// Contains a TCP and UDP connection to the server.
#[cfg_attr(feature = "bevy", derive(bevy::prelude::Resource))]
pub struct Client {
    /// The configuration of the client.
    config: Config,
    /// The status of the client. Whether it is connected/disconnected etc.
    status: Status,
    /// The received message buffer.
    ///
    /// Each [`MId`] has its own vector.
    msg_buff: Vec<Vec<ErasedNetMsg>>,

    /// The TCP connection for this client.
    tcp: TcpCon,
    /// The UDP connection for this client.
    udp: UdpCon,

    /// The [`MsgTableParts`] to use for sending messages.
    parts: MsgTableParts,
}

impl Client {
    /// Creates a new [`Client`].
    ///
    /// Creates a new [`Client`] on another thread, passing back a [`PendingClient`].
    /// This [`PendingClient`] allows you to wait for the client to send the connection
    /// message, and for the server to send back the response message.
    #[allow(clippy::new_ret_no_self)]
    pub fn new<C: Any + Send + Sync, A: ToSocketAddrs + Send + 'static>(
        peer: A,
        parts: MsgTableParts,
        config: Config,
        con_msg: C,
    ) -> PendingClient {
        let (client_tx, client_rx) = crossbeam_channel::bounded(1);

        std::thread::spawn(move || {
            client_tx.send(Self::new_blocking(peer, parts, config, con_msg))
        });

        PendingClient { channel: client_rx }
    }

    /// Creates a new [`Client`] by blocking.
    fn new_blocking<C: Any + Send + Sync, A: ToSocketAddrs>(
        peer: A,
        parts: MsgTableParts,
        config: Config,
        con_msg: C,
    ) -> io::Result<(Self, Box<dyn Any + Send + Sync>)> {
        debug!("Attempting to create a client connection.");
        // TCP & UDP Connections.
        let tcp = TcpStream::connect(peer)?;
        tcp.set_read_timeout(Some(config.timeout))?;
        let tcp = TcpCon::from_stream(tcp, config.max_msg_size);
        let local_addr = tcp.local_addr().unwrap();
        let peer = tcp.peer_addr().unwrap();
        trace!(
            "TcpStream established from {} to {}",
            local_addr,
            tcp.peer_addr().unwrap()
        );
        let udp = UdpCon::new(local_addr, Some(peer), config.max_msg_size)?;
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
            config,
            status: Status::Connected,
            msg_buff,
            tcp,
            udp,
            parts,
        };

        // Send connection message
        client.send(&con_msg)?;
        trace!("Client connection message sent. Awaiting response...");

        // Get response message.
        let (mid, net_msg) = client.recv_tcp()?;
        trace!("Got response message from the server.");

        if mid != RESPONSE_TYPE_MID {
            let msg = format!(
                "Client: First received message was MId: {} not MId: {} (Response message)",
                mid, RESPONSE_TYPE_MID
            );
            return Err(Error::new(ErrorKind::InvalidData, msg));
        }

        debug!(
            "New Client created at {}, to {}.",
            client.tcp.local_addr().unwrap(),
            client.tcp.peer_addr().unwrap(),
        );

        client.tcp.set_nonblocking(true)?;
        client.udp.set_nonblocking(true)?;

        Ok((client, net_msg.msg))
    }

    /// A function that encapsulates the sending logic for the TCP transport.
    fn send_tcp(&self, mid: MId, payload: &[u8]) -> io::Result<()> {
        self.tcp.send(mid, payload)
    }

    /// A function that encapsulates the sending logic for the UDP transport.
    fn send_udp(&self, mid: MId, payload: &[u8]) -> io::Result<()> {
        self.udp.send(mid, payload)
    }

    /// A function that encapsulates the receiving logic for the TCP transport.
    ///
    /// Any errors in receiving are returned. An error of type [`WouldBlock`] means
    /// no more messages can be yielded without blocking. [`InvalidData`] likely means
    /// carrier-pigeon got bad data.
    fn recv_tcp(&mut self) -> io::Result<(MId, ErasedNetMsg)> {
        let (mid, bytes) = self.tcp.recv()?;

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
            cid: 0,
            time: None,
            msg,
        };

        Ok((mid, net_msg))
    }

    /// A function that encapsulates the receiving logic for the UDP transport.
    ///
    /// Any errors in receiving are returned. An error of type [`WouldBlock`] means
    /// no more messages can be yielded without blocking. [`InvalidData`] likely means
    /// carrier-pigeon got bad data.
    fn recv_udp(&mut self) -> io::Result<(MId, ErasedNetMsg)> {
        let (mid, time, bytes) = self.udp.recv()?;

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
            cid: 0,
            time: Some(time),
            msg,
        };

        Ok((mid, net_msg))
    }

    /// Gets the config of the client.
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Disconnects from the server. You should call this
    /// method before dropping the client to let the server know that
    /// you intentionally disconnected. The `discon_msg` allows you to
    /// give a reason for the disconnect.
    pub fn disconnect<D: Any + Send + Sync>(&mut self, discon_msg: &D) -> io::Result<()> {
        let tid = TypeId::of::<D>();
        if self.parts.tid_map[&tid] != DISCONNECT_TYPE_MID {
            return Err(Error::new(InvalidData, "The generic parameter `D` must be the disconnection message type (the same `D` that you passed into `MsgTable::build`)."));
        }
        debug!("Disconnecting client.");
        self.send(discon_msg)?;
        self.tcp.close()?;
        // No shutdown method on udp.
        self.status = Status::Closed;
        Ok(())
    }

    /// Gets the status of the connection.
    pub fn status(&self) -> &Status {
        &self.status
    }

    /// Returns whether the connection is open.
    pub fn open(&self) -> bool {
        self.status().connected()
    }

    /// Sends a message to the peer.
    ///
    /// `T` must be registered in the [`MsgTable`].
    pub fn send<T: Any + Send + Sync>(&self, msg: &T) -> io::Result<()> {
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

    /// Gets an iterator for the messages of type `T`.
    ///
    /// ### Panics
    /// Panics if the type `T` was not registered.
    /// For a non-panicking version, see [try_recv()](Self::try_recv).
    pub fn recv<T: Any + Send + Sync>(&self) -> impl Iterator<Item = NetMsg<T>> + '_ {
        let tid = TypeId::of::<T>();
        if !self.parts.valid_tid(tid) {
            panic!("Type ({}) not registered.", type_name::<T>());
        }
        let mid = self.parts.tid_map[&tid];

        self.msg_buff[mid].iter().map(|m| m.to_typed().unwrap())
    }

    /// Gets an iterator for the messages of type `T`.
    ///
    /// Returns `None` if the type `T` was not registered.
    pub fn try_recv<T: Any + Send + Sync>(&self) -> Option<impl Iterator<Item = NetMsg<T>> + '_> {
        let tid = TypeId::of::<T>();
        let mid = *self.parts.tid_map.get(&tid)?;

        Some(self.msg_buff[mid].iter().map(|m| m.to_typed().unwrap()))
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
                    error!("TCP: IO error occurred while receiving data. {}", e);
                    self.status = Status::Dropped(e);
                }
                // Successfully got a message.
                Ok((mid, net_msg)) => {
                    i += 1;
                    if mid == DISCONNECT_TYPE_MID {
                        self.status = Status::Disconnected(net_msg.msg);
                    } else {
                        self.msg_buff[mid].push(net_msg);
                    }
                }
            }
        }

        // UDP
        loop {
            if !self.status.connected() {
                break;
            }

            let recv = self.recv_udp();
            match recv {
                // No more data.
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                // IO Error occurred.
                Err(e) => {
                    error!("UDP: IO error occurred while receiving data. {}", e);
                }
                // Successfully got a message.
                Ok((mid, net_msg)) => {
                    i += 1;
                    self.msg_buff[mid].push(net_msg);
                }
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

    /// Gets the local address.
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.tcp.local_addr()
    }

    /// Gets the address of the peer.
    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        self.tcp.peer_addr()
    }
}

impl Debug for Client {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client")
            .field("status", self.status())
            .field("local", &self.local_addr())
            .field("peer", &self.peer_addr())
            .finish()
    }
}

/// A client that has started connecting, but might not have finished connecting.
///
/// When creating a client, a new thread is spawned for the client connection cycle.
/// When the client is done being created, it will send it back through this pending client.
#[derive(Debug)]
#[cfg_attr(feature = "bevy", derive(bevy::prelude::Resource))]
pub struct PendingClient {
    channel: Receiver<io::Result<(Client, Box<dyn Any + Send + Sync>)>>,
}

// Impl display so that `get()` can be unwrapped.
impl Display for PendingClient {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Pending client connection ({}).",
            if self.done() { "done" } else { "not done" }
        )
    }
}

impl PendingClient {
    /// Returns whether the client is finished connecting.
    pub fn done(&self) -> bool {
        self.channel.is_ready()
    }

    /// Takes the [`io::Result<Client>`] from the [`PendingClient`].
    /// This **will** yield a value if [`done()`](Self::done) returned `true`.
    ///
    /// ### Panics
    /// Panics if the generic parameter `R` isn't the response message type (the same `R` that you passed into `MsgTable::build`).
    pub fn take<R: Any + Send + Sync>(self) -> Result<io::Result<(Client, R)>, Self> {
        if self.done() {
            Ok(self.channel.recv().unwrap().map(|(client, m)| (client, *m.downcast::<R>().expect("The generic parameter `R` must be the response message type (the same `R` that you passed into `MsgTable::build`)."))))
        } else {
            Err(self)
        }
    }

    /// Blocks until the client is ready.
    ///
    /// ### Panics
    /// Panics if the generic parameter `R` isn't the response message type (the same `R` that you passed into `MsgTable::build`).
    pub fn block<R: Any + Send + Sync>(self) -> io::Result<(Client, R)> {
        self.channel.recv().unwrap().map(|(client, m)| (client, *m.downcast::<R>().expect("The generic parameter `R` must be the response message type (the same `R` that you passed into `MsgTable::build`).")))
    }

    /// Converts this into a [`OptionPendingClient`].
    pub fn option(self) -> OptionPendingClient {
        OptionPendingClient {
            channel: Some(self.channel),
        }
    }
}

impl From<PendingClient> for OptionPendingClient {
    fn from(value: PendingClient) -> Self {
        value.option()
    }
}

/// An optional version of the [`PendingClient`].
///
/// Get a [`OptionPendingClient`] with the `option()` method of the [`PendingClient`],
/// or with the [`Into`]/[`From`] traits.
///
/// Represents a pending client that could have been received already.
/// This type is useful when you can only get a mutable reference to this (not own it).
///
/// The most notable difference is the `take` method only takes `&mut self`, instead of `self`,
/// and the returns from most methods are wrapped in an option.
#[derive(Debug)]
#[cfg_attr(feature = "bevy", derive(bevy::prelude::Resource))]
pub struct OptionPendingClient {
    #[allow(clippy::type_complexity)]
    channel: Option<Receiver<io::Result<(Client, Box<dyn Any + Send + Sync>)>>>,
}

// Impl display so that `get()` can be unwrapped.
impl Display for OptionPendingClient {
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

impl OptionPendingClient {
    /// Returns whether the client is finished connecting.
    pub fn done(&self) -> Option<bool> {
        Some(self.channel.as_ref()?.is_ready())
    }

    /// Takes the [`io::Result<Client>`] from the [`PendingClient`].
    /// This **will** yield a value if [`done()`](Self::done) returned `true`.
    ///
    /// ### Panics
    /// Panics if `R` is not the response message type.
    pub fn take<R: Any + Send + Sync>(&mut self) -> Option<io::Result<(Client, R)>> {
        if self.done()? {
            Some(self.channel.as_ref().unwrap().recv().unwrap().map(|(client, m)| (client, *m.downcast::<R>().expect("The generic parameter `R` must be the response message type (the same `R` that you passed into `MsgTable::build`)."))))
        } else {
            None
        }
    }

    /// Blocks until the client is ready.
    ///
    /// ### Panics
    /// Panics if the generic parameter `R` isn't the response message type (the same `R` that you passed into `MsgTable::build`).
    pub fn block<R: Any + Send + Sync>(self) -> Option<io::Result<(Client, R)>> {
        Some(self.channel?.recv().unwrap().map(|(client, m)| (client, *m.downcast::<R>().expect("The generic parameter `R` must be the response message type (the same `R` that you passed into `MsgTable::build`)."))))
    }
}
