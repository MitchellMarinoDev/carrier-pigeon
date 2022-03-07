# carrier-pigeon
A rusty networking library for games.

Carrier pigeon uses tokio's async TCP and UDP connection types and handles all the serialization, sending, receiving, 
and deserialization. This way you can worry about what to send, and pigeon will worry about how to send it. It uses 
channels allowing you to send and receive messages from a synchronous context, while still gaining the efficiency of 
async code.

### Messages
Messages are simple rust types that implement the `NetMsg` trait, serde's `Serialize` and `Deserialize` traits. 
(it also must be `send` and `sync`). This means making new message types is simple and almost boilerplate free. 
Each message type is independent of the others. This means adding another message type won't mess with any of the other 
messages. This not only makes it scalable for your code, but makes it easy for modders to add their own message types 
without interfacing with yours. 

Messages need to be registered in a `MsgTable` for them to be used. The Message tables on all clients and the server
**need** to have the same exact types registered in the same exact order. If you can not guarantee a constant
registration order, the `SortedMsgTable` type is provided. The same exact types still must be registered on all clients
and the server.

### Connect/Disconnect Logic
Carrier pigeon also takes care of connecting and disconnecting clients and servers. You specify what needs to be sent
when connecting (such as a password, or game version). You also specify the logic for accepting or rejecting the 
incoming connections. This is done using rust's closures, allowing you to use any context 
(such as current player count) that you need to decide whether you should accept or reject the incoming connection. 
You also specify what should be sent in the disconnection message (such as a reason for disconnect) so the peers know 
why the connection was terminated or if it was a game crash or network failure. All this makes the 
connection/rejection/disconnect logic very flexible.

Games often have a need for both reliable/slower and unreliable/quicker data transfer. In carrier pigeon, each 
client/server connection makes a TCP and UDP connection on the same address and port. This means that you have both a 
reliable and unreliable way to send data. When registering a message type, you specify whether you would like it sent 
over TCP(reliable) or UDP(unreliable).

### Message Buffering
Carrier pigeon keeps all received messages in a buffer, and iterating through it does **not** remove the messages from 
the buffer. You clear the buffer and put the new messages into the buffer at your discretion 
(usually at the start of every frame). This is an intentional design choice as this allows multiple independent systems
to use all the messages received that frame, allowing for greater scalability. This also allows multiple systems to 
concurrently use the client/server as you only need an immutable reference to it.

## Documentation

The documentation can be found on [Docs.rs](https://docs.rs/carrier-pigeon)

### Examples

There is a simple chat program example in the
[`examples/` directory](https://github.com/MitchellMarinoDev/carrier-pigeon/tree/main/examples).
This contains a client and server command line programs.

## Features

 - [x] Connection, response, disconnect messages built in and extremely flexible.
 - [x] Independent message sending.
 - [x] TCP and UDP channels.
 - [x] Client and Server types.
 - [x] Built in serialization/deserialization.

### Planned Features

 - [ ] Async runtime abstractions.
 - [ ] [Bevy](https://bevyengine.org/) integration.

## Contributing

To contribute, fork the repo and make a PR. If you find a bug, feel free to open an issue. If you have any questions, 
concerns or suggestions you can shoot me an email (found in Cargo.toml) or DM me on discord `@TheHourGlass34#0459`.
