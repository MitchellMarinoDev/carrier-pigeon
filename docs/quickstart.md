# Quickstart Guide

This will walk you through what you need to know to use `carrier-pigeon`.

### Messages

Messages are regular rust types that implement serde's `Serialize` and `DeserializeOwned` traits.
This means making new message types is simple and almost boilerplate free. Each message type is independent of the
others. This means adding another message type to your game won't mess with any of the other messages/logic that you
already have. This not only makes it scalable for your code, but makes it easy for plugins to add their own message
types without interfering with yours.

For (de)serialization, carrier-pigeon uses [bincode](https://docs.rs/bincode/latest/bincode/) for speed and serialized
size.

Messages need to be registered in a `MsgTable` for them to be used. The Message tables on all clients and the server
**need** to have the same exact types registered in the table.

In order to assign each message a unique message id (MId), you either need to ensure you register the messages in a
particular order on all clients and servers, or you can register the messages with a unique string key (It would be a
good idea to use the name of the struct, as long as they are all unique). Carrier-pigeon will sort the message types
based on their string key to get a constant assignment of message ids. You can also mix and match these two methods.

```rust
#[derive(Copy, Clone, Eq, PartialEq, Serialize Deserialize)]
struct MyOrderedMessage {
    contents: String,
}

#[derive(Copy, Clone, Eq, PartialEq, Serialize Deserialize)]
struct MyStringKeyMessage {
    favorite_number: i32,
}

fn main() {
    let mut table_builder = MsgTableBuilder::new();
    table_builder.register_in_order::<MyOrderedMessage>(Guarentees::Unreliable).unwrap();
    table_builder.register_sorted::<MyStringKeyMessage>(Guarentees::Unreliable, "MyStringKeyMessage").unwrap();
}
```

### Connect/Disconnect Logic

Carrier Pigeon also takes care of some of the connecting and disconnecting logic. You specify what data needs to be
sent when connecting (such as a password, or game version) in a message. You also specify the logic for accepting or
rejecting the incoming connections. This is done using rust's closures, allowing you to use any context
(such as current player count) that you need in order to decide whether you should accept or reject the incoming
connection. You also specify what should be sent in the disconnection message (such as a reason for disconnect) so the
peers know why the connection was terminated or if it was a game crash/network failure. All this makes the
connection/rejection/disconnect logic very flexible.

```rust
fn main() {
    let current_players = 12;

    // Handle the new connections with whatever logic you want.
    server.handle_pending(&mut |cid, address, connection_message| {
        if current_players < 20 {
            // Accept.
            Response::Accepted(CustomAcceptMessage::new("Hello!"))
        } else {
            // Reject
            Response::Rejected(CustomRejectionMessage::new("Sorry, server is full"))
        }
    });

    // Handle disconnects using context however you want.
    while let Some(disconnect_event) = server.handle_disconnect() {
        println!("CId {} disconnected with status: {:?}", cid, status);
    };
}
```

Games often have a need for both reliable/slower and unreliable/quicker data transfer. Carrier Pigeon addresses this
issue by creating an unreliable socket, and implementing reliability on top. You will read more about these
reliability guarantees on the next page.

### Message Buffering

Carrier Pigeon keeps all received messages in a buffer, and iterating through it does **not** remove the messages from
the buffer. Instead, messages are cleared and new messages are received when you call
`server.tick()`, or `client.tick()`. This makes it harder to accidentally miss a message. Therefore, you should handle
all of your network logic in between calls to `tick()`. Usually you would call `tick()` early each frame.
This way of keeping messages allows multiple independent systems to all use the messages received that frame, allowing
for greater scalability. This also allows multiple systems to concurrently use the client/server as you only need an
immutable reference to the client/server to iterate through the messages.

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

Next, read [delivery_guarantees.md](delivery_guarantees.md).
