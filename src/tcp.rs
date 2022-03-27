use std::any::Any;
use std::io;
use std::io::{Error, ErrorKind};
use std::marker::PhantomData;
use std::net::SocketAddr;
use log::{debug, error, trace, warn};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::TcpStream;
use tokio::runtime::Handle;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use crate::message_table::DISCONNECT_TYPE_MID;
use crate::net::{CId, MId};
use crate::net::{DeserFn, SerFn};
use crate::net::{Header, NetError, TaskStatus};
use crate::net::{MAX_PACKET_SIZE, MAX_SAFE_PACKET_SIZE};

/// A raw TCP connection.
///
/// This hosts 2 async tasks that allow the sending and
/// receiving of messages. Messages are sent to and from
/// these tasks from a sync context using channels.
///
/// Higher level [`Client`] and [`Server`] abstractions
/// are recommended. [TcpCon] and [UdpCon] should only be
/// used if you really don't like my client/server impl :(
/// and would like more control.
///
/// The status of the send/receive tasks can be fetched
/// with the `get_send_status()` and `get_recv_status()`
/// functions. Remember to call the `update_status()`
/// function before calling either of those.
pub struct TcpCon<D>
where
    D: Any + Send + Sync,
{
    /// The Connection ID. This is used in
    /// logging for discerning between connections.
    /// Not functionally used.
    cid: CId,
    /// The local address.
    local: SocketAddr,
    /// The peer address.
    peer: SocketAddr,

    /// The message sender.
    send: Option<UnboundedSender<(MId, Vec<u8>)>>,
    /// The message receiver.
    recv: UnboundedReceiver<(MId, Box<dyn Any + Send + Sync>)>,

    /// The status of the `send` task.
    send_status: TaskStatus<io::Result<()>>,
    /// The status of the `recv` task.
    recv_status: TaskStatus<io::Result<D>>,

    /// The list of serialization functions for messages.
    ser: Vec<SerFn>,

    /// A handle to the receiving task.
    /// It will not close on it's own and should be manually aborted.
    recv_task: JoinHandle<()>,
    _pd: PhantomData<D>,
}

impl<D: Any + Send + Sync> TcpCon<D> {
    /// Creates a new [`TcpCon`].
    pub async fn new(
        cid: CId,
        peer: SocketAddr,
        deser: Vec<DeserFn>,
        ser: Vec<SerFn>,
        rt: Handle,
    ) -> io::Result<Self> {
        let steam = TcpStream::connect(peer).await?;
        Ok(TcpCon::from_stream(cid, steam, deser, ser, rt))
    }

