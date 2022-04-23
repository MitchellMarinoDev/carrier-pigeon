# TODO:
- TcpConnection buffering (Buffer all tcp messages sent, then write them all at the end of the frame).
- MessageTableJoining (Join the registrations of 2 message tables).
- Send client connect/disconnect messages to other clients.
- Config options (instead of constants)
- Add quick start example
- Add Cargo.toml dependency copy-pasta in docs.rs and GitHub.
- Perhaps bring back NetMsg and `impl<T: Any + Send + Sync> NetMsg for T;`
