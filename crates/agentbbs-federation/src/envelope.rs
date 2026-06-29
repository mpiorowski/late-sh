//! The signed federation envelope — the unit of zero-trust replication.
//!
//! Every byte that crosses a node boundary is wrapped in a
//! [`FederationEnvelope`]: a [`FederationPayload`] plus the sending node's
//! [`AgentId`], a monotonic sequence number, and an Ed25519 signature over a
//! deterministic canonical encoding. A receiver re-derives those canonical
//! bytes and verifies the signature before trusting *anything* inside, exactly
//! as core's [`MessageBody`](agentbbs_core::MessageBody) self-authenticates a
//! post. Forged or tampered envelopes are rejected with
//! [`Error::BadSignature`].

use agentbbs_core::{
    AgentId, Board, Error, Identity, Message, Result, SignatureBytes, PROTOCOL_VERSION,
};
use serde::{Deserialize, Serialize};

/// What a node is telling its peers.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FederationPayload {
    /// "This board exists and is federated; mirror its metadata."
    AnnounceBoard(Board),
    /// "Here is a verified, content-addressed message; store it idempotently."
    ReplicateMessage(Message),
    /// A peer introducing itself on link-up.
    PeerHello {
        /// The greeting node's identity.
        node: AgentId,
        /// The protocol version string the node speaks.
        protocol: String,
    },
    /// Acknowledgement of a previously-seen envelope/message id.
    Ack {
        /// The id being acknowledged.
        id: String,
    },
}

/// A signed, replayable federation message.
///
/// The signature covers [`signing_bytes`](FederationEnvelope::signing_bytes):
/// a version tag, the node's hex id, the sequence number, and the
/// length-prefixed JSON of the payload. Because the payload length is mixed in
/// before its bytes, no field can be smuggled across the framing boundary.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct FederationEnvelope {
    /// The node that sealed (signed) this envelope.
    pub node: AgentId,
    /// Per-node monotonic sequence number (replay/ordering aid).
    pub seq: u64,
    /// The wrapped payload.
    pub payload: FederationPayload,
    /// The node's detached signature over [`Self::signing_bytes`].
    pub signature: SignatureBytes,
}

impl FederationEnvelope {
    /// Canonical, deterministic bytes the node signs. Field order is fixed and
    /// the payload is length-prefixed; two nodes computing this for the same
    /// logical envelope must produce identical bytes.
    ///
    /// `payload_json` is passed in so seal/verify share one serialization and
    /// can never disagree on the bytes.
    fn compose_signing_bytes(node: &AgentId, seq: u64, payload_json: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(payload_json.len() + 128);
        out.extend_from_slice(b"agentbbs.fed.v1\n");
        out.extend_from_slice(PROTOCOL_VERSION.as_bytes());
        out.push(b'\n');
        out.extend_from_slice(node.to_hex().as_bytes());
        out.push(b'\n');
        out.extend_from_slice(seq.to_string().as_bytes());
        out.push(b'\n');
        out.extend_from_slice(format!("{}:", payload_json.len()).as_bytes());
        out.extend_from_slice(payload_json);
        out
    }

    /// The canonical signing bytes for this envelope as it stands.
    pub fn signing_bytes(&self) -> Result<Vec<u8>> {
        let payload_json = serde_json::to_vec(&self.payload)?;
        Ok(Self::compose_signing_bytes(
            &self.node,
            self.seq,
            &payload_json,
        ))
    }

    /// Seal `payload` under `identity` at sequence `seq`, producing a signed
    /// envelope whose `node` is the signer's id.
    pub fn seal(identity: &Identity, payload: FederationPayload, seq: u64) -> Result<Self> {
        let node = identity.id();
        let payload_json = serde_json::to_vec(&payload)?;
        let bytes = Self::compose_signing_bytes(&node, seq, &payload_json);
        let signature = identity.sign(&bytes);
        Ok(FederationEnvelope {
            node,
            seq,
            payload,
            signature,
        })
    }

    /// Verify the node signature and return the inner payload.
    ///
    /// Returns [`Error::BadSignature`] if the envelope was forged (signed by a
    /// different key than `node`) or tampered with (payload/seq/node altered
    /// after signing).
    pub fn open(&self) -> Result<&FederationPayload> {
        let bytes = self.signing_bytes()?;
        self.node.verify(&bytes, &self.signature)?;
        Ok(&self.payload)
    }

    /// Serialize to wire bytes (JSON).
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        Ok(serde_json::to_vec(self)?)
    }

    /// Parse wire bytes into an envelope (does NOT verify; call [`Self::open`]).
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        serde_json::from_slice(bytes).map_err(Error::from)
    }
}
