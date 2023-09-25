# TODO:

- [ ] Change handle_disconnects to handle_disconnect in loop.
- [ ] The connection abstraction in its own crate could be useful.
- [ ] Server discovery.
- [ ] Query support for server. (Optionally listen on another port and respond to query requests).
- [ ] Compile messages into MsgTable using macros.
- [ ] Support Async if possible.
- [ ] Packet Fragmentation.
- [ ] Congestion control maybe?
- [ ] Perhaps use Tokio instead of the std lib's UdpTransport
- [ ] Perhaps extract the connection stuff to a separate crate. This would decouple it completely and allow the
  reliable udp implementation to be used without carrier-pigeon.
- [ ] Make this work for web!
