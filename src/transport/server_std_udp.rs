use crate::net::{HEADER_SIZE, MAX_MESSAGE_SIZE, MAX_SAFE_MESSAGE_SIZE};
use crate::transport::ServerTransport;
use crate::MId;
use log::*;
use std::io;
use std::io::{Error, ErrorKind};
use std::net::{SocketAddr, UdpSocket};
use std::sync::{Arc};

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

    fn send_to(&self, to: SocketAddr, mid: MId, payload: Arc<Vec<u8>>) -> io::Result<()> {
        // Check if the message is valid, and should be sent.
        let payload_len = payload.len();
        if payload_len > MAX_MESSAGE_SIZE {
            let e_msg = format!(
                "UDP: Outgoing message size is greater than the maximum message size ({}). \
                MId: {}, size: {}. Discarding message.",
                MAX_MESSAGE_SIZE, mid, payload_len
            );
            return Err(Error::new(ErrorKind::InvalidData, e_msg));
        }

        if payload_len > MAX_SAFE_MESSAGE_SIZE {
            debug!(
                "UDP: Outgoing message size is greater than the maximum SAFE message size.\
                MId: {}, size: {}. Sending message anyway.",
                mid, payload_len
            );
        }
        // Message can be sent!

        trace!(
            "Server: Sending message with MId: {}, len: {}.",
            mid,
            payload_len
        );
        let n = self.socket.send_to(&payload, to)?;

        // Make sure it sent correctly.
        if n != payload_len {
            error!(
                "UDP: Couldn't send all the bytes of a message (mid: {}). \
				Wanted to send {} but could only send {}. This will likely \
				cause issues on the other side.",
                mid, payload_len, n
            );
        }
        Ok(())
    }

    fn recv_from(&mut self) -> io::Result<(SocketAddr, Vec<u8>)> {
        let (n, from) = self.socket.recv_from(&mut self.buf)?;
        Ok((from, (self.buf[HEADER_SIZE..n]).to_vec()))
    }

    fn listen_addr(&self) -> io::Result<SocketAddr> {
        self.socket.local_addr()
    }
}
