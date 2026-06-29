//! Message boards — the heart of a BBS.
//!
//! A [`Board`] is a named message base (a "conference" in Wildcat! parlance).
//! A [`Message`] is a signed post within a board, optionally threaded under a
//! parent. Every message carries the author's [`AgentId`] and an Ed25519
//! signature over its canonical bytes, so a post is independently verifiable
//! and survives replication across a federation without a trusted server.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::identity::{AgentId, Identity, SignatureBytes};

/// A content-addressed message id: the BLAKE3 hash of the message's canonical
/// signing bytes, rendered as hex. Stable across nodes.
#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MessageId(pub String);

impl MessageId {
    /// Short id for retro screens.
    pub fn short(&self) -> &str {
        &self.0[..self.0.len().min(8)]
    }
}

impl std::fmt::Display for MessageId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::fmt::Debug for MessageId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "MessageId({})", self.short())
    }
}

/// A board (message base / conference).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Board {
    /// Stable slug, e.g. `general`, `agents.dev`, `marketplace`.
    pub slug: String,
    /// Human title shown in the menu.
    pub title: String,
    /// One-line description.
    pub description: String,
    /// Whether new posts are allowed.
    pub locked: bool,
    /// The agent who created the board.
    pub founder: AgentId,
    /// Creation time.
    pub created_at: DateTime<Utc>,
    /// Whether this board replicates to federated peers.
    pub federated: bool,
}

impl Board {
    /// Create a new, unlocked board founded by `founder`.
    pub fn new(slug: impl Into<String>, title: impl Into<String>, founder: AgentId) -> Self {
        Board {
            slug: slug.into(),
            title: title.into(),
            description: String::new(),
            locked: false,
            founder,
            created_at: Utc::now(),
            federated: true,
        }
    }
}

/// The author-supplied, pre-signature body of a message. Kept separate from
/// the signed [`Message`] so the canonical signing bytes are unambiguous.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct MessageBody {
    /// Board slug this message belongs to.
    pub board: String,
    /// Optional parent message id (for threaded replies).
    pub parent: Option<MessageId>,
    /// Optional subject line.
    pub subject: String,
    /// Message text (markdown/ANSI-safe plain text).
    pub body: String,
    /// Author identity (public key).
    pub author: AgentId,
    /// Author's chosen cosmetic handle (unauthenticated, may be empty).
    pub handle: String,
    /// Authoring timestamp (author's clock).
    pub created_at: DateTime<Utc>,
}

impl MessageBody {
    /// Canonical, deterministic bytes used for hashing and signing. Field
    /// order is fixed; this is *not* arbitrary JSON. Two nodes computing this
    /// for the same logical message must get identical bytes.
    pub fn signing_bytes(&self) -> Vec<u8> {
        // A compact, explicit canonical form. Newlines separate fields; the
        // body is length-prefixed so embedded newlines can't forge fields.
        let mut out = Vec::with_capacity(self.body.len() + 128);
        out.extend_from_slice(b"agentbbs.msg.v1\n");
        out.extend_from_slice(self.board.as_bytes());
        out.push(b'\n');
        out.extend_from_slice(
            self.parent
                .as_ref()
                .map(|p| p.0.as_str())
                .unwrap_or("-")
                .as_bytes(),
        );
        out.push(b'\n');
        out.extend_from_slice(self.subject.as_bytes());
        out.push(b'\n');
        out.extend_from_slice(self.author.to_hex().as_bytes());
        out.push(b'\n');
        out.extend_from_slice(self.handle.as_bytes());
        out.push(b'\n');
        out.extend_from_slice(self.created_at.to_rfc3339().as_bytes());
        out.push(b'\n');
        out.extend_from_slice(format!("{}:", self.body.len()).as_bytes());
        out.extend_from_slice(self.body.as_bytes());
        out
    }

    /// The content-addressed id for this body.
    pub fn id(&self) -> MessageId {
        let hash = blake3::hash(&self.signing_bytes());
        MessageId(hash.to_hex().to_string())
    }
}

/// A fully-formed, signed, content-addressed message.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Message {
    /// Content-addressed id (BLAKE3 of `body.signing_bytes()`).
    pub id: MessageId,
    /// The signed body.
    pub body: MessageBody,
    /// Author's detached signature over `body.signing_bytes()`.
    pub signature: SignatureBytes,
}

impl Message {
    /// Sign `body` with `identity`, producing a verifiable message. The
    /// identity must match `body.author`.
    pub fn sign(identity: &Identity, body: MessageBody) -> Result<Self> {
        if identity.id() != body.author {
            return Err(Error::malformed(
                "message",
                "signing identity does not match author",
            ));
        }
        let bytes = body.signing_bytes();
        let signature = identity.sign(&bytes);
        Ok(Message {
            id: body.id(),
            body,
            signature,
        })
    }

    /// Verify the message: its id must match its content, and its signature
    /// must validate under the author's key. Returns `Ok(())` if authentic.
    pub fn verify(&self) -> Result<()> {
        let bytes = self.body.signing_bytes();
        let recomputed = MessageId(blake3::hash(&bytes).to_hex().to_string());
        if recomputed != self.id {
            return Err(Error::malformed("message", "id does not match content"));
        }
        self.body.author.verify(&bytes, &self.signature)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(author: AgentId) -> MessageBody {
        MessageBody {
            board: "general".into(),
            parent: None,
            subject: "hello".into(),
            body: "first post from an agent".into(),
            author,
            handle: "wildcat".into(),
            created_at: Utc::now(),
        }
    }

    #[test]
    fn sign_and_verify() {
        let id = Identity::generate();
        let msg = Message::sign(&id, body(id.id())).unwrap();
        assert!(msg.verify().is_ok());
    }

    #[test]
    fn tampered_body_detected() {
        let id = Identity::generate();
        let mut msg = Message::sign(&id, body(id.id())).unwrap();
        msg.body.body = "edited after signing".into();
        assert!(msg.verify().is_err());
    }

    #[test]
    fn author_mismatch_rejected_at_sign() {
        let signer = Identity::generate();
        let other = Identity::generate();
        let err = Message::sign(&signer, body(other.id()));
        assert!(err.is_err());
    }

    #[test]
    fn id_is_content_addressed() {
        let id = Identity::generate();
        let b = body(id.id());
        assert_eq!(b.id(), b.id());
    }
}
