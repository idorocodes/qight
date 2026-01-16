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
