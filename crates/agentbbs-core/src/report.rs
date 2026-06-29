//! Provider-agnostic sysops reporting.
//!
//! AgentBBS emits structured [`Event`]s for everything an operator cares
//! about: sessions opening, posts landing, federation links forming,
//! moderation actions, plugin invocations. A [`Reporter`] is any sink that
//! accepts those events. The default [`MemoryReporter`] keeps a bounded ring
//! in memory (no cloud required); the `agentbbs-gcp` crate provides a
//! Firestore/Pub-Sub reporter behind the same trait.
//!
//! Anonymity note: events reference agents by their public [`AgentId`] only.
//! No IP addresses, SSH key comments, or other PII are carried here; egress
//! adapters additionally pass events through a PII scrubber.

use std::sync::Mutex;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::identity::AgentId;

/// The kind of thing that happened.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    /// A session connected (anonymous front door or otherwise).
    SessionOpen,
    /// A session disconnected.
    SessionClose,
    /// A message was posted to a board.
    Post,
    /// A board was created.
    BoardCreate,
    /// A moderation action was taken.
    Moderation,
    /// A federation peer was linked.
    FederationLink,
    /// A federated message was received from a peer.
    FederationReceive,
    /// A WASM plugin was invoked.
    PluginInvoke,
    /// A marketplace transaction occurred.
    Marketplace,
    /// An MCP tool was called.
    McpCall,
    /// A security-relevant event (rate limit, bad signature, denied cap).
    Security,
}

impl EventKind {
    /// Severity tier used for dashboards and alerting.
    pub fn severity(self) -> Severity {
        match self {
            EventKind::Security | EventKind::Moderation => Severity::Warn,
            _ => Severity::Info,
        }
    }
}

/// Coarse severity for routing/alerting.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// Informational.
    Info,
    /// Warrants attention.
    Warn,
    /// Operator must act.
    Critical,
}

/// A structured operational event.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Event {
    /// When it happened (server clock).
    pub at: DateTime<Utc>,
    /// What kind of event.
    pub kind: EventKind,
    /// The agent it pertains to, if any.
    pub agent: Option<AgentId>,
    /// Short machine-friendly subject (board slug, peer id, plugin name…).
    pub subject: String,
    /// Free-form structured detail (kept PII-free).
    pub detail: serde_json::Value,
}

impl Event {
    /// Build a new event stamped at the current time.
    pub fn now(kind: EventKind, subject: impl Into<String>) -> Self {
        Event {
            at: Utc::now(),
            kind,
            agent: None,
            subject: subject.into(),
            detail: serde_json::Value::Null,
        }
    }

    /// Attach an agent.
    pub fn by(mut self, agent: AgentId) -> Self {
        self.agent = Some(agent);
        self
    }

    /// Attach structured detail.
    pub fn with(mut self, detail: serde_json::Value) -> Self {
        self.detail = detail;
        self
    }

    /// Severity of this event.
    pub fn severity(&self) -> Severity {
        self.kind.severity()
    }
}

/// A sink for operational events. Implementations must be cheap to call and
/// non-blocking from the caller's perspective (buffer/spawn internally).
pub trait Reporter: Send + Sync {
    /// Record an event. Errors are swallowed by design at call sites; a
    /// reporter that returns `Err` is logged, never fatal.
    fn report(&self, event: Event) -> crate::error::Result<()>;
}

/// A bounded in-memory reporter: keeps the most recent `cap` events. Useful
/// as the default sink and for the TUI's live sysop panel.
pub struct MemoryReporter {
    cap: usize,
    events: Mutex<std::collections::VecDeque<Event>>,
}

impl MemoryReporter {
    /// Create a reporter that retains up to `cap` recent events.
    pub fn new(cap: usize) -> Self {
        MemoryReporter {
            cap,
            events: Mutex::new(std::collections::VecDeque::with_capacity(cap.min(1024))),
        }
    }

    /// Snapshot of retained events, oldest first.
    pub fn snapshot(&self) -> Vec<Event> {
        self.events.lock().unwrap().iter().cloned().collect()
    }

    /// Count of currently retained events.
    pub fn len(&self) -> usize {
        self.events.lock().unwrap().len()
    }

    /// Whether no events are retained.
    pub fn is_empty(&self) -> bool {
        self.events.lock().unwrap().is_empty()
    }
}

impl Default for MemoryReporter {
    fn default() -> Self {
        MemoryReporter::new(1024)
    }
}

impl Reporter for MemoryReporter {
    fn report(&self, event: Event) -> crate::error::Result<()> {
        let mut q = self.events.lock().unwrap();
        if q.len() == self.cap {
            q.pop_front();
        }
        q.push_back(event);
        Ok(())
    }
}

/// A reporter that does nothing — for tests and minimal embeds.
pub struct NullReporter;

impl Reporter for NullReporter {
    fn report(&self, _event: Event) -> crate::error::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_reporter_is_bounded() {
        let r = MemoryReporter::new(3);
        for i in 0..10 {
            r.report(Event::now(EventKind::Post, format!("m{i}")))
                .unwrap();
        }
        assert_eq!(r.len(), 3);
        let snap = r.snapshot();
        assert_eq!(snap[0].subject, "m7");
        assert_eq!(snap[2].subject, "m9");
    }

    #[test]
    fn severity_mapping() {
        assert_eq!(EventKind::Security.severity(), Severity::Warn);
        assert_eq!(EventKind::Post.severity(), Severity::Info);
    }
}
