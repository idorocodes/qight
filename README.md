# Qight — Secure QUIC-Based Messaging Relay

[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/license-MIT-blue)](LICENSE)
[![Crates.io](https://img.shields.io/crates/v/qight)](https://crates.io/crates/qight)

**Qight** is a lightweight, secure messaging relay built on QUIC (HTTP/3) for low-latency, authenticated communication. It enables clients to send and receive ephemeral messages through a central relay server, with end-to-end signing for authenticity. Perfect for IoT, decentralized apps, or secure event-driven systems.

## 🚀 Features

- **QUIC Transport**: Fast, encrypted connections over UDP using TLS 1.3.
- **Message Signing**: Ed25519-based signatures ensure message authenticity and prevent tampering.
- **SQLite Storage**: Persistent message storage with TTL-based expiration.
- **Service Discovery**: Automatic relay discovery via mDNS (Bonjour/Avahi).
- **Offline Queuing**: Clients queue messages locally when disconnected.
- **Async Architecture**: Built with Tokio for high concurrency.
- **Cross-Platform**: Runs on Linux, macOS, Windows.

## 📋 Table of Contents

- [Installation](#installation)
- [Quick Start](#quick-start)
- [Architecture](#architecture)
- [API Reference](#api-reference)
- [Security](#security)
- [Contributing](#contributing)
- [License](#license)

## 🛠 Installation

### Prerequisites
- Rust 1.70+
- SQLite (bundled via `rusqlite`)

### Add to Your Project
```toml
[dependencies]
qight = { git = "https://github.com/idorocodes/qight.git", branch = "outbound-queue" }
```

### Build from Source
```bash
git clone https://github.com/idorocodes/qight.git
cd qight
cargo build --release
```

## 🚀 Quick Start

### 1. Run the Relay Server
```bash
cargo run --bin relay
```
The server listens on `127.0.0.1:4433` and advertises via mDNS.

### 2. Run the Demo Client
```bash
cargo run --bin qight_demo
```
This connects, sends a signed message, and fetches it back.

### 3. Use in Your Code

#### Client Example
```rust
use qight::{RelayClient, MessageEnvelope};
use qight::gen_keypair;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Generate keys
    let (recipient_key, _) = gen_keypair();
    let (sender_pub, sender_priv) = gen_keypair();

    // Connect to relay (auto-discovers via mDNS)
    let client = RelayClient::connect_discovered().await?;

    // Say hello
    client.hello("my-client").await?;

    // Create and sign a message
    let mut envelope = MessageEnvelope::new(
        "alice".to_string(),
        recipient_key,
        sender_pub,
        b"Hello, world!".to_vec(),
        3600, // 1 hour TTL
    );
    envelope.sign(&sender_priv);

    // Send it
    client.send(&envelope).await?;

    // Fetch messages for recipient
    let messages = client.fetch(&hex::encode(recipient_key)).await?;
    for msg in messages {
        println!("From {}: {}", msg.sender, String::from_utf8_lossy(&msg.payload));
    }

    client.close(Some("done")).await;
    Ok(())
}
```

#### Server Example
```rust
use qight::relay::RelayServer;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let server = RelayServer::new("0.0.0.0:4433".parse()?)?;
    server.run().await?;
    Ok(())
}
```

## 🏗 Architecture

### Components
- **Relay Server** (`relay` binary): Central hub handling connections, storage, and message routing.
- **Client Library** (`qight` crate): API for connecting, sending, and fetching messages.
- **Message Envelope**: Structured message format with signing.
- **Key Management**: Ed25519 utilities for signing/verification.

### Message Flow
1. Client generates keys and signs message.
2. Sends over QUIC to relay.
3. Relay verifies signature and stores in SQLite.
4. Client fetches by recipient key (hex-encoded).
5. Relay returns messages and deletes them (fire-and-forget).

### Storage
- **SQLite Database**: `quic.db` for messages, `qight_outbox.db` for client queues.
- **Expiration**: Messages auto-delete after TTL.

## 📚 API Reference

### RelayClient
- `connect(addr: SocketAddr)`: Connect to specific relay.
- `connect_discovered()`: Auto-discover via mDNS.
- `hello(client_id: &str)`: Handshake.
- `send(envelope: &MessageEnvelope)`: Send signed message.
- `fetch(recipient_hex: &str)`: Fetch messages for recipient.
- `close(reason: Option<&str>)`: Disconnect.

### MessageEnvelope
- `new(sender, recipient, sender_key, payload, ttl)`: Create envelope.
- `sign(&mut self, private_key)`: Sign payload.
- `verify(&self)`: Verify signature.
- `to_bytes()` / `from_bytes(bytes)`: Serialize/deserialize.

### Key Functions
- `gen_key()`: Random 32-byte key.
- `gen_keypair()`: (public, private) Ed25519 keys.
- `sign_message(priv, msg)`: Sign bytes.
- `verify_message(pub, msg, sig)`: Verify signature.

## 🔒 Security

- **Transport Security**: QUIC with TLS 1.3 (self-signed certs for testing).
- **Message Authenticity**: Ed25519 signatures prevent tampering.
- **No Encryption**: Payloads are signed but not encrypted—add AES for confidentiality.
- **Key Management**: Clients handle keys; relay doesn't store them.
- **Denial of Service**: Basic rate limiting recommended for production.

**Warning**: Use strong keys and avoid self-signed certs in production. Implement authentication for real deployments.

## 🧪 Testing

Run tests:
```bash
cargo test
```

Includes unit tests for signing, serialization, DB ops, and integration tests.

## 🤝 Contributing

1. Fork the repo.
2. Create a feature branch: `git checkout -b feature-name`.
3. Make changes, add tests.
4. Run `cargo fmt` and `cargo clippy`.
5. Submit a PR.

### Planned Features
- Payload encryption.
- WebSocket fallback.
- REST API.
- Clustering for scalability.

## 📄 License

MIT License. See [LICENSE](LICENSE) for details.

---

Built with ❤️ in Rust. Questions? Open an issue!
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
