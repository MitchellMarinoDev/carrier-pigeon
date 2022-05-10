use crate::header::TCP_HEADER_LEN;
use crate::net::TcpHeader;
use crate::MId;
use io::Error;
use log::trace;
use std::io;
use std::io::{ErrorKind, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpStream};
use std::sync::RwLock;

/// A type wrapping a [`TcpStream`].
///
/// Provides read/write abstractions for sending `carrier_pigeon` messages.
pub struct TcpCon {
    buff: Vec<u8>,
    tcp: RwLock<TcpStream>,
}

impl TcpCon {
    /// Creates a new [`TcpCon`] from the [`TcpStream`] `tcp`.
    pub fn from_stream(tcp: TcpStream, max_msg_size: usize) -> Self {
        TcpCon {
            buff: vec![0; max_msg_size + TCP_HEADER_LEN],
            tcp: tcp.into(),
        }
    }

    /// Gets the total buffer size.
    fn buff_size(&self) -> usize {
        self.buff.len()
    }

    /// Gets the maximum message size.
    fn max_msg_size(&self) -> usize {
        self.buff.len() - TCP_HEADER_LEN
    }

    /// Sends the payload `payload` to the peer.
    ///
    /// This constructs a header, and builds the message, and sends it.
    pub fn send(&self, mid: MId, payload: &[u8]) -> io::Result<()> {
        let total_len = payload.len() + TCP_HEADER_LEN;
        let mut buff = vec![0; total_len];
        // Check if the message is valid, and should be sent.
        if total_len > self.buff_size() {
            let e_msg = format!(
                "TCP: Outgoing message size is greater than the maximum message size ({}). \
				MId: {}, size: {}. Discarding message.",
                self.max_msg_size(),
                mid,
                total_len
            );
            return Err(Error::new(ErrorKind::InvalidData, e_msg));
        }
        // Message can be sent!

        let header = TcpHeader::new(mid, payload.len());
        let h_bytes = header.to_be_bytes();
        // write the header and message to the buffer to combine them.

        for (i, b) in h_bytes.into_iter().enumerate() {
            buff[i] = b;
        }
        for (i, b) in payload.iter().enumerate() {
            buff[i + TCP_HEADER_LEN] = *b;
        }

        // Send
        trace!("TCP: Sending message with MId: {}, len: {}", mid, total_len);
        let mut tcp = self.tcp.write().unwrap();
        tcp.write_all(&buff[..total_len])?;
        Ok(())
    }

    pub fn recv(&mut self) -> io::Result<(MId, &[u8])> {
        // Peak the header.
        let mut tcp = self.tcp.write().unwrap();
        match tcp.peek(&mut self.buff[..TCP_HEADER_LEN])? {
            TCP_HEADER_LEN => {} // Success
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
        let header = TcpHeader::from_be_bytes(&self.buff[..TCP_HEADER_LEN]);
        let total_expected_len = header.len + TCP_HEADER_LEN;

        if header.len > self.max_msg_size() {
            let e_msg = format!(
                "TCP: The header of a received message indicates a size of {},\
	                but the max allowed message size is {}.\
					carrier_pigeon never sends a message greater than this; \
					this message was likely not sent by carrier_pigeon. \
	                This will cause issues when trying to read; \
	                Discarding this message and closing connection.",
                header.len,
                self.max_msg_size()
            );
            tcp.shutdown(Shutdown::Both)?;
            return Err(Error::new(ErrorKind::InvalidData, e_msg));
        }

        // Read data. The header will be read again as it was peaked earlier.
        tcp.read_exact(&mut self.buff[..header.len + TCP_HEADER_LEN])?;
        trace!(
            "TCP: Received msg of MId {}, len {}",
            header.mid,
            total_expected_len,
        );

        Ok((
            header.mid,
            &self.buff[TCP_HEADER_LEN..header.len + TCP_HEADER_LEN],
        ))
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
