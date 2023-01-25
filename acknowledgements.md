# Acknowledgement Strategy

In order to implement reliability on top of UDP, we need to acknowledge all the messages
that come in. If we don't get an acknowledgment, for a particular message in a certain time,
we can assume that it got lost.

## carrier-pigeon's acknowledgement strategy

Pulling ideas from 
[Gaffer On Games](https://gafferongames.com/post/reliability_ordering_and_congestion_avoidance_over_udp/)
and 
[Laminar](https://github.com/TimonPost/laminar), carrier-pigeon uses an 
acknowledgement number and a 32-bit bitfield in the header of other messages.
This means we can acknowledge up to 32 messages at a time. These acknowledgements
sent in the header of every message so all the acknowledgements can't be lost 
(on the network) all at once.

The acknowledgement is done with a `u16` as the ack_offset, and then a `u32` as bitflags
for the next 32 ack_numbers. The ack_num will sit on a multiple of 32 to simplify
the implementation of this acknowledgement system.

Acknowledgments are packed in the header in this format: 
```
[ack_offset] u16 // The offset of the bitfield
[ack_bitfield] u32 // The bitfield of the succeeding 32 ack_numbers
```
In carrier-pigeon, when a packet is resent, it retains the same AckNum. This is so
the message can stay immutable after it is sent the first time. Because of this, that
AckNum could get out of the bitfield window before it arrives.

In addition, if messages are only sent 1 way (or very little traffic is sent one way)
There might not be enough outgoing messages to ack all the incoming messages.

To solve these issues, there is a dedicated type for acknowledging all messages, 
including the ones that don't fit in the window.
