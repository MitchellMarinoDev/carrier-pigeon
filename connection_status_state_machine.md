# Connection Status State Machine


```mermaid
flowchart TD
    subgraph Connection
        ConnectingDelay[["Wait for a response"]]
        NotConnected([NotConnected])

        NotConnected --> |"connect()"| Connecting

        Connecting --- ConnectingDelay
        ConnectingDelay --> |Accepted| Accepted
        ConnectingDelay --> |Rejected| Rejected

        Accepted --> |"status()"| Connected
        Rejected --> |"status()"| 2NotConnected([NotConnected])
    end
    subgraph Disconnection
        2Connected([Connected])
        2Connected --> |connection drops| Dropped
        2Connected --> |peer disconnects| Disconnected
        2Connected --> |local disconnects| Disconnecting

        3NotConnected([NotConnected])
        Dropped ---> |"status()"| 3NotConnected
        Disconnected ---> |"status()"| 3NotConnected
        Disconnecting --- DisconnectDelay[[Wait for Disconnect Message to be acked]] --> 3NotConnected
    end
```

The status of a client or peer connected to a server is mutated in two places.
1. The `tick()` function.
2. The `handle_status()` function.

When the user calls `handle_status()`, the status might change. 
This function gives the user a chance to handle any changes to the connection status.
For instance, if `handle_status()` returns `Disconnected`, the state will change to 
`NotConnected`. The `Disconnected` state contains the disconnection message. This
gives you an opportunity to handle the event of the peer disconnecting.
