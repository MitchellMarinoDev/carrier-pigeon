# TODO:

## For v0.4.0:
- [ ] Server discovery.
- [ ] Query support for server. (Optionally listen on another port and respond to query requests).
- [ ] Optional buffering of TCP messages (Buffer all tcp messages sent, then write them all when a `send_tcp` method is called).
- [ ] Compile messages into MsgTable using macros.
