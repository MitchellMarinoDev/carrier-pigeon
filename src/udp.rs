use std::any::Any;
use crate::net::{CId, MId};
use crate::net::{DeserFn, SerFn};
use crate::net::{Header, NetError, TaskStatus};
use crate::net::{MAX_PACKET_SIZE, MAX_SAFE_PACKET_SIZE};
use log::{debug, error, trace, warn};
use std::io;
use std::io::{Error, ErrorKind};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::runtime::Handle;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

/// A raw UDP connection.
///
/// Represents either a Server type connection, or Client type connection.
/// A Client type connection is connected to 1 other address and only
/// sends/receives from that 1 addr. A Server connection can send/receive
/// from any address.
///
/// This hosts 2 async tasks that allow the sending and
/// receiving of messages. Messages are sent to and from
/// these tasks from a sync context using channels.
///
/// The status of the send/receive tasks can be fetched
/// with the `get_send_status()` and `get_recv_status()`
/// functions. Remember to call the `update_status()`
/// function before calling either of those.
pub struct UdpCon {
    /// The Connection ID. This is used in
    /// logging for discerning between connections.
    /// Not functionally used.
    cid: CId,
    /// The local address.
    local: SocketAddr,
    /// The optional peer address.
    /// [`Some`] if this is a Client type connection,
    /// [`None`] if this is a Server type connection.
    peer: Option<SocketAddr>,

    /// The message sender.
    send: Option<UnboundedSender<(MId, Vec<u8>, SocketAddr)>>,
    /// The message receiver.
    recv: UnboundedReceiver<(MId, Box<dyn Any + Send + Sync>, SocketAddr)>,

    /// The status of the `send` task.
    send_status: TaskStatus<io::Result<()>>,
    /// The status of the `recv` task.
    recv_status: TaskStatus<io::Result<()>>,

    /// The list of serialization functions for messages.
    ser: Vec<SerFn>,

    /// A handle to the sending task.
    _send_task: JoinHandle<()>,
    /// A handle to the receiving task.
    recv_task: JoinHandle<()>,
}

impl UdpCon {
    /// Creates a new [`UdpConn`].
    ///
    /// Creates a new Client type or Server type connection.
    /// To new_async a Client type connection pass [`Some`]\(addr)
    /// into the peer parameter. For a Server type, pass
    /// in [`None`].
    pub async fn new(
        cid: CId,
        local: SocketAddr,
        peer: Option<SocketAddr>,
        deser: Vec<DeserFn>,
        ser: Vec<SerFn>,
        rt: Handle,
    ) -> io::Result<Self> {
        let socket = UdpSocket::bind(local).await?;
        if let Some(addr) = peer {
            socket.connect(addr).await?;
        }
        let local = socket.local_addr()?; // Connecting might change the local addr

        let (send_tx, send_rx) = unbounded_channel();
        let (recv_tx, recv_rx) = unbounded_channel();

        if let Some(peer_addr) = peer {
            debug!(
                "UDP({}): Creating a Client socket at (local: {}, peer: {})",
                cid, local, peer_addr
            );
        } else {
            debug!(
                "UDP({}): Creating a Server socket at (local: {})",
                cid, local
            );
        }

        let socket = Arc::new(socket);

        let (recv_status_tx, recv_status_rx) = oneshot::channel();
        let (send_status_tx, send_status_rx) = oneshot::channel();

        let recv_task = rt.spawn(recv(
            recv_tx,
            socket.clone(),
            deser,
            peer.clone(),
            cid,
            recv_status_tx,
        ));
        let send_task = rt.spawn(send(
            send_rx,
            socket.clone(),
            peer.is_some(),
            cid,
            send_status_tx,
        ));

        Ok(UdpCon {
            cid,
            local,
            peer,
            recv: recv_rx,
            send: Some(send_tx),
            ser,
            recv_task,
            _send_task: send_task,
            recv_status: TaskStatus::new(recv_status_rx),
            send_status: TaskStatus::new(send_status_rx),
        })
    }

    /// Disconnects from the peer.
    ///
    /// You will no longer be able to send or receive
    /// from the peer.
    pub fn disconnect(&mut self) {
        self.recv_task.abort();
        // Closing the send channel will kill the send task once
        // it's msg send que is finished. This is better than
        // simply aborting the task, as any `send()` calls before
        // this will still go through.
        self.send = None;
    }

    /// Updates the statuses of the send receive tasks.
    ///
    /// This should be called before doing several other operations
    /// in order to get accurate results and errors.
    pub fn update_status(&mut self) {
        self.send_status.update();
        self.recv_status.update();
    }

    /// Checks if the connection is open.
    ///
    /// It is ***strongly*** recommended to call
    /// [`update_status()`](Self::update_status)
    /// before calling this function, as the result is only
    /// accurate to the last time that
    /// [`update_status()`](Self::update_status) was called.
    pub fn open(&self) -> bool {
        self.get_send_status().is_none() && self.get_recv_status().is_none() && self.send.is_some()
    }

