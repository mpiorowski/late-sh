//! Cross-session presence — who is online on a node right now.
//!
//! A node shares one [`Presence`] registry across all its live sessions (every
//! SSH connection, the local TUI, MCP callers). A session heartbeats while it
//! is active and leaves on disconnect; members that stop heartbeating expire
//! after a TTL. Like [`crate::ratelimit`], it is clock-injectable (the caller
//! supplies a monotonic millisecond timestamp), so it stays `wasm32`-safe and
//! deterministic in tests.
//!
//! Only the public [`AgentId`] and a cosmetic handle are tracked — no IPs, no
//! PII — so presence does not weaken anonymity.

use std::collections::HashMap;
use std::sync::Mutex;

use crate::identity::AgentId;

/// A currently-online participant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Member {
    /// Public id.
    pub id: AgentId,
    /// Cosmetic handle.
    pub handle: String,
    /// Whether this participant presents as an agent.
    pub agent: bool,
    /// When they first joined (monotonic ms).
    pub joined_ms: u64,
    /// Last heartbeat (monotonic ms).
    pub last_seen_ms: u64,
}

/// A shared, thread-safe presence registry for one node.
pub struct Presence {
    ttl_ms: u64,
    members: Mutex<HashMap<AgentId, Member>>,
}

impl Presence {
    /// A registry whose members expire after `ttl_ms` without a heartbeat.
    pub fn new(ttl_ms: u64) -> Self {
        Presence {
            ttl_ms,
            members: Mutex::new(HashMap::new()),
        }
    }

    /// Register or refresh `id`'s presence at `now_ms`.
    pub fn heartbeat(&self, id: AgentId, handle: &str, agent: bool, now_ms: u64) {
        let mut members = self.members.lock().unwrap();
        members
            .entry(id)
            .and_modify(|m| {
                m.last_seen_ms = now_ms;
                if !handle.is_empty() {
                    m.handle = handle.to_string();
                }
                m.agent = agent;
            })
            .or_insert(Member {
                id,
                handle: handle.to_string(),
                agent,
                joined_ms: now_ms,
                last_seen_ms: now_ms,
            });
    }

    /// Remove `id` (explicit disconnect).
    pub fn leave(&self, id: &AgentId) {
        self.members.lock().unwrap().remove(id);
    }

    /// The members considered online at `now_ms` (heartbeat within the TTL),
    /// most-recently-seen first.
    pub fn online(&self, now_ms: u64) -> Vec<Member> {
        let members = self.members.lock().unwrap();
        let mut live: Vec<Member> = members
            .values()
            .filter(|m| now_ms.saturating_sub(m.last_seen_ms) < self.ttl_ms)
            .cloned()
            .collect();
        live.sort_by_key(|m| std::cmp::Reverse(m.last_seen_ms));
        live
    }

    /// Count of currently-online members at `now_ms`.
    pub fn count(&self, now_ms: u64) -> usize {
        self.members
            .lock()
            .unwrap()
            .values()
            .filter(|m| now_ms.saturating_sub(m.last_seen_ms) < self.ttl_ms)
            .count()
    }

    /// Drop expired members so the map stays bounded.
    pub fn gc(&self, now_ms: u64) {
        self.members
            .lock()
            .unwrap()
            .retain(|_, m| now_ms.saturating_sub(m.last_seen_ms) < self.ttl_ms);
    }
}

impl Default for Presence {
    fn default() -> Self {
        // 60s default liveness window.
        Presence::new(60_000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Identity;

    #[test]
    fn heartbeat_and_online() {
        let p = Presence::new(1000);
        let a = Identity::generate().id();
        let b = Identity::generate().id();
        p.heartbeat(a, "alice", false, 0);
        p.heartbeat(b, "bot", true, 100);
        let online = p.online(200);
        assert_eq!(online.len(), 2);
        assert_eq!(online[0].id, b); // most recent first
        assert!(online[1].handle == "alice");
    }

    #[test]
    fn members_expire_after_ttl() {
        let p = Presence::new(1000);
        let a = Identity::generate().id();
        p.heartbeat(a, "alice", false, 0);
        assert_eq!(p.count(500), 1);
        assert_eq!(p.count(1500), 0); // expired
                                      // A fresh heartbeat brings them back.
        p.heartbeat(a, "alice", false, 1500);
        assert_eq!(p.count(1600), 1);
    }

    #[test]
    fn leave_removes() {
        let p = Presence::new(1000);
        let a = Identity::generate().id();
        p.heartbeat(a, "alice", false, 0);
        p.leave(&a);
        assert_eq!(p.count(0), 0);
    }

    #[test]
    fn gc_prunes() {
        let p = Presence::new(1000);
        let a = Identity::generate().id();
        p.heartbeat(a, "alice", false, 0);
        p.gc(2000);
        assert_eq!(p.online(2000).len(), 0);
    }
}
