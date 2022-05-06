# carrier-pigeon

A rusty networking library for games.

Carrier pigeon builds on the standard library's `TcpStream` and `UdpSocket` types and handles all the serialization, 
sending, receiving, and deserialization. This way you can worry about what to send, and pigeon will worry about how 
to send it. This also allows you to send and receive different types of messages independently.

### Add carrier-pigeon to your `Cargo.toml`:

`carrier-pigeon = "0.3.0"`

### Also check out the [Bevy](https://bevyengine.org/) plugin.

[bevy-pigeon](https://github.com/MitchellMarinoDev/bevy-pigeon).

## Documentation

The documentation can be found on [Docs.rs](https://docs.rs/carrier-pigeon)

### Quickstart

A quickstart guide that goes in more detail is found at [/Quickstart.md](Quickstart.md)

### Examples

There is a simple chat program example in the
[`examples/` directory](examples).
This contains a client and server command line programs.

## Features

- [x] Connection, response, disconnect messages built in and extremely flexible.
- [x] Independent message sending.
- [x] TCP and UDP channels.
- [x] Client and Server types.
- [x] Built in serialization/deserialization.
- [x] Custom serialization support.
- [x] [Bevy](https://bevyengine.org/) integration ([bevy-pigeon](https://github.com/MitchellMarinoDev/bevy-pigeon)).

### Planned Features

- [ ] More config options.
- [ ] Benchmarks (On the way!)
- [ ] RPCs

## Contributing

To contribute, fork the repo and make a PR. If you find a bug, feel free to open an issue. If you have any questions, 
concerns or suggestions you can shoot me an email (found in Cargo.toml) or DM me on discord `@TheHourGlass34#0459`.

By contributing, you agree that your changes are subject to the license found in /LICENSE.
