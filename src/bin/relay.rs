use anyhow::{Context, Result};
use ed25519_dalek::SIGNATURE_LENGTH;
use hex;
use mdns_sd::{ServiceDaemon, ServiceInfo};
use qight::MessageEnvelope;
use quinn::{Endpoint, ServerConfig};
use quinn_proto::crypto::rustls::QuicServerConfig;
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rcgen::generate_simple_self_signed;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::ServerConfig as RustlsServerConfig;
use std::fs::{read, write};
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::time::{UNIX_EPOCH,SystemTime};
#[tokio::main]
async fn main() -> Result<()> {
    println!("Relay Started! Listening! ");
    let addr: SocketAddr = "127.0.0.1:4433".parse()?;

    let mdns = ServiceDaemon::new().expect("Failed to create daemon");

    let receiver = mdns.monitor().expect("Failed to monitor daemon");

    std::thread::spawn(move || {
    while let Ok(event) = receiver.recv() {
        match event {
            mdns_sd::DaemonEvent::Error(error) => {
                eprintln!("Daemon error: {error}");
            }
            _ => {}
        }
    }
});

let service_type =  "_qight._udp.local.";
let instance_name = "my_instance";
let ip = "127.0.0.1";
let host_name = "127.0.0.1:4433.local.";
let port = 4433;
let properties = [("property_1", "test"), ("property_2", "1234")];

let my_service = ServiceInfo::new(
    service_type,
    instance_name,
    host_name,
    ip,
    port,
    &properties[..],
).unwrap();

// Register with the daemon, which publishes the service.
mdns.register(my_service).expect("Failed to register our service");




    let _subject_alt_names: Vec<_> = vec!["localhost"];

    let cert_path = "server_cert";
    let key_path = "server_key";

    let (cert_der, key_der) = if Path::new(cert_path).exists() {
        (
            CertificateDer::from(read(cert_path)?),
            PrivatePkcs8KeyDer::from(read(key_path)?),
        )
    } else {
        let cert_key = generate_simple_self_signed(vec!["localhost".into()])?;
        let cert = CertificateDer::from(cert_key.cert.der().to_vec());
        let key = PrivatePkcs8KeyDer::from(cert_key.signing_key.serialize_der());

        write(cert_path, cert.as_ref())?;
        write(key_path, key.secret_pkcs8_der())?;

        (cert, key)
    };

    let certs = vec![cert_der];

    let key = PrivateKeyDer::from(key_der);

    // Build rustls server configuration
    let mut rustls_config = RustlsServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .context("failed to build rustls server config")?;

    rustls_config.alpn_protocols = vec![b"qight".to_vec()];

    println!(
        "Server ALPN protocols configured: {:?}",
        rustls_config.alpn_protocols
    );

    // Create Quinn crypto layer
    let crypto =
        QuicServerConfig::try_from(rustls_config).context("failed to create QUIC crypto config")?;

    // Build Quinn server config
    let mut server_config = ServerConfig::with_crypto(Arc::new(crypto));

    // Configure transport parameters
    Arc::get_mut(&mut server_config.transport)
        .expect("transport config should be uniquely owned")
        .max_concurrent_bidi_streams(100u8.into());

    let manager = SqliteConnectionManager::file("quic.db");

    // 2. Create the pool
    let pool = Pool::builder()
        .max_size(15) // Maximum number of connections in the pool
        .build(manager)?;

    // 3. Get a connection from the pool
    let storage = pool.get()?;

   storage.execute(
    "CREATE TABLE IF NOT EXISTS messages (
        msg_id      BLOB PRIMARY KEY,  
        sender      TEXT NOT NULL,
        sender_key  BLOB NOT NULL,    
        recipient   BLOB NOT NULL,    
        timestamp   INTEGER NOT NULL,
        ttl         INTEGER NOT NULL,
        payload     BLOB NOT NULL
    )",
    (),
)?;
    storage.execute_batch("CREATE INDEX IF NOT EXISTS idx_recipient ON messages(recipient)")?;
    let endpoint =
        Endpoint::server(server_config, addr).context("failed to create QUIC endpoint")?;

    println!("QUIC server listening on {}", addr);

    while let Some(connecting) = endpoint.accept().await {
        let pool_clone = pool.clone();
        tokio::spawn(async move {
            match connecting.await {
                Ok(connection) => {
                    println!(
                        "New connection established from {}",
                        connection.remote_address()
                    );
                    if let Err(e) = handle_connection(connection, pool_clone).await {
                        eprintln!("Connection handling error: {}", e);
                    }
                }
                Err(e) => {
                    eprintln!("Handshake failed: {}", e);
                }
            }
        });
    }

    Ok(())
}

