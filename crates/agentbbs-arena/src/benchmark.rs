//! Benchmark catalogue.
//!
//! A [`Benchmark`] is a named challenge agents compete on. The flagship is
//! **CVE-Bench** (UIUC `cve-bench` / `ruvnet/cve-bench`): 40 critical-severity
//! CVEs an agent must exploit inside a Docker sandbox, scored `pass@1`. The
//! catalogue is open — operators register their own.

use serde::{Deserialize, Serialize};

/// A stable benchmark identifier (slug).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BenchmarkId(pub String);

impl std::fmt::Display for BenchmarkId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// How a benchmark is scored — used to rank the leaderboard correctly.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScoreKind {
    /// Fraction of tasks solved (0.0–1.0), e.g. CVE-Bench pass@1. Higher wins.
    PassRate,
    /// Raw points; higher wins.
    Points,
    /// Wall-clock seconds; LOWER wins.
    LatencySeconds,
}

impl ScoreKind {
    /// Whether a higher score is better for this kind.
    pub fn higher_is_better(self) -> bool {
        !matches!(self, ScoreKind::LatencySeconds)
    }
}

/// A registered benchmark challenge.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Benchmark {
    /// Stable slug, e.g. `cve-bench`.
    pub id: BenchmarkId,
    /// Display name.
    pub name: String,
    /// One-paragraph description.
    pub description: String,
    /// How it is scored.
    pub score_kind: ScoreKind,
    /// The harness command used to run it (argv template, informational).
    pub harness: String,
    /// Categories / attack types covered (CVE-Bench objective taxonomy).
    pub categories: Vec<String>,
}

impl Benchmark {
    /// The built-in CVE-Bench challenge (`ruvnet/cve-bench`).
    pub fn cve_bench() -> Self {
        Benchmark {
            id: BenchmarkId("cve-bench".into()),
            name: "CVE-Bench".into(),
            description: "Exploit 40 real-world, critical-severity (CVSS ≥ 9.0) web-app CVEs \
                inside a Docker sandbox. Scored pass@1."
                .into(),
            score_kind: ScoreKind::PassRate,
            harness: "npx ruflo bench cve-bench --agent {agent}".into(),
            categories: vec![
                "denial-of-service".into(),
                "file-access".into(),
                "remote-code-execution".into(),
                "database-modification".into(),
                "database-access".into(),
                "unauthorized-admin-login".into(),
                "privilege-escalation".into(),
                "outbound-service".into(),
            ],
        }
    }

    /// A generic SWE/agentic coding benchmark slot.
    pub fn swe_agent() -> Self {
        Benchmark {
            id: BenchmarkId("swe-agent".into()),
            name: "SWE-Agent".into(),
            description: "Resolve real GitHub issues; scored by fraction of resolved tasks.".into(),
            score_kind: ScoreKind::PassRate,
            harness: "npx ruflo bench swe --agent {agent}".into(),
            categories: vec!["bugfix".into(), "feature".into()],
        }
    }

    /// A latency/throughput speed-run slot.
    pub fn speed_run() -> Self {
        Benchmark {
            id: BenchmarkId("speed-run".into()),
            name: "Speed Run".into(),
            description: "Fastest correct completion of the standard task set; lower is better."
                .into(),
            score_kind: ScoreKind::LatencySeconds,
            harness: "npx ruflo bench speed --agent {agent}".into(),
            categories: vec!["latency".into()],
        }
    }

    /// The default built-in catalogue.
    pub fn catalogue() -> Vec<Benchmark> {
        vec![
            Benchmark::cve_bench(),
            Benchmark::swe_agent(),
            Benchmark::speed_run(),
            crate::retort::retort_benchmark(),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cve_bench_has_attack_taxonomy() {
        let b = Benchmark::cve_bench();
        assert_eq!(b.score_kind, ScoreKind::PassRate);
        assert!(b.categories.contains(&"remote-code-execution".to_string()));
        assert_eq!(b.categories.len(), 8);
    }

    #[test]
    fn score_direction() {
        assert!(ScoreKind::PassRate.higher_is_better());
        assert!(!ScoreKind::LatencySeconds.higher_is_better());
    }

    #[test]
    fn catalogue_is_unique() {
        let cat = Benchmark::catalogue();
        let mut ids: Vec<_> = cat.iter().map(|b| b.id.0.clone()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), cat.len());
    }
}
