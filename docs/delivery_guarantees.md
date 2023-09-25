# Delivery Guarantees

Carrier Pigeon allows each message type to specify its delivery guarantees. The options are:

- `Reliable`: Messages are guaranteed to arrive. Not necessarily in order.
- `ReliableOrdered`: Messages are guaranteed to arrive in order.
- `ReliableNewest`: The most recent (newest) message is guaranteed to arrive. Older messages may not arrive.
  In fact, an older messages are discarded if they arrive after a newer message.
- `Unreliable`: No guarantees about delivery. Messages might or might not arrive. Messages might be duplicated.
- `UnreliableNewest`: No delivery guarantee, but you will only get the newest message.
  If an older message arrives after a newer one, it will be discarded.

In order to implement reliability on top of UDP, Carrier Pigeon need to acknowledge all the messages
that come in. If we don't get an acknowledgment, for a particular message in a certain time,
we can assume that it got lost.

If you are interested in how Carrier Pigeon implements reliability, you may read
[acknowledgement_strategy.md](acknowledgement_strategy.md) (optional).

Next, read [connection_status_state_machine.md](connection_status_state_machine.md).
