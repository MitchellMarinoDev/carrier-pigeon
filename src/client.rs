use crate::connection::client_connection::ClientConnection;
use crate::message_table::{MsgTable, DISCONNECT_M_TYPE, RESPONSE_M_TYPE};
use crate::messages::NetMsg;
use crate::net::{AckNum, ClientConfig, ErasedNetMsg, Message, Status};
use crate::transport::client_std_udp::UdpClientTransport;
use crate::Response;
use log::{debug, trace};
use std::any::TypeId;
use std::fmt::{Debug, Formatter};
use std::io;
use std::io::{Error, ErrorKind};
use std::net::SocketAddr;

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
    status: Status<A, R, D>,
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

        let connection = ClientConnection::new(config, msg_table.clone());
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

        self.connection.connect(local_addr, peer_addr, con_msg)?;
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
        // TODO: change to custom error type.
        if !self.status.is_connected() {
            return Err(Error::new(
                ErrorKind::NotConnected,
                "Client is not connected.",
            ));
        }
        debug!("Client disconnecting from server.");
        let discon_ack = self.send(discon_msg)?;

        // TODO: start to close the udp connection.
        //       It needs to stay open long enough to reliably send the disconnection message.
        self.status = Status::Disconnecting(discon_ack);
        Ok(())
    }

    pub fn handle_status(&mut self) -> Status<A, R, D> {
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
            Disconnecting(ack_num) => Disconnecting(*ack_num),
        };

        std::mem::replace(&mut self.status, new_status)
    }

    /// Gets the status of the connection.
    pub fn get_status(&self) -> &Status<A, R, D> {
        &self.status
    }

    /// Sends a message to the peer.
    ///
    /// `T` must be registered in the [`MsgTable`].
    pub fn send<T: NetMsg>(&mut self, msg: &T) -> io::Result<AckNum> {
        self.connection.send(msg)
    }

    /// Gets an iterator for the messages of type `M`.
    ///
    /// ### Panics
    /// Panics if the type `M` was not registered.
    /// For a non-panicking version, see [try_get_msgs()](Self::try_get_msgs).
    pub fn recv<M: NetMsg>(&self) -> impl Iterator<Item = Message<M>> + '_ {
        self.msg_table.check_type::<M>().expect(
            "`get_msgs` panics if generic type `M` is not registered in the MsgTable. \
            For a non panicking version, use `try_get_msgs`",
        );
        let tid = TypeId::of::<M>();
        let m_type = self.msg_table.tid_map[&tid];

        self.msg_buff[m_type].iter().map(|m| m.get_typed().unwrap())
    }

    /// Gets an iterator for the messages of type `M`.
    ///
    /// Returns `None` if the type `M` was not registered.
    pub fn try_recv<M: NetMsg>(&self) -> Option<impl Iterator<Item = Message<M>> + '_> {
        let tid = TypeId::of::<M>();
        let m_type = *self.msg_table.tid_map.get(&tid)?;

        Some(self.msg_buff[m_type].iter().map(|m| m.get_typed().unwrap()))
    }

    /// This handles everything that the client needs to do each frame.
    ///
    /// This includes:
    ///
    ///  - Clearing the message buffer. This gets rid of all the messages from last frame.
    ///  - Getting the messages for this frame.
    ///  - Resending messages that are needed for the reliability layer.
    ///  - Updating the status.
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
            match header.m_type {
                DISCONNECT_M_TYPE => {
                    if self.status.is_connected() {
                        self.status = Status::Disconnected(*msg.downcast().expect("since the MType is `DISCONNECT_M_TYPE`, the message should be the disconnection type"));
                    }
                }
                RESPONSE_M_TYPE => {
                    if self.status.is_connecting() {
                        match *msg.downcast::<Response<A, R>>().expect("since the MType is `RESPONSE_M_TYPE`, the message should be the response type") {
                            Response::Accepted(a) => self.status = Status::Accepted(a),
                            Response::Rejected(r) => self.status = Status::Rejected(r),
                        }
                    }
                }
                _ => {
                    self.msg_buff[header.m_type].push(ErasedNetMsg::new(
                        0,
                        header.sender_ack_num,
                        header.order_num,
                        msg,
                    ));
                }
            }
        }
    }

    fn update_status(&mut self) {
        match self.status {
            Status::Connecting => {
                if let Some(err) = self.connection.take_err() {
                    self.status = Status::ConnectionFailed(err);
                }
            }
            Status::Connected => {
                if let Some(err) = self.connection.take_err() {
                    self.status = Status::Dropped(err);
                }
            }
            Status::Disconnecting(ack_num) => {
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
