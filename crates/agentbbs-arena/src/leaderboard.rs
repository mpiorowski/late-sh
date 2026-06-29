//! Leaderboard ranking over verified submissions.

use std::collections::HashMap;

use agentbbs_core::identity::AgentId;

use crate::benchmark::{BenchmarkId, ScoreKind};
use crate::submission::Submission;

/// A single ranked row.
#[derive(Clone, Debug, PartialEq)]
pub struct Standing {
    /// Rank, 1-based.
    pub rank: u32,
    /// Competitor id.
    pub competitor: AgentId,
    /// Competitor handle (from their best submission).
    pub handle: String,
    /// Their best score for the benchmark.
    pub best_score: f64,
    /// Tasks passed in the best run.
    pub passed: u32,
    /// Tasks attempted in the best run.
    pub total: u32,
}

/// Compute the leaderboard for one benchmark from a set of *verified*
/// submissions. Each competitor is represented by their single best run.
/// Ranking direction follows the benchmark's [`ScoreKind`].
pub fn rank(score_kind: ScoreKind, submissions: &[Submission]) -> Vec<Standing> {
    // Keep each competitor's best result.
    let mut best: HashMap<AgentId, &Submission> = HashMap::new();
    for s in submissions {
        let better = match best.get(&s.result.competitor) {
            None => true,
            Some(prev) => {
                if score_kind.higher_is_better() {
                    s.result.score > prev.result.score
                } else {
                    s.result.score < prev.result.score
                }
            }
        };
        if better {
            best.insert(s.result.competitor, s);
        }
    }

    let mut rows: Vec<&Submission> = best.into_values().collect();
    rows.sort_by(|a, b| {
        let (a, b) = (a.result.score, b.result.score);
        let ord = a.partial_cmp(&b).unwrap_or(std::cmp::Ordering::Equal);
        if score_kind.higher_is_better() {
            ord.reverse()
        } else {
            ord
        }
    });

    rows.into_iter()
        .enumerate()
        .map(|(i, s)| Standing {
            rank: (i + 1) as u32,
            competitor: s.result.competitor,
            handle: s.result.handle.clone(),
            best_score: s.result.score,
            passed: s.result.passed,
            total: s.result.total,
        })
        .collect()
}

/// Filter submissions to a single benchmark.
pub fn for_benchmark(benchmark: &BenchmarkId, submissions: &[Submission]) -> Vec<Submission> {
    submissions
        .iter()
        .filter(|s| &s.result.benchmark == benchmark)
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::submission::RunResult;
    use agentbbs_core::identity::Identity;
    use chrono::Utc;

    fn sub(score: f64, passed: u32) -> Submission {
        let id = Identity::generate();
        let r = RunResult {
            benchmark: BenchmarkId("cve-bench".into()),
            competitor: id.id(),
            handle: format!("a{passed}"),
            score,
            passed,
            total: 40,
            harness: "ruflo".into(),
            at: Utc::now(),
            detail: serde_json::Value::Null,
        };
        Submission::sign(&id, r).unwrap()
    }

    #[test]
    fn higher_pass_rate_ranks_first() {
        let subs = vec![sub(0.2, 8), sub(0.5, 20), sub(0.35, 14)];
        let board = rank(ScoreKind::PassRate, &subs);
        assert_eq!(board[0].rank, 1);
        assert_eq!(board[0].passed, 20);
        assert_eq!(board[2].passed, 8);
    }

    #[test]
    fn lower_latency_ranks_first() {
        let subs = vec![sub(12.0, 1), sub(3.0, 1), sub(7.0, 1)];
        let board = rank(ScoreKind::LatencySeconds, &subs);
        assert_eq!(board[0].best_score, 3.0);
    }

    #[test]
    fn best_run_per_competitor() {
        let id = Identity::generate();
        let mk = |score: f64, passed: u32| {
            let r = RunResult {
                benchmark: BenchmarkId("cve-bench".into()),
                competitor: id.id(),
                handle: "same".into(),
                score,
                passed,
                total: 40,
                harness: "ruflo".into(),
                at: Utc::now(),
                detail: serde_json::Value::Null,
            };
            Submission::sign(&id, r).unwrap()
        };
        let board = rank(ScoreKind::PassRate, &[mk(0.1, 4), mk(0.6, 24)]);
        assert_eq!(board.len(), 1);
        assert_eq!(board[0].passed, 24);
    }
}
