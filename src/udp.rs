use crate::net::MAX_SAFE_MESSAGE_SIZE;
use crate::MId;
use log::{debug, error, trace};
use std::io;
use std::io::{Error, ErrorKind};
use std::net::{SocketAddr, UdpSocket};
use crate::header::{TCP_HEADER_LEN, UDP_HEADER_LEN, UdpHeader};

/// A type wrapping a [`UdpSocket`].
///
/// Provides read/write abstractions for sending `carrier-pigeon` messages.
pub struct UdpCon {
    /// Used for receiving only. This way send calls can take immutable refs.
    buff: Vec<u8>,
    udp: UdpSocket,
}

impl UdpCon {
    /// Creates a new [`UdpCon`] by creating a new [`UdpSocket`] that connects to `peer`. Sets the
    /// socket to non-blocking.
    pub fn new(local: SocketAddr, peer: Option<SocketAddr>, max_msg_size: usize) -> io::Result<Self> {
        let udp = UdpSocket::bind(local)?;
        if let Some(peer) = peer {
            udp.connect(peer)?;
        }
        udp.set_nonblocking(true)?;
        Ok(UdpCon {
            buff: vec![0; max_msg_size + UDP_HEADER_LEN],
            udp,
        })
    }

    /// Gets the total buffer size.
    fn buff_size(&self) -> usize {
        self.buff.len()
    }

    /// Gets the maximum message size.
    fn max_msg_size(&self) -> usize {
        self.buff.len()-TCP_HEADER_LEN
    }

    pub fn send_to(&self, addr: SocketAddr, mid: MId, payload: &[u8]) -> io::Result<()> {
        let buff = self.send_shared(mid, payload)?;
        let len = buff.len();

        trace!(
            "UDP: Sending message with MId: {}, len: {} to {}.",
            mid, len, addr
        );
        let n = self.udp.send_to(&buff[..len], addr)?;

        // Make sure it sent correctly.
        if n != len {
            error!(
                "UDP: Couldn't send all the bytes of a message (mid: {}). \
				Wanted to send {} but could only send {}. This will likely \
				cause issues on the other side.",
                mid, len, n
            );
        }
        Ok(())
    }

    pub fn send(&self, mid: MId, payload: &[u8]) -> io::Result<()> {
        let buff = self.send_shared(mid, payload)?;
        let len = buff.len();

        trace!("UDP: Sending message with MId: {}, len: {}.", mid, len);
        let n = self.udp.send(&buff[..len])?;

        // Make sure it sent correctly.
        if n != len {
            error!(
                "UDP: Couldn't send all the bytes of a message (mid: {}). \
				Wanted to send {} but could only send {}. This will likely \
				cause issues on the other side.",
                mid, len, n
            );
        }
        Ok(())
    }

    /// The shared code for sending a message. Produces a buffer given the payload
    fn send_shared(&self, mid: MId, payload: &[u8]) -> io::Result<Vec<u8>> {
        let total_len = payload.len() + UDP_HEADER_LEN;
        let mut buff = vec![0; total_len];
        // Check if the message is valid, and should be sent.
        if total_len > self.buff_size() {
            let e_msg = format!(
                "UDP: Outgoing message size is greater than the maximum message size ({}). \
                MId: {}, size: {}. Discarding message.",
                self.max_msg_size(), mid, total_len
            );
            return Err(Error::new(ErrorKind::InvalidData, e_msg));
        }

        if total_len > MAX_SAFE_MESSAGE_SIZE {
            debug!(
                "UDP: Outgoing message size is greater than the maximum SAFE message size.\
                MId: {}, size: {}. Sending message anyway.",
                mid, total_len
            );
        }
        // Message can be sent!

        let header = UdpHeader::new(mid);
        let h_bytes = header.to_be_bytes();
        // put the header in the front of the message
        for (i, b) in h_bytes.into_iter().enumerate() {
            buff[i] = b;
        }
        for (i, b) in payload.iter().enumerate() {
            buff[i+UDP_HEADER_LEN] = *b;
        }
        Ok(buff)
    }

    pub fn recv(&mut self) -> io::Result<(MId, u32, &[u8])> {
        let n = self.udp.recv(&mut self.buff)?;
        let (mid, time, bytes) = self.recv_shared(n)?;
        trace!("UDP: Received msg of MId {}, len {}", mid, bytes.len());
        Ok((mid, time, bytes))
    }

    pub fn recv_from(&mut self) -> io::Result<(SocketAddr, MId, u32, &[u8])> {
        let (n, from) = self.udp.recv_from(&mut self.buff)?;
        let (mid, time, bytes) = self.recv_shared(n)?;
        trace!(
            "UDP: Received msg of MId {}, len {}, from {}",
            mid,
            bytes.len(),
            from
        );
        Ok((from, mid, time, bytes))
    }

    fn recv_shared(&mut self, n: usize) -> io::Result<(MId, u32, &[u8])> {
        // Data should already be received.
        if n == 0 {
            let e_msg = format!("UDP: The connection was dropped.");
            return Err(Error::new(ErrorKind::NotConnected, e_msg));
        }

        let header = UdpHeader::from_be_bytes(&self.buff[..UDP_HEADER_LEN]);

        Ok((header.mid, header.time, &self.buff[UDP_HEADER_LEN..n]))
    }

    /// Moves the internal [`UdpSocket`] into or out of nonblocking mode.
    pub fn set_nonblocking(&self, nonblocking: bool) -> io::Result<()> {
        self.udp.set_nonblocking(nonblocking)
    }

    /// Returns the socket address of the remote peer of this UDP connection.
    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        self.udp.peer_addr()
    }

    /// Returns the socket address of the local half of this UDP connection.
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.udp.local_addr()
    }
}
