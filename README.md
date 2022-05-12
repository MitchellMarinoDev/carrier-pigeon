# carrier_pigeon

[![crates.io](https://img.shields.io/crates/v/carrier_pigeon)](https://crates.io/crates/carrier_pigeon)
[![docs.rs](https://docs.rs/carrier_pigeon/badge.svg)](https://docs.rs/carrier_pigeon)

A rusty networking library for games.

Carrier pigeon builds on the standard library's `TcpStream` and `UdpSocket` types and handles all the serialization, 
sending, receiving, and deserialization. This way you can worry about what to send, and pigeon will worry about how 
to send it. This also allows you to send and receive different types of messages independently.

### Add carrier_pigeon to your `Cargo.toml`:

`carrier_pigeon = "0.3.0"`

### Also check out the [Bevy](https://bevyengine.org/) plugin.

[bevy_pigeon](https://github.com/MitchellMarinoDev/bevy_pigeon).

## Documentation

The documentation can be found on [Docs.rs](https://docs.rs/carrier_pigeon)

### Quickstart

A quickstart guide that goes in more detail is found at [`/quickstart.md`](quickstart.md)

### Examples

There is a simple chat program example in the [`examples/` directory](examples).
This contains a client and server command line programs.

## Features

- [x] Connection, response, disconnect messages built in and extremely flexible.
- [x] Configuration for buffer size, timeouts and more.
- [x] Send/Recv calls take an immutable reference allowing for parallelism.
- [x] Independent message sending.
- [x] TCP and UDP connections.
- [x] Client and Server types.
- [x] Built in serialization/deserialization.
- [x] [Bevy](https://bevyengine.org/) integration ([bevy_pigeon](https://github.com/MitchellMarinoDev/bevy_pigeon)).

### Planned Features

- [ ] Server discovery.
- [ ] Query support for server. (Optionally listen on another port and respond to query requests).
- [ ] Optional buffering of TCP messages (Buffer all tcp messages sent, then write them all when a `send_tcp` method is called).
- [ ] Compile messages into MsgTable using macros.

## Contributing

To contribute, fork the repo and make a PR. If you find a bug, feel free to open an issue. If you have any questions, 
concerns or suggestions you can shoot me an email (found in Cargo.toml) or DM me on discord `@TheHourGlass34#0459`.

By contributing, you agree that your changes are subject to the license found in /LICENSE.
