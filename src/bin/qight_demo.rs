use anyhow::Result;
use mdns_sd::{ServiceDaemon, ServiceEvent};
use std::net::SocketAddr;
use std::sync::mpsc;
use std::net::IpAddr;
use mdns_sd::ScopedIp;
#[tokio::main]
async fn main() -> Result<()> {
    let (tx, rx) = mpsc::channel::<SocketAddr>();
    let mdns = ServiceDaemon::new().expect("Failed to create daemon");

    // Browse for a service type.
    let service_type = "_qight._udp.local.";
    let receiver = mdns.browse(service_type).expect("Failed to browse");

    std::thread::spawn(move || {
        while let Ok(event) = receiver.recv() {
            match event {
                ServiceEvent::ServiceResolved(resolved) => {
                    if let Some(scoped) = resolved.get_addresses().iter().next() {
                        let ip: IpAddr = match scoped {
                            ScopedIp::V4(v4) => IpAddr::V4(*v4.addr()),
                            ScopedIp::V6(v6) => IpAddr::V6(*v6.addr()),
                             &_ => todo!(),
                        };
                        let addr = SocketAddr::new(ip, resolved.get_port());
                        tx.send(addr).unwrap();
                    }
                }
                _ => {}
            }
        }
    });

    let addr = rx.recv().unwrap();
    let client = qight::RelayClient::connect(addr).await?;
    client.hello("test-client-123").await?;

    let envelope = qight::MessageEnvelope::new(
        "alice".into(),
        "alice".into(),
        b"idorocodes  is saying hello via quic!".to_vec(),
        3600,
    );
    client.send(&envelope).await?;

    let messages = client.fetch("alice").await?;

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
