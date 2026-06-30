//! Budget guardrails (ADR-0040).
//!
//! Tracks spend per key (pod / account / board) against a cap, for the
//! control-plane's accounting + guardrails display. The meta-llm gateway is the
//! authoritative meter and hard enforcer; this is defense-in-depth — it shows
//! spend coming and flags over-budget, and offers a Reserve-and-Commit
//! pre-check (`reserve`) before a costly step.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Accumulated spend per key.
#[derive(Default, Debug)]
pub struct BudgetLedger {
    spent: BTreeMap<String, f64>,
}

/// A key's spend against a cap — the shape a guardrails UI / alert renders.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BudgetStatus {
    /// The budget key (e.g. a pod id).
    pub key: String,
    /// Total spent, USD.
    pub spent: f64,
    /// The cap, USD.
    pub cap: f64,
    /// Remaining headroom (never negative).
    pub remaining: f64,
    /// Whether spend has met or exceeded the cap.
    pub over_budget: bool,
    /// Fraction of the cap used in `[0, ∞)` (0 when cap ≤ 0).
    pub pct: f64,
}

impl BudgetLedger {
    /// An empty ledger.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add `amount` (USD) to `key`'s spend. Non-finite or negative amounts are
    /// ignored (spend only goes up).
    pub fn record(&mut self, key: &str, amount: f64) {
        if amount.is_finite() && amount > 0.0 {
            *self.spent.entry(key.to_string()).or_insert(0.0) += amount;
        }
    }

    /// Total spent for `key`.
    pub fn spent(&self, key: &str) -> f64 {
        self.spent.get(key).copied().unwrap_or(0.0)
    }

    /// Reserve-and-Commit pre-check: would spending `amount` more keep `key`
    /// within `cap`? (`spent + amount ≤ cap`).
    pub fn reserve(&self, key: &str, amount: f64, cap: f64) -> bool {
        self.spent(key) + amount <= cap
    }

    /// Status of `key` against `cap`.
    pub fn status(&self, key: &str, cap: f64) -> BudgetStatus {
        let spent = self.spent(key);
        BudgetStatus {
            key: key.to_string(),
            spent,
            cap,
            remaining: (cap - spent).max(0.0),
            over_budget: spent >= cap && cap > 0.0,
            pct: if cap > 0.0 { spent / cap } else { 0.0 },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_accumulates_and_ignores_bad_amounts() {
        let mut led = BudgetLedger::new();
        led.record("pod-1", 0.04);
        led.record("pod-1", 0.03);
        led.record("pod-1", -1.0); // ignored
        led.record("pod-1", f64::NAN); // ignored
        assert!((led.spent("pod-1") - 0.07).abs() < 1e-9);
        assert_eq!(led.spent("pod-2"), 0.0);
    }

    #[test]
    fn reserve_respects_cap() {
        let mut led = BudgetLedger::new();
        led.record("p", 0.08);
        assert!(led.reserve("p", 0.02, 0.10)); // 0.08 + 0.02 == 0.10 → ok
        assert!(!led.reserve("p", 0.05, 0.10)); // 0.13 > 0.10 → refused
    }

    #[test]
    fn status_math_and_over_budget() {
        let mut led = BudgetLedger::new();
        led.record("p", 0.07);
        let s = led.status("p", 0.10);
        assert!((s.remaining - 0.03).abs() < 1e-9);
        assert!(!s.over_budget);
        assert!((s.pct - 0.7).abs() < 1e-9);

        led.record("p", 0.05); // 0.12 > cap
        let s = led.status("p", 0.10);
        assert!(s.over_budget);
        assert_eq!(s.remaining, 0.0);
        assert!(s.pct > 1.0);
    }
}
