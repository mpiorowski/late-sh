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
fn ingest_results_produce_signed_ranked_stacks() {
    let results = load();
    let operator = Identity::generate();

    let mut arena = Arena::new();
    let n = arena.ingest_retort(&results, &operator).expect("ingest");
    // 4 stacks: 2 models × 2 harness configs × 1 language.
    assert_eq!(n, 4);
    assert_eq!(arena.submission_count(), 4);

    // The Retort benchmark is in the catalogue.
    assert!(arena.benchmark(RETORT_BENCHMARK_ID).is_some());

    let board = arena.retort_leaderboard();
    assert_eq!(board.len(), 4, "all four stacks rank despite one operator");

    // Placement is by requirement_coverage: opus/ruflo-3tier on top.
    assert_eq!(board[0].rank, 1);
    assert!(board[0].stack.starts_with("claude-opus-4.8 · ruflo-3tier"));
    assert!((board[0].requirement_coverage - 0.925).abs() < 1e-9);

    // Honest scoring: the opus/single-shot stack dropped its TOOLING false-fail,
    // so its coverage is 0.85 (the surviving cell), not dragged toward zero.
    let opus_ss = board
        .iter()
        .find(|s| s.stack.starts_with("claude-opus-4.8 · single-shot"))
        .expect("opus single-shot present");
    assert_eq!(opus_ss.excluded_tooling, 1);
    assert!((opus_ss.requirement_coverage - 0.85).abs() < 1e-9);

    // ANOVA attribution survives ingestion.
    assert_eq!(board[0].dominant_factor.as_deref(), Some("model"));

    // Cost is carried for equal-cost comparison; deepseek is the cheap tier.
    let cheapest = board
        .iter()
        .min_by(|a, b| a.cost_usd.partial_cmp(&b.cost_usd).unwrap())
        .unwrap();
    assert!(cheapest.stack.starts_with("deepseek-v4"));
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
    assert_eq!(arena.submission_count(), 4);
}
