//! Agent reputation / track record (ADR-0039).
//!
//! Aggregates per-agent **outcome records** (bench passes, completed pod tasks,
//! approvals — all already-verifiable signals) into a confidence-adjusted score.
//! The ranking metric is the **Wilson lower bound** of the (weighted) success
//! proportion at 95%, so an agent with a long clean record outranks one with a
//! lucky 1/1, and few-sample agents rank conservatively.

use serde::{Deserialize, Serialize};

use crate::identity::AgentId;

/// One observed outcome for an agent.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OutcomeRecord {
    /// The agent the outcome is about.
    pub agent: AgentId,
    /// Whether the outcome was a success.
    pub success: bool,
    /// Relative weight (higher = higher-stakes outcome). Use `1.0` by default.
    pub weight: f64,
    /// Provenance (e.g. `arena`, `pod`, `approval`).
    pub source: String,
}

/// A computed reputation for one agent.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ReputationScore {
    /// The agent (hex id).
    pub agent: String,
    /// Weighted successful outcomes.
    pub successes: f64,
    /// Weighted total outcomes.
    pub total: f64,
    /// Raw weighted success rate in `[0,1]` (0 when no outcomes).
    pub rate: f64,
    /// Wilson 95% lower bound of the success proportion — the ranking score.
    pub score: f64,
}

/// The 95% Wilson lower bound of a success proportion. `n` is the (weighted)
/// sample size, `successes` the (weighted) positive count. Returns 0 for n == 0.
fn wilson_lower_bound(successes: f64, n: f64) -> f64 {
    if n <= 0.0 {
        return 0.0;
    }
    let z = 1.96_f64;
    let z2 = z * z;
    let p = (successes / n).clamp(0.0, 1.0);
    let denom = 1.0 + z2 / n;
    let centre = p + z2 / (2.0 * n);
    let margin = z * ((p * (1.0 - p) + z2 / (4.0 * n)) / n).sqrt();
    ((centre - margin) / denom).max(0.0)
}

/// Accumulates outcome records and computes per-agent reputation.
#[derive(Default, Debug)]
pub struct ReputationLedger {
    records: Vec<OutcomeRecord>,
}

impl ReputationLedger {
    /// An empty ledger.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record an outcome (non-positive weights are ignored).
    pub fn record(&mut self, outcome: OutcomeRecord) {
        if outcome.weight > 0.0 && outcome.weight.is_finite() {
            self.records.push(outcome);
        }
    }

    /// The reputation of one agent (zeroed if never observed).
    pub fn score(&self, agent: &AgentId) -> ReputationScore {
        let hex = agent.to_hex();
        let mut successes = 0.0;
        let mut total = 0.0;
        for r in self.records.iter().filter(|r| r.agent == *agent) {
            total += r.weight;
            if r.success {
                successes += r.weight;
            }
        }
        let rate = if total > 0.0 { successes / total } else { 0.0 };
        ReputationScore {
            agent: hex,
            successes,
            total,
            rate,
            score: wilson_lower_bound(successes, total),
        }
    }

    /// Every observed agent, ranked by the Wilson lower bound (desc), then by
    /// weighted total (desc), then agent id for determinism.
    pub fn ranking(&self) -> Vec<ReputationScore> {
        let mut agents: Vec<AgentId> = Vec::new();
        for r in &self.records {
            if !agents.contains(&r.agent) {
                agents.push(r.agent);
            }
        }
        let mut out: Vec<ReputationScore> = agents.iter().map(|a| self.score(a)).collect();
        out.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(
                    b.total
                        .partial_cmp(&a.total)
                        .unwrap_or(std::cmp::Ordering::Equal),
                )
                .then(a.agent.cmp(&b.agent))
        });
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::Identity;

    fn rec(agent: &AgentId, success: bool) -> OutcomeRecord {
        OutcomeRecord {
            agent: *agent,
            success,
            weight: 1.0,
            source: "test".into(),
        }
    }

    #[test]
    fn unseen_agent_scores_zero() {
        let led = ReputationLedger::new();
        let s = led.score(&Identity::generate().id());
        assert_eq!(s.total, 0.0);
        assert_eq!(s.rate, 0.0);
        assert_eq!(s.score, 0.0);
    }

    #[test]
    fn small_sample_is_penalised_vs_long_clean_record() {
        let lucky = Identity::generate().id();
        let proven = Identity::generate().id();
        let mut led = ReputationLedger::new();
        led.record(rec(&lucky, true)); // 1/1
        for _ in 0..100 {
            led.record(rec(&proven, true)); // 100/100
        }
        let s_lucky = led.score(&lucky);
        let s_proven = led.score(&proven);
        assert_eq!(s_lucky.rate, 1.0);
        assert_eq!(s_proven.rate, 1.0); // same raw rate…
        assert!(s_proven.score > s_lucky.score); // …but the proven record ranks higher
    }

    #[test]
    fn weighting_and_rate() {
        let a = Identity::generate().id();
        let mut led = ReputationLedger::new();
        led.record(OutcomeRecord {
            agent: a,
            success: true,
            weight: 3.0,
            source: "pod".into(),
        });
        led.record(OutcomeRecord {
            agent: a,
            success: false,
            weight: 1.0,
            source: "pod".into(),
        });
        let s = led.score(&a);
        assert_eq!(s.total, 4.0);
        assert_eq!(s.successes, 3.0);
        assert!((s.rate - 0.75).abs() < 1e-9);
        // non-positive weights ignored
        led.record(OutcomeRecord {
            agent: a,
            success: true,
            weight: 0.0,
            source: "x".into(),
        });
        assert_eq!(led.score(&a).total, 4.0);
    }

    #[test]
    fn ranking_orders_by_confidence() {
        let strong = Identity::generate().id();
        let weak = Identity::generate().id();
        let mut led = ReputationLedger::new();
        for _ in 0..20 {
            led.record(rec(&strong, true));
        }
        led.record(rec(&weak, true));
        led.record(rec(&weak, false)); // 1/2
        let r = led.ranking();
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].agent, strong.to_hex());
    }
}
