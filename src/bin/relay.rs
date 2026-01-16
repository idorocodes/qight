use anyhow::{Context, Result};
use qight::MessageEnvelope;
use quinn::{Endpoint, ServerConfig};
use quinn_proto::crypto::rustls::QuicServerConfig;
use rcgen::generate_simple_self_signed;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::ServerConfig as RustlsServerConfig;
use std::collections::HashMap;
use std::fs::{read, write};
use std::net::SocketAddr;
use std::path::Path;
use std::sync::{Arc, Mutex};
#[tokio::main]
async fn main() -> Result<()> {
    let addr: SocketAddr = "127.0.0.1:4433".parse()?;

    // Generate self-signed certificate for local testing
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

    let storage: Arc<Mutex<HashMap<String, Vec<MessageEnvelope>>>> =
        Arc::new(Mutex::new(HashMap::new()));

    let endpoint =
        Endpoint::server(server_config, addr).context("failed to create QUIC endpoint")?;

    println!("QUIC server listening on {}", addr);

    while let Some(connecting) = endpoint.accept().await {
        let storage_clone = storage.clone();
        tokio::spawn(async move {
            match connecting.await {
                Ok(connection) => {
                    println!(
                        "New connection established from {}",
                        connection.remote_address()
                    );
                    if let Err(e) = handle_connection(connection, storage_clone).await {
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
    storage: Arc<Mutex<HashMap<String, Vec<MessageEnvelope>>>>,
) -> Result<()> {
    while let Ok((send, recv)) = connection.accept_bi().await {
        let storage = storage.clone();
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
    storage: Arc<Mutex<HashMap<String, Vec<MessageEnvelope>>>>,
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

                // Drain anything after the \n (safety)
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
    println!("HELLO received from client: {}", client_id);
    let welcome = format!("Welcome, {}!\n", client_id);
    send.write_all(welcome.as_bytes()).await?;
    Ok(())
}

async fn handle_send(
    recv: &mut quinn::RecvStream,
    send: &mut quinn::SendStream,
    storage: Arc<Mutex<HashMap<String, Vec<MessageEnvelope>>>>,
) -> Result<()> {
    let mut len_bytes = [0u8; 4];
    recv.read_exact(&mut len_bytes)
        .await
        .context("failed to read payload length")?;

    let len = u32::from_be_bytes(len_bytes) as usize;

    if len > 10_000_000 {
        anyhow::bail!("payload too large: {} bytes", len);
    }

    let mut payload = vec![0u8; len];
    recv.read_exact(&mut payload)
        .await
        .context("failed to read payload")?;

    println!("Received SEND payload ({} bytes)", len);

    let envelope: MessageEnvelope =
        wincode::deserialize(&payload).context("failed to deserialize MessageEnvelope")?;
    println!("Stored message for recipient: {}", envelope.recipient);
    {
        let mut store = storage.lock().unwrap();
        store
            .entry(envelope.recipient.clone())
            .or_insert_with(Vec::new)
            .push(envelope);
    }
    println!("message stored");
    send.write_all(b"OK\n").await?;
    Ok(())
}

async fn handle_fetch(
    recipient: &str,
    send: &mut quinn::SendStream,
    storage: Arc<Mutex<HashMap<String, Vec<MessageEnvelope>>>>,
) -> Result<()> {
    println!("FETCH request received for recipient: {}", recipient);

    let messages = {
        let store = storage.lock().unwrap();
        store.get(recipient).cloned().unwrap_or_default()
    };
    println!(
        "Sending FETCH response for {}: {} messages",
        recipient,
        messages.len()
    );
    for msg in messages {
        let bytes = msg.to_bytes()?;
        send.write_all(&(bytes.len() as u32).to_be_bytes()).await?;
        send.write_all(&bytes).await?;
    }

    send.write_all(&0u32.to_be_bytes()).await?;
    Ok(())
}
