//! # agentbbs-gcp
//!
//! GCP-backed sysops/admin reporting for AgentBBS. This crate provides two
//! [`agentbbs_core::report::Reporter`] implementations plus pure encoding and
//! aggregation helpers, all designed to run against the **local Firestore and
//! Pub/Sub emulators** so development works fully offline.
//!
//! ## What's here
//!
//! * [`encode`] — pure `Event → REST JSON` converters
//!   ([`to_firestore_fields`](encode::to_firestore_fields),
//!   [`pubsub_publish_body`](encode::pubsub_publish_body)). No network.
//! * [`env`] — emulator-aware base-URL selection from
//!   `FIRESTORE_EMULATOR_HOST` / `PUBSUB_EMULATOR_HOST`.
//! * [`firestore`] — [`FirestoreReporter`](firestore::FirestoreReporter), which
//!   writes each event as a document. Non-blocking `report()` via an mpsc
//!   bridge to a background async-HTTP drain task.
//! * [`pubsub`] — [`PubSubPublisher`](pubsub::PubSubPublisher) and
//!   [`PubSubReporter`](pubsub::PubSubReporter).
//! * [`aggregate`] — pure [`aggregate`](aggregate::aggregate) →
//!   [`SysopReport`](aggregate::SysopReport); the canonical logic mirrored by
//!   the Pub/Sub-triggered Cloud Function under `functions/`.
//!
//! ## Architecture (dev)
//!
//! ```text
//!   AgentBBS ──report()──► PubSubReporter ──REST──► [Pub/Sub emulator topic]
//!                                                         │
//!                                                         ▼
//!                                                  Cloud Function
//!                                                         │ aggregate()
//!                                                         ▼
//!                                          [Firestore] sysop_reports/latest
//!
//!   AgentBBS ──report()──► FirestoreReporter ──REST──► [Firestore] agentbbs_events
//! ```
//!
//! See the crate `README.md` for emulator setup and the Terraform deployment.

pub mod aggregate;
pub mod encode;
pub mod env;
pub mod firestore;
pub mod pubsub;

pub use aggregate::{aggregate, EventSummary, SysopReport, RECENT_LIMIT};
pub use encode::{event_json, pubsub_publish_body, to_firestore_fields};
pub use env::{firestore_base, pubsub_base, resolve_base};
pub use firestore::{FirestoreReporter, EVENTS_COLLECTION};
pub use pubsub::{PubSubPublisher, PubSubReporter, DEFAULT_TOPIC};
