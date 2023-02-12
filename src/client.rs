use crate::connection::client_connection::ClientConnection;
use crate::message_table::{MsgTable, DISCONNECT_M_TYPE, RESPONSE_M_TYPE};
use crate::net::{ClientConfig, ErasedNetMsg, Message, Status};
use crate::transport::client_std_udp::UdpClientTransport;
use log::{debug, trace};
use std::any::TypeId;
use std::fmt::{Debug, Formatter};
use std::io;
use std::io::ErrorKind::{InvalidData};
use std::io::{Error, ErrorKind};
use std::net::SocketAddr;
use crate::messages::NetMsg;

/// A Client connection.
///
/// This can only connect to 1 server.
///
/// Contains a TCP and UDP connection to the server.
#[cfg_attr(feature = "bevy", derive(bevy::prelude::Resource))]
pub struct Client<C: NetMsg, A: NetMsg, R: NetMsg, D: NetMsg> {
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
    connection: ClientConnection<UdpClientTransport, C, A, R, D>,

    /// The [`MsgTable`] to use for sending messages.
    msg_table: MsgTable<C, A, R, D>,
}

impl<C: NetMsg, A: NetMsg, R: NetMsg, D: NetMsg> Client<C, A, R, D> {
    /// Creates a new [`Client`].
    pub fn new(msg_table: MsgTable<C, A, R, D>, config: ClientConfig) -> Self {
        trace!("Creating a Client.");

        let connection = ClientConnection::new(msg_table.clone());
        let msg_buff = (0..msg_table.mtype_count()).map(|_| vec![]).collect();

        Client {
            config,
            status: Status::NotConnected,
            msg_buff,
            connection,
            msg_table,
        }
    }

    // TODO: make a custom error type. Add invalid state.
    pub fn connect(
        &mut self,
        local_addr: SocketAddr,
        peer_addr: SocketAddr,
        con_msg: &C,
    ) -> io::Result<()> {
        if !self.status.is_not_connected() {
            return Err(Error::new(
                ErrorKind::Other,
                "the client needs to be in the NotConnected status in order to call connect()",
            ));
        }

        self.connection.connect(local_addr, peer_addr)?;
        self.send(con_msg)?;
        self.status = Status::Connecting;
        Ok(())
    }

    /// Gets the [`NetConfig`] of the client.
    pub fn config(&self) -> &ClientConfig {
        &self.config
    }

    /// Gets the [`MsgTable`] of the client.
    pub fn msg_table(&self) -> &MsgTable<C, A, R, D> {
        &self.msg_table
    }

    /// Disconnects from the server. You should call this
    /// method before dropping the client to let the server know that
    /// you intentionally disconnected. The `discon_msg` allows you to
    /// give a reason for the disconnect.
    pub fn disconnect(&mut self, discon_msg: &D) -> io::Result<()> {
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
        self.status = Status::Disconnecting;
        Ok(())
    }

    pub fn handle_status(&mut self) -> Status {
        use Status::*;

        let new_status = match &self.status {
            NotConnected => NotConnected,
            Connecting => Connecting,
            Accepted(_) => Connected,
            Rejected(_) => NotConnected,
            ConnectionFailed(_) => NotConnected,
            Connected => Connected,
            Disconnected(_) => NotConnected,
            Dropped(_) => NotConnected,
            Disconnecting => Disconnecting,
        };

        std::mem::replace(&mut self.status, new_status)
    }

    /// Gets the status of the connection.
    pub fn get_status(&self) -> &Status {
        &self.status
    }

    /// Sends a message to the peer.
    ///
    /// `T` must be registered in the [`MsgTable`].
    pub fn send<T: NetMsg>(&mut self, msg: &T) -> io::Result<()> {
        self.connection.send(msg)
    }

    /// Gets an iterator for the messages of type `T`.
    ///
    /// ### Panics
    /// Panics if the type `T` was not registered.
    /// For a non-panicking version, see [try_get_msgs()](Self::try_get_msgs).
    pub fn recv<T: NetMsg>(&self) -> impl Iterator<Item = Message<T>> + '_ {
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
    pub fn try_recv<T: NetMsg>(&self) -> Option<impl Iterator<Item = Message<T>> + '_> {
        let tid = TypeId::of::<T>();
        let m_type = *self.msg_table.tid_map.get(&tid)?;

        Some(self.msg_buff[m_type].iter().map(|m| m.get_typed().unwrap()))
    }

    /// This handles everything that the client needs to do each frame.
    ///
    /// This includes:
    ///
    ///  - Clearing the message buffer. This gets rid of all the messages from last frame.
    ///  - (Re)sending messages that are needed for the reliability layer.
    ///  - Getting the messages for this frame.
    pub fn tick(&mut self) {
        self.clear_msgs();
        self.connection.send_ack_msg();
        self.connection.send_ping();
        self.connection.resend_reliable();
        self.get_msgs();
        self.update_status();
    }

    /// Gets message from the transport, and moves them to this struct's buffer.
    ///
    /// This should be called in the `tick()` and should done before calling `recv<T>()`.
    fn get_msgs(&mut self) {
        self.connection.get_msgs();

        while let Some((header, msg)) = self.connection.recv() {
            self.msg_buff[header.m_type].push(ErasedNetMsg::new(
                0,
                header.sender_ack_num,
                header.order_num,
                msg,
            ));
        }
    }

    fn update_status(&mut self) {
        match self.status {
            Status::Connecting => {
                if let Some(response_msg) = self.msg_buff[RESPONSE_M_TYPE].get(0) {
                    // TODO: generics are needed in order to unwrap this as this cant be downcast
                    //      unless we know the types.
                    todo!()
                }

                if let Some(err) = self.connection.take_err() {
                    self.status = Status::ConnectionFailed(err);
                }
            }
            Status::Connected => {
                if let Some(err) = self.connection.take_err() {
                    self.status = Status::Dropped(err);
                }
            }
            Status::Disconnecting => {
                // When we are disconnecting, if we send our disconnection message, but the
                // ack for it gets lost and the peer closes their socket we could get an error.
                // This could also happen if the connection drops after we have disconnected, but
                // that is unlikely. With this, we can avoid some false positive `Dropped(err)`
                // states.

                // TODO: if the disconnection message has been acknowledged, move states.
                if let Some(_) = self.connection.take_err() {
                    self.status = Status::NotConnected;
                }
            }
            _ => {}
        }
    }

    /// Clears messages from the buffer.
    fn clear_msgs(&mut self) {
        for buff in self.msg_buff.iter_mut() {
            buff.clear();
        }
    }

    /// Gets the local address.
    pub fn local_addr(&self) -> Option<SocketAddr> {
        self.connection.local_addr()
    }

    /// Gets the address of the peer.
    pub fn peer_addr(&self) -> Option<SocketAddr> {
        self.connection.peer_addr()
    }

    /// Gets the estimated round trip time (RTT) of the connection
    /// in microseconds (divide by 1000 for ms).
    ///
    /// Returns `None` iff `cid` is an invalid Connection ID.
    pub fn rtt(&self) -> u32 {
        self.connection.rtt()
    }
}

impl<C: NetMsg, A: NetMsg, R: NetMsg, D: NetMsg> Debug for Client<C, A, R, D> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client")
            .field("status", self.get_status())
            .field("local", &self.local_addr())
            .field("peer_addr", &self.peer_addr())
            .finish()
    }
}
