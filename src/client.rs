use crate::message_table::{MsgTableParts, DISCONNECT_TYPE_MID, RESPONSE_TYPE_MID};
use crate::net::{Header, Status, Transport, MAX_MESSAGE_SIZE, MAX_SAFE_MESSAGE_SIZE};
use crate::MId;
use crossbeam_channel::internal::SelectHandle;
use crossbeam_channel::Receiver;
use log::{debug, error, trace, warn};
use std::any::{Any, TypeId};
use std::fmt::{Display, Formatter};
use std::io;
use std::io::{Error, ErrorKind, Read, Write};
use std::marker::PhantomData;
use std::net::{Shutdown, SocketAddr, TcpStream, UdpSocket};
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
    status: Status<D>,
    /// The buffer used for sending and receiving packets
    buff: [u8; MAX_MESSAGE_SIZE],
    /// The received message buffer.
    ///
    /// Each [`MId`] has its own vector.
    msg_buff: Vec<Vec<Box<dyn Any + Send + Sync>>>,

    /// The TCP connection for this client.
    tcp: TcpStream,
    /// The UDP connection for this client.
    udp: UdpSocket,

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

    /// Creates a new [`Client`] asynchronously.
    fn new_blocking(
        peer: SocketAddr,
        parts: MsgTableParts<C, R, D>,
        con_msg: C,
    ) -> io::Result<(Self, R)> {
        debug!("Attempting to create a client connection to {}", peer);
        // TODO: add timeout.
        // TODO: add an option to get udp addr
        // TCP & UDP Connections.
        let tcp = TcpStream::connect(peer)?;
        let local_addr = tcp.local_addr().unwrap();
        trace!(
            "TcpStream established from {} to {}",
            local_addr,
            tcp.peer_addr().unwrap()
        );
        let udp = UdpSocket::bind(local_addr)?;
        udp.connect(peer)?;
        trace!(
            "UdpSocket connected from {} to {}",
            udp.local_addr().unwrap(),
            udp.peer_addr().unwrap()
        );

        tcp.set_read_timeout(Some(Duration::from_millis(10_000)))?;

        let mid_count = parts.tid_map.len();
        let mut msg_buff = Vec::with_capacity(mid_count);
        for _ in 0..mid_count {
            msg_buff.push(vec![]);
        }

        let mut client = Client {
            status: Status::Connected,
            buff: [0; MAX_MESSAGE_SIZE],
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
    fn send_tcp(&mut self, mid: MId, packet: Vec<u8>) -> io::Result<()> {
        let total_len = packet.len() + 4;
        // Check if the packet is valid, and should be sent.
        if total_len > MAX_MESSAGE_SIZE {
            error!(
                "TCP: Outgoing packet size is greater than the maximum packet size ({}). \
				MId: {}, size: {}. Discarding packet.",
                MAX_MESSAGE_SIZE, mid, total_len
            );
            return Err(io::Error::new(
                ErrorKind::InvalidData,
                "The packet was too long",
            ));
        }
        if packet.len() > MAX_SAFE_MESSAGE_SIZE {
            warn!(
                "TCP: Outgoing packet size is greater than the maximum SAFE packet size.\
			    MId: {}, size: {}. Sending packet anyway.",
                mid, total_len
            );
        }
        // Packet can be sent!

        let header = Header::new(mid, packet.len());
        let h_bytes = header.to_be_bytes();
        // write the header and packet to the buffer to combine them.
        for (i, b) in h_bytes.into_iter().chain(packet.into_iter()).enumerate() {
            self.buff[i] = b;
        }

        // Send
        trace!("TCP: Sending packet with MId: {}, len: {}", mid, total_len);
        let n = self.tcp.write(&self.buff[..total_len])?;

        // Make sure it sent correctly.
        if n != total_len {
            error!(
                "TCP: Couldn't send all the bytes of a packet (mid: {}). \
				Wanted to send {} but could only send {}.",
                mid, total_len, n
            );
        }
        Ok(())
    }

    /// A function that encapsulates the sending logic for the UDP transport.
    fn send_udp(&mut self, mid: MId, packet: Vec<u8>) -> io::Result<()> {
        let total_len = packet.len() + 4;

        // Check if the packet is valid, and should be sent.
        if total_len > MAX_MESSAGE_SIZE {
            let msg = format!(
                "UDP: Outgoing packet size is greater than the maximum packet size ({}). \
                MId: {}, size: {}. Discarding packet.",
                MAX_MESSAGE_SIZE, mid, total_len
            );
            error!("{}", msg);
            return Err(io::Error::new(ErrorKind::InvalidData, msg));
        }

        if total_len > MAX_SAFE_MESSAGE_SIZE {
            warn!(
                "UDP: Outgoing packet size is greater than the maximum SAFE packet size.\
                MId: {}, size: {}. Sending packet anyway.",
                mid, total_len
            );
        }
        // Packet can be sent!

        let header = Header::new(mid, packet.len());
        let h_bytes = header.to_be_bytes();
        // put the header in the front of the packet
        for (i, b) in h_bytes.into_iter().chain(packet.into_iter()).enumerate() {
            self.buff[i] = b;
        }

        // Send
        trace!("UDP: Sending mid {}.", mid);
        let n = self.udp.send(&self.buff[..total_len])?;

        // Make sure it sent correctly.
        if n != total_len {
            error!(
                "UDP: Couldn't send all the bytes of a packet (mid: {}). \
				Wanted to send {} but could only send {}",
                mid, total_len, n
            );
        }
        Ok(())
    }

    /// A function that encapsulates the receiving logic for the TCP transport.
    ///
    /// Any errors in receiving are returned. An error of type [`WouldBlock`] means
    /// no more packets can be yielded without blocking.
    fn recv_tcp(&mut self) -> io::Result<(MId, Box<dyn Any + Send + Sync>)> {
        // TODO: peak to not block
        let h_n = self.tcp.read(&mut self.buff[..4])?;

        if h_n == 0 {
            let e_msg = format!("TCP: The peer closed the connection.");
            debug!("{}", e_msg);
            return Err(Error::new(ErrorKind::ConnectionAborted, e_msg));
        } else if h_n < 4 {
            let e_msg = format!("TCP: Not enough bytes for header ({}).", h_n);
            warn!("{}", e_msg);
            return Err(Error::new(ErrorKind::ConnectionAborted, e_msg));
        }
        let header = Header::from_be_bytes(&self.buff[..4]);
        if header.mid > self.parts.mid_count() {
            let e_msg = format!(
                "TCP: Got a packet specifying MId {}, but the maximum MId is {}.",
                header.mid,
                self.parts.mid_count()
            );
            return Err(Error::new(ErrorKind::InvalidData, e_msg));
        }

        if header.len + 4 > MAX_MESSAGE_SIZE {
            let e_msg = format!(
                "TCP: The header of a received packet indicates a size of {},\
	                but the max allowed packet size is {}.\
					carrier-pigeon never sends a packet greater than this. \
					This packet was likely not sent by carrier-pigeon. \
	                Discarding this packet.",
                header.len, MAX_MESSAGE_SIZE
            );
            error!("{}", e_msg);
            return Err(Error::new(ErrorKind::InvalidData, e_msg));
        }

        // Read data.
        self.tcp.read_exact(&mut self.buff[..header.len])?;

        let deser_fn = match self.parts.deser.get(header.mid) {
            Some(d) => *d,
            None => {
                let msg = format!(
                    "Invalid MId {} read from peer. Max MId: {}.",
                    header.mid,
                    self.parts.deser.len() - 1
                );
                return Err(io::Error::new(ErrorKind::InvalidData, msg));
            }
        };

        let msg = deser_fn(&self.buff[..header.len]).map_err(|_| {
            io::Error::new(
                ErrorKind::InvalidData,
                "Got error when deserializing data from peer.",
            )
        })?;

        if header.mid == DISCONNECT_TYPE_MID {
            // Remote connection disconnected.
            debug!("TCP: Remote computer sent disconnect packet.");
        }

        trace!(
            "TCP: Received packet with MId: {}, len: {}",
            header.mid,
            header.len + 4
        );
        Ok((header.mid, msg))
    }

    /// A function that encapsulates the receiving logic for the UDP transport.
    ///
    /// Any errors in receiving are returned. An error of type [`WouldBlock`] means
    /// no more packets can be yielded without blocking. [`InvalidData`] likely means
    /// carrier-pigeon detected bad data.
    pub fn recv_udp(&mut self) -> io::Result<(MId, Box<dyn Any + Send + Sync>)> {
        // Receive the data.
        let n = self.udp.recv(&mut self.buff)?;

        if n == 0 {
            let e_msg = format!("UDP: The connection was dropped.");
            warn!("{}", e_msg);
            return Err(Error::new(ErrorKind::ConnectionAborted, e_msg));
        }

        let header = Header::from_be_bytes(&self.buff[..4]);
        if header.mid > self.parts.mid_count() {
            let e_msg = format!(
                "UDP: Got a packet specifying MId {}, but the maximum MId is {}.",
                header.mid,
                self.parts.mid_count()
            );
            return Err(Error::new(ErrorKind::InvalidData, e_msg));
        }
        let total_expected_len = header.len + 4;

        if total_expected_len > MAX_MESSAGE_SIZE {
            let e_msg = format!(
                "UDP: The header of a received packet indicates a size of {}, \
                but the max allowed packet size is {}.\
                carrier-pigeon never sends a packet greater than this. \
                This packet was likely not sent by carrier-pigeon. \
                Discarding this packet.",
                total_expected_len, MAX_MESSAGE_SIZE
            );
            error!("{}", e_msg);
            return Err(Error::new(ErrorKind::InvalidData, e_msg));
        }

        if n != total_expected_len {
            let e_msg = format!(
                "UDP: The header specified that the packet size was {} bytes.\
                However, {} bytes were read. Discarding invalid packet.",
                total_expected_len, n
            );
            error!("{}", e_msg);
            return Err(Error::new(ErrorKind::InvalidData, e_msg));
        }

        let deser_fn = match self.parts.deser.get(header.mid) {
            Some(d) => *d,
            None => {
                let e_msg = format!(
                    "UDP: Invalid MId read: {}. Max MId: {}.",
                    header.mid,
                    self.parts.deser.len() - 1
                );
                error!("{}", e_msg);
                return Err(Error::new(ErrorKind::InvalidData, e_msg));
            }
        };

        let msg = match deser_fn(&self.buff[4..]) {
            Ok(msg) => msg,
            Err(e) => {
                let e_msg = format!("UDP: Deserialization error occurred. {}", e);
                error!("{}", e_msg);
                return Err(Error::new(ErrorKind::InvalidData, e_msg));
            }
        };

        trace!(
            "UDP: Received packet with MId: {}, len: {}",
            header.mid,
            total_expected_len
        );

        Ok((header.mid, msg))
    }

    /// Disconnects from the server. You should ***always*** call this
    /// method before dropping the client to let the server know that
    /// you intentionally disconnected. The `discon_msg` allows you to
    /// give a reason for the disconnect.
    pub fn disconnect(&mut self, discon_msg: &D) -> io::Result<()> {
        debug!("Disconnecting client.");
        self.send(discon_msg)?;
        self.tcp.flush()?;
        self.tcp.shutdown(Shutdown::Both)?;
        // No shutdown method on udp.
        // TODO: check status on send client and server
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
        if !self.parts.tid_map.contains_key(&tid) {
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
            Transport::TCP => self.send_tcp(mid, b),
            Transport::UDP => self.send_udp(mid, b),
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

    /// Gets the local address.
    pub fn local_addr(&self) -> SocketAddr {
        self.tcp.local_addr().unwrap()
    }

    /// Gets the address of the peer.
    pub fn peer_addr(&self) -> SocketAddr {
        self.tcp.peer_addr().unwrap()
    }
}

#[derive(Debug)]
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

    /// Gets the client. This will yield a value if [`done()`](Self::done)
    /// returned `true`.
    pub fn get(self) -> Result<io::Result<(Client<C, R, D>, R)>, Self> {
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
