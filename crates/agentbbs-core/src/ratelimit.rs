//! A small, clock-injectable fixed-window rate limiter.
//!
//! Used to bound abuse at the anonymous entry points (SSH connection rate, MCP
//! tool-call rate, web posts). It is deliberately clock-free — the caller
//! supplies a monotonic millisecond timestamp — so the type stays `wasm32`-safe
//! and trivially testable (no `Instant`, no sleeping). Keys are opaque strings
//! (a source-IP, a session token, the literal `"mcp"` for a single client),
//! never logged here, so the limiter does not weaken anonymity.

use std::collections::HashMap;
use std::sync::Mutex;

struct Window {
    start_ms: u64,
    count: u32,
}

/// A per-key fixed-window limiter: at most `max` events per `window_ms`.
pub struct RateLimiter {
    max: u32,
    window_ms: u64,
    windows: Mutex<HashMap<String, Window>>,
}

impl RateLimiter {
    /// Allow `max` events per `window_ms` milliseconds per key.
    pub fn new(max: u32, window_ms: u64) -> Self {
        RateLimiter {
            max,
            window_ms,
            windows: Mutex::new(HashMap::new()),
        }
    }

    /// Record an event for `key` at monotonic time `now_ms`; return `true` if it
    /// is within the quota, `false` if the key has exceeded `max` this window.
    pub fn allow(&self, key: &str, now_ms: u64) -> bool {
        let mut windows = self.windows.lock().unwrap();
        let w = windows.entry(key.to_string()).or_insert(Window {
            start_ms: now_ms,
            count: 0,
        });
        if now_ms.saturating_sub(w.start_ms) >= self.window_ms {
            w.start_ms = now_ms;
            w.count = 0;
        }
        if w.count >= self.max {
            return false;
        }
        w.count += 1;
        true
    }

    /// Drop windows untouched for at least `window_ms` before `now_ms`, so the
    /// map cannot grow without bound under a churn of unique keys.
    pub fn gc(&self, now_ms: u64) {
        let mut windows = self.windows.lock().unwrap();
        windows.retain(|_, w| now_ms.saturating_sub(w.start_ms) < self.window_ms);
    }

    /// Number of tracked keys (for tests/metrics).
    pub fn tracked(&self) -> usize {
        self.windows.lock().unwrap().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_up_to_max_then_blocks() {
        let rl = RateLimiter::new(3, 1000);
        assert!(rl.allow("a", 0));
        assert!(rl.allow("a", 100));
        assert!(rl.allow("a", 200));
        assert!(!rl.allow("a", 300)); // 4th in window blocked
    }

    #[test]
    fn window_resets() {
        let rl = RateLimiter::new(1, 1000);
        assert!(rl.allow("a", 0));
        assert!(!rl.allow("a", 500)); // same window
        assert!(rl.allow("a", 1000)); // new window
    }

    #[test]
    fn keys_are_independent() {
        let rl = RateLimiter::new(1, 1000);
        assert!(rl.allow("a", 0));
        assert!(rl.allow("b", 0));
        assert!(!rl.allow("a", 0));
    }

    #[test]
    fn gc_prunes_stale_keys() {
        let rl = RateLimiter::new(5, 1000);
        rl.allow("a", 0);
        rl.allow("b", 0);
        assert_eq!(rl.tracked(), 2);
        rl.gc(2000); // both windows are now stale
        assert_eq!(rl.tracked(), 0);
    }
}
