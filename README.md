# qight

**Offline-first local messaging over QUIC. No internet. No configuration. Guaranteed delivery.**

qight is a Rust-powered messaging system for environments where the internet can't be trusted. Deploy one relay on any machine — a laptop, a Raspberry Pi — and every device on the local network connects automatically and exchanges messages reliably, even through connection drops and relay restarts.

Built on QUIC for native resilience to packet loss and connection migration. No cloud dependency. No DNS. No polling.

---

## How it works

```
[ Device A ] ──┐
[ Device B ] ──┼──► [ qight relay ] (Raspberry Pi / laptop)
[ Device C ] ──┘         │
                     SQLite persistence
                     mDNS discovery
```

One relay runs on the network. Clients discover it automatically via mDNS — no hardcoded IPs, no configuration. Messages are persisted to SQLite and delivered when the recipient connects, even if the relay restarted in between.

---

## Deployment

**Step 1 — Run the relay on any machine on the local network:**

```bash
cargo run --bin relay
```

The relay announces itself via mDNS. Any device on the same network will find it automatically.

**Step 2 — Add the library to your app:**

```toml
[dependencies]
qight = { git = "https://github.com/idorocodes/qight.git" }
```

**Step 3 — Connect, send, receive:**

```rust
use anyhow::Result;
use mdns_sd::{ServiceDaemon, ServiceEvent, ScopedIp};
use std::net::{IpAddr, SocketAddr};
use std::sync::mpsc;

#[tokio::main]
async fn main() -> Result<()> {
    // Discover relay automatically — no hardcoded IP
    let (tx, rx) = mpsc::channel::<SocketAddr>();
    let mdns = ServiceDaemon::new()?;
    let receiver = mdns.browse("_qight._udp.local.")?;

    std::thread::spawn(move || {
        while let Ok(event) = receiver.recv() {
            if let ServiceEvent::ServiceResolved(resolved) = event {
                if let Some(scoped) = resolved.get_addresses().iter().next() {
                    let ip: IpAddr = match scoped {
                        ScopedIp::V4(v4) => IpAddr::V4(*v4.addr()),
                        ScopedIp::V6(v6) => IpAddr::V6(*v6.addr()),
                        _ => continue,
                    };
                    tx.send(SocketAddr::new(ip, resolved.get_port())).unwrap();
                }
            }
        }
    });

    let addr = rx.recv()?;
    let client = qight::RelayClient::connect(addr).await?;
    client.hello("device-001").await?;

    // Send a message
    let envelope = qight::MessageEnvelope::new(
        "alice".into(),
        "bob".into(),
        b"coordinates: 4.8156, 7.0498".to_vec(),
        3600,
    );
    client.send(&envelope).await?;

    // Fetch messages for this device
    let messages = client.fetch("bob").await?;
    for msg in messages {
        println!("{} → {}: {:?}", msg.sender, msg.recipient, String::from_utf8_lossy(&msg.payload));
    }

    client.close(None).await;
    Ok(())
}
```

---

## What works today

- QUIC transport with TLS (self-signed cert, auto-generated on first run)
- Automatic relay discovery via mDNS — zero configuration
- Persistent message storage via SQLite — survives relay restarts
- TTL-based message expiration
- Message delivery confirmation — send blocks until relay confirms storage
- Store-and-forward — messages queue on relay until recipient connects

## Roadmap

- [ ] Client-side outbound queue — messages survive relay downtime
- [ ] Pre-shared key authentication
- [ ] Clean high-level API (`send_to` / `receive` instead of raw protocol)
- [ ] ARM cross-compilation — first-class Raspberry Pi support
- [ ] Published crate on crates.io

---

## Use cases

- Rescue and disaster response field communications
- Remote industrial sites with no internet connectivity
- Rural environments with unreliable or no connectivity
- Offline-first applications requiring local device coordination
- Off-chain signaling for blockchain applications

---

## Architecture

The relay is a standalone binary. The library is what applications embed. They are separate — the relay handles all persistence and routing, the library handles connection and protocol.

```
qight (library)   — embed in your app, handles QUIC connection and protocol
qight-relay (bin) — deploy on field machine, handles storage and message routing
```

---

## API Reference

- `RelayClient::connect(addr: SocketAddr) -> Result<RelayClient>` — establish QUIC connection to relay
- `RelayClient::hello(client_id: &str) -> Result<()>` — register with relay
- `RelayClient::send(envelope: &MessageEnvelope) -> Result<()>` — send message, blocks until relay confirms storage
- `RelayClient::fetch(recipient: &str) -> Result<Vec<MessageEnvelope>>` — retrieve and delete all pending messages
- `RelayClient::close(reason: Option<&str>)` — close connection
- `MessageEnvelope::new(sender, recipient, payload, ttl)` — construct a message

---

## License

MIT — [@idorocodes](https://github.com/idorocodes)
