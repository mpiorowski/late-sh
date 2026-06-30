//! The [`Federator`] — the zero-trust replication engine.
//!
//! It owns this node's [`Identity`], a [`Store`], a [`PeerBook`], a
//! [`Reporter`], and a [`Transport`]. Egress ([`announce_board`],
//! [`replicate_message`]) seals payloads and pushes them to *trusted* peers
//! after scrubbing PII. Ingress ([`ingest`]) opens an envelope, verifies the
//! node signature, additionally verifies replicated message authenticity, then
//! stores idempotently and audits a `FederationReceive` event. Forged or
//! tampered envelopes are rejected before they touch the store.
//!
//! [`announce_board`]: Federator::announce_board
//! [`replicate_message`]: Federator::replicate_message
//! [`ingest`]: Federator::ingest

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

use agentbbs_core::{Board, Event, EventKind, Identity, Message, Reporter, Result, Store};
use serde_json::json;

use crate::envelope::{FederationEnvelope, FederationPayload};
use crate::peer::PeerBook;
use crate::pii::strip_pii;
use crate::transport::Transport;

/// Zero-trust federation node.
pub struct Federator {
    identity: Identity,
    store: Arc<dyn Store>,
    reporter: Arc<dyn Reporter>,
    transport: Arc<dyn Transport>,
    peers: RwLock<PeerBook>,
    seq: AtomicU64,
}

impl Federator {
    /// Build a federator. `peers` is the (mutable) peer registry; egress only
    /// reaches its [`PeerBook::trusted`] members.
    pub fn new(
        identity: Identity,
        store: Arc<dyn Store>,
        reporter: Arc<dyn Reporter>,
        transport: Arc<dyn Transport>,
        peers: PeerBook,
    ) -> Self {
        Federator {
            identity,
            store,
            reporter,
            transport,
            peers: RwLock::new(peers),
            seq: AtomicU64::new(0),
        }
    }

    /// This node's public id.
    pub fn node_id(&self) -> agentbbs_core::AgentId {
        self.identity.id()
    }

    /// Add or update a peer (interior-mutable, safe under `Arc<Federator>`).
    pub fn add_peer(&self, peer: crate::peer::Peer) {
        self.peers.write().unwrap().add(peer);
    }

    /// Snapshot of every known peer.
    pub fn peers(&self) -> Vec<crate::peer::Peer> {
        self.peers.read().unwrap().all()
    }

    /// Snapshot of the trusted (egress) peers.
    pub fn trusted_peers(&self) -> Vec<crate::peer::Peer> {
        self.peers.read().unwrap().trusted()
    }

    fn next_seq(&self) -> u64 {
        self.seq.fetch_add(1, Ordering::SeqCst)
    }

    fn emit(&self, event: Event) {
        let _ = self.reporter.report(event);
    }

    /// Seal `payload` and best-effort deliver it to every trusted peer.
    async fn broadcast(&self, payload: FederationPayload) -> Result<()> {
        let seq = self.next_seq();
        let envelope = FederationEnvelope::seal(&self.identity, payload, seq)?;
        let bytes = envelope.to_bytes()?;
        let targets = self.peers.read().unwrap().trusted();
        for peer in targets {
            self.transport.send(&peer, bytes.clone()).await?;
        }
        Ok(())
    }

    /// Build a signed peer-discovery gossip of this node's known peers (node +
    /// addr only). A receiver merges new ones at `TrustLevel::Unknown` (G5).
    pub fn make_peer_exchange(&self) -> Result<FederationEnvelope> {
        let peers = self.peers.read().unwrap().infos();
        FederationEnvelope::seal(
            &self.identity,
            FederationPayload::PeerExchange { peers },
            self.next_seq(),
        )
    }

    /// Announce a board to trusted peers. The board's `description` is scrubbed
    /// of PII before egress.
    pub async fn announce_board(&self, board: &Board) -> Result<()> {
        let mut clean = board.clone();
        // Scrub any PII that leaked into the free-form description.
        let mut desc = json!({ "description": clean.description });
        strip_pii(&mut desc);
        if let Some(d) = desc.get("description").and_then(|v| v.as_str()) {
            clean.description = d.to_string();
        }
        self.broadcast(FederationPayload::AnnounceBoard(clean))
            .await
    }

