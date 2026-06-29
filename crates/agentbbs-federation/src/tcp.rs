//! Live peer-to-peer federation over TCP.
//!
//! [`TcpTransport`] is a production [`Transport`]: it dials a peer's `addr` and
//! writes a length-prefixed sealed envelope. [`FederationServer`] is the
//! matching ingress — it accepts TCP connections, reads framed envelopes, and
//! feeds each to [`Federator::ingest`], which verifies the node signature (and,
//! for replicated posts, the author signature) before anything touches the
//! store.
//!
//! Wire framing: a 4-byte big-endian length prefix followed by that many bytes
//! of [`FederationEnvelope::to_bytes`] output. Frames larger than
//! [`MAX_FRAME`] are refused to bound memory.

use std::net::SocketAddr;
use std::sync::Arc;

use agentbbs_core::{Error, Result};
use async_trait::async_trait;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use crate::federator::Federator;
use crate::peer::Peer;
use crate::transport::Transport;

/// Maximum accepted frame size (8 MiB) — a sanity bound against abuse.
pub const MAX_FRAME: u32 = 8 * 1024 * 1024;

/// A [`Transport`] that delivers envelopes to peers over TCP.
#[derive(Clone, Default)]
pub struct TcpTransport {
    timeout: std::time::Duration,
}

impl TcpTransport {
    /// A transport with a default 5s connect/write timeout.
    pub fn new() -> Self {
        TcpTransport {
            timeout: std::time::Duration::from_secs(5),
        }
    }

    /// Override the connect/write timeout.
    pub fn with_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.timeout = timeout;
        self
    }

    async fn deliver(&self, addr: &str, bytes: &[u8]) -> std::io::Result<()> {
        let mut stream = TcpStream::connect(addr).await?;
        let len = (bytes.len() as u32).to_be_bytes();
        stream.write_all(&len).await?;
        stream.write_all(bytes).await?;
        stream.flush().await?;
        stream.shutdown().await?;
        Ok(())
    }
}

#[async_trait]
impl Transport for TcpTransport {
    async fn send(&self, peer: &Peer, bytes: Vec<u8>) -> Result<()> {
        // Federation egress is best-effort and idempotent: an unreachable peer
        // is a no-op, not a failure (it can re-sync later). We only surface a
        // hard error for clearly invalid framing.
        if bytes.len() as u32 > MAX_FRAME {
            return Err(Error::Other("federation frame exceeds MAX_FRAME".into()));
        }
        let _ = tokio::time::timeout(self.timeout, self.deliver(&peer.addr, &bytes)).await;
        Ok(())
    }
}

/// A TCP ingress server that applies inbound envelopes to a [`Federator`].
pub struct FederationServer {
    federator: Arc<Federator>,
}

impl FederationServer {
    /// Build a server over a shared federator.
    pub fn new(federator: Arc<Federator>) -> Self {
        FederationServer { federator }
    }

    /// Bind a listener, returning it and the resolved local address (use
    /// `127.0.0.1:0` to get an ephemeral port).
    pub async fn bind(addr: &str) -> Result<(TcpListener, SocketAddr)> {
        let listener = TcpListener::bind(addr)
            .await
            .map_err(|e| Error::Other(format!("federation bind {addr}: {e}")))?;
        let local = listener
            .local_addr()
            .map_err(|e| Error::Other(format!("federation local_addr: {e}")))?;
        Ok((listener, local))
    }

    /// Accept connections forever, applying every framed envelope. Each
    /// connection is handled on its own task; a malformed frame ends only that
    /// connection.
    pub async fn serve(self, listener: TcpListener) -> Result<()> {
        loop {
            let (stream, _peer_addr) = listener
                .accept()
                .await
                .map_err(|e| Error::Other(format!("federation accept: {e}")))?;
            let federator = self.federator.clone();
            tokio::spawn(async move {
                let _ = handle_connection(stream, federator).await;
            });
        }
    }
}

async fn handle_connection(
    mut stream: TcpStream,
    federator: Arc<Federator>,
) -> std::io::Result<()> {
    loop {
        let mut len_buf = [0u8; 4];
        // Clean EOF between frames ends the connection.
        match stream.read_exact(&mut len_buf).await {
            Ok(_) => {}
            Err(_) => return Ok(()),
        }
        let len = u32::from_be_bytes(len_buf);
        if len == 0 || len > MAX_FRAME {
            return Ok(());
        }
        let mut buf = vec![0u8; len as usize];
        stream.read_exact(&mut buf).await?;
        // Zero-trust ingest verifies before persisting; a bad frame is dropped.
        let _ = federator.ingest(&buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::peer::{PeerBook, TrustLevel};
    use crate::transport::Transport;
    use agentbbs_core::{Board, Identity, MemoryStore, Message, MessageBody, NullReporter, Store};
    use chrono::Utc;

    fn author_message(author: &Identity, board: &str, text: &str) -> Message {
        let body = MessageBody {
            board: board.into(),
            parent: None,
            subject: "s".into(),
            body: text.into(),
            author: author.id(),
            handle: "h".into(),
            created_at: Utc::now(),
        };
        Message::sign(author, body).unwrap()
    }

    #[tokio::test]
    async fn two_node_tcp_replication() {
        // Receiver node B with a TCP ingress server.
        let b_store: Arc<dyn Store> = Arc::new(MemoryStore::new());
        let b_fed = Arc::new(Federator::new(
            Identity::generate(),
            b_store.clone(),
            Arc::new(NullReporter),
            Arc::new(TcpTransport::new()),
            PeerBook::new(),
        ));
        let (listener, addr) = FederationServer::bind("127.0.0.1:0").await.unwrap();
        let server = FederationServer::new(b_fed.clone());
        tokio::spawn(async move { server.serve(listener).await.ok() });

        // Sender node A trusts B at its bound address.
        let mut peers = PeerBook::new();
        peers.add(Peer::new(
            b_fed.node_id(),
            addr.to_string(),
            TrustLevel::Trusted,
        ));
        let a_store: Arc<dyn Store> = Arc::new(MemoryStore::new());
        let a_fed = Federator::new(
            Identity::generate(),
            a_store.clone(),
            Arc::new(NullReporter),
            Arc::new(TcpTransport::new()),
            peers,
        );

        // An ordinary author (distinct from the relaying node) signs content.
        let author = Identity::generate();
        let board = Board::new("general", "General", author.id());
        let msg = author_message(&author, "general", "hello over tcp");

        a_fed.announce_board(&board).await.unwrap();
        a_fed.replicate_message(&msg).await.unwrap();

        // Poll B's store until the replicated content lands (or time out).
        let mut got = false;
        for _ in 0..50 {
            if b_store.get_board("general").unwrap().is_some()
                && b_store.get_message(&msg.id).unwrap().is_some()
            {
                got = true;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        assert!(got, "message did not replicate over TCP");
        let replicated = b_store.get_message(&msg.id).unwrap().unwrap();
        assert_eq!(replicated.body.body, "hello over tcp");
        assert!(replicated.verify().is_ok());
    }

    #[tokio::test]
    async fn unreachable_peer_is_best_effort() {
        // Sending to a closed port must not error (best-effort egress).
        let t = TcpTransport::new().with_timeout(std::time::Duration::from_millis(200));
        let peer = Peer::new(
            Identity::generate().id(),
            "127.0.0.1:9",
            TrustLevel::Trusted,
        );
        assert!(t.send(&peer, vec![1, 2, 3]).await.is_ok());
    }
}
