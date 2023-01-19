use crate::connection::client::ClientConnection;
use crate::message_table::{MsgTable, CONNECTION_M_TYPE, DISCONNECT_M_TYPE, RESPONSE_M_TYPE};
use crate::net::{ClientConfig, ErasedNetMsg, NetMsg, Status};
use crate::transport::client_std_udp::UdpClientTransport;
use crossbeam_channel::internal::SelectHandle;
use crossbeam_channel::Receiver;
use log::{debug, error, trace};
use std::any::{Any, TypeId};
use std::fmt::{Debug, Display, Formatter};
use std::io;
use std::io::ErrorKind::InvalidData;
use std::io::{Error, ErrorKind};
use std::net::SocketAddr;

/// A Client connection.
///
/// This can only connect to 1 server.
///
/// Contains a TCP and UDP connection to the server.
#[cfg_attr(feature = "bevy", derive(bevy::prelude::Resource))]
pub struct Client {
    /// The configuration of the client.
    config: ClientConfig,
    /// The status of the client. Whether it is connected/disconnected etc.
    status: Status,
    /// The received message buffer.
    ///
    /// Each [`MType`](crate::MType) has its own vector.
    msg_buff: Vec<Vec<ErasedNetMsg>>,

    // TODO: make this generic/behind a feature flag somehow.
    /// The UDP connection for this client.
    connection: ClientConnection<UdpClientTransport>,

    /// The [`MsgTable`] to use for sending messages.
    msg_table: MsgTable,
}

impl Client {
    /// Creates a new [`Client`].
    ///
    /// Creates a new [`Client`] on another thread, passing back a [`PendingClient`].
    /// This [`PendingClient`] allows you to wait for the client to send the connection
    /// message, and for the server to send back the response message.
    #[allow(clippy::new_ret_no_self)]
    pub fn new<C: Any + Send + Sync>(
        local: SocketAddr,
        peer: SocketAddr,
        msg_table: MsgTable,
        config: ClientConfig,
        con_msg: C,
    ) -> PendingClient {
        let (client_tx, client_rx) = crossbeam_channel::bounded(1);

        std::thread::spawn(move || {
            client_tx.send(Self::new_blocking(local, peer, msg_table, config, con_msg))
        });

        PendingClient { channel: client_rx }
    }

    /// Creates a new [`Client`] by blocking.
    fn new_blocking<C: Any + Send + Sync>(
        local: SocketAddr,
        peer: SocketAddr,
        msg_table: MsgTable,
        config: ClientConfig,
        con_msg: C,
    ) -> io::Result<(Self, Box<dyn Any + Send + Sync>)> {
        // verify that `con_msg` is the correct type.
        let tid = TypeId::of::<C>();
        let con_msg_m_type = msg_table.tid_map.get(&tid);
        match con_msg_m_type {
            Some(&CONNECTION_M_TYPE) => Ok(()),
            _ => Err(Error::new(
                InvalidData,
                "the type of `con_msg` passed into `Client::new` or `Client::new_blocking` \
                    should match the first type parameter of the `MsgTable::build` function."
                    .to_string(),
            )),
        }?;

        debug!("Attempting to create a client connection");
        let connection = ClientConnection::new(msg_table.clone(), local, peer)?;

        let msg_buff = (0..msg_table.mtype_count()).map(|_| vec![]).collect();

        let mut client = Client {
            config,
            status: Status::Connected,
            msg_buff,
            connection,
            msg_table,
        };

        // Send connection message
        client.send(&con_msg)?;
        trace!("Client connection message sent. Awaiting response...");

        // Get response message.
        let (header, response_msg) = client.connection.recv_blocking()?;
        trace!("Got response message from the server.");

        if header.m_type != RESPONSE_M_TYPE {
            return Err(io::Error::new(
                InvalidData,
                format!(
                    "Client: First received message was MType: {} not MType: {} (Response message)",
                    header.m_type, RESPONSE_M_TYPE
                ),
            ));
        }

        debug!(
            "New Client created at {}, to {}.",
            client
                .connection
                .local_addr()
                .map(|addr| addr.to_string())
                .unwrap_or("UNKNOWN".to_owned()),
            client
                .connection
                .peer_addr()
                .map(|addr| addr.to_string())
                .unwrap_or("UNKNOWN".to_owned()),
        );

        Ok((client, response_msg))
    }

