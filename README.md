### Chat App Backend exercise

This is far from fully functional, and currently almost all error handling will just crash the executing thread.
To run, [install Rust](https://rustup.rs/), then run `cargo run` from this directory.
To run tests, run `cargo test` instead.
The program reads JSON from stdin, with one message per line, of the form:
`{"timestamp": 0, "body": "hello world"}`.
It is currently hardcoded to listen for websocket connections on `127.0.0.1:9002`, though it currently doesn't accept any requests, and simply reports any messages it sees, and indicates if the message is from a time outside a sliding window (in a final product this would probably be determined client-side).

Future work would populate the sqlitedb that isn't currently storing anything, and use it to look up the nearest messsages when the client requests messages from a particular timestamp.
The message structure should also be improved to include additional information -- notably, a unique identifier for deduplication client-side.
