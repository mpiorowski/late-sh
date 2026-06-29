//! # agentbbs-federation
//!
//! Zero-trust federation for **AgentBBS**, in the spirit of ruflo /
//! ruv-swarm: anonymous Ed25519 node identity, signed and idempotent
//! replication, PII-stripped egress, and a fully auditable receive path.
//!
//! Nothing crosses a node boundary unsigned. The [`FederationEnvelope`] wraps a
//! [`FederationPayload`] and is sealed by the sending node over deterministic
//! canonical bytes; a receiver re-derives those bytes and verifies the node
//! signature before trusting anything, then independently re-verifies any
//! replicated [`Message`](agentbbs_core::Message). Forged (wrong key) and
//! tampered (altered payload) envelopes are rejected.
//!
//! - [`envelope`] — the signed [`FederationEnvelope`] + [`FederationPayload`].
//! - [`peer`] — [`Peer`], [`PeerBook`], [`TrustLevel`].
//! - [`transport`] — the [`Transport`] trait + in-process [`LoopbackTransport`].
//! - [`pii`] — [`strip_pii`] egress scrubber.
//! - [`federator`] — the [`Federator`] replication engine.
//! - [`adapter`] — [`RufloAdapter`]/[`AgentDbAdapter`] over a [`CommandRunner`].
#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod adapter;
pub mod envelope;
pub mod federator;
pub mod peer;
pub mod pii;
pub mod tcp;
pub mod transport;

pub use adapter::{
    AgentDbAdapter, CommandRunner, FakeCommandRunner, MemoryRecord, RufloAdapter,
    TokioCommandRunner,
};
pub use envelope::{FederationEnvelope, FederationPayload};
pub use federator::Federator;
pub use peer::{Peer, PeerBook, TrustLevel};
pub use pii::{scrubbed, strip_pii, REDACTED};
pub use tcp::{FederationServer, TcpTransport, MAX_FRAME};
pub use transport::{LoopbackTransport, Transport};

#[cfg(test)]
mod tests {
    use super::*;
    use agentbbs_core::{Board, Identity, Message, MessageBody, NullReporter, Reporter, Store};
    use chrono::Utc;
    use serde_json::json;
    use std::sync::Arc;

    fn signed_message(id: &Identity, board: &str, text: &str) -> Message {
        let body = MessageBody {
            board: board.into(),
            parent: None,
            subject: "subj".into(),
            body: text.into(),
            author: id.id(),
            handle: "handle".into(),
            created_at: Utc::now(),
        };
        Message::sign(id, body).unwrap()
    }

    // 1. Envelope seal/open roundtrip.
    #[test]
    fn envelope_seal_open_roundtrip() {
        let node = Identity::generate();
        let msg = signed_message(&node, "general", "hi");
        let env =
            FederationEnvelope::seal(&node, FederationPayload::ReplicateMessage(msg.clone()), 7)
                .unwrap();
        assert_eq!(env.node, node.id());
        assert_eq!(env.seq, 7);
        match env.open().unwrap() {
            FederationPayload::ReplicateMessage(m) => assert_eq!(m, &msg),
            _ => panic!("wrong payload"),
        }
        // Survives a wire roundtrip.
        let bytes = env.to_bytes().unwrap();
        let parsed = FederationEnvelope::from_bytes(&bytes).unwrap();
        assert!(parsed.open().is_ok());
    }

    // 2. Forged signature rejection: swap the node id to a different identity.
    #[test]
    fn forged_node_id_rejected() {
        let signer = Identity::generate();
        let attacker = Identity::generate();
        let mut env = FederationEnvelope::seal(
            &signer,
            FederationPayload::PeerHello {
                node: signer.id(),
                protocol: "agentbbs/0.1".into(),
            },
            0,
        )
        .unwrap();
        // Claim to be a different node while keeping signer's signature.
        env.node = attacker.id();
        assert!(matches!(
            env.open(),
            Err(agentbbs_core::Error::BadSignature)
        ));
    }

    // 3. Tampered payload rejection.
    #[test]
    fn tampered_payload_rejected() {
        let node = Identity::generate();
        let mut env =
            FederationEnvelope::seal(&node, FederationPayload::Ack { id: "abc".into() }, 3)
                .unwrap();
        env.payload = FederationPayload::Ack {
            id: "tampered".into(),
        };
        assert!(matches!(
            env.open(),
            Err(agentbbs_core::Error::BadSignature)
        ));
        // Also: tampering with the seq breaks the signature.
        let mut env2 =
            FederationEnvelope::seal(&node, FederationPayload::Ack { id: "abc".into() }, 3)
                .unwrap();
        env2.seq = 99;
        assert!(matches!(
            env2.open(),
            Err(agentbbs_core::Error::BadSignature)
        ));
    }

