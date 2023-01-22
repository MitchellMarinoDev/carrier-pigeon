# Acknowledgement Strategy

In order to implement reliability on top of UDP, we need to acknowledge all the messages
that come in. If we don't get an acknowledgment, for a particular message in a certain time,
we can assume that it got lost.

## carrier-pigeon's acknowledgement strategy

Pulling ideas from 
[Gaffer On Games](https://gafferongames.com/post/reliability_ordering_and_congestion_avoidance_over_udp/)
and 
[Laminar](https://github.com/TimonPost/laminar), carrier-pigeon uses an 
acknowledgement number and bitflags in the header of other messages.
This means we can acknowledge multiple messages at a time. These acknowledgements
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

