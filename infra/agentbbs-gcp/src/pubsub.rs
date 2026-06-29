//! Pub/Sub publishing of operational events.
//!
//! [`PubSubPublisher`] is a thin async REST client for the Pub/Sub `publish`
//! endpoint; [`PubSubReporter`] adapts it to core's synchronous [`Reporter`]
//! trait using the same sync→async mpsc bridge as the Firestore reporter (see
//! [`crate::firestore`] for the detailed rationale).
//!
//! Wire flow: reporter → `topics/{topic}:publish` → topic → Cloud Function →
//! `sysop_reports/latest`.

use agentbbs_core::error::{Error, Result};
use agentbbs_core::report::{Event, Reporter};
use tokio::runtime::Handle;
use tokio::sync::mpsc;

use crate::encode::pubsub_publish_body;
use crate::env::pubsub_base;

/// The default topic operational events are published to.
pub const DEFAULT_TOPIC: &str = "agentbbs-events";

/// An async client that publishes events to a Pub/Sub topic over REST.
#[derive(Clone)]
pub struct PubSubPublisher {
    client: reqwest::Client,
    publish_url: String,
}

impl PubSubPublisher {
    /// Build a publisher for `project`/`topic`.
    ///
    /// `base_url` overrides the endpoint; when `None` it is derived from
    /// `PUBSUB_EMULATOR_HOST` (falling back to production).
    pub fn new(project: impl Into<String>, topic: impl Into<String>, base_url: Option<&str>) -> Self {
        let base = pubsub_base(base_url);
        let publish_url = format!(
            "{base}/v1/projects/{}/topics/{}:publish",
            project.into(),
            topic.into()
        );
        PubSubPublisher {
            client: reqwest::Client::new(),
            publish_url,
        }
    }

    /// The fully-resolved publish URL (useful for diagnostics/tests).
    pub fn publish_url(&self) -> &str {
        &self.publish_url
    }

    /// Publish a batch of events. Returns the number of events sent on success.
    pub async fn publish(&self, events: &[Event]) -> Result<usize> {
        if events.is_empty() {
            return Ok(0);
        }
        let body = pubsub_publish_body(events);
        let resp = self
            .client
            .post(&self.publish_url)
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::Other(format!("pubsub publish request failed: {e}")))?;
        if !resp.status().is_success() {
            return Err(Error::Other(format!(
                "pubsub publish returned {}",
                resp.status()
            )));
        }
        Ok(events.len())
    }
}

/// A [`Reporter`] that publishes each event to Pub/Sub off the caller's thread.
pub struct PubSubReporter {
    tx: mpsc::UnboundedSender<Event>,
}

impl PubSubReporter {
    /// Create a reporter and spawn its background publish task on `handle`.
    pub fn start(
        project: impl Into<String>,
        topic: impl Into<String>,
        base_url: Option<&str>,
        handle: &Handle,
    ) -> Self {
        let publisher = PubSubPublisher::new(project, topic, base_url);
        let (tx, mut rx) = mpsc::unbounded_channel::<Event>();

        handle.spawn(async move {
            // Publish one-at-a-time; the body builder still supports batches
            // for callers that want to publish a slice directly.
            while let Some(event) = rx.recv().await {
                if let Err(e) = publisher.publish(std::slice::from_ref(&event)).await {
                    tracing::warn!(error = %e, "pubsub reporter: publish failed");
                }
            }
            tracing::debug!("pubsub reporter publish task exiting (channel closed)");
        });

        PubSubReporter { tx }
    }
}

impl Reporter for PubSubReporter {
    fn report(&self, event: Event) -> Result<()> {
        self.tx
            .send(event)
            .map_err(|_| Error::Other("pubsub reporter channel closed".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentbbs_core::report::EventKind;

    #[test]
    fn publish_url_is_well_formed() {
        let p = PubSubPublisher::new("demo-project", "agentbbs-events", Some("http://localhost:8085"));
        assert_eq!(
            p.publish_url(),
            "http://localhost:8085/v1/projects/demo-project/topics/agentbbs-events:publish"
        );
    }

    #[tokio::test]
    async fn report_enqueues_without_blocking() {
        let reporter = PubSubReporter::start(
            "demo-project",
            DEFAULT_TOPIC,
            Some("http://127.0.0.1:1"),
            &Handle::current(),
        );
        for i in 0..25 {
            reporter
                .report(Event::now(EventKind::McpCall, format!("call{i}")))
                .expect("report should enqueue");
        }
    }

    #[tokio::test]
    async fn publish_empty_is_noop() {
        let p = PubSubPublisher::new("demo-project", DEFAULT_TOPIC, Some("http://127.0.0.1:1"));
        assert_eq!(p.publish(&[]).await.unwrap(), 0);
    }

    // Requires a live Pub/Sub emulator on PUBSUB_EMULATOR_HOST with the topic
    // already created.
    #[tokio::test]
    #[ignore]
    async fn publishes_to_emulator() {
        let p = PubSubPublisher::new("demo-project", DEFAULT_TOPIC, None);
        let n = p
            .publish(&[Event::now(EventKind::SessionOpen, "emulator-smoke")])
            .await
            .unwrap();
        assert_eq!(n, 1);
    }
}