    // 4. Ingest replay idempotency.
    #[test]
    fn ingest_replay_is_idempotent() {
        let node = Identity::generate();
        let author = Identity::generate();
        let store: Arc<dyn Store> = Arc::new(agentbbs_core::MemoryStore::new());
        let reporter: Arc<dyn Reporter> = Arc::new(NullReporter);
        let fed = Federator::new(
            Identity::generate(),
            store.clone(),
            reporter,
            Arc::new(LoopbackTransport::new()),
            PeerBook::new(),
        );

        let msg = signed_message(&author, "general", "replay me");
        let env =
            FederationEnvelope::seal(&node, FederationPayload::ReplicateMessage(msg), 0).unwrap();
        let bytes = env.to_bytes().unwrap();

        fed.ingest(&bytes).unwrap();
        fed.ingest(&bytes).unwrap();
        fed.ingest(&bytes).unwrap();
        assert_eq!(store.message_count().unwrap(), 1);
    }

    // 4b. Ingest rejects a replicated message whose author signature is forged.
    #[test]
    fn ingest_rejects_unauthentic_message() {
        let node = Identity::generate();
        let author = Identity::generate();
        let store: Arc<dyn Store> = Arc::new(agentbbs_core::MemoryStore::new());
        let fed = Federator::new(
            Identity::generate(),
            store.clone(),
            Arc::new(NullReporter),
            Arc::new(LoopbackTransport::new()),
            PeerBook::new(),
        );

        let mut msg = signed_message(&author, "general", "real");
        // Tamper after signing: id no longer matches content.
        msg.body.body = "forged".into();
        let env =
            FederationEnvelope::seal(&node, FederationPayload::ReplicateMessage(msg), 0).unwrap();
        let bytes = env.to_bytes().unwrap();
        assert!(fed.ingest(&bytes).is_err());
        assert_eq!(store.message_count().unwrap(), 0);
    }

    // 5. strip_pii redaction.
    #[test]
    fn strip_pii_redacts_sensitive_keys() {
        let mut v = json!({
            "title": "General",
            "email": "ruv@ruv.net",
            "contact": {
                "phone": "555-1234",
                "ip_addr": "10.0.0.1",
                "note": "safe text"
            },
            "peers": [
                { "host": "secret.example", "label": "ok" }
            ],
            "api_token": "deadbeef",
            "secret_key": "xyz"
        });
        strip_pii(&mut v);
        assert_eq!(v["title"], "General");
        assert_eq!(v["email"], REDACTED);
        assert_eq!(v["contact"]["phone"], REDACTED);
        assert_eq!(v["contact"]["ip_addr"], REDACTED);
        assert_eq!(v["contact"]["note"], "safe text");
        assert_eq!(v["peers"][0]["host"], REDACTED);
        assert_eq!(v["peers"][0]["label"], "ok");
        assert_eq!(v["api_token"], REDACTED);
        assert_eq!(v["secret_key"], REDACTED);
    }

    // 5b. announce_board replicates board metadata end-to-end (egress path
    // exercises the PII scrubber on the description).
    #[tokio::test]
    async fn announce_board_replicates() {
        let transport = LoopbackTransport::new();
        let node_a = Identity::generate();
        let node_b = Identity::generate();
        let store_b: Arc<dyn Store> = Arc::new(agentbbs_core::MemoryStore::new());
        let mut rx = transport.inbox(node_b.id());

        let mut book = PeerBook::new();
        book.add(Peer::new(node_b.id(), "loopback://b", TrustLevel::Trusted));
        let fed_a = Federator::new(
            node_a,
            Arc::new(agentbbs_core::MemoryStore::new()),
            Arc::new(NullReporter),
            Arc::new(transport.clone()),
            book,
        );
        let founder = Identity::generate();
        let board = Board::new("general", "General", founder.id());
        fed_a.announce_board(&board).await.unwrap();

        let bytes = rx.recv().await.unwrap();
        let fed_b = Federator::new(
            node_b,
            store_b.clone(),
            Arc::new(NullReporter),
            Arc::new(transport),
            PeerBook::new(),
        );
        fed_b.ingest(&bytes).unwrap();
        assert!(store_b.get_board("general").unwrap().is_some());
    }

