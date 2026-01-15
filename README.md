# Qight Crate

**A Rust implementation of the Qight Protocol: Lightweight, QUIC-based messaging transport for low-latency, bidirectional, event-driven communication.**

Qight is a minimal transport-layer protocol constructed directly upon QUIC, facilitating the efficient transmission of small, reliable, ephemeral messages. It functions as foundational infrastructure for real-time signaling and event dissemination, serving as a complement to REST APIs by obviating the necessity for HTTP polling, protracted connections, or intricate webhook configurations.

## Features

- **Optimized Low Latency**: Engineered for expeditious delivery of concise messages.
- **Bidirectional Capability**: Permits both clients and services to initiate and receive communications.
- **QUIC Integration**: Leverages QUIC's inherent attributes, including TLS 1.3 encryption, assured delivery, stream multiplexing, and seamless connection migration.
- **Lightweight Identity Management**: Employs ephemeral identifiers without mandating user accounts or authentication mechanisms.
- **Transient Message Queuing**: Relay nodes maintain messages temporarily until retrieval or expiration based on time-to-live (TTL) parameters.
- **Modular Composability**: Treats payloads as opaque entities, enabling higher-level protocols to incorporate supplementary features such as encryption, prioritization, or persistent storage.

## Installation

To incorporate the Qight crate into your Rust project, amend your `Cargo.toml` file accordingly. Given that the implementation remains under development and has not yet been published to crates.io, employ a Git dependency referencing the repository (presumed to be hosted at `https://github.com/idorocodes/qight`):

```toml
[dependencies]
qight = { git = "https://github.com/idorocodes/qight.git" }
```

Upon formal publication to crates.io, transition to a versioned dependency, for instance:

```toml
[dependencies]
qight = "0.1.0"
```

Verify that your project adheres to Rust edition 2021 or subsequent versions. If future releases introduce optional features (e.g., for extensions like message prioritization), activate them as required. Execute `cargo build` post-modification to retrieve and compile the dependency.

The crate depends on `quinn` for QUIC handling and operates asynchronously, compatible with the Tokio runtime.

## Usage

Qight furnishes an intuitive API for QUIC connection management to relay nodes, message submission through `SEND`, and retrieval via `FETCH`. All operations are asynchronous. Principal components encompass:

- `RelayClient`: Oversees the QUIC connection and fundamental protocol operations.
- `MessageEnvelope`: A struct encapsulating the message format, including fields for `msg_id`, `sender`, `recipient`, `timestamp`, `ttl`, and `payload`.

### Example 1: Basic Connection and Message Submission

This example illustrates establishing a connection, executing a `HELLO` handshake, formulating a message envelope, and submitting it via `SEND`.

```rust
use qight::{RelayClient, MessageEnvelope};
use tokio;
use uuid::Uuid;
use std::time::{SystemTime, UNIX_EPOCH};
use base64::{self, Engine as _};
use base64::engine::general_purpose::STANDARD as BASE64;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Establish a QUIC connection to the relay node
    let mut client = RelayClient::connect("relay.example.com:443").await?;

    // Initiate HELLO with an ephemeral client ID
    let client_id = Uuid::new_v4().to_string();
    client.hello(&client_id).await?;

    // Construct the message envelope
    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let envelope = MessageEnvelope {
        msg_id: Uuid::new_v4().to_string(),
        sender: client_id.clone(),
        recipient: "recipient_id_123".to_string(),
        timestamp,
        ttl: 600, // 10 minutes
        payload: BASE64.encode("Sample message payload"),
    };

    // Submit the message
    client.send(envelope).await?;

    println!("Message submitted successfully.");
    Ok(())
}
```

### Example 2: Fetching Messages and Sending Acknowledgments

This extends the prior example by retrieving messages for a designated recipient and dispatching optional `ACK` confirmations.

```rust
// Continuing from the basic example...

// Fetch pending messages
let messages = client.fetch("recipient_id_123").await?;
for msg in &messages {
    println!("Received message: ID={}, Payload={}", msg.msg_id, BASE64.decode(&msg.payload)?);
}

// Acknowledge receipts
for msg in messages {
    client.ack(&msg.msg_id).await?;
    println!("Acknowledged: {}", msg.msg_id);
}
```