    /// Gets the status of the send task.
    ///
    /// [`None`] indicates that the send task has not
    /// completed yet. Otherwise a reference is given
    /// to the [`Result`] of the task.
    ///
    /// It is ***strongly*** recommended to call
    /// [`update_status()`](Self::update_status)
    /// before calling this function, as the result is only
    /// accurate to the last time that
    /// [`update_status()`](Self::update_status) was called.
    pub fn get_send_status(&self) -> Option<&io::Result<()>> {
        self.send_status.done()
    }

    /// Gets the status of the receive task.
    ///
    /// [`None`] indicates that the send task has not
    /// completed yet. Otherwise a reference is given
    /// to the [`Result`] of the task.
    ///
    /// It is ***strongly*** recommended to call
    /// [`update_status()`](Self::update_status)
    /// before calling this function, as the result is only
    /// accurate to the last time that
    /// [`update_status()`](Self::update_status) was called.
    pub fn get_recv_status(&self) -> Option<&io::Result<()>> {
        self.recv_status.done()
    }

    /// Sends `msg` to `addr`.
    ///
    /// `mid` ***MUST*** already be checked to be a
    /// valid [`MId`].
    ///
    /// It is ***strongly*** recommended to call
    /// [`update_status()`](Self::update_status)
    /// before calling this function, as the result is only
    /// accurate to the last time that
    /// [`update_status()`](Self::update_status) was called.
    ///
    /// If called on a client type connection `addr`
    /// will be ignored and `msg` will be  sent to
    /// the connected computer.
    pub fn send_to(&self, mid: MId, msg: &(dyn Any + Send + Sync), addr: SocketAddr) -> Result<(), NetError> {
        let ser_fn = self.ser[mid];
        let ser = ser_fn(msg).map_err(|_| NetError::Ser)?;

        if !self.open() {
            return Err(NetError::Closed);
        }
        let sender = self.send.as_ref().ok_or(NetError::Closed)?;
        sender.send((mid, ser, addr)).map_err(|_| NetError::Closed)
    }

    /// Sends `msg` to the connected computer.
    ///
    /// `mid` ***MUST*** already be checked to be a
    /// valid [`MId`].
    ///
    /// It is ***strongly*** recommended to call
    /// [`update_status()`](Self::update_status)
    /// before calling this function, as the result is only
    /// accurate to the last time that
    /// [`update_status()`](Self::update_status) was called.
    ///
    /// Only works for a Client type connection.
    pub fn send(&self, mid: MId, msg: &(dyn Any + Send + Sync)) -> Result<(), NetError> {
        if let Some(addr) = self.peer {
            // The ip addr will get ignored on the receive task
            self.send_to(mid, msg, addr)
        } else {
            Err(NetError::NotClient)
        }
    }

    /// Receives a message if there is one available, otherwise returns [`None`].
    pub fn try_recv(&mut self) -> Option<(MId, Box<dyn Any + Send + Sync>, SocketAddr)> {
        self.recv.try_recv().ok()
    }

    /// Gets the [`CId`] of this [`UdpCon`].
    pub fn cid(&self) -> CId {
        self.cid
    }

    /// Gets the local address of this [`UdpCon`].
    pub fn local_addr(&self) -> SocketAddr {
        self.local
    }

    /// Gets the peer address of this [`UdpCon`].
    pub fn peer_addr(&self) -> Option<SocketAddr> {
        self.peer
    }

    /// Returns whether this [`UdpCon`] is a server type connection.
    pub fn server(&mut self) -> bool {
        self.peer.is_none()
    }

    /// Returns whether this [`UdpCon`] is a client type connection.
    pub fn client(&mut self) -> bool {
        self.peer.is_some()
    }
}

// SEND AND RECEIVE TASKS
/// The receiving task for udp.
///
/// Receives messages from the [`UdpSocket`], deserializes them,
/// and passes them through the channel `messages`.
///
/// If `peer` is [`Some`], It will make sure packets only come from that addr.
async fn recv(
    messages: UnboundedSender<(MId, Box<dyn Any + Send + Sync>, SocketAddr)>,
    socket: Arc<UdpSocket>,
    deser: Vec<DeserFn>,
    peer: Option<SocketAddr>,
    cid: CId,
    status: oneshot::Sender<io::Result<()>>,
) {
    let task = recv_inner(messages, socket, deser, peer, cid).await;
    let _ = status.send(task);
}

