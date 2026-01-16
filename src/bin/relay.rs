use anyhow::{Context, Result};
use quinn::{Endpoint, ServerConfig};
use quinn_proto::crypto::rustls::QuicServerConfig;
use rcgen::generate_simple_self_signed;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::ServerConfig as RustlsServerConfig;
use std::net::SocketAddr;
use std::sync::Arc;
use std::fs::{write,read};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::main]
async fn main() -> Result<()> {
    let addr: SocketAddr = "127.0.0.1:4433".parse()?;

    // Generate self-signed certificate for local testing
    let subject_alt_names = vec!["localhost".into()];

    let cert_path = "server_path";
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
    write(key_path, key.as_ref())?;

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

  
    println!("Server ALPN protocols configured: {:?}", rustls_config.alpn_protocols);

    // Create Quinn crypto layer
    let crypto = QuicServerConfig::try_from(rustls_config)
        .context("failed to create QUIC crypto config")?;

    // Build Quinn server config
    let mut server_config = ServerConfig::with_crypto(Arc::new(crypto));

    // Configure transport parameters
    Arc::get_mut(&mut server_config.transport)
        .expect("transport config should be uniquely owned")
        .max_concurrent_bidi_streams(100u8.into());

    // Create and bind endpoint
    let endpoint = Endpoint::server(server_config, addr)
        .context("failed to create QUIC endpoint")?;

    println!("QUIC server listening on {}", addr);

    while let Some(connecting) = endpoint.accept().await {
        tokio::spawn(async move {
            match connecting.await {
                Ok(connection) => {
                    println!("New connection established from {}", connection.remote_address());
                    if let Err(e) = handle_connection(connection).await {
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

async fn handle_connection(connection: quinn::Connection) -> Result<()> {
    while let Ok((send, recv)) = connection.accept_bi().await {
        tokio::spawn(async move {
            if let Err(e) = handle_stream(send, recv).await {
                eprintln!("Stream error: {}", e);
            }
        });
    }

    Ok(())
}

async fn handle_stream(
    mut send: quinn::SendStream,
    mut recv: quinn::RecvStream,
) -> Result<()> {
    let mut buf = vec![0u8; 4096];

    let n = recv
        .read(&mut buf)
        .await
        .context("failed to read initial data")?
        .context("stream closed before receiving command")?;

    let command_text = String::from_utf8_lossy(&buf[0..n]).trim().to_string();

    if command_text.starts_with("HELLO\n") {
        let client_id = command_text.strip_prefix("HELLO\n").unwrap_or("").to_string();
        handle_hello(&client_id, &mut send).await?;
    } else if command_text == "SEND" {
        handle_send(&mut recv, &mut send).await?;
    } else if command_text.starts_with("FETCH\n") {
        let recipient = command_text.strip_prefix("FETCH\n").unwrap_or("").to_string();
        handle_fetch(&recipient, &mut send).await?;
    } else {
        send.write_all(b"ERROR: Unknown command\n").await?;
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

async fn handle_send(recv: &mut quinn::RecvStream, send: &mut quinn::SendStream) -> Result<()> {
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

    send.write_all(b"OK\n").await?;
    Ok(())
}

async fn handle_fetch(recipient: &str, send: &mut quinn::SendStream) -> Result<()> {
    println!("FETCH request received for recipient: {}", recipient);

    // Currently sending dummy messages – replace with real storage later
    for i in 1..=2 {
        let dummy_msg = format!(
            "msg-{}\nfrom:sender{}\nto:{}\npayload:hello from server",
            i, i, recipient
        );
        let bytes = dummy_msg.as_bytes();

        send.write_all(&(bytes.len() as u32).to_be_bytes()).await?;
        send.write_all(bytes).await?;
    }

    // End marker
    send.write_all(&0u32.to_be_bytes()).await?;

    Ok(())
}