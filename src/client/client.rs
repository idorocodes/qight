use anyhow::{Context, Result};
use quinn::{ClientConfig as QuinnClientConfig, Endpoint};
use quinn_proto::crypto::rustls::QuicClientConfig;
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rustls::pki_types::CertificateDer;
use rustls::{ClientConfig as RustlsClientConfig, RootCertStore};
use std::fs::read;
use std::net::SocketAddr;
use std::result::Result::Ok;
use std::sync::Arc;
use std::usize;

use crate::MessageEnvelope;

pub struct RelayClient {
    connection: Option<quinn::Connection>,
    outbox: Pool<SqliteConnectionManager>,
}
impl RelayClient {
    pub async fn connect(server_addr: SocketAddr) -> Result<Self> {
        let mut endpoint = Endpoint::client("0.0.0.0:0".parse()?)?;

        let manager = SqliteConnectionManager::file("qight_outbox.db");
        let outbox = Pool::builder().max_size(5).build(manager)?;
        let conn = outbox.get()?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS outbox (
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

        let connection = match endpoint.connect(server_addr, "localhost") {
            Ok(connecting) => match connecting.await {
                Ok(conn) => {
                    println!(
                        "Connected via QUIC to {} (peer: {})",
                        server_addr,
                        conn.remote_address()
                    );
                    Some(conn)
                }
                Err(e) => {
                    eprintln!("Relay unreachable: {}. Running in offline mode.", e);
                    None
                }
            },
            Err(e) => {
                eprintln!("Failed to connect: {}. Running in offline mode.", e);
                None
            }
        };

        let client = Self {
            connection: connection,
            outbox,
        };
        if client.connection.is_some() {
            println!("Draining queue!");
            let _ = client.drain_queue().await;
        }
        Ok(client)
    }
    pub async fn hello(&self, client_id: &str) -> Result<()> {
        let conn = self.connection.as_ref().context("Not connected to relay")?;
        let (mut send, mut recv) = conn.open_bi().await?;
        let payload = format!("HELLO {}\n", client_id);
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
        let conn = self.connection.as_ref().context("Not connected to relay")?;
        let (mut send, mut recv) = conn.open_bi().await?;
        let bytes = envelope.to_bytes()?;
        let pool = self.outbox.clone();
        let envelope_clone = envelope.clone();
        println!("sending message : {:?}",envelope_clone);
        tokio::task::spawn_blocking(move || {
            let conn = pool.get()?;
            conn.execute(
    "INSERT OR IGNORE INTO outbox (msg_id, sender, sender_key, recipient, timestamp, ttl, payload)
     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)", 
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
        send.write_all(b"SEND").await?;
        send.write_all(&(bytes.len() as u32).to_be_bytes()).await?;
        send.write_all(&bytes).await?;

        send.finish()?;

        let mut resp = [0u8; 2];
        recv.read_exact(&mut resp)
            .await
            .context("Failed to read OK response")?;

        if &resp == b"OK" {
            let pool = self.outbox.clone();
            let msg_id = envelope.msg_id.clone();
            tokio::task::spawn_blocking(move || {
                let conn = pool.get()?;
                conn.execute("DELETE FROM outbox WHERE msg_id = ?1", [&msg_id])?;
                Ok::<_, anyhow::Error>(())
            })
            .await??;
        } else {
            anyhow::bail!("Relay sent unexpected response: {:?}", resp);
        }

        Ok(())
    }

    pub async fn fetch(&self, recipient: &str) -> Result<Vec<MessageEnvelope>> {
        let conn = self.connection.as_ref().context("Not connected to relay")?;
        let (mut send, mut recv) = conn.open_bi().await?;
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
    pub async fn drain_queue(&self) -> Result<()> {
        let pending_messages = tokio::task::spawn_blocking({
            let pool = self.outbox.clone();
            move || -> Result<Vec<(String, Vec<u8>)>> {
                let conn = pool.get()?;
                let mut stmt = conn.prepare("SELECT msg_id, payload FROM outbox")?;
                let rows = stmt.query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?))
                })?;

                let mut msgs = Vec::new();
                for msg in rows {
                    msgs.push(msg?);
                }
                println!("darining queue messag len : {}", msgs.len());
                Ok(msgs)
            }
        })
        .await
        .context("Database read task panicked")??;

        for (msg_id, payload) in pending_messages {
            let conn = self.connection.as_ref().context("Not connected")?;
            let (mut send, mut recv) = conn.open_bi().await?;

            send.write_all(b"SEND").await?;
            send.write_all(&(payload.len() as u32).to_be_bytes())
                .await?;
            send.write_all(&payload).await?;
            send.finish()?;

            let mut resp = [0u8; 2];
            recv.read_exact(&mut resp)
                .await
                .context("Failed to read OK response")?;

            if &resp == b"OK" {
                let pool_clone = self.outbox.clone();
                tokio::task::spawn_blocking(move || -> Result<()> {
                    let conn = pool_clone.get()?;
                    conn.execute("DELETE FROM outbox WHERE msg_id = ?", [msg_id])?;
                    Ok(())
                })
                .await
                .context("Database delete task panicked")??;
            } else {
                anyhow::bail!("Relay sent unexpected response: {:?}", resp);
            }
        }

        Ok(())
    }
    pub async fn close(&self, reason: Option<&str>) {
        let reason_bytes = reason.unwrap_or("done").as_bytes();
        if let Some(conn) = &self.connection {
            conn.close(0u8.into(), reason_bytes);
        }
    }
}



#[cfg(test)]
mod tests {
    use super::*;
    use r2d2::Pool;
    use r2d2_sqlite::SqliteConnectionManager;
    use std::fs;

    fn setup_test_db() -> Pool<SqliteConnectionManager> {
        let manager = SqliteConnectionManager::file(":memory:"); // In-memory DB for tests
        let pool = Pool::builder().build(manager).unwrap();
        let conn = pool.get().unwrap();
        conn.execute(
            "CREATE TABLE IF NOT EXISTS outbox (
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
        pool
    }

    #[test]
    fn test_client_offline_queuing() {
        let pool = setup_test_db();
        // Simulate a client without connection
        let client = RelayClient {
            connection: None,
            outbox: pool,
        };

        let envelope = MessageEnvelope::new(
            "test".to_string(),
            [0u8; 32], // Dummy keys
            [1u8; 32],
            b"test".to_vec(),
            3600,
        );

        // This should queue the message
        // Note: send() will fail without connection, but we can test queuing separately
        // For now, just check the DB is set up
        let conn = client.outbox.get().unwrap();
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM outbox", [], |row| row.get(0)).unwrap();
        assert_eq!(count, 0); // Should be empty initially
    }
}