//! Pluggable byte transport between nodes.
//!
//! Federation logic is transport-agnostic: it hands sealed envelope bytes to a
//! [`Transport`] addressed to a [`Peer`]. Production deployments wire this to
//! HTTP/QUIC/SSH; tests use [`LoopbackTransport`], which routes bytes into
//! per-node in-memory inboxes over tokio channels.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use agentbbs_core::{AgentId, Result};
use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::peer::Peer;

/// A way to deliver opaque bytes to a peer.
#[async_trait]
pub trait Transport: Send + Sync {
    /// Deliver `bytes` to `peer`. Returns once handed off (not once processed).
    async fn send(&self, peer: &Peer, bytes: Vec<u8>) -> Result<()>;
}

/// An in-process transport for tests and single-host clusters.
///
/// Each node registered via [`LoopbackTransport::inbox`] gets an unbounded
/// mpsc channel. `send` looks up the destination peer's node id and pushes the
/// bytes onto that node's inbox, which the receiving side drains and feeds to
/// its [`Federator::ingest`](crate::Federator::ingest).
#[derive(Clone, Default)]
pub struct LoopbackTransport {
    inboxes: Arc<Mutex<HashMap<String, mpsc::UnboundedSender<Vec<u8>>>>>,
}

impl LoopbackTransport {
    /// A fresh transport with no registered nodes.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register `node` and obtain the receiver end of its inbox. Bytes sent to
    /// a peer whose `node` equals this id will arrive on the returned channel.
    pub fn inbox(&self, node: AgentId) -> mpsc::UnboundedReceiver<Vec<u8>> {
        let (tx, rx) = mpsc::unbounded_channel();
        self.inboxes.lock().unwrap().insert(node.to_hex(), tx);
        rx
    }
}

#[async_trait]
impl Transport for LoopbackTransport {
    async fn send(&self, peer: &Peer, bytes: Vec<u8>) -> Result<()> {
        let tx = {
            let inboxes = self.inboxes.lock().unwrap();
            inboxes.get(&peer.node.to_hex()).cloned()
        };
        match tx {
            // A missing inbox is a no-op delivery (peer offline), not an error:
            // federation egress is best-effort and idempotent.
            Some(tx) => {
                let _ = tx.send(bytes);
                Ok(())
            }
            None => Ok(()),
        }
    }
}
