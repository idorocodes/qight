use anyhow::{Context, Result};
use quinn::{ClientConfig as QuinnClientConfig, Endpoint};
use quinn_proto::crypto::rustls::QuicClientConfig;
use rustls::pki_types::CertificateDer;
use rustls::{ClientConfig as RustlsClientConfig, RootCertStore};
use std::fs::read;
use std::net::SocketAddr;
use std::result::Result::Ok;
use std::sync::Arc;
use std::usize;

use crate::MessageEnvelope;

pub struct RelayClient {
    connection: quinn::Connection,
}

impl RelayClient {
    pub async fn connect(server_addr: SocketAddr) -> Result<Self> {
        let mut endpoint = Endpoint::client("0.0.0.0:0".parse()?)?;

        let mut roots = RootCertStore::empty();
        let read_cert = read("server_cert")?;
        roots.add(CertificateDer::from(read_cert))?;

        let mut rustls_config = RustlsClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        rustls_config.alpn_protocols = vec![b"qight".to_vec()];

        let quic_crypto =
            QuicClientConfig::try_from(rustls_config).context("invalid rustls config")?;

        let client_config = QuinnClientConfig::new(Arc::new(quic_crypto));

        endpoint.set_default_client_config(client_config);

        let connecting = endpoint
            .connect(server_addr, "localhost")
            .context("failed to create connecting future")?;

        let connection = connecting.await.context("QUIC handshake failed")?;

        println!(
            "Connected via QUIC to {} (peer: {})",
            server_addr,
            connection.remote_address()
        );
        Ok(Self { connection })
    }

    pub async fn hello(&self, client_id: &str) -> Result<()> {
        let (mut send, mut recv) = self
            .connection
            .open_bi()
            .await
            .context("failed to open bidirectional stream")?;

        let payload = format!("HELLO\n{}", client_id);
        send.write_all(payload.as_bytes()).await?;
        send.finish()?;

        let mut buf = vec![0; 1024];
        let n = recv.read(&mut buf).await.context("read hello response")?;

        match n {
            Some(_) => println!("Hello Response Recieved from Server"),
            None => println!("No response from server"),
        }

        Ok(())
    }

    pub async fn send(&self, envelope: &MessageEnvelope) -> Result<()> {
        let (mut send, mut _recv) = self
            .connection
            .open_bi()
            .await
            .context("failed to open bidirectional stream")?;

        let bytes = envelope.to_bytes()?;

        send.write_all(b"SEND").await?;
        send.write_all(&(bytes.len() as u32).to_be_bytes()).await?;
        send.write_all(&bytes).await?;

        send.finish()?;

        Ok(())
    }

    pub async fn fetch(&self, recipient: &str) -> Result<Vec<MessageEnvelope>> {
        let (mut send, mut recv) = self
            .connection
            .open_bi()
            .await
            .context("open bi for fetch")?;

        let req = format!("FETCH {}\n", recipient.trim());
        send.write_all(req.as_bytes()).await?;
        send.finish()?;

        let mut messages = Vec::new();

        loop {
            let mut len_bytes = [0u8; 4];
            match recv.read_exact(&mut len_bytes).await {
                Ok(()) => {
                    let len = u32::from_be_bytes(len_bytes) as usize;

                    if len == 0 {
                        break;
                    }

                    if len > 5_000_000 {
                        anyhow::bail!(
                            "refusing to read suspiciously large message ({} bytes)",
                            len
                        );
                    }

                    let mut payload = vec![0u8; len];
                    recv.read_exact(&mut payload)
                        .await
                        .context("failed to read message payload")?;

                    let envelope = wincode::deserialize(&payload)
                        .context("failed to deserialize MessageEnvelope")?;

                    messages.push(envelope);
                }

                Err(e) => {
                    return Err(e.into());
                }
            }
        }
        Ok(messages)
    }
    pub async fn close(&self, reason: Option<&str>) {
        let reason_bytes = reason.unwrap_or("done").as_bytes();
        self.connection.close(0u8.into(), reason_bytes);
    }
}
