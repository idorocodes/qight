use anyhow::Result;
use mdns_sd::{ServiceDaemon, ServiceEvent};
use qight::gen_keypair;
use std::net::SocketAddr;
use std::net::IpAddr;
use tokio::sync::oneshot;
use mdns_sd::ScopedIp;
#[tokio::main]
async fn main() -> Result<()> {
    let (tx, rx) = oneshot::channel::<SocketAddr>();
let mdns = ServiceDaemon::new().expect("Failed to create daemon");
let service_type = "_qight._udp.local.";
let receiver = mdns.browse(service_type).expect("Failed to browse");

tokio::spawn(async move {
    while let Ok(event) = receiver.recv_async().await {
        match event {
            ServiceEvent::ServiceResolved(resolved) => {
                if let Some(scoped) = resolved.get_addresses().iter().next() {
                    let ip: IpAddr = match scoped {
                        ScopedIp::V4(v4) => IpAddr::V4(*v4.addr()),
                        ScopedIp::V6(v6) => IpAddr::V6(*v6.addr()),
                        _ => continue,
                    };
                    let addr = SocketAddr::new(ip, resolved.get_port());
                    let _ = tx.send(addr);
                    break;
                }
            }
            _ => {}
        }
    }
});

let addr = tokio::time::timeout(
    std::time::Duration::from_secs(5),
    rx
).await
.expect("Discovery timed out")?;

    let client = qight::RelayClient::connect(addr).await?;
    client.hello("test-client-123").await?;

    let (recipient_key, _) = gen_keypair();
    let (sender_pub, sender_priv) = gen_keypair();

    let mut envelope = qight::MessageEnvelope::new(
        "alice".into(),
        recipient_key,
        sender_pub,
        b"idorocodes is saying hell!".to_vec(),
        3600,
    );
    envelope.sign(&sender_priv);
    client.send(&envelope).await?;

    let messages = client.fetch(&hex::encode(recipient_key)).await?;

    println!("Fetched {} message(s):", messages.len());
    for msg in messages {
        println!(
            "  {:?} → {:?} : {:?}",
            &msg.sender,
            &msg.recipient,
            String::from_utf8_lossy(&msg.payload)
        );
    }

    client.close(Some("test complete")).await;

    Ok(())
}
