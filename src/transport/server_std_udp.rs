use crate::message_table::CONNECTION_TYPE_MID;
use crate::net::{MsgHeader, HEADER_SIZE, MAX_MESSAGE_SIZE, MAX_SAFE_MESSAGE_SIZE};
use crate::transport::ServerTransport;
use crate::{MId, MsgTable};
use log::*;
use std::any::Any;
use std::collections::BTreeSet;
use std::io;
use std::io::{Error, ErrorKind};
use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
use std::sync::{Arc, Mutex};

pub struct UdpServerTransport {
    socket: UdpSocket,
    connected_addrs: Arc<Mutex<BTreeSet<SocketAddr>>>,
    buf: [u8; MAX_MESSAGE_SIZE],
    msg_table: MsgTable,
}

impl ServerTransport for UdpServerTransport {
    fn new(
        listen: impl ToSocketAddrs,
        msg_table: MsgTable,
        connected_addrs: Arc<Mutex<BTreeSet<SocketAddr>>>,
    ) -> io::Result<Self> {
        let socket = UdpSocket::bind(listen)?;

        // TODO: should this be non blocking?
        socket.set_nonblocking(true)?;

        let buf = [0; MAX_MESSAGE_SIZE];

        Ok(UdpServerTransport {
            connected_addrs,
            socket,
            buf,
            msg_table,
        })
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

    fn recv_from(&mut self) -> io::Result<(SocketAddr, MsgHeader, Box<dyn Any + Send + Sync>)> {
        let (n, header, from) = self.recv_next()?;

        self.msg_table.check_mid(header.mid)?;

        let deser_fn = self.msg_table.deser[header.mid];
        deser_fn(&self.buf[HEADER_SIZE..n]).map(|msg| (from, header, msg))
    }

    fn listen_addr(&self) -> io::Result<SocketAddr> {
        self.socket.local_addr()
    }
}

impl UdpServerTransport {
    /// Gets the next udp packet and reads it into the buffer.
    ///
    /// This discards any packets that come from an address that is not in `self.connected_addrs`.
    fn recv_next(&mut self) -> io::Result<(usize, MsgHeader, SocketAddr)> {
        loop {
            let (n, from) = self.socket.recv_from(&mut self.buf)?;
            if n < HEADER_SIZE {
                warn!(
                    "Received a packet of length {} which is not big enough \
                         to be a carrier pigeon message. Discarding",
                    n
                );
                continue;
            }

            let header = MsgHeader::from_be_bytes(&self.buf[..HEADER_SIZE]);
            trace!(
                "Server: received message with MId: {}, len: {}.",
                header.mid,
                n,
            );

            // If we get a message that is a type other than the connection type message ID,
            // make sure it is from a connected address
            // TODO: This is done to prevent un-needed deserialization work being done on
            //      potentially malicious packets. However, this requires us to have
            //      shared state (self.connected_addrs) between the Connection and the Transport.
            //      This might not be worth it since attackers can just send connection messages
            //      instead.
            if header.mid != CONNECTION_TYPE_MID {
                let connected_addrs = self.connected_addrs.lock().expect("failed to acquire lock");
                if !connected_addrs.contains(&from) {
                    trace!(
                        "Got packet from {} which is not a connected client. Discarding",
                        from
                    );
                    continue;
                }
            }
            return Ok((n, header, from));
        }
    }
}
