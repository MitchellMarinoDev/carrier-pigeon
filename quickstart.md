# Quickstart guide

This will walk you through what you need to know to use `carrier-pigeon`.

### Messages

Messages are simple rust types that implement the `Any + Send + Sync` traits, and serde's `Serialize` and 
`DeserializeOwned` traits. This means making new message types is simple and almost boilerplate free. Each message type
is independent of the others. This means adding another message type won't mess with any of the other messages/logic.
This not only makes it scalable for your code, but makes it easy for plugins to add their own message types without
interfacing with yours.

For (de)serialization, carrier-pigeon uses [bincode](https://docs.rs/bincode/latest/bincode/) for speed and serialized
size.

Messages need to be registered in a `MsgTable` for them to be used. The Message tables on all clients and the server
**need** to have the same exact types registered in the same exact order.

```rust
use carrier_pigeon::MsgTable;
use serde::{Serialize, Deserialize};

#[derive(Copy, Clone, Eq, PartialEq, Serialize Deserialize)]
struct MyMessage {
    contents: String,
}

fn main() {
    // Register MyMessage to be sent reliably.
    let mut table = MsgTable::new();
    table.register::<MyMessage>(Transport::TCP).unwrap();
}
```

If you can not guarantee a constant registration order, the `SortedMsgTable` type is provided. This allows types to be
registered with a string key. The keys are then sorted when building the table to provide a constant order. The same
exact types still must be registered (with the same keys) on all clients and the server.

### Connect/Disconnect Logic
Carrier pigeon also takes care of connecting and disconnecting clients and servers. You specify what data needs to be 
sent when connecting (such as a password, or game version) in a message. You also specify the logic for accepting or 
rejecting the incoming connections. This is done using rust's closures, allowing you to use any context
(such as current player count) that you need in order to decide whether you should accept or reject the incoming 
connection. You also specify what should be sent in the disconnection message (such as a reason for disconnect) so the
peers know why the connection was terminated or if it was a game crash or network failure. All this makes the 
connection/rejection/disconnect logic very flexible.

```rust
fn main() {
    let context = 12;
    
    // Handle the new connections with whatever logic you want.
    server.handle_new_cons::<Connection, Response>(&mut |cid, con_msg| {
        if context == 12 {
            // Accept.
            (true, Response::Accepted)
        } else {
            // Reject
            (false, Response::Rejected)
        }
    });
    
    // Handle disconnects using context however you want.
    server.handle_disconnects(&mut |cid, status| {
        println!("CId {} disconnected with status: {:?}", cid, status);
        println!("Inside this closure I can use context vars like {}", context);
    });

}
```

Games often have a need for both reliable/slower and unreliable/quicker data transfer. In carrier pigeon, each
client/server connection makes a TCP and UDP connection on the same address and port. This means that you have both a
reliable and unreliable way to send data. When registering a message type, you specify whether you would like it sent
over TCP(reliable) or UDP(unreliable).

### Message Buffering
Carrier pigeon keeps all received messages in a buffer, and iterating through it does **not** remove the messages from
the buffer. You clear the buffer and put the new messages into the buffer at your discretion
(usually at the start of every frame). This is an intentional design choice as this allows multiple independent systems
to use all the messages received that frame, allowing for greater scalability. This also allows multiple systems to
concurrently use the client/server as you only need an immutable reference to the client/server.

```rust
#[derive(Copy, Clone, Eq, PartialEq, Serialize Deserialize)]
struct MyMessage {
    contents: String,
}

fn main() {
    // ... Setup client and server
    
    for _ in 0..10 {
        // You can concurrently recv message. 
        // All these threads will get the same messages.
        std::thread::spawn(|| {
            for msg in client.recv::<MyMessage>() {
                do_something_with(msg);
            }
        })
    }
}
```
