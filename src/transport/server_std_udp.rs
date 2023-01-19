use crate::net::{MAX_MESSAGE_SIZE, MAX_SAFE_MESSAGE_SIZE};
use crate::transport::ServerTransport;
use crate::MType;
use log::*;
use std::io;
use std::io::{Error, ErrorKind};
use std::net::{SocketAddr, UdpSocket};
use std::sync::Arc;

pub struct UdpServerTransport {
    socket: UdpSocket,
    buf: [u8; MAX_MESSAGE_SIZE],
}

impl ServerTransport for UdpServerTransport {
    fn new(listen: SocketAddr) -> io::Result<Self> {
        let socket = UdpSocket::bind(listen)?;

        // TODO: should this be non blocking?
        socket.set_nonblocking(true)?;

        let buf = [0; MAX_MESSAGE_SIZE];

        Ok(UdpServerTransport { socket, buf })
    }

    fn send_to(&self, addr: SocketAddr, m_type: MType, payload: Arc<Vec<u8>>) -> io::Result<()> {
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
            "Server: Sending message with MType: {}, len: {}.",
            m_type,
            payload_len
        );
        let n = self.socket.send_to(&payload, addr)?;

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

    fn recv_from(&mut self) -> io::Result<(SocketAddr, Vec<u8>)> {
        let (n, from) = self.socket.recv_from(&mut self.buf)?;
        Ok((from, (self.buf[..n]).to_vec()))
    }

    fn listen_addr(&self) -> io::Result<SocketAddr> {
        self.socket.local_addr()
    }
}
