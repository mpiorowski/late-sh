//! The Arena service — registration, submission intake, and leaderboards.

use std::collections::HashMap;

use agentbbs_core::identity::{AgentId, Identity};
use agentbbs_core::{Error, Result};
use chrono::Utc;

use crate::benchmark::{Benchmark, BenchmarkId};
use crate::harness::{HarnessRunner, MetaHarness};
use crate::leaderboard::{self, Standing};
use crate::submission::{RunResult, Submission};

/// A competing agent (anonymous).
#[derive(Clone, Debug, PartialEq)]
pub struct Competitor {
    /// Public id.
    pub id: AgentId,
    /// Cosmetic handle.
    pub handle: String,
}

/// The Arena: a benchmark catalogue plus a log of verified submissions.
pub struct Arena {
    benchmarks: HashMap<String, Benchmark>,
    competitors: HashMap<AgentId, Competitor>,
    submissions: Vec<Submission>,
}

impl Arena {
    /// A new arena seeded with the built-in benchmark catalogue.
    pub fn new() -> Self {
        let benchmarks = Benchmark::catalogue()
            .into_iter()
            .map(|b| (b.id.0.clone(), b))
            .collect();
        Arena {
            benchmarks,
            competitors: HashMap::new(),
            submissions: Vec::new(),
        }
    }

    /// Register (or re-register) a benchmark.
    pub fn register_benchmark(&mut self, b: Benchmark) {
        self.benchmarks.insert(b.id.0.clone(), b);
    }

    /// All known benchmarks, sorted by id.
    pub fn benchmarks(&self) -> Vec<Benchmark> {
        let mut v: Vec<_> = self.benchmarks.values().cloned().collect();
        v.sort_by(|a, b| a.id.0.cmp(&b.id.0));
        v
    }

    /// Look up a benchmark.
    pub fn benchmark(&self, id: &str) -> Option<&Benchmark> {
        self.benchmarks.get(id)
    }

    /// Register a competitor.
    pub fn register_competitor(&mut self, id: AgentId, handle: impl Into<String>) {
        self.competitors.insert(
            id,
            Competitor {
                id,
                handle: handle.into(),
            },
        );
    }

    /// Accept a signed submission. Rejects unknown benchmarks, bad signatures,
    /// and impossible scores. Idempotent on identical re-submission.
    pub fn submit(&mut self, submission: Submission) -> Result<()> {
        if !self.benchmarks.contains_key(&submission.result.benchmark.0) {
            return Err(Error::NotFound(format!(
                "benchmark {}",
                submission.result.benchmark
            )));
        }
        submission.verify()?;
        let r = &submission.result;
        if r.total == 0 || r.passed > r.total || !(r.score.is_finite()) {
            return Err(Error::malformed("submission", "invalid score/totals"));
        }
        // Auto-register the competitor if new.
        self.competitors
            .entry(r.competitor)
            .or_insert_with(|| Competitor {
                id: r.competitor,
                handle: r.handle.clone(),
            });
        // Idempotent: skip exact duplicates.
        if !self.submissions.contains(&submission) {
            self.submissions.push(submission);
        }
        Ok(())
    }

    /// Total verified submissions on file.
    pub fn submission_count(&self) -> usize {
        self.submissions.len()
    }

    /// The leaderboard for a benchmark (best run per competitor, ranked).
    pub fn leaderboard(&self, benchmark: &str) -> Result<Vec<Standing>> {
        let bench = self
            .benchmarks
            .get(benchmark)
            .ok_or_else(|| Error::NotFound(format!("benchmark {benchmark}")))?;
        let subs = leaderboard::for_benchmark(&BenchmarkId(benchmark.into()), &self.submissions);
        Ok(leaderboard::rank(bench.score_kind, &subs))
    }

    /// Ingest a Retort-MetaHarness results bundle as `identity` (the run
    /// operator): aggregate per stack (excluding TOOLING false-fails), produce
    /// one signed [`Submission`] per stack, and accept them. Returns the number
    /// of stack submissions accepted. The Retort benchmark is auto-registered
    /// if missing. See [`crate::retort`].
    pub fn ingest_retort(
        &mut self,
        results: &crate::retort::RetortResults,
        identity: &Identity,
    ) -> Result<usize> {
        if !self.benchmarks.contains_key(crate::retort::RETORT_BENCHMARK_ID) {
            self.register_benchmark(crate::retort::retort_benchmark());
        }
        let subs = crate::retort::ingest(results, identity)?;
        let n = subs.len();
        for s in subs {
            self.submit(s)?;
        }
        Ok(n)
    }