async fn handle_connection(
    connection: quinn::Connection,
    pool: Pool<SqliteConnectionManager>
) -> Result<()> {
    while let Ok((send, recv)) = connection.accept_bi().await {
        let storage = pool.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_stream(send, recv, storage).await {
                eprintln!("Stream error: {}", e);
            }
        });
    }

    Ok(())
}

async fn handle_stream(
    mut send: quinn::SendStream,
    mut recv: quinn::RecvStream,
    storage: Pool<SqliteConnectionManager>,
) -> Result<()> {
    // Read first 4 bytes
    let mut prefix = [0u8; 4];
    let n = recv
        .read(&mut prefix)
        .await
        .context("read initial 4 bytes")?
        .unwrap_or(0);

    if n == 4 && prefix == [b'S', b'E', b'N', b'D'] {
        // It's a SEND command — proceed to read length + payload
        handle_send(&mut recv, &mut send, storage).await?;
    } else {
        // Text command — accumulate until \n
        let mut command_buf = Vec::from(&prefix[0..n]);

        loop {
            let mut chunk = [0u8; 512];
            let n_opt = recv.read(&mut chunk).await.context("read text command")?;

            if let Some(n) = n_opt {
                command_buf.extend_from_slice(&chunk[..n]);
            } else {
                anyhow::bail!("stream closed before command terminator");
            }

            if let Some(pos) = command_buf.iter().position(|&b| b == b'\n') {
                let line = String::from_utf8_lossy(&command_buf[..pos])
                    .trim()
                    .to_string();
                let parts: Vec<&str> = line.split_whitespace().collect();

                if parts.is_empty() {
                    send.write_all(b"ERROR: Empty command\n").await?;
                    break;
                }

                match parts[0].to_uppercase().as_str() {
                    "HELLO" => {
                        let client_id = parts.get(1).unwrap_or(&"").to_string();
                        handle_hello(&client_id, &mut send).await?;
                    }
                    "FETCH" => {
                        let recipient = parts.get(1).unwrap_or(&"").to_string();
                        handle_fetch(&recipient, &mut send, storage).await?;
                    }
                    _ => {
                        send.write_all(b"ERROR: Unknown command\n").await?;
                    }
                }

               
                if pos + 1 < command_buf.len() {
                    // leftover garbage — log & ignore
                    println!(
                        "Warning: extra bytes after command: {:?}",
                        &command_buf[pos + 1..]
                    );
                }

                break;
            }
        }
    }
    send.finish().context("failed to finish sending stream")?;
    Ok(())
}

async fn handle_hello(client_id: &str, send: &mut quinn::SendStream) -> Result<()> {
    println!("HELLO received from client: {:?}", client_id);
    let welcome = format!("Welcome, {:?}", client_id);
    send.write_all(welcome.as_bytes()).await?;
    Ok(())
}

async fn handle_send(
    recv: &mut quinn::RecvStream,
    send: &mut quinn::SendStream,
    connection: Pool<SqliteConnectionManager>,
) -> Result<()> {
    let mut len_bytes = [0u8; 4];
    recv.read_exact(&mut len_bytes)
        .await
        .context("failed to read payload length")?;

    let len = u32::from_be_bytes(len_bytes) as usize;

    if len > 10_000_000_ {
        anyhow::bail!("payload too large: {} bytes", len);
    }

    let mut payload = vec![0u8; len];
    recv.read_exact(&mut payload)
        .await
        .context("failed to read payload")?;

    println!("Received SEND payload ({} bytes)", len);

    let envelope: MessageEnvelope =
        wincode::deserialize(&payload).context("failed to deserialize MessageEnvelope")?;
    
    if !envelope.verify() {
        send.write_all(b"ERROR: Invalid signature\n").await?;
        return Ok(());
    }
    
    println!("Stored message for recipient: {:?}", hex::encode(envelope.recipient));
    let envelope_clone = envelope.clone();

    tokio::task::spawn_blocking(move || {
        let conn = connection.clone().get()?;
        conn.execute(
            "INSERT INTO messages (msg_id,sender,sender_key,recipient,timestamp,ttl,payload)
         VALUES (?1,?2,?3,?4,?5,?6,?7)",
            (
                &envelope_clone.msg_id,
                &envelope_clone.sender,
                &envelope_clone.sender_key, 
                &envelope_clone.recipient,
                &envelope_clone.timestamp,
                &envelope_clone.ttl,
                &envelope_clone.payload,
            ),
        )?;
        Ok::<_, anyhow::Error>(())
    })
    .await??;
    println!("message stored");
    send.write_all(b"OK\n").await?;
    Ok(())
}

