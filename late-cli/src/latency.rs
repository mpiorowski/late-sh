//! Round trips only the client can time, reported over the pair WebSocket
//! (`ws.rs`) as `latency` events for the server's
//! `late_ssh_cli_latency_seconds`.
//!
//! The server times the whole trip to the user's terminal itself: it follows
//! a frame with a DSR status query (`CSI 5 n`) and waits for the terminal's
//! `CSI 0 n` (`late-ssh/src/terminal_probe.rs`). The CLI sits in the middle
//! of that trip, so it can split it:
//!
//! - `ssh_rtt`: an SSH keepalive and its reply, every [`SSH_PING_INTERVAL`].
//!   The network and both SSH stacks, no terminal.
//! - `terminal`: the server's query leaving this process for the terminal, to
//!   the terminal's answer coming back on stdin. The user's terminal alone,
//!   including any frame it was still parsing.
//!
//! Both bytes pass through untouched; the CLI only watches them. Native mode
//! only: `openssh` gives the terminal to an ssh child outright, and `old`
//! (which relays it through a local pty) is not wired.

use std::{
    sync::Mutex,
    time::{Duration, Instant},
};

use serde_json::json;
use tokio::sync::mpsc::{self, error::TrySendError};

/// The server's DSR "report status" query.
pub(super) const PROBE_QUERY: &[u8] = b"\x1b[5n";
/// The terminal's DSR "terminal OK" answer.
pub(super) const PROBE_REPLY: &[u8] = b"\x1b[0n";
pub(super) const SSH_PING_INTERVAL: Duration = Duration::from_secs(10);
/// Samples waiting while the pair socket reconnects; past this many they are
/// stale and dropped.
const SAMPLE_QUEUE_CAP: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LatencyKind {
    SshRtt,
    Terminal,
}

impl LatencyKind {
    fn wire_name(self) -> &'static str {
        match self {
            Self::SshRtt => "ssh_rtt",
            Self::Terminal => "terminal",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct LatencySample {
    pub(super) kind: LatencyKind,
    pub(super) elapsed: Duration,
}

impl LatencySample {
    pub(super) fn payload(&self) -> serde_json::Value {
        let micros =
            u64::try_from(self.elapsed.as_micros()).expect("a round trip fits u64 microseconds");
        json!({
            "event": "latency",
            "kind": self.kind.wire_name(),
            "micros": micros,
        })
    }
}

/// Where the samplers drop their samples. Sampling never waits on the pair
/// socket.
#[derive(Clone)]
pub(super) struct LatencySink(mpsc::Sender<LatencySample>);

impl LatencySink {
    pub(super) fn record(&self, sample: LatencySample) {
        match self.0.try_send(sample) {
            Ok(()) => {}
            // The pair socket is down or behind; a round trip is only worth
            // reporting fresh.
            Err(TrySendError::Full(_)) => {}
            // The pair loop ended with the session.
            Err(TrySendError::Closed(_)) => {}
        }
    }
}

pub(super) fn latency_queue() -> (LatencySink, mpsc::Receiver<LatencySample>) {
    let (tx, rx) = mpsc::channel(SAMPLE_QUEUE_CAP);
    (LatencySink(tx), rx)
}

/// Times the user's terminal answering the server's query: the SSH output
/// loop marks the query on its way to the terminal, the stdin thread closes
/// the mark when the answer comes back.
#[derive(Default)]
pub(super) struct TerminalProbeWatch {
    query_seen_at: Mutex<Option<Instant>>,
}

impl TerminalProbeWatch {
    /// One chunk from the server, read before it is written to the terminal.
    pub(super) fn saw_output(&self, chunk: &[u8], now: Instant) {
        if contains(chunk, PROBE_QUERY) {
            *self.lock() = Some(now);
        }
    }

    /// One read from stdin. `Some` when it carries the answer to a query
    /// this watch saw go out.
    pub(super) fn saw_input(&self, chunk: &[u8], now: Instant) -> Option<LatencySample> {
        if !contains(chunk, PROBE_REPLY) {
            return None;
        }
        let seen_at = self.lock().take()?;
        Some(LatencySample {
            kind: LatencyKind::Terminal,
            elapsed: now - seen_at,
        })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Option<Instant>> {
        self.query_seen_at
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

#[cfg(test)]
#[path = "latency_test.rs"]
mod latency_test;