### Example 3: Error Handling in Operations

Demonstrates robust error management during connection and message handling.

```rust
use qight::{RelayClient, MessageEnvelope, Error as QightError};
use tokio;
use uuid::Uuid;
use std::time::{SystemTime, UNIX_EPOCH};
use base64::{self, Engine as _};
use base64::engine::general_purpose::STANDARD as BASE64;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut client = match RelayClient::connect("invalid-relay.example.com:443").await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Connection error: {}", e);
            return Err(e.into());
        }
    };

    let client_id = Uuid::new_v4().to_string();
    if let Err(e) = client.hello(&client_id).await {
        eprintln!("HELLO error: {}", e);
        return Err(e.into());
    }

    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let envelope = MessageEnvelope {
        msg_id: Uuid::new_v4().to_string(),
        sender: client_id,
        recipient: "recipient_id_123".to_string(),
        timestamp,
        ttl: 300,
        payload: BASE64.encode("Payload with potential issues"),
    };

    match client.send(envelope).await {
        Ok(_) => println!("Send successful."),
        Err(QightError::InvalidMessage(_)) => eprintln!("Invalid message format."),
        Err(e) => eprintln!("Unexpected error: {}", e),
    }

    Ok(())
}
```

### Example 4: Advanced: Multiple Streams and Concurrent Operations

This example showcases leveraging QUIC's multiplexing for concurrent message submissions.

```rust
use qight::{RelayClient, MessageEnvelope};
use tokio::{self, join};
use uuid::Uuid;
use std::time::{SystemTime, UNIX_EPOCH};
use base64::{self, Engine as _};
use base64::engine::general_purpose::STANDARD as BASE64;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut client = RelayClient::connect("relay.example.com:443").await?;
    let client_id = Uuid::new_v4().to_string();
    client.hello(&client_id).await?;

    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();

    let envelope1 = MessageEnvelope {
        msg_id: Uuid::new_v4().to_string(),
        sender: client_id.clone(),
        recipient: "recipient_a".to_string(),
        timestamp,
        ttl: 300,
        payload: BASE64.encode("Message 1"),
    };

    let envelope2 = MessageEnvelope {
        msg_id: Uuid::new_v4().to_string(),
        sender: client_id.clone(),
        recipient: "recipient_b".to_string(),
        timestamp,
        ttl: 300,
        payload: BASE64.encode("Message 2"),
    };

    // Concurrent sends using Tokio's join!
    let (res1, res2) = join!(
        client.send(envelope1.clone()),
        client.send(envelope2.clone())
    );

    res1?;
    res2?;

    println!("Concurrent messages submitted.");
    Ok(())
}
```

## Architecture Overview

Clients initiate QUIC connections to relay nodes. Messages are enveloped with metadata and dispatched via `SEND`, temporarily queued, and accessed through `FETCH`. The protocol enforces TTL-based expiration for resource efficiency.

## Status

- **Specification Version**: Draft v0.1
- **Crate Implementation**: Under active development; Rust-based reference implementation forthcoming, including client library and relay node binary.

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
- `RelayClient::ack(msg_id: &str) -> Result<()>`: Confirms message receipt.
- `MessageEnvelope`: Struct with fields as per the protocol envelope.

Consult the crate documentation for exhaustive details.

## Built With Rust Drawing inspiration from Solana's QUIC applications and contemporary blockchain infrastructures.

## Specification

The comprehensive protocol specification resides in the repository: [SPECIFICATION.md](SPECIFICATION.md)

## Contributing

Contributions are encouraged. Kindly submit issues or pull requests to initiate discussions on enhancements, bug resolutions, or feature additions.

## License

Distributed under the MIT License.

## Author

Authored by [@idorocodes](https://x.com/idorocodes), a computer science undergraduate possessing specialized knowledge in Rust programming and blockchain technologies.
