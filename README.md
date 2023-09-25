# carrier-pigeon

[![crates.io](https://img.shields.io/crates/v/carrier-pigeon)](https://crates.io/crates/carrier-pigeon)
[![docs.rs](https://docs.rs/carrier-pigeon/badge.svg)](https://docs.rs/carrier-pigeon)

A rusty networking library for games.

Carrier Pigeon provides an optionally reliable message based interface for communicating between a server and clients.
This message abstraction provides separate and independent channels for different parts of your game (your chat
messages don't block your player movement messages). Pigeon also provides a connection procedure on top of a
connectionless transport like UDP.

Each message can be configured with delivery guarantees. For example, you can guarantee that chat messages arrive,
and arrive in order, but unreliably send the player position messages. This way you don't suffer from the reliability
and ordering overhead when you don't need to.

Carrier Pigeon supports the client server model, rather than the peer to peer model. If you want to connect players
to other players, one of the players should host the server.

### Add carrier-pigeon to your `Cargo.toml`:

`carrier-pigeon = "0.3.0"`

### Also check out the [Bevy](https://bevyengine.org/) plugin.

[bevy-pigeon](https://github.com/MitchellMarinoDev/bevy-pigeon).

## Documentation

The API documentation can be found on [Docs.rs](https://docs.rs/carrier-pigeon)
More in depth documentation about carrier-pigeon, design decisions and the interworkings is found in [`docs`/](docs).

### Quickstart

A quickstart guide that goes in more detail is found at [`/quickstart.md`](quickstart.md)

### Examples

There is a simple chat program example in the [`examples/` directory](examples).
This contains a client and server command line programs.

## Features

- [x] Connection, and disconnect messages built in and flexible.
- [x] Configuration for buffer size, timeouts and more.
- [x] Messages are stored in the server/client until they are cleared on the next tick.
    - Iterating though these messages doesn't mutate the server/client, so you are free to do it in parallel.
- [x] Independent message channels.
- [x] Reliable/Unreliable and Ordered/Newest/Unordered Guarantees.
- [x] Client and Server types.
- [x] Serialization/deserialization handled by carrier-pigeon using serde.
- [x] [Bevy](https://bevyengine.org/) integration ([bevy-pigeon](https://github.com/MitchellMarinoDev/bevy-pigeon)).

### Planned Features

- [ ] Server discovery.
- [ ] Encryption.
- [ ] Authentication.
- [ ] Query support for server. (Optionally listen and respond to query requests).
- [ ] Getting the send time of a message.
- [ ] Compile messages into MsgTable using macros.

## Contributing

Contributions are more than welcome. To contribute, fork the repo and make a PR.
If you find a bug, feel free to open an issue.
If you have any questions, concerns or suggestions you can shoot me an email (found in Cargo.toml).

By contributing, you agree that your changes are subject to the license found in /LICENSE.
