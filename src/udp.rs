use crate::net::{MAX_MESSAGE_SIZE, MAX_SAFE_MESSAGE_SIZE};
use crate::{Header, MId};
use log::{error, trace, warn};
use std::io;
use std::io::{Error, ErrorKind};
use std::net::{SocketAddr, UdpSocket};

/// A type wrapping a [`UdpSocket`].
///
/// Provides read/write abstractions for sending `carrier-pigeon` messages.
pub struct UdpCon {
    /// Used for receiving only. This way send calls can take immutable refs.
    buff: [u8; MAX_MESSAGE_SIZE],
    udp: UdpSocket,
}

impl UdpCon {
    /// Creates a new [`UdpCon`] by creating a new [`UdpSocket`] that connects to `peer`.
    /// Sets the socket to non-blocking.
    pub fn new(local: SocketAddr, peer: Option<SocketAddr>) -> io::Result<Self> {
        let udp = UdpSocket::bind(local)?;
        if let Some(peer) = peer {
            udp.connect(peer)?;
        }
        udp.set_nonblocking(true)?;
        Ok(UdpCon {
            buff: [0; MAX_MESSAGE_SIZE],
            udp,
        })
    }

    pub fn send_to(&self, addr: SocketAddr, mid: MId, payload: &[u8]) -> io::Result<()> {
        let buff = self.send_shared(mid, payload)?;
        let len = buff.len();

        trace!(
            "UDP: Sending packet with MId: {}, len: {} to {}.",
            mid, len, addr
        );
        let n = self.udp.send_to(&buff[..len], addr)?;

        // Make sure it sent correctly.
        if n != len {
            error!(
                "UDP: Couldn't send all the bytes of a packet (mid: {}). \
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

        trace!("UDP: Sending packet with MId: {}, len: {}.", mid, len);
        let n = self.udp.send(&buff[..len])?;

        // Make sure it sent correctly.
        if n != len {
            error!(
                "UDP: Couldn't send all the bytes of a packet (mid: {}). \
				Wanted to send {} but could only send {}.",
                mid, len, n
            );
        }
        Ok(())
    }

    /// The shared code for sending a message.
    /// Produces a buffer given the payload
    fn send_shared(&self, mid: MId, payload: &[u8]) -> io::Result<Vec<u8>> {
        let total_len = payload.len() + 4;
        let mut buff = vec![0; total_len];
        // Check if the packet is valid, and should be sent.
        if total_len > MAX_MESSAGE_SIZE {
            let e_msg = format!(
                "UDP: Outgoing packet size is greater than the maximum packet size ({}). \
                MId: {}, size: {}. Discarding packet.",
                MAX_MESSAGE_SIZE, mid, total_len
            );
            error!("{}", e_msg);
            return Err(Error::new(ErrorKind::InvalidData, e_msg));
        }

        if total_len > MAX_SAFE_MESSAGE_SIZE {
            warn!(
                "UDP: Outgoing packet size is greater than the maximum SAFE packet size.\
                MId: {}, size: {}. Sending packet anyway.",
                mid, total_len
            );
        }
        // Packet can be sent!

        let header = Header::new(mid, payload.len());
        let h_bytes = header.to_be_bytes();
        // put the header in the front of the packet
        for (i, b) in h_bytes.into_iter().enumerate() {
            buff[i] = b;
        }
        for (i, b) in payload.iter().enumerate() {
            buff[i+4] = *b;
        }
        Ok(buff)
    }

    pub fn recv(&mut self) -> io::Result<(MId, &[u8])> {
        let n = self.udp.recv(&mut self.buff)?;
        let (mid, bytes) = self.recv_shared(n)?;
        trace!("UDP: Received msg of MId {}, len {}", mid, bytes.len());
        Ok((mid, bytes))
    }

    pub fn recv_from(&mut self) -> io::Result<(SocketAddr, MId, &[u8])> {
        let (n, from) = self.udp.recv_from(&mut self.buff)?;
        let (mid, bytes) = self.recv_shared(n)?;
        trace!(
            "UDP: Received msg of MId {}, len {}, from {}",
            mid,
            bytes.len(),
            from
        );
        Ok((from, mid, bytes))
    }

    fn recv_shared(&mut self, n: usize) -> io::Result<(MId, &[u8])> {
        // Data should already be received.
        if n == 0 {
            let e_msg = format!("UDP: The connection was dropped.");
            warn!("{}", e_msg);
            return Err(Error::new(ErrorKind::NotConnected, e_msg));
        }

        let header = Header::from_be_bytes(&self.buff[..4]);
        let total_expected_len = header.len + 4;

        if total_expected_len > MAX_MESSAGE_SIZE {
            let e_msg = format!(
                "UDP: The header of a received message indicates a size of {}, \
                but the max allowed message size is {}.\
                carrier-pigeon never sends a message greater than this. \
                This message was likely not sent by carrier-pigeon. \
                Discarding this message.",
                total_expected_len, MAX_MESSAGE_SIZE
            );
            error!("{}", e_msg);
            return Err(Error::new(ErrorKind::InvalidData, e_msg));
        }

        if n != total_expected_len {
            let e_msg = format!(
                "UDP: The header specified that the message size was {} bytes.\
                However, {} bytes were read. Discarding invalid message.",
                total_expected_len, n
            );
            error!("{}", e_msg);
            return Err(Error::new(ErrorKind::InvalidData, e_msg));
        }

        Ok((header.mid, &self.buff[4..header.len + 4]))
    }

    /// Moves the internal [`TcpStream`] into or out of nonblocking mode.
    pub fn set_nonblocking(&self, nonblocking: bool) -> io::Result<()> {
        self.udp.set_nonblocking(nonblocking)
    }

    /// Returns the socket address of the remote peer of this TCP connection.
    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        self.udp.peer_addr()
    }

    /// Returns the socket address of the local half of this TCP connection.
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.udp.local_addr()
    }
}