    // 6. Loopback two-node replication end-to-end.
    #[tokio::test]
    async fn loopback_two_node_replication() {
        let transport = LoopbackTransport::new();
        let node_a = Identity::generate();
        let node_b = Identity::generate();
        let author = Identity::generate();

        // Node B drains its inbox.
        let mut rx_b = transport.inbox(node_b.id());

        // Node A trusts B.
        let mut book_a = PeerBook::new();
        book_a.add(Peer::new(node_b.id(), "loopback://b", TrustLevel::Trusted));
        assert_eq!(book_a.trusted().len(), 1);

        let store_a: Arc<dyn Store> = Arc::new(agentbbs_core::MemoryStore::new());
        let store_b: Arc<dyn Store> = Arc::new(agentbbs_core::MemoryStore::new());

        let fed_a = Federator::new(
            node_a,
            store_a.clone(),
            Arc::new(NullReporter),
            Arc::new(transport.clone()),
            book_a,
        );
        let fed_b = Federator::new(
            node_b,
            store_b.clone(),
            Arc::new(NullReporter),
            Arc::new(transport.clone()),
            PeerBook::new(),
        );

        // Post to A's store, then replicate.
        let msg = signed_message(&author, "general", "federated hello");
        store_a.put_message(&msg).unwrap();
        fed_a.replicate_message(&msg).await.unwrap();

        // B receives, ingests, and the message appears via list_messages.
        let bytes = rx_b.recv().await.unwrap();
        fed_b.ingest(&bytes).unwrap();

        let listed = store_b.list_messages("general", 10).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].body.body, "federated hello");
        assert_eq!(listed[0].id, msg.id);
    }

    // 7a. ruflo adapter via FakeCommandRunner.
    #[tokio::test]
    async fn ruflo_adapter_shells_npx() {
        let fake = FakeCommandRunner::with_output("{\"linked\":true}");
        let ruflo = RufloAdapter::new(fake.clone());

        let out = ruflo.federation_join("peer.example:9000").await.unwrap();
        assert_eq!(out, "{\"linked\":true}");
        ruflo.federation_init().await.unwrap();
        ruflo.federation_status().await.unwrap();

        let calls = fake.calls();
        assert_eq!(calls.len(), 3);
        assert_eq!(
            calls[0],
            vec!["npx", "ruflo", "federation", "join", "peer.example:9000"]
        );
        assert_eq!(calls[1], vec!["npx", "ruflo", "federation", "init"]);
        assert_eq!(calls[2], vec!["npx", "ruflo", "federation", "status"]);
    }

    // 7b. agentdb adapter via FakeCommandRunner returns typed records.
    #[tokio::test]
    async fn agentdb_adapter_typed_results() {
        let canned = serde_json::to_string(&vec![
            MemoryRecord {
                key: "k1".into(),
                value: "v1".into(),
            },
            MemoryRecord {
                key: "k2".into(),
                value: "v2".into(),
            },
        ])
        .unwrap();
        let fake = FakeCommandRunner::with_output(canned);
        let db = AgentDbAdapter::new(fake.clone());

        db.store_memory("topic", "value").await.unwrap();
        let rows = db.query_memory("topic").await.unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(
            rows[0],
            MemoryRecord {
                key: "k1".into(),
                value: "v1".into()
            }
        );

        let calls = fake.calls();
        assert_eq!(calls[0], vec!["npx", "agentdb", "store", "topic", "value"]);
        assert_eq!(calls[1], vec!["npx", "agentdb", "query", "topic"]);
    }

    // PeerBook trust filtering.
    #[test]
    fn peerbook_trusted_filtering() {
        let mut book = PeerBook::new();
        let a = Identity::generate().id();
        let b = Identity::generate().id();
        book.add(Peer::new(a, "x", TrustLevel::Unknown));
        book.add(Peer::new(b, "y", TrustLevel::Trusted));
        assert_eq!(book.all().len(), 2);
        assert_eq!(book.trusted().len(), 1);
        assert_eq!(book.trusted()[0].node, b);
        assert!(book.get(&a).is_some());
        assert!(book.remove(&a).is_some());
        assert_eq!(book.all().len(), 1);
    }
}
