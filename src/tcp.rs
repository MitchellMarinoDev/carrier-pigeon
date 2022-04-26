use crate::net::{Header, MAX_MESSAGE_SIZE};
use crate::MId;
use io::Error;
use log::{error, trace};
use std::io;
use std::io::{ErrorKind, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpStream};
use std::sync::RwLock;
use crate::header::HEADER_LEN;

/// A type wrapping a [`TcpStream`].
///
/// Provides read/write abstractions for sending `carrier-pigeon` messages.
pub struct TcpCon {
    buff: [u8; MAX_MESSAGE_SIZE],
    tcp: RwLock<TcpStream>,
}

impl TcpCon {
    /// Creates a new [`TcpCon`] from the [`TcpStream`] `tcp`.
    pub fn from_stream(tcp: TcpStream) -> Self {
        TcpCon {
            buff: [0; MAX_MESSAGE_SIZE],
            tcp: tcp.into(),
        }
    }

    /// Sends the payload `payload` to the peer.
    ///
    /// This constructs a header, and builds the message, and sends it.
    pub fn send(&self, mid: MId, payload: &[u8]) -> io::Result<()> {
        let total_len = payload.len() + HEADER_LEN;
        let mut buff = vec![0; total_len];
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

        for (i, b) in h_bytes.into_iter().enumerate() {
            buff[i] = b;
        }
        for (i, b) in payload.iter().enumerate() {
            buff[i+HEADER_LEN] = *b;
        }

        // Send
        trace!("TCP: Sending packet with MId: {}, len: {}", mid, total_len);
        let mut tcp = self.tcp.write().unwrap();
        tcp.write_all(&buff[..total_len])?;
        Ok(())
    }

    pub fn recv(&mut self) -> io::Result<(MId, &[u8])> {
        // Peak the header.
        let mut tcp = self.tcp.write().unwrap();
        match tcp.peek(&mut self.buff[..HEADER_LEN])? {
            HEADER_LEN => {} // Success
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
        let header = Header::from_be_bytes(&self.buff[..HEADER_LEN]);
        let total_expected_len = header.len + HEADER_LEN;

        if header.len > MAX_MESSAGE_SIZE - HEADER_LEN {
            let e_msg = format!(
                "TCP: The header of a received message indicates a size of {},\
	                but the max allowed message size is {}.\
					carrier-pigeon never sends a message greater than this; \
					this message was likely not sent by carrier-pigeon. \
	                Discarding this message and closing connection.",
                header.len, MAX_MESSAGE_SIZE
            );
            error!("{}", e_msg);
            tcp.shutdown(Shutdown::Both)?;
            return Err(Error::new(ErrorKind::InvalidData, e_msg));
        }

        // Read data. The header will be read again as it was peaked earlier.
        tcp.read_exact(&mut self.buff[..header.len + HEADER_LEN])?;
        trace!(
            "TCP: Received msg of MId {}, len {}",
            header.mid,
            total_expected_len,
        );

        Ok((header.mid, &self.buff[HEADER_LEN..header.len + HEADER_LEN]))
    }

    /// Moves the internal [`TcpStream`] into or out of nonblocking mode.
    pub fn set_nonblocking(&self, nonblocking: bool) -> io::Result<()> {
        self.tcp.read().unwrap().set_nonblocking(nonblocking)
    }

    /// Closes the connection by flushing then shutting down the [`TcpStream`].
    pub fn close(&mut self) -> io::Result<()> {
        let mut tcp = self.tcp.write().unwrap();
        tcp.flush()?;
        tcp.shutdown(Shutdown::Both)
    }

    /// Returns the socket address of the remote peer of this TCP connection.
    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        self.tcp.read().unwrap().peer_addr()
    }

    /// Returns the socket address of the local half of this TCP connection.
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.tcp.read().unwrap().local_addr()
    }
}
