use std::any::TypeId;
use std::io::{Error, ErrorKind};
use std::marker::PhantomData;
use std::net::SocketAddr;
use hashbrown::HashMap;
use log::{debug, error};
use tokio::io;
use tokio::runtime::Handle;
use tokio::sync::oneshot;
use crate::{MId, NetMsg};
use crate::message_table::{CONNECTION_TYPE_MID, MsgTableParts, RESPONSE_TYPE_MID};
use crate::net::{NetError, Transport};
use crate::tcp::TcpCon;
use crate::udp::UdpCon;

/// A Client connection.
///
/// This can only connect to 1 server.
///
/// Contains a TCP and UDP connection to the server.
pub struct Client<C, R, D>
where
    C: NetMsg,
    R: NetMsg,
    D: NetMsg,
{
    /// The received message buffer.
    ///
    /// Each [`MId`] has its own vector.
    msg_buff: Vec<Vec<Box<dyn NetMsg>>>,

    /// The TCP connection for this client.
    tcp: TcpCon<D>,
    /// The UDP connection for this client.
    udp: UdpCon,

    /// The map from [`TypeId`] to [`MId`].
    tid_map: HashMap<TypeId, MId>,
    /// The transport that is associated with each MId.
    transports: Vec<Transport>,
    _pd: PhantomData<(C, R, D)>,
}