    /// Gets the [`NetConfig`] of the client.
    pub fn config(&self) -> &ClientConfig {
        &self.config
    }

    /// Gets the [`MsgTable`] of the client.
    pub fn msg_table(&self) -> &MsgTable {
        &self.msg_table
    }

    /// Disconnects from the server. You should call this
    /// method before dropping the client to let the server know that
    /// you intentionally disconnected. The `discon_msg` allows you to
    /// give a reason for the disconnect.
    pub fn disconnect<D: Any + Send + Sync>(&mut self, discon_msg: &D) -> io::Result<()> {
        let tid = TypeId::of::<D>();
        if self.msg_table.tid_map[&tid] != DISCONNECT_M_TYPE {
            return Err(Error::new(
                InvalidData,
                "The `discon_msg` type must be the disconnection message type \
                (the same `D` that you passed into `MsgTable::build`).",
            ));
        }
        debug!("Disconnecting client.");
        self.send(discon_msg)?;
        // TODO: start to close the udp connection.
        //       It needs to stay open long enough to reliably send the disconnection message.
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
    pub fn send<T: Any + Send + Sync>(&mut self, msg: &T) -> io::Result<()> {
        self.connection.send(msg)
    }

    /// Gets an iterator for the messages of type `T`.
    ///
    /// ### Panics
    /// Panics if the type `T` was not registered.
    /// For a non-panicking version, see [try_get_msgs()](Self::try_get_msgs).
    pub fn recv<T: Any + Send + Sync>(&self) -> impl Iterator<Item = NetMsg<T>> + '_ {
        self.msg_table.check_type::<T>().expect(
            "`get_msgs` panics if generic type `T` is not registered in the MsgTable. \
            For a non panicking version, use `try_get_msgs`",
        );
        let tid = TypeId::of::<T>();
        let m_type = self.msg_table.tid_map[&tid];

        self.msg_buff[m_type].iter().map(|m| m.get_typed().unwrap())
    }

    /// Gets an iterator for the messages of type `T`.
    ///
    /// Returns `None` if the type `T` was not registered.
    pub fn try_recv<T: Any + Send + Sync>(&self) -> Option<impl Iterator<Item = NetMsg<T>> + '_> {
        let tid = TypeId::of::<T>();
        let m_type = *self.msg_table.tid_map.get(&tid)?;

        Some(self.msg_buff[m_type].iter().map(|m| m.get_typed().unwrap()))
    }

    /// Receives the messages from the connections.
    /// This should be done before calling `recv<T>()`.
    ///
    /// When done in a game loop, you should call `clear_msgs()`, then `get_msgs()`
    /// before default time. This will clear the messages between frames.
    pub fn get_msgs(&mut self) -> u32 {
        let mut i = 0;

        // UDP
        loop {
            if !self.status.connected() {
                break;
            }

            let recv = self.connection.recv();
            match recv {
                // No more data.
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                // IO Error occurred.
                Err(e) => {
                    error!("IO error occurred while receiving data. {}", e);
                }
                // Successfully got a message.
                Ok((header, msg)) => {
                    i += 1;
                    self.msg_buff[header.m_type].push(ErasedNetMsg::new(0, header.ack_num, msg));
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
        self.connection.local_addr()
    }

    /// Gets the address of the peer.
    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        self.connection.peer_addr()
    }
}

impl Debug for Client {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client")
            .field("status", self.status())
            .field("local", &self.local_addr())
            .field("peer_addr", &self.peer_addr())
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
