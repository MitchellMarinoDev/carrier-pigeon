## Carrier Pigeon's acknowledgement strategy

*You do not need to know the following as a user to the library.*

Pulling ideas from
[Gaffer On Games](https://gafferongames.com/post/reliability_ordering_and_congestion_avoidance_over_udp/)
and
[Laminar](https://github.com/TimonPost/laminar), Carrier Pigeon uses an acknowledgement number and a 32-bit bitfield in
the header of other messages. This means we can acknowledge up to 32 messages at a time. These acknowledgements sent in
the header of every message so all the acknowledgements can't be lost (on the network) all at once.

The acknowledgement is done with a `u16` as the ack_offset, and then a `u32` as bitflags for the next 32 ack_numbers.
The ack_num will sit on a multiple of 32 to simplify the implementation of this acknowledgement system.

Acknowledgments are packed in the header in this format:

```
[ack_offset] u16 // The offset of the bitfield
[ack_bitfield] u32 // The bitfield of the 32 ack_numbers after ack_offset
```

Unlike some other implementations, in Carrier Pigeon, when a packet is resent, it keeps the same AckNum. This is so
the message can stay immutable after it is sent the first time. Because of this, the AckNum could be out of the
bitfield window when it arrives.

In addition, if messages are only sent 1 way (or very little traffic is sent one way), there might not be enough
outgoing messages to ack all the incoming messages.

To solve these issues, there is a dedicated message type (`AckMsg`) for acknowledging all messages, including the ones
that don't fit in the window. All other messages acknowledge up to 32 messages in the window in their message header,
but the `AckMsg` acknowledges *all* the messages in the window, and messages that passed the window. The messages that
passed the window are internally called residuals.

The acknowledgement window will grow dynamically so that each message will at least be acknowledged
`NetConfig.ack_send_count` times before falling out of the window.