impl<C, R, D> Client<C, R, D>
where
    C: NetMsg,
    R: NetMsg,
    D: NetMsg,
{
    /// Creates a oneshot to a new [`Client`].
    ///
    /// Creates a new [`Client`] asynchronously, passing back a oneshot receiver.
    /// This oneshot receiver allows you to wait on the connection however you like.
    pub fn new(
        peer: SocketAddr,
        parts: MsgTableParts<C, R, D>,
        con_msg: C,
        rt: Handle,
    ) -> oneshot::Receiver<io::Result<(Self, R)>> {
        let (client_tx, client_rx) = oneshot::channel();

        let rt0 = rt.clone();
        rt0.spawn(async move {
            let _ = client_tx.send(Self::new_async(peer, parts, &con_msg, rt).await);
        });

        client_rx
    }

    /// Creates a new [`Client`] asynchronously.
    pub async fn new_async(
        peer: SocketAddr,
        parts: MsgTableParts<C, R, D>,
        con_msg: &C,
        rt: Handle,
    ) -> io::Result<(Self, R)> {
        let mut tcp =
            TcpCon::new(0, peer, parts.deser.clone(), parts.ser.clone(), rt.clone()).await?;

        let udp = UdpCon::new(0, tcp.local_addr(), Some(peer), parts.deser, parts.ser, rt).await?;

        // Send connection packet
        let _ = tcp.send(CONNECTION_TYPE_MID, con_msg);
        debug!(
            "Client({}): Connection message sent. Awaiting response...",
            tcp.cid()
        );

        // Get response packet
        let (r_mid, response) = match tcp.recv().await {
            Some(r) => r,
            None => {
                let msg = format!(
                    "Client({}): Did not get a response packet from the server.",
                    tcp.cid()
                );
                error!("{}", msg);
                return Err(Error::new(ErrorKind::InvalidData, msg));
            }
        };
        if r_mid != RESPONSE_TYPE_MID {
            let msg = format!(
                "Client({}): First received packet was MId: {} not MId: {} (Response packet)",
                tcp.cid(),
                r_mid,
                RESPONSE_TYPE_MID
            );
            error!("{}", msg);
            return Err(Error::new(ErrorKind::InvalidData, msg));
        }
        let response = *response.downcast::<R>().map_err(|_| "").unwrap();

        debug!(
            "Client({}): New client created at {}.",
            tcp.cid(),
            tcp.local_addr()
        );

        let mid_count = parts.tid_map.len();
        let mut msg_buff = Vec::with_capacity(mid_count);
        for _i in 0..mid_count {
            msg_buff.push(vec![]);
        }

        Ok((
            Client {
                msg_buff,
                tcp,
                udp,
                tid_map: parts.tid_map,
                transports: parts.transports,
                _pd: PhantomData,
            },
            response,
        ))
    }

    /// Disconnects from the server. You should ***always*** call this
    /// method before dropping the client to let the server know that
    /// you intentionally disconnected. The `discon_msg` allows you to
    /// give a reason for the disconnect.
    pub fn disconnect<T: NetMsg>(&mut self, discon_msg: &T) -> Result<(), NetError> {
        debug!("Disconnecting client.");
        self.send(discon_msg)?;
        self.tcp.disconnect();
        self.udp.disconnect();
        Ok(())
    }

    pub fn get_disconnect(&mut self) -> Option<&io::Result<D>> {
        if self.open() {
            return None;
        }

        self.tcp.get_recv_status()
    }

    /// Returns whether the connection is open.
    pub fn open(&mut self) -> bool {
        self.get_tcp_recv_status().is_none()
            && self.get_udp_recv_status().is_none()
            && self.get_send_status(Transport::TCP).is_none()
            && self.get_send_status(Transport::UDP).is_none()
    }

    /// Sends a message to the connected computer.
    /// ### Errors
    /// If the client isn't connected to another computer,
    /// This will return [`NetError::NotConnected`].
    /// If the message type isn't registered, this will return
    /// [`NetError::TypeNotRegistered`]. If the msg fails to be
    /// serialized this will return [`NetError::SerdeError`].
    pub fn send<T: NetMsg>(&self, msg: &T) -> Result<(), NetError> {
        let tid = TypeId::of::<T>();
        if !self.tid_map.contains_key(&tid) {
            return Err(NetError::TypeNotRegistered);
        }
        let mid = self.tid_map[&tid];
        let transport = self.transports[mid];

        match transport {
            Transport::TCP => self.tcp.send(mid, msg),
            Transport::UDP => self.udp.send(mid, msg),
        }?;

        Ok(())
    }

    /// Gets an iterator for the messages of type T.
    ///
    /// Returns None if the type T was not registered.
    pub fn recv<T: NetMsg>(&self) -> Option<impl Iterator<Item = &T>> {
        let tid = TypeId::of::<T>();
        let mid = *self.tid_map.get(&tid)?;

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
        while let Some((mid, msg)) = self.tcp.try_recv() {
            i += 1;
            self.msg_buff[mid].push(msg);
        }
        while let Some((mid, msg, _addr)) = self.udp.try_recv() {
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

    /// Gets the status of the send task of the given
    /// `transport`.
    ///
    /// [`None`] indicates that the send task has not
    /// completed yet. Otherwise a reference is given
    /// to the [`Result`] of the task.
    pub fn get_send_status(&mut self, transport: Transport) -> Option<&io::Result<()>> {
        match transport {
            Transport::UDP => {
                self.udp.update_status();
                self.udp.get_send_status()
            }
            Transport::TCP => {
                self.tcp.update_status();
                self.tcp.get_send_status()
            }
        }
    }

    /// Gets the status of the receive task of udp.
    ///
    /// [`None`] indicates that the receive task has not
    /// completed yet. Otherwise a reference is given
    /// to the [`Result`] of the task.
    pub fn get_udp_recv_status(&mut self) -> Option<&io::Result<()>> {
        self.udp.update_status();
        self.udp.get_recv_status()
    }

    /// Gets the status of the receive task of tcp.
    ///
    /// [`None`] indicates that the receive task has not
    /// completed yet. Otherwise a reference is given
    /// to the [`Result`] of the task.
    pub fn get_tcp_recv_status(&mut self) -> Option<&io::Result<D>> {
        self.tcp.update_status();
        self.tcp.get_recv_status()
    }

    /// Gets the local address.
    pub fn local_addr(&self) -> SocketAddr {
        self.tcp.local_addr()
    }

    /// Gets the address of the peer.
    pub fn peer_addr(&self) -> SocketAddr {
        self.tcp.peer_addr()
    }
}
