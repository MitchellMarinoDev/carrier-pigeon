# TODO:
- [x] MessageTableJoining (Join the registrations of 2 message tables).
- [x] Config options (instead of constants)
- [x] Add Cargo.toml dependency copy-pasta in docs.rs and GitHub.
- [x] Make send calls take an immutable reference by using a ReadWriteLock.
- [x] Remove the Register custom, as custom `Serialize`/`Deserialize` impls are allowed.
- [x] Remove the `<C, R, D>` generics as they end up everywhere.
- [x] Standardize logging.
- [x] Add try_recv.
- [x] make arg order consistent.
- [x] check docs

## For v0.4.0:
- [ ] Server discovery.
- [ ] Query support for server. (Optionally listen on another port and respond to query requests).
- [ ] TcpConnection buffering (Buffer all tcp messages sent, then write them all when a `send_tcp` method is called).
- [ ] Compile messages into MsgTable using macros.