    /// The Retort track leaderboard — ranked per **stack** (model · harness ·
    /// language), not per competitor, so one operator's many stacks all show.
    /// Placement: `requirement_coverage` desc, then cheaper `$/task`.
    pub fn retort_leaderboard(&self) -> Vec<crate::retort::StackStanding> {
        crate::retort::rank_stacks(&self.submissions)
    }

    /// Run a benchmark through the meta-harness as `identity`, then build and
    /// accept a signed submission from the result. Returns the new standing
    /// position (1-based) on that benchmark's leaderboard.
    pub async fn compete<R: HarnessRunner>(
        &mut self,
        harness: &MetaHarness<R>,
        identity: &Identity,
        handle: &str,
        benchmark: &str,
    ) -> Result<u32> {
        if !self.benchmarks.contains_key(benchmark) {
            return Err(Error::NotFound(format!("benchmark {benchmark}")));
        }
        let report = harness.run_benchmark(benchmark, handle).await?;
        let result = RunResult {
            benchmark: BenchmarkId(benchmark.into()),
            competitor: identity.id(),
            handle: handle.into(),
            score: report.score,
            passed: report.passed,
            total: report.total,
            harness: report.harness,
            at: Utc::now(),
            detail: report.detail,
        };
        let submission = Submission::sign(identity, result)?;
        self.submit(submission)?;
        let board = self.leaderboard(benchmark)?;
        Ok(board
            .iter()
            .find(|s| s.competitor == identity.id())
            .map(|s| s.rank)
            .unwrap_or(0))
    }
}

impl Default for Arena {
    fn default() -> Self {
        Arena::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::{FakeHarnessRunner, HarnessReport};
    use crate::submission::RunResult;

    fn signed(id: &Identity, score: f64, passed: u32) -> Submission {
        let r = RunResult {
            benchmark: BenchmarkId("cve-bench".into()),
            competitor: id.id(),
            handle: "h".into(),
            score,
            passed,
            total: 40,
            harness: "ruflo".into(),
            at: Utc::now(),
            detail: serde_json::Value::Null,
        };
        Submission::sign(id, r).unwrap()
    }

    #[test]
    fn submit_and_rank() {
        let mut arena = Arena::new();
        let a = Identity::generate();
        let b = Identity::generate();
        arena.submit(signed(&a, 0.2, 8)).unwrap();
        arena.submit(signed(&b, 0.5, 20)).unwrap();
        let board = arena.leaderboard("cve-bench").unwrap();
        assert_eq!(board[0].competitor, b.id());
        assert_eq!(arena.submission_count(), 2);
    }

    #[test]
    fn unknown_benchmark_rejected() {
        let mut arena = Arena::new();
        let id = Identity::generate();
        let mut s = signed(&id, 0.5, 20);
        s.result.benchmark = BenchmarkId("does-not-exist".into());
        // Re-sign so the signature matches the tampered benchmark field.
        let s = Submission::sign(&id, s.result).unwrap();
        assert!(matches!(arena.submit(s), Err(Error::NotFound(_))));
    }

    #[test]
    fn duplicate_submission_is_idempotent() {
        let mut arena = Arena::new();
        let id = Identity::generate();
        let s = signed(&id, 0.3, 12);
        arena.submit(s.clone()).unwrap();
        arena.submit(s).unwrap();
        assert_eq!(arena.submission_count(), 1);
    }

    #[tokio::test]
    async fn compete_runs_harness_and_places() {
        let report = HarnessReport {
            benchmark: "cve-bench".into(),
            score: 0.8,
            passed: 32,
            total: 40,
            harness: "ruflo@3.5".into(),
            detail: serde_json::Value::Null,
        };
        let runner = FakeHarnessRunner::new(serde_json::to_string(&report).unwrap());
        let mh = MetaHarness::new(runner);
        let mut arena = Arena::new();
        let id = Identity::generate();
        let rank = arena
            .compete(&mh, &id, "claude-opus", "cve-bench")
            .await
            .unwrap();
        assert_eq!(rank, 1);
        assert_eq!(arena.submission_count(), 1);
        // The stored submission must verify.
        assert!(arena.leaderboard("cve-bench").unwrap()[0].passed == 32);
    }
}
