//! Peers and the trust they're held at.
//!
//! Federation is zero-trust by default: a freshly-seen node is [`Unknown`] and
//! receives nothing. An operator promotes a node to [`Linked`] (mutual hello)
//! or [`Trusted`] (egress target). Only [`Trusted`] peers receive announces
//! and replicated messages; ingest still cryptographically verifies everything
//! regardless of trust, so trust governs *egress*, not authenticity.
//!
//! [`Unknown`]: TrustLevel::Unknown
//! [`Linked`]: TrustLevel::Linked
//! [`Trusted`]: TrustLevel::Trusted

use std::collections::BTreeMap;

use agentbbs_core::AgentId;
use serde::{Deserialize, Serialize};

/// How much a peer is trusted for egress.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrustLevel {
    /// Seen but not vetted; receives no egress.
    #[default]
    Unknown,
    /// Mutually linked; metadata may flow.
    Linked,
    /// Fully trusted egress target.
    Trusted,
}

/// A known federation peer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Peer {
    /// The peer node's anonymous identity.
    pub node: AgentId,
    /// Transport address (opaque to federation; interpreted by the transport).
    pub addr: String,
    /// Egress trust level.
    pub trust: TrustLevel,
}

impl Peer {
    /// A new peer at the given trust level.
    pub fn new(node: AgentId, addr: impl Into<String>, trust: TrustLevel) -> Self {
        Peer {
            node,
            addr: addr.into(),
            trust,
        }
    }
}

/// A thread-unaware registry of peers keyed by node id. Cheap to clone-iterate.
#[derive(Default, Debug, Clone)]
pub struct PeerBook {
    peers: BTreeMap<String, Peer>,
}

impl PeerBook {
    /// An empty book.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add or replace a peer (keyed by node id).
    pub fn add(&mut self, peer: Peer) {
        self.peers.insert(peer.node.to_hex(), peer);
    }

    /// Remove a peer by node id; returns it if present.
    pub fn remove(&mut self, node: &AgentId) -> Option<Peer> {
        self.peers.remove(&node.to_hex())
    }

    /// Look up a peer by node id.
    pub fn get(&self, node: &AgentId) -> Option<&Peer> {
        self.peers.get(&node.to_hex())
    }

    /// Every known peer, in node-id order.
    pub fn all(&self) -> Vec<Peer> {
        self.peers.values().cloned().collect()
    }

    /// Only the [`TrustLevel::Trusted`] peers — the egress set.
    pub fn trusted(&self) -> Vec<Peer> {
        self.peers
            .values()
            .filter(|p| p.trust == TrustLevel::Trusted)
            .cloned()
            .collect()
    }

    /// Number of known peers.
    pub fn len(&self) -> usize {
        self.peers.len()
    }

    /// Whether the book is empty.
    pub fn is_empty(&self) -> bool {
        self.peers.is_empty()
    }
}