    /// Replicate a verified message to trusted peers.
    pub async fn replicate_message(&self, message: &Message) -> Result<()> {
        self.broadcast(FederationPayload::ReplicateMessage(message.clone()))
            .await
    }

    /// Build a signed bootstrap snapshot of a board (metadata + up to `limit`
    /// messages), sealed under this node's identity, for a peer to ingest in one
    /// shot (ADR-0026 G5). Errors if the board is unknown. The board description
    /// is PII-scrubbed for egress, like [`announce_board`](Self::announce_board).
    pub fn make_snapshot(&self, slug: &str, limit: usize) -> Result<FederationEnvelope> {
        let mut board = self.store.get_board(slug)?.ok_or_else(|| {
            agentbbs_core::Error::malformed("board", format!("unknown board: {slug}"))
        })?;
        let mut desc = json!({ "description": board.description });
        strip_pii(&mut desc);
        if let Some(d) = desc.get("description").and_then(|v| v.as_str()) {
            board.description = d.to_string();
        }
        let messages = self.store.list_messages(slug, limit)?;
        let payload = FederationPayload::BoardSnapshot { board, messages };
        FederationEnvelope::seal(&self.identity, payload, self.next_seq())
    }

    /// Open, verify, and apply an inbound envelope.
    ///
    /// 1. Parse bytes into an envelope (malformed → error).
    /// 2. [`FederationEnvelope::open`] verifies the node signature; a forged or
    ///    tampered envelope returns [`BadSignature`](agentbbs_core::Error::BadSignature).
    /// 3. For `ReplicateMessage`, the message's own signature is verified too
    ///    before [`Store::put_message`] (idempotent).
    /// 4. For `AnnounceBoard`, the board metadata is stored.
    /// 5. A `FederationReceive` audit event is emitted on success.
    pub fn ingest(&self, bytes: &[u8]) -> Result<()> {
        let envelope = FederationEnvelope::from_bytes(bytes)?;
        // Zero-trust: verify the sealing node signature before anything else.
        let payload = match envelope.open() {
            Ok(p) => p,
            Err(e) => {
                self.emit(
                    Event::now(EventKind::Security, "federation.bad_envelope")
                        .by(envelope.node)
                        .with(json!({ "seq": envelope.seq })),
                );
                return Err(e);
            }
        };

        match payload {
            FederationPayload::ReplicateMessage(message) => {
                // Independently authenticate the post; the relaying node's
                // signature does not vouch for the author's signature.
                message.verify()?;
                self.store.put_message(message)?;
                self.emit(
                    Event::now(EventKind::FederationReceive, message.body.board.clone())
                        .by(envelope.node)
                        .with(json!({
                            "kind": "message",
                            "id": message.id.0,
                            "author": message.body.author.to_hex(),
                        })),
                );
            }
            FederationPayload::AnnounceBoard(board) => {
                self.store.put_board(board)?;
                self.emit(
                    Event::now(EventKind::FederationReceive, board.slug.clone())
                        .by(envelope.node)
                        .with(json!({ "kind": "board" })),
                );
            }
            FederationPayload::BoardSnapshot { board, messages } => {
                // Fail closed: verify EVERY contained message before storing any,
                // so a snapshot with one forged post is rejected wholesale.
                for m in messages {
                    m.verify()?;
                }
                self.store.put_board(board)?;
                for m in messages {
                    self.store.put_message(m)?;
                }
                self.emit(
                    Event::now(EventKind::FederationReceive, board.slug.clone())
                        .by(envelope.node)
                        .with(json!({ "kind": "snapshot", "messages": messages.len() })),
                );
            }
            FederationPayload::PeerExchange { peers } => {
                // Discovery never grants trust: new nodes land at Unknown.
                let added = self.peers.write().unwrap().merge_discovered(peers);
                self.emit(
                    Event::now(EventKind::FederationReceive, "peers")
                        .by(envelope.node)
                        .with(json!({ "kind": "peer_exchange", "discovered": added })),
                );
            }
            FederationPayload::PeerHello { node, protocol } => {
                self.emit(
                    Event::now(EventKind::FederationLink, node.to_hex())
                        .by(envelope.node)
                        .with(json!({ "protocol": protocol })),
                );
            }
            FederationPayload::Ack { id } => {
                self.emit(
                    Event::now(EventKind::FederationReceive, "ack")
                        .by(envelope.node)
                        .with(json!({ "kind": "ack", "id": id })),
                );
            }
        }
        Ok(())
    }
}
