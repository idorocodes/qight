use anyhow::Result;
use std::net::SocketAddr;

#[tokio::main]
async fn main() -> Result<()> {
    let addr: SocketAddr = "127.0.0.1:4433".parse()?;

    let client = qight::RelayClient::connect(addr).await?;

    // Test 1: HELLO
    client.hello("test-client-123").await?;

    // Test 2: SEND
    let envelope = qight::MessageEnvelope::new(
        "alice".into(),
        "bob".into(),
        b"hello via QUIC".to_vec(),
        3600,
    );
    client.send(&envelope).await?;

    // Test 3: FETCH
    let messages = client.fetch("bob").await?;
    println!("Fetched {} message(s):", messages.len());
    for msg in messages {
        println!("  {} → {} : {:?}", msg.sender, msg.recipient, String::from_utf8_lossy(&msg.payload));
    }

    client.close(Some("test complete")).await;

    Ok(())
}