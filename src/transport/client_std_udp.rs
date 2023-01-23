use crate::net::{MAX_MESSAGE_SIZE, MAX_SAFE_MESSAGE_SIZE};
use crate::transport::ClientTransport;
use crate::MType;
use log::*;
use std::io;
use std::io::{Error, ErrorKind};
use std::net::{SocketAddr, UdpSocket};
use std::sync::Arc;

pub struct UdpClientTransport {
    socket: UdpSocket,
    buf: [u8; MAX_MESSAGE_SIZE],
}

impl ClientTransport for UdpClientTransport {
    fn new(local: SocketAddr, peer: SocketAddr) -> io::Result<Self> {
        let socket = UdpSocket::bind(local)?;
        socket.connect(peer)?;

        socket.set_nonblocking(true)?;

        let buf = [0; MAX_MESSAGE_SIZE];

        Ok(UdpClientTransport { socket, buf })
    }

    fn send(&self, m_type: MType, payload: Arc<Vec<u8>>) -> io::Result<()> {
        // Check if the message is valid, and should be sent.
        let payload_len = payload.len();
        if payload_len > MAX_MESSAGE_SIZE {
            let e_msg = format!(
                "UDP: Outgoing message size is greater than the maximum message size ({}). \
                MType: {}, size: {}. Discarding message.",
                MAX_MESSAGE_SIZE, m_type, payload_len
            );
            return Err(Error::new(ErrorKind::InvalidData, e_msg));
        }

        if payload_len > MAX_SAFE_MESSAGE_SIZE {
            debug!(
                "UDP: Outgoing message size is greater than the maximum SAFE message size.\
                MType: {}, size: {}. Sending message anyway.",
                m_type, payload_len
            );
        }
        // Message can be sent!

        trace!(
            "Client: Sending message with MType: {}, len: {}.",
            m_type,
            payload_len
        );
        let n = self.socket.send(&payload)?;

        // Make sure it sent correctly.
        if n != payload_len {
            error!(
                "UDP: Couldn't send all the bytes of a message (MType: {}). \
				Wanted to send {} but could only send {}. This will likely \
				cause issues on the other side.",
                m_type, payload_len, n
            );
        }
        Ok(())
    }

    fn recv(&mut self) -> io::Result<Vec<u8>> {
        let n = self.socket.recv(&mut self.buf)?;
        Ok(self.buf[..n].to_vec())
    }

    fn recv_blocking(&mut self) -> io::Result<Vec<u8>> {
        self.socket.set_nonblocking(false)?;
        let result = self.recv();
        self.socket.set_nonblocking(true)?;
        result
    }

    fn local_addr(&self) -> io::Result<SocketAddr> {
        self.socket.local_addr()
    }

    fn peer_addr(&self) -> io::Result<SocketAddr> {
        self.socket.peer_addr()
    }
}