/// The logic for the receive task.
async fn recv_inner(
    messages: UnboundedSender<(MId, Box<dyn Any + Send + Sync>, SocketAddr)>,
    socket: Arc<UdpSocket>,
    deser: Vec<DeserFn>,
    peer: Option<SocketAddr>,
    cid: CId,
) -> Result<(), Error> {
    let client = peer.is_some();
    let mut buff = [0; MAX_PACKET_SIZE];

    loop {
        // Receive the data.
        let (n, addr) = socket.recv_from(&mut buff).await.map_err(|e| {
            error!("UDP_R({}): Got IO error: {} while receiving data.", cid, e);
            e
        })?;

        // If we are a client connection, make sure the packet was
        // from the address that we are connected to
        if let Some(peer) = peer {
            if addr != peer {
                warn!(
                    "UDP_R({}): Got a packet from a different address than the peer \
					connected peer: ({}), packet from: ({}). Discarding packet.",
                    cid, peer, addr
                );
                continue;
            }
        }

        if n == 0 {
            let e_msg = format!("UDP_R({}): The peer ({}) closed the connection.", cid, addr);
            debug!("{}", e_msg);
            if client {
                return Err(Error::new(ErrorKind::ConnectionAborted, e_msg));
            } else {
                continue;
            }
        }

        let header = Header::from_be_bytes(&buff[..4]);
        let total_expected_len = header.len + 4;

        if total_expected_len > MAX_PACKET_SIZE {
            let e_msg = format!(
                "UDP_R({}): The header of a received packet indicates a size of {}, \
                but the max allowed packet size is {}.\
				carrier-pigeon never sends a packet greater than this. \
				This packet was likely not sent by carrier-pigeon. \
                Discarding this packet.",
                cid, total_expected_len, MAX_PACKET_SIZE
            );
            error!("{}", e_msg);
            if client {
                return Err(Error::new(ErrorKind::InvalidData, e_msg));
            } else {
                continue;
            }
        }

        if n != total_expected_len {
            error!(
                "UDP_R({}): The header specified that the packet size was {} bytes.\
                However, {} bytes were read. Discarding this possibly corrupted packet",
                cid, total_expected_len, n
            );
            continue;
        }

        let deser_fn = match deser.get(header.mid) {
            Some(d) => *d,
            None => {
                error!(
                    "UDP_R({}): Invalid MId read: {}. Max MId: {}.",
                    cid,
                    header.mid,
                    deser.len() - 1
                );
                continue;
            }
        };

        let msg = match deser_fn(&buff[4..]) {
            Ok(msg) => msg,
            Err(e) => {
                error!("UDP_R({}): Deserialization error occurred. {}", cid, e);
                continue;
            }
        };

        trace!(
            "TCP_R({}): Received packet with MId: {}, len: {}",
            cid,
            header.mid,
            total_expected_len
        );
        if let Err(_) = messages.send((header.mid, msg, addr)) {
            // The receiver (or more likely the entire connection type) was dropped.
            // This means we should disconnect.
            return Ok(());
        }
    }
}

/// The sending task for UDP.
///
/// Gets messages from the `messages` channel, adds a 4 byte header,
/// and sends them through the [`UdpSocket`].
///
/// If `client` is `true`, It will ignore the [`SocketAddr`] from the channel,
/// and send it on the connected address instead. If `client` is false, it will
/// send the message to the address from the channel.
///
/// When the task exits it will return its exit status through the
/// `status` channel.
async fn send(
    messages: UnboundedReceiver<(MId, Vec<u8>, SocketAddr)>,
    socket: Arc<UdpSocket>,
    client: bool,
    cid: CId,
    status: oneshot::Sender<io::Result<()>>,
) {
    let task = send_inner(messages, socket, client, cid).await;
    let _ = status.send(task);
}

/// The logic for the send task.
async fn send_inner(
    mut messages: UnboundedReceiver<(MId, Vec<u8>, SocketAddr)>,
    socket: Arc<UdpSocket>,
    client: bool,
    cid: CId,
) -> Result<(), Error> {
    let mut buff = [0; MAX_PACKET_SIZE];

    while let Some((mid, packet, addr)) = messages.recv().await {
        let total_len = packet.len() + 4;
        // Check if the packet is valid, and should be sent.
        if total_len > MAX_PACKET_SIZE {
            error!(
                "UDP_S({}): Outgoing packet size is greater than the maximum packet size ({}). \
				MId: {}, size: {}. Discarding packet.",
                cid, MAX_PACKET_SIZE, mid, total_len
            );
            continue;
        }
        if total_len > MAX_SAFE_PACKET_SIZE {
            warn!(
                "UDP_S({}): Outgoing packet size is greater than the maximum SAFE packet size.\
			    MId: {}, size: {}. Sending packet anyway.",
                cid, mid, total_len
            );
        }
        // Packet can be sent!

        let header = Header::new(mid, packet.len());
        let h_bytes = header.to_be_bytes();
        // put the header in the front of the packet
        for (i, b) in h_bytes.into_iter().chain(packet.into_iter()).enumerate() {
            buff[i] = b;
        }

        // Send
        let n = if client {
            trace!("UDP_S({}): Sending mid {}.", cid, mid);
            socket.send(&buff[..total_len]).await?
        } else {
            trace!("UDP_S({}): Sending mid {} to {}.", cid, mid, addr);
            socket.send_to(&buff[..total_len], addr).await?
        };

        // Make sure it sent correctly.
        if n != total_len {
            error!(
                "UDP_S({}): Couldn't send all the bytes of a packet (mid: {}). \
				Wanted to send {} but could only send {}",
                cid, mid, total_len, n
            );
        }
    }
    Ok(())
}

impl Drop for UdpCon {
    fn drop(&mut self) {
        self.recv_task.abort();
    }
}