async fn handle_fetch(
    recipient: &str,
    send: &mut quinn::SendStream,
    connection: Pool<SqliteConnectionManager>,
) -> Result<()> {
    println!("FETCH request received for recipient: {}", recipient);

    let recipient_bytes = hex::decode(recipient).context("invalid hex recipient")?;

    let now = SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .expect("Time went backwards")
    .as_secs();

   let messages = tokio::task::spawn_blocking( move || {
         let conn = connection.clone().get()?;
         conn.execute("DELETE FROM messages WHERE timestamp + ttl < (?1)", (now,))?;

       let mut messages = conn.prepare("SELECT msg_id, sender, sender_key, recipient, timestamp, ttl, payload FROM messages WHERE recipient = ?1")?;

          let msgs: Vec<MessageEnvelope> = messages.query_map([&recipient_bytes], |row| {
        Ok(MessageEnvelope {
            msg_id: row.get(0)?,
            sender: row.get(1)?,
            sender_key:row.get(2)?,
            recipient: row.get(3)?,
            timestamp: row.get(4)?,
            ttl: row.get(5)?,
            payload: row.get(6)?,
            signature: [0u8; SIGNATURE_LENGTH],
        })
    })?.filter_map(|r| r.ok()).collect();

      for msg in &msgs {


        conn.execute("DELETE FROM messages WHERE msg_id = ?1", [&msg.msg_id])?;
    }
    
    Ok::<_, anyhow::Error>(msgs)
}).await??;

   // Phase 2 - async streaming
for msg in messages {
    let bytes = msg.to_bytes()?;
    send.write_all(&(bytes.len() as u32).to_be_bytes()).await?;
    send.write_all(&bytes).await?;
}
send.write_all(&0u32.to_be_bytes()).await?;

    // let messages = {
    //     let store = storage.lock().unwrap();
    //     store.get(recipient).cloned().unwrap_or_default()
    // };
    // println!(
    //     "Sending FETCH response for {}: {} messages",
    //     recipient,
    //     messages.len()
    // );
    // for msg in messages {
    //     let bytes = msg.to_bytes()?;
    //     send.write_all(&(bytes.len() as u32).to_be_bytes()).await?;
    //     send.write_all(&bytes).await?;
    // }
    
    Ok(())
}


#[cfg(test)]
mod tests {
    use super::*;
    use r2d2::Pool;
    use r2d2_sqlite::SqliteConnectionManager;

    fn setup_test_db() -> Pool<SqliteConnectionManager> {
        let manager = SqliteConnectionManager::file(":memory:");
        let pool = Pool::builder().build(manager).unwrap();
        let conn = pool.get().unwrap();
        conn.execute(
            "CREATE TABLE IF NOT EXISTS messages (
                msg_id BLOB PRIMARY KEY,
                sender TEXT NOT NULL,
                sender_key BLOB NOT NULL,
                recipient BLOB NOT NULL,
                timestamp INTEGER NOT NULL,
                ttl INTEGER NOT NULL,
                payload BLOB NOT NULL
            )",
            (),
        ).unwrap();
        conn.execute("CREATE INDEX IF NOT EXISTS idx_recipient ON messages(recipient)", ()).unwrap();
        pool
    }

    #[test]
    fn test_db_insert_and_query() {
        let pool = setup_test_db();
        let conn = pool.get().unwrap();

        let envelope = MessageEnvelope::new(
            "test_sender".to_string(),
            [0u8; 32],
            [1u8; 32],
            b"payload".to_vec(),
            3600,
        );

        conn.execute(
            "INSERT INTO messages (msg_id, sender, sender_key, recipient, timestamp, ttl, payload)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            (
                &envelope.msg_id,
                &envelope.sender,
                &envelope.sender_key,
                &envelope.recipient,
                &envelope.timestamp,
                &envelope.ttl,
                &envelope.payload,
            ),
        ).unwrap();

        let count: i64 = conn.query_row("SELECT COUNT(*) FROM messages", [], |row| row.get(0)).unwrap();
        assert_eq!(count, 1);
    }
}