# Qight — Lightweight QUIC-based Messaging Transport (WIP)

**A Rust powered, lightweight, QUIC-based messaging transport library for low-latency, bidirectional, event-driven communication.**

Qight is a minimal transport-layer protocol constructed directly upon QUIC, facilitating the efficient transmission of small, reliable, ephemeral messages. It functions as foundational infrastructure for real-time signaling and event dissemination, serving as a complement to REST APIs by obviating the necessity for HTTP polling, protracted connections, or intricate webhook configurations.


**Current status:** Early working prototype — core HELLO/SEND/FETCH loop functions, but many planned features are still stubs or missing.

## What works today 

- QUIC connection with self-signed cert (localhost testing)
- `HELLO <id>` handshake
- `SEND` of arbitrary binary payloads (length-prefixed, `wincode`-serialized envelope)
- `FETCH <recipient>` returning all stored messages as length-prefixed stream (0 terminator)
- In-memory storage on the relay (messages persist only until process restart)
- Basic demo (`qight_demo`) that sends and receives messages

## What is **not** implemented yet

- Message IDs, timestamps, TTL / expiration
- Message acknowledgment / deletion after fetch
- Authentication or client certificate validation
- Persistent storage
- Published crate on crates.io
- 
## Add to your project

```toml
# Cargo.toml
[dependencies]
qight = { git = "https://github.com/idorocodes/qight.git" }
```

### Client 

```rust

use anyhow::Result;
use std::net::SocketAddr;

#[tokio::main]
async fn main() -> Result<()> {
    let addr: SocketAddr = "127.0.0.1:4433".parse()?;

    let client = qight::RelayClient::connect(addr).await?;

    client.hello("test-client-123").await?;

    let envelope = qight::MessageEnvelope::new(
        "alice".into(),
        "bob".into(),
        b"idorocodes is sending hello via quic!".to_vec(),
        3600,
    );
    client.send(&envelope).await?;

    let messages = client.fetch("bob").await?;
    println!("Fetched {} message(s):", messages.len());
    for msg in messages {
        println!(
            "  {} → {} : {:?}",
            msg.sender,
            msg.recipient,
            String::from_utf8_lossy(&msg.payload)
        );
    }

    client.close(Some("test complete")).await;

    Ok(())
}

```


### Server / relay must be implemented to control the core logic of the system 
Code is too long so i have a demo here https://github.com/idorocodes/qight/blob/main/src/bin/relay.rs


## Test 

cargo run server 
cargo run client(in another terminal)

for this project, 

its cargo run --bin relay 
cargo run qight_demo in another terminal

## Architecture Overview

Clients initiate QUIC connections to relay nodes. Messages are enveloped with metadata and dispatched via `SEND`, temporarily queued, and accessed through `FETCH`. The protocol enforces TTL-based expiration for resource efficiency.

## Use Cases

- Facilitation of real-time service signaling in distributed architectures.
- Implementation of event-driven notification systems.
- Support for mobile-oriented push delivery mechanisms resilient to network variability.
- Enablement of anonymous or decoupled coordination protocols.
- Off-chain signaling within blockchain ecosystems.
- Substitution for polling or webhook patterns in microservices environments.

## API Reference

- `RelayClient::connect(addr: &str) -> Result<RelayClient>`: Establishes a QUIC connection.
- `RelayClient::hello(client_id: &str) -> Result<()>`: Performs protocol initiation.
- `RelayClient::send(envelope: MessageEnvelope) -> Result<()>`: Submits a message.
- `RelayClient::fetch(recipient: &str) -> Result<Vec<MessageEnvelope>>`: Retrieves pending messages.
- `MessageEnvelope`: Struct with fields as per the protocol envelope.

Consult the crate documentation for exhaustive details.

## Built With Rust Drawing inspiration from Solana's QUIC applications and contemporary blockchain infrastructures.


## Contributing

Contributions are encouraged. Kindly submit issues or pull requests to initiate discussions on enhancements, bug resolutions, or feature additions.

## License

Distributed under the MIT License.

## Author

Authored by [@idorocodes]
