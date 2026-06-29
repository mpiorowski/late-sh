//! Pure sysop-report aggregation.
//!
//! [`aggregate`] folds a slice of [`Event`]s into a [`SysopReport`]: totals,
//! per-kind counts, severity tallies, and a tail of recent event summaries.
//! It is deliberately side-effect-free and fully unit-tested so it can serve
//! as the **canonical reference implementation**. The Pub/Sub-triggered Cloud
//! Function (`functions/index.ts`) mirrors this exact logic when it folds
//! incoming messages into the `sysop_reports/latest` Firestore document.

use std::collections::BTreeMap;

use agentbbs_core::report::{Event, Severity};
use serde::{Deserialize, Serialize};

/// How many recent event summaries [`aggregate`] retains.
pub const RECENT_LIMIT: usize = 20;

/// A compact summary of a single event, suitable for a dashboard tail.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventSummary {
    /// The event kind, snake_case (e.g. `"security"`).
    pub kind: String,
    /// The event's subject string.
    pub subject: String,
}

/// The aggregated operator view over a batch of events.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SysopReport {
    /// Total events seen.
    pub total: u64,
    /// Count per event kind, keyed by snake_case kind (sorted).
    pub by_kind: BTreeMap<String, u64>,
    /// Number of `Warn`-severity events.
    pub warnings: u64,
    /// Number of `Critical`-severity events.
    pub criticals: u64,
    /// The most recent `RECENT_LIMIT` event summaries, oldest→newest.
    pub recent: Vec<EventSummary>,
}

/// snake_case rendering of an event kind (matches core's serde representation).
fn kind_str(event: &Event) -> String {
    serde_json::to_value(event.kind)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_else(|| "unknown".to_string())
}

/// Aggregate a slice of events into a [`SysopReport`].
///
/// Pure: deterministic given its input, no I/O. `recent` keeps the last
/// [`RECENT_LIMIT`] events in input order (assumed chronological).
pub fn aggregate(events: &[Event]) -> SysopReport {
    let mut report = SysopReport::default();
    for event in events {
        report.total += 1;
        *report.by_kind.entry(kind_str(event)).or_insert(0) += 1;
        match event.severity() {
            Severity::Warn => report.warnings += 1,
            Severity::Critical => report.criticals += 1,
            Severity::Info => {}
        }
    }

    let start = events.len().saturating_sub(RECENT_LIMIT);
    report.recent = events[start..]
        .iter()
        .map(|e| EventSummary {
            kind: kind_str(e),
            subject: e.subject.clone(),
        })
        .collect();

    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentbbs_core::report::EventKind;

    fn synthetic() -> Vec<Event> {
        vec![
            Event::now(EventKind::SessionOpen, "s1"),
            Event::now(EventKind::Post, "p1"),
            Event::now(EventKind::Post, "p2"),
            Event::now(EventKind::Security, "rate"),  // Warn
            Event::now(EventKind::Moderation, "ban"), // Warn
            Event::now(EventKind::McpCall, "tool"),
            Event::now(EventKind::Post, "p3"),
        ]
    }

    #[test]
    fn totals_and_by_kind() {
        let report = aggregate(&synthetic());
        assert_eq!(report.total, 7);
        assert_eq!(report.by_kind.get("post"), Some(&3));
        assert_eq!(report.by_kind.get("session_open"), Some(&1));
        assert_eq!(report.by_kind.get("security"), Some(&1));
        assert_eq!(report.by_kind.get("moderation"), Some(&1));
        assert_eq!(report.by_kind.get("mcp_call"), Some(&1));
        // by_kind is a BTreeMap → keys are sorted.
        let keys: Vec<&String> = report.by_kind.keys().collect();
        let mut sorted = keys.clone();
        sorted.sort();
        assert_eq!(keys, sorted);
    }

    #[test]
    fn warnings_and_criticals() {
        let report = aggregate(&synthetic());
        // Security + Moderation both map to Warn in core.
        assert_eq!(report.warnings, 2);
        assert_eq!(report.criticals, 0);
    }

    #[test]
    fn recent_is_tail_limited() {
        let mut events = Vec::new();
        for i in 0..(RECENT_LIMIT + 5) {
            events.push(Event::now(EventKind::Post, format!("m{i}")));
        }
        let report = aggregate(&events);
        assert_eq!(report.total as usize, RECENT_LIMIT + 5);
        assert_eq!(report.recent.len(), RECENT_LIMIT);
        // Oldest retained is m5, newest is the last.
        assert_eq!(report.recent.first().unwrap().subject, "m5");
        assert_eq!(
            report.recent.last().unwrap().subject,
            format!("m{}", RECENT_LIMIT + 4)
        );
    }

    #[test]
    fn empty_input() {
        let report = aggregate(&[]);
        assert_eq!(report, SysopReport::default());
        assert_eq!(report.total, 0);
        assert!(report.recent.is_empty());
        assert!(report.by_kind.is_empty());
    }
}
