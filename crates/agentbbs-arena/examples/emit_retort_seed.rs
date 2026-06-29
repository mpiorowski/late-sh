//! Emit the genesis web `SEED_RETORT` rows for a real retort bundle, exactly as
//! `rank_stacks` produces them — so the static web seed is byte-faithful to the
//! Rust ranker. Usage: `cargo run -p agentbbs-arena --example emit_retort_seed -- <bundle.json>`
use agentbbs_arena::{Arena, RetortResults};
use agentbbs_core::identity::Identity;

fn jstr(s: &str) -> String {
    serde_json::to_string(s).unwrap()
}

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("usage: emit_retort_seed <bundle.json>");
    let json = std::fs::read_to_string(&path).expect("read bundle");
    let results = RetortResults::from_json(&json).expect("parse bundle");
    let operator = Identity::generate();
    let mut arena = Arena::new();
    arena.ingest_retort(&results, &operator).expect("ingest");
    let board = arena.retort_leaderboard();

    println!("[");
    let n = board.len();
    for (i, s) in board.iter().enumerate() {
        let comma = if i + 1 < n { "," } else { "" };
        println!(
            "    {{ rank: {}, stack: {}, requirement_coverage: {}, code_quality: {}, cost_usd: {}, cost_bin: {}, passed: {}, total: {}, excluded_tooling: {}, dominant_factor: {}, pareto_optimal: {}, pareto_tier: {}, is_baseline: {}, reported_frontier: {}, insight: {} }}{}",
            s.rank,
            jstr(&s.stack),
            (s.requirement_coverage * 1e6).round() / 1e6,
            (s.code_quality * 1e6).round() / 1e6,
            (s.cost_usd * 1e6).round() / 1e6,
            jstr(&s.cost_bin),
            s.passed,
            s.total,
            s.excluded_tooling,
            s.dominant_factor.as_deref().map(jstr).unwrap_or_else(|| "null".into()),
            s.pareto_optimal,
            s.pareto_tier,
            s.is_baseline,
            s.reported_frontier.map(|b| b.to_string()).unwrap_or_else(|| "null".into()),
            jstr(&s.insight),
            comma
        );
    }
    println!("]");
}
