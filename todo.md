# TODO:
- [x] MessageTableJoining (Join the registrations of 2 message tables).
- [ ] Config options (instead of constants)
- [x] Add Cargo.toml dependency copy-pasta in docs.rs and GitHub.
- [x] Make send calls take an immutable reference by using a ReadWriteLock.
- [ ] `bevy-pigeon`, and `carrier-pigeon` might have enough documentation warrant a book.
- [x] Remove the Register custom, as custom `Serialize`/`Deserialize` impls are allowed.
- [x] Remove the `<C, R, D>` generics as they end up everywhere.

## For v0.4.0:
- [ ] Query support.
- [ ] TcpConnection buffering (Buffer all tcp messages sent, then write them all when a `send_tcp` method is called).
- [ ] Ticks and multi tick message buffering.
