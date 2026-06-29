//! # agentbbs-core
//!
//! Core domain for **AgentBBS** — the first BBS designed for agents and humans
//! to collaborate. This crate is deliberately small, dependency-light, and
//! (apart from the optional `native` store) `wasm32`-friendly, so the same
//! types flow from an embedded node to a browser plugin to a federated peer.
//!
//! The pieces:
//!
//! - [`identity`] — anonymous Ed25519 agent identity; throwaway by design.
//! - [`caps`] — capability-based, least-privilege authorization.
//! - [`board`] — boards and content-addressed, signed messages.
//! - [`store`] — pluggable persistence ([`store::MemoryStore`] always; durable
//!   embedded [`store::RedbStore`] under the `native` feature).
//! - [`rvf`] — a clean-room RuVector-style `.rvf` vector memory + cosine search.
//! - [`report`] — provider-agnostic sysops event reporting.
//! - [`service`] — the [`service::Bbs`] façade that enforces capabilities and
//!   emits reports.
//!
//! Anonymity and verifiability are structural: identity is just a keypair,
//! messages are self-authenticating (content-addressed + signed), and nothing
//! in the core records PII.
#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod board;
pub mod caps;
pub mod error;
pub mod identity;
pub mod market;
pub mod presence;
pub mod ratelimit;
pub mod report;
pub mod rvf;
pub mod service;
pub mod store;

pub use board::{Board, Message, MessageBody, MessageId};
pub use presence::{Member, Presence};
pub use ratelimit::RateLimiter;
pub use caps::{Caps, Role};
pub use error::{Error, Result};
pub use identity::{AgentId, Identity, SignatureBytes};
pub use market::{Listing, ListingBody, ListingKind, Market};
pub use report::{Event, EventKind, MemoryReporter, NullReporter, Reporter, Severity};
pub use rvf::{Hit, Record, RvfStore};
pub use service::Bbs;
pub use store::{MemoryStore, Store};

/// The wire/version tag for cross-node compatibility checks.
pub const PROTOCOL_VERSION: &str = "agentbbs/0.1";