    /// Creates a new [`TcpCon`] from an existing [`TcpStream`].
    pub fn from_stream(
        cid: CId,
        stream: TcpStream,
        deser: Vec<DeserFn>,
        ser: Vec<SerFn>,
        rt: Handle,
    ) -> Self {
        let local = stream.local_addr().unwrap();
        let peer = stream.peer_addr().unwrap();
        let (read, write) = stream.into_split();

        let (send_tx, send_rx) = unbounded_channel();
        let (recv_tx, recv_rx) = unbounded_channel();

        debug!(
            "TCP({}): Creating a TCP connection at (local: {}, peer: {}).",
            cid, local, peer
        );

        let (recv_status_tx, recv_status_rx) = oneshot::channel();
        let (send_status_tx, send_status_rx) = oneshot::channel();

        let recv_task = rt.spawn(recv(recv_tx, read, deser, cid, recv_status_tx));
        let _send_task = rt.spawn(send(send_rx, write, cid, send_status_tx));

        TcpCon {
            cid,
            local,
            peer,
            send: Some(send_tx),
            recv: recv_rx,
            ser,
            recv_task,
            recv_status: TaskStatus::new(recv_status_rx),
            send_status: TaskStatus::new(send_status_rx),
            _pd: PhantomData,
        }
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
    pub fn get_recv_status(&self) -> Option<&io::Result<D>> {
        self.recv_status.done()
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
    pub fn send(&self, mid: MId, msg: &(dyn Any + Send + Sync)) -> Result<(), NetError> {
        let ser_fn = self.ser[mid];
        let ser = ser_fn(msg).map_err(|_| NetError::Ser)?;

        if !self.open() {
            return Err(NetError::Closed);
        }
        let sender = self.send.as_ref().ok_or(NetError::Closed)?;
        sender.send((mid, ser)).map_err(|_| NetError::Closed)
    }

    /// Receives a message if there is one available, otherwise returns [`None`]
    pub fn try_recv(&mut self) -> Option<(MId, Box<dyn Any + Send + Sync>)> {
        self.recv.try_recv().ok()
    }

    /// Receives a message asynchronously.
    pub async fn recv(&mut self) -> Option<(MId, Box<dyn Any + Send + Sync>)> {
        self.recv.recv().await
    }

    /// Gets the [`CId`] of the [`TcpCon`].
    pub fn cid(&self) -> CId {
        self.cid
    }

    /// Gets the local address of this [`TcpCon`].
    pub fn local_addr(&self) -> SocketAddr {
        self.local
    }

    /// Gets the peer address of this [`TcpCon`].
    pub fn peer_addr(&self) -> SocketAddr {
        self.peer
    }
}

/// The receiving task for udp.
///
/// Receives messages from the [`UdpSocket`], deserializes them,
/// and passes them through the channel `messages`.
///
/// If `peer` is [`Some`], It will make sure packets only come from that addr.
///
/// This task does need to be canceled or it will leave the connection open.
async fn recv<D: Any + Send + Sync>(
    messages: UnboundedSender<(MId, Box<dyn Any + Send + Sync>)>,
    read: OwnedReadHalf,
    deser: Vec<DeserFn>,
    cid: CId,
    status: oneshot::Sender<io::Result<D>>,
) {
    let task = recv_inner(messages, read, deser, cid).await;
    debug!("TCP_R({}): Receive task end", cid);
    if let Err(_e) = status.send(task) {
        error!("TCP_R({}): Failed to send result of task back.", cid);
    }
}

/// The logic for the receive task.
async fn recv_inner<D: Any + Send + Sync>(
    messages: UnboundedSender<(MId, Box<dyn Any + Send + Sync>)>,
    mut read: OwnedReadHalf,
    deser: Vec<DeserFn>,
    cid: CId,
) -> io::Result<D> {
    let mut header_buff = [0; 4];
    let mut buff = [0; MAX_PACKET_SIZE];

    loop {
        // Read the header first
        let h_n = read.read(&mut header_buff).await.map_err(|e| {
            error!("TCP_R({}): Got IO error: {} on tcp receiving task", cid, e);
            e
        })?;

        if h_n == 0 {
            let e_msg = format!("TCP_R({}): The peer closed the connection.", cid);
            debug!("{}", e_msg);
            return Err(Error::new(ErrorKind::ConnectionAborted, e_msg));
        } else if h_n < 4 {
            let e_msg = format!("TCP_R({}): Not enough bytes for header ({}).", cid, h_n);
            warn!("{}", e_msg);
            return Err(Error::new(ErrorKind::ConnectionAborted, e_msg));
        }
        let header = Header::from_be_bytes(&header_buff);

        if header.len + 4 > MAX_PACKET_SIZE {
            let e_msg = format!(
                "TCP_R({}): The header of a received packet indicates a size of {},\
	                but the max allowed packet size is {}.\
					carrier-pigeon never sends a packet greater than this. \
					This packet was likely not sent by carrier-pigeon. \
	                Discarding this packet.",
                cid, header.len, MAX_PACKET_SIZE
            );
            error!("{}", e_msg);
            return Err(Error::new(ErrorKind::InvalidData, e_msg));
        }

        // Read data.
        let n = read
            .read_exact(&mut buff[..header.len])
            .await
            .map_err(|e| {
                error!("TCP_R({}): Got IO error: {} on tcp receiving task", cid, e);
                e
            })?;

        if n != header.len {
            error!(
                "TCP_R({}): The header specified that the packet size was {} bytes.\
                However, {} bytes were read. Discarding this possibly corrupted packet.",
                cid, header.len, n
            );
        }

        let deser_fn = match deser.get(header.mid) {
            Some(d) => *d,
            None => {
                error!(
                    "TCP_R({}): Invalid MId read: {}. Max MId: {}.",
                    cid,
                    header.mid,
                    deser.len()
                );
                continue;
            }
        };

        let msg = deser_fn(&buff[..header.len]).unwrap();

        if header.mid == DISCONNECT_TYPE_MID {
            // Remote connection disconnected.
            debug!("TCP_R({}): Remote computer send disconnect packet.", cid);
            let discon_msg = msg.downcast().map_err(|_| "").unwrap();
            return Ok(*discon_msg);
        }

        trace!(
            "TCP_R({}): Received packet with MId: {}, len: {}",
            cid,
            header.mid,
            header.len + 4
        );
        if let Err(_) = messages.send((header.mid, msg)) {
            // The receiver (or more likely the entire connection type) was dropped.
            // This means we should disconnect.
            return Err(Error::new(
                ErrorKind::BrokenPipe,
                "The receive channel was closed.",
            ));
        }
    }
}

/// The sending task for TCP.
///
/// Gets messages from the `messages` channel, adds a 4 byte header,
/// and sends them through the [`TcpStream`].
///
/// When the task exits it will return its exit status through the
/// `status` channel.
///
/// This task does ***not*** need to be canceled.
/// If the `status` sender closes, so does the task.
async fn send(
    messages: UnboundedReceiver<(MId, Vec<u8>)>,
    mut write: OwnedWriteHalf,
    cid: CId,
    status: oneshot::Sender<io::Result<()>>,
) {
    let task = send_inner(messages, &mut write, cid).await;
    debug!("TCP_S({}): Task closed with status {:?}", cid, task);
    if let Err(e) = write.shutdown().await {
        error!(
            "TCP_S({}): Error occurred when shutting down the TCP stream writer. {}",
            cid, e
        );
    }
    let _ = status.send(task);
}

/// The logic for the send task.
async fn send_inner(
    mut messages: UnboundedReceiver<(MId, Vec<u8>)>,
    write: &mut OwnedWriteHalf,
    cid: CId,
) -> Result<(), Error> {
    let mut buff = [0; MAX_PACKET_SIZE];

    while let Some((mid, packet)) = messages.recv().await {
        let total_len = packet.len() + 4;
        // Check if the packet is valid, and should be sent.
        if total_len > MAX_PACKET_SIZE {
            error!(
                "TCP_S({}): Outgoing packet size is greater than the maximum packet size ({}). \
				MId: {}, size: {}. Discarding packet.",
                cid, MAX_PACKET_SIZE, mid, total_len
            );
            continue;
        }
        if total_len > MAX_SAFE_PACKET_SIZE {
            warn!(
                "TCP_S({}): Outgoing packet size is greater than the maximum SAFE packet size.\
			    MId: {}, size: {}. Sending packet anyway.",
                cid, mid, total_len
            );
        }
        // Packet can be sent!

        let header = Header::new(mid, packet.len());
        let h_bytes = header.to_be_bytes();
        // write the header and packet to the buffer to combine them.
        for (i, b) in h_bytes.into_iter().chain(packet.into_iter()).enumerate() {
            buff[i] = b;
        }

        // Send
        trace!(
            "TCP_S({}): Sending packet with MId: {}, len: {}",
            cid,
            mid,
            total_len
        );
        let n = write.write(&buff[..total_len]).await?;

        // Make sure it sent correctly.
        if n != total_len {
            error!(
                "TCP_S({}): Couldn't send all the bytes of a packet (mid: {}). \
				Wanted to send {} but could only send {}",
                cid, mid, total_len, n
            );
        }
    }
    Ok(())
}

impl<D: Any + Send + Sync> Drop for TcpCon<D> {
    fn drop(&mut self) {
        // The send task should complete automatically after sending
        // everything it needs to because the `self.send` channel will
        // close. The receive task might not however.
        self.recv_task.abort();
    }
}
