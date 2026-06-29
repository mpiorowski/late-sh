//! A [`Reporter`] that writes each event as a Firestore document.
//!
//! ## The sync-report / async-HTTP bridge
//!
//! Core's [`Reporter::report`] is a **synchronous, non-blocking, infallible-ish**
//! call: it is invoked from hot paths (a session opening, a post landing) and
//! must never block the caller on network I/O nor propagate transport failures
//! up the stack. Firestore, on the other hand, is reached over async HTTP.
//!
//! We bridge the two with an **unbounded tokio mpsc channel**:
//!
//! ```text
//!   caller thread                         tokio runtime
//!   ┌────────────┐   report(event)   ┌──────────────────────────┐
//!   │ report() ──┼──► tx.send(event) │ drain loop: rx.recv()     │
//!   │  (sync)    │   (lock-free,     │   └─► POST .../documents   │
//!   └────────────┘    never blocks)  │        (async reqwest)     │
//!                                     └──────────────────────────┘
//! ```
//!
//! * `report()` only does a non-blocking `UnboundedSender::send` and returns
//!   `Ok(())`. A closed channel is logged, never fatal.
//! * A background task spawned on a provided runtime [`Handle`] drains the
//!   receiver and POSTs each event as a typed-value document to
//!   `{base}/v1/projects/{project}/databases/(default)/documents/agentbbs_events`.
//! * HTTP errors are logged and dropped — sysop reporting is best-effort.

use agentbbs_core::error::{Error, Result};
use agentbbs_core::report::{Event, Reporter};
use tokio::runtime::Handle;
use tokio::sync::mpsc;

use crate::encode::to_firestore_fields;
use crate::env::firestore_base;

/// The Firestore collection events are written to.
pub const EVENTS_COLLECTION: &str = "agentbbs_events";

/// A [`Reporter`] that asynchronously persists events to Firestore.
///
/// Construct with [`FirestoreReporter::start`], which both wires up the
/// background drain task on the supplied runtime [`Handle`] and returns a
/// ready-to-use reporter.
pub struct FirestoreReporter {
    tx: mpsc::UnboundedSender<Event>,
}

impl FirestoreReporter {
    /// Create a reporter and spawn its background drain task.
    ///
    /// * `project` — GCP project id.
    /// * `base_url` — explicit REST base, or `None` to derive from
    ///   `FIRESTORE_EMULATOR_HOST` (falling back to the production endpoint).
    /// * `handle` — the tokio runtime the drain task runs on.
    pub fn start(project: impl Into<String>, base_url: Option<&str>, handle: &Handle) -> Self {
        let project = project.into();
        let base = firestore_base(base_url);
        let (tx, mut rx) = mpsc::unbounded_channel::<Event>();

        let url = format!(
            "{base}/v1/projects/{project}/databases/(default)/documents/{EVENTS_COLLECTION}"
        );
        let client = reqwest::Client::new();

        handle.spawn(async move {
            while let Some(event) = rx.recv().await {
                let body = to_firestore_fields(&event);
                match client.post(&url).json(&body).send().await {
                    Ok(resp) if resp.status().is_success() => {}
                    Ok(resp) => {
                        tracing::warn!(
                            status = %resp.status(),
                            "firestore reporter: non-success writing event"
                        );
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "firestore reporter: POST failed");
                    }
                }
            }
            tracing::debug!("firestore reporter drain task exiting (channel closed)");
        });

        FirestoreReporter { tx }
    }

    /// The REST base URL this reporter resolved to (useful for diagnostics).
    pub fn resolved_base(base_url: Option<&str>) -> String {
        firestore_base(base_url)
    }
}

impl Reporter for FirestoreReporter {
    fn report(&self, event: Event) -> Result<()> {
        // Non-blocking enqueue. The only failure is a closed channel (the
        // drain task panicked or was dropped); surface it as a soft error so
        // the caller can log but it is never fatal by core's contract.
        self.tx
            .send(event)
            .map_err(|_| Error::Other("firestore reporter channel closed".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentbbs_core::report::EventKind;

    #[tokio::test]
    async fn report_enqueues_without_blocking() {
        // No emulator: with a bogus base the POSTs will simply fail and be
        // logged inside the drain task. report() itself must still succeed.
        let reporter =
            FirestoreReporter::start("demo-project", Some("http://127.0.0.1:1"), &Handle::current());
        for i in 0..50 {
            reporter
                .report(Event::now(EventKind::Post, format!("m{i}")))
                .expect("report should enqueue");
        }
    }

    #[test]
    fn resolved_base_uses_override() {
        assert_eq!(
            FirestoreReporter::resolved_base(Some("http://localhost:8080")),
            "http://localhost:8080"
        );
    }

    // Requires a live Firestore emulator on FIRESTORE_EMULATOR_HOST.
    #[tokio::test]
    #[ignore]
    async fn writes_to_emulator() {
        let reporter = FirestoreReporter::start("demo-project", None, &Handle::current());
        reporter
            .report(Event::now(EventKind::SessionOpen, "emulator-smoke"))
            .unwrap();
        // Give the drain task a moment to flush.
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
}
