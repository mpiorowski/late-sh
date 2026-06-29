//! Criterion benchmark for the arena leaderboard `rank()` over ~500 signed
//! submissions.
//!
//! Run with the repo's lld override (the pinned `mold` linker may be absent):
//!
//! ```bash
//! RUSTFLAGS="-Clink-arg=-fuse-ld=lld" cargo bench -p agentbbs-arena
//! ```

use agentbbs_arena::{
    benchmark::{BenchmarkId, ScoreKind},
    leaderboard::rank,
    submission::{RunResult, Submission},
};
use agentbbs_core::identity::Identity;
use chrono::Utc;
use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};

const N: usize = 500;

/// Build `N` signed submissions from distinct competitors with spread scores.
fn submissions() -> Vec<Submission> {
    (0..N)
        .map(|i| {
            let id = Identity::generate();
            let passed = (i % 41) as u32; // 0..=40
            let result = RunResult {
                benchmark: BenchmarkId("cve-bench".into()),
                competitor: id.id(),
                handle: format!("agent-{i}"),
                score: passed as f64 / 40.0,
                passed,
                total: 40,
                harness: "ruflo@3.5".into(),
                at: Utc::now(),
                detail: serde_json::Value::Null,
            };
            Submission::sign(&id, result).unwrap()
        })
        .collect()
}

fn bench_rank(c: &mut Criterion) {
    let subs = submissions();

    let mut group = c.benchmark_group("leaderboard_rank");
    group.throughput(Throughput::Elements(subs.len() as u64));
    group.bench_function("rank_pass_rate_500", |b| {
        b.iter(|| {
            let board = rank(ScoreKind::PassRate, black_box(&subs));
            black_box(board);
        });
    });
    group.finish();
}

criterion_group!(benches, bench_rank);
criterion_main!(benches);
