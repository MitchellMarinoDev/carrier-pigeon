use crate::net::{MsgHeader, HEADER_SIZE, MAX_MESSAGE_SIZE, MAX_SAFE_MESSAGE_SIZE};
use crate::transport::ClientTransport;
use crate::{MId, MsgTable};
use log::{debug, error, trace, warn};
use std::any::Any;
use std::io;
use std::io::ErrorKind::InvalidData;
use std::io::{Error, ErrorKind};
use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
use std::sync::Arc;

pub struct UdpClientTransport {
    socket: UdpSocket,
    buf: [u8; MAX_MESSAGE_SIZE],
    msg_table: MsgTable,
}

impl ClientTransport for UdpClientTransport {
    fn new(
        local: impl ToSocketAddrs,
        remote: impl ToSocketAddrs,
        msg_table: MsgTable,
    ) -> io::Result<UdpClientTransport> {
        let socket = UdpSocket::bind(local)?;
        socket.connect(remote)?;

        // TODO: should this be non blocking?
        socket.set_nonblocking(true)?;

        let buf = [0; MAX_MESSAGE_SIZE];

        Ok(UdpClientTransport {
            socket,
            buf,
            msg_table,
        })
    }

    fn send(&self, mid: MId, payload: Arc<Vec<u8>>) -> io::Result<()> {
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
            "UDP: Sending message with MId: {}, len: {}.",
            mid,
            payload_len
        );
        let n = self.socket.send(&payload)?;

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

    fn recv(&mut self) -> io::Result<(MsgHeader, Box<dyn Any + Send + Sync>)> {
        let n = self.socket.recv(&mut self.buf)?;

        if n < HEADER_SIZE {
            warn!(
                "Received a packet of length {} \
                 which is not big enough to be a carrier pigeon message",
                n
            );
        }

        let header = MsgHeader::from_be_bytes(&self.buf[0..HEADER_SIZE]);

        if !self.msg_table.valid_mid(header.mid) {
            return Err(Error::new(
                InvalidData,
                format!(
                    "Got a message specifying MId {}, but the maximum MId is {}.",
                    header.mid,
                    self.msg_table.mid_count()
                ),
            ));
        }

        let deser_fn = self.msg_table.deser[header.mid];
        deser_fn(&self.buf[HEADER_SIZE..n]).map(|msg| (header, msg))
    }

    fn recv_blocking(&mut self) -> io::Result<(MsgHeader, Box<dyn Any + Send + Sync>)> {
        self.socket.set_nonblocking(false)?;
        let result = self.recv();
        self.socket.set_nonblocking(true)?;
        return result;
    }

    fn local_addr(&self) -> io::Result<SocketAddr> {
        self.socket.local_addr()
    }

    fn peer_addr(&self) -> io::Result<SocketAddr> {
        self.socket.peer_addr()
    }
}
