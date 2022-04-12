use crate::net::{Header, MAX_MESSAGE_SIZE};
use crate::MId;
use io::Error;
use log::{error, trace};
use std::io;
use std::io::{ErrorKind, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpStream};

/// A type wrapping a [`TcpStream`].
///
/// Provides read/write abstractions for sending `carrier-pigeon` messages.
pub struct TcpCon {
    buff: [u8; MAX_MESSAGE_SIZE],
    tcp: TcpStream,
}

impl TcpCon {
    /// Creates a new [`TcpCon`] from the [`TcpStream`] `tcp`.
    pub fn from_stream(tcp: TcpStream) -> Self {
        TcpCon {
            buff: [0; MAX_MESSAGE_SIZE],
            tcp,
        }
    }

    /// Sends the payload `payload` to the peer.
    ///
    /// This constructs a header, and builds the message, and sends it.
    pub fn send(&mut self, mid: MId, payload: &[u8]) -> io::Result<()> {
        let total_len = payload.len() + 4;
        // Check if the packet is valid, and should be sent.
        if total_len > MAX_MESSAGE_SIZE {
            let e_msg = format!(
                "TCP: Outgoing packet size is greater than the maximum packet size ({}). \
				MId: {}, size: {}. Discarding packet.",
                MAX_MESSAGE_SIZE, mid, total_len
            );
            error!("{}", e_msg);
            return Err(Error::new(ErrorKind::InvalidData, e_msg));
        }
        // Packet can be sent!

        let header = Header::new(mid, payload.len());
        let h_bytes = header.to_be_bytes();
        // write the header and packet to the buffer to combine them.

        for (i, b) in h_bytes.iter().chain(payload.iter()).enumerate() {
            self.buff[i] = *b;
        }

        // Send
        trace!("TCP: Sending packet with MId: {}, len: {}", mid, total_len);
        self.tcp.write_all(&self.buff[..total_len])?;
        Ok(())
    }

    pub fn recv(&mut self) -> io::Result<(MId, &[u8])> {
        // Peak the first 4 bytes for the header.
        match self.tcp.peek(&mut self.buff[..4])? {
            4 => {} // Success
            0 => {
                return Err(Error::new(
                    ErrorKind::ConnectionAborted,
                    "TCP: The peer closed the connection.",
                ))
            }
            _ => {
                return Err(Error::new(
                    ErrorKind::WouldBlock,
                    "Data for entire message has not arrived yet.",
                ))
            }
        }
        let header = Header::from_be_bytes(&self.buff[..4]);
        let total_expected_len = header.len + 4;

        if header.len > MAX_MESSAGE_SIZE - 4 {
            let e_msg = format!(
                "TCP: The header of a received message indicates a size of {},\
	                but the max allowed message size is {}.\
					carrier-pigeon never sends a message greater than this; \
					this message was likely not sent by carrier-pigeon. \
	                Discarding this message and closing connection.",
                header.len, MAX_MESSAGE_SIZE
            );
            error!("{}", e_msg);
            self.tcp.shutdown(Shutdown::Both)?;
            return Err(Error::new(ErrorKind::InvalidData, e_msg));
        }

        // Read data. The header will be read again as it was peaked earlier.
        self.tcp.read_exact(&mut self.buff[..header.len + 4])?;
        trace!(
            "TCP: Received msg of MId {}, len {}",
            header.mid,
            total_expected_len,
        );

        Ok((header.mid, &self.buff[4..header.len + 4]))
    }

    /// Moves the internal [`TcpStream`] into or out of nonblocking mode.
    pub fn set_nonblocking(&self, nonblocking: bool) -> io::Result<()> {
        self.tcp.set_nonblocking(nonblocking)
    }

    /// Closes the connection by flushing then shutting down the [`TcpStream`].
    pub fn close(&mut self) -> io::Result<()> {
        self.tcp.flush()?;
        self.tcp.shutdown(Shutdown::Both)
    }

    /// Returns the socket address of the remote peer of this TCP connection.
    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        self.tcp.peer_addr()
    }

    /// Returns the socket address of the local half of this TCP connection.
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.tcp.local_addr()
    }
}
