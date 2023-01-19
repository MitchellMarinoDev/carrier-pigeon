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
This means we can acknowledge multiple messages at a time, and it is distributed 
among several messages so all the acknowledgements can't be lost all at once.

Acknowledgement numbers are assigned on a per-message type basis
