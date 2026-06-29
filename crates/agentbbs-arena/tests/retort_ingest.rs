//! Integration test: a real retort-metaharness results file → `Arena`
//! ingestion → **signed** stack submissions → stack leaderboard.
//!
//! This is the path a live `$100` retort run takes: drop its
//! `retort.metaharness.results.v1` JSON in, get a verifiable, honestly-scored
//! Arena board out. The fixture is loaded from disk (not `include_str!`) so it
//! mirrors how a real result file is fed.

use agentbbs_arena::{Arena, RetortResults, RETORT_BENCHMARK_ID};
use agentbbs_core::identity::Identity;

fn load() -> RetortResults {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/retort-results.v1.json"
    );
    let json = std::fs::read_to_string(path).expect("read fixture");
    RetortResults::from_json(&json).expect("parse fixture")
}

#[test]
fn ingest_results_produce_pareto_ranked_signed_stacks() {
    let results = load();
    let operator = Identity::generate();

    let mut arena = Arena::new();
    let n = arena.ingest_retort(&results, &operator).expect("ingest");
    // 5 stacks: claude-code baseline + ruflo-3tier + single-shot (opus), and
    // ruflo-3tier + single-shot (deepseek).
    assert_eq!(n, 5);
    assert_eq!(arena.submission_count(), 5);

    // The Retort benchmark is in the catalogue.
    assert!(arena.benchmark(RETORT_BENCHMARK_ID).is_some());

    let board = arena.retort_leaderboard();
    assert_eq!(board.len(), 5, "all five stacks rank despite one operator");

    // PRIMARY RANKING IS PARETO: the most-accurate frontier stack leads…
    assert_eq!(board[0].rank, 1);
    assert!(board[0].pareto_optimal);
    assert!(board[0].stack.starts_with("claude-opus-4.8 · ruflo-3tier"));

    // …and the expensive claude-code baseline (higher raw accuracy than 3 of the
    // frontier stacks) ranks LAST because it is dominated — the cost-lever story.
    let last = board.last().unwrap();
    assert!(last.stack.contains("claude-code"));
    assert!(!last.pareto_optimal);
    assert!(last.is_baseline);
    assert!(last.insight.contains("lower cost"));

    // Honest scoring: opus/single-shot dropped its TOOLING false-fail (coverage
    // 0.85, not dragged toward zero), so the frontier isn't polluted by artifacts.
    let opus_ss = board
        .iter()
        .find(|s| s.stack.starts_with("claude-opus-4.8 · single-shot"))
        .expect("opus single-shot present");
    assert_eq!(opus_ss.excluded_tooling, 1);
    assert!((opus_ss.requirement_coverage - 0.85).abs() < 1e-9);

    // The Arena's recomputed frontier agrees with report.py's pareto_analysis.
    for s in &board {
        assert_eq!(Some(s.pareto_optimal), s.reported_frontier);
    }

    // ANOVA attribution survives ingestion.
    assert_eq!(board[0].dominant_factor.as_deref(), Some("model"));
}

#[test]
fn ingested_submissions_are_idempotent() {
    let results = load();
    let operator = Identity::generate();
    let mut arena = Arena::new();
    arena.ingest_retort(&results, &operator).unwrap();
    // Re-ingesting the same bundle by the same operator is a no-op (signed
    // submissions are byte-identical → deduped).
    arena.ingest_retort(&results, &operator).unwrap();
    assert_eq!(arena.submission_count(), 5);
}
