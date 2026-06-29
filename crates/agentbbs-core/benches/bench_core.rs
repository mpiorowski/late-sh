//! Criterion benchmarks for the performance-critical paths in
//! `agentbbs-core`:
//!
//! - identity sign + verify roundtrip (Ed25519)
//! - message sign + verify (BLAKE3 content-address + Ed25519)
//! - RVF cosine search over ~1000 vectors of dim 64
//! - `MemoryStore::put_message` throughput
//!
//! Run with the repo's lld override (the pinned `mold` linker may be absent):
//!
//! ```bash
//! RUSTFLAGS="-Clink-arg=-fuse-ld=lld" cargo bench -p agentbbs-core
//! ```

use agentbbs_core::{
    board::{Message, MessageBody},
    identity::Identity,
    rvf::{Record, RvfStore},
    store::{MemoryStore, Store},
};
use chrono::Utc;
use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};

/// Deterministic pseudo-random f32 vector of `dim` elements, seeded by `seed`.
/// A tiny xorshift keeps the benches dependency-free and reproducible.
fn vector(seed: u64, dim: usize) -> Vec<f32> {
    let mut state = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15).wrapping_add(1);
    (0..dim)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            // Map into roughly [-1, 1).
            ((state >> 11) as f32 / (1u64 << 53) as f32) * 2.0 - 1.0
        })
        .collect()
}

fn body(author: agentbbs_core::AgentId, n: usize) -> MessageBody {
    MessageBody {
        board: "general".into(),
        parent: None,
        subject: format!("subject {n}"),
        body: format!("benchmark message body number {n} with some agentic chatter"),
        author,
        handle: "benchcat".into(),
        created_at: Utc::now(),
    }
}

fn bench_identity_roundtrip(c: &mut Criterion) {
    let id = Identity::generate();
    let msg = b"post: hello agents, this is a signed line";
    c.bench_function("identity_sign_verify_roundtrip", |b| {
        b.iter(|| {
            let sig = id.sign(black_box(msg));
            id.id().verify(black_box(msg), &sig).unwrap();
        });
    });
}

fn bench_message_sign_verify(c: &mut Criterion) {
    let id = Identity::generate();
    let mut group = c.benchmark_group("message");

    group.bench_function("sign", |b| {
        b.iter(|| {
            let m = Message::sign(&id, body(id.id(), 0)).unwrap();
            black_box(m);
        });
    });

    let signed = Message::sign(&id, body(id.id(), 0)).unwrap();
    group.bench_function("verify", |b| {
        b.iter(|| {
            black_box(&signed).verify().unwrap();
        });
    });

    group.finish();
}

fn bench_rvf_search(c: &mut Criterion) {
    const DIM: usize = 64;
    const N: usize = 1000;

    let mut store = RvfStore::new(DIM);
    for i in 0..N {
        store
            .upsert(Record {
                id: format!("rec-{i}"),
                vector: vector(i as u64, DIM),
                meta: serde_json::json!({ "i": i }),
            })
            .unwrap();
    }
    let query = vector(999_999, DIM);

    let mut group = c.benchmark_group("rvf_search");
    group.throughput(Throughput::Elements(N as u64));
    group.bench_function("search_top10_over_1000x64", |b| {
        b.iter(|| {
            let hits = store.search(black_box(&query), 10).unwrap();
            black_box(hits);
        });
    });
    group.finish();
}

fn bench_memory_store_put(c: &mut Criterion) {
    let id = Identity::generate();
    // Pre-sign a batch so the bench measures store insertion, not signing.
    let messages: Vec<Message> = (0..1000)
        .map(|n| Message::sign(&id, body(id.id(), n)).unwrap())
        .collect();

    let mut group = c.benchmark_group("memory_store");
    group.throughput(Throughput::Elements(messages.len() as u64));
    group.bench_function("put_message_batch_1000", |b| {
        b.iter(|| {
            let store = MemoryStore::new();
            for m in &messages {
                store.put_message(black_box(m)).unwrap();
            }
            black_box(store.message_count().unwrap());
        });
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_identity_roundtrip,
    bench_message_sign_verify,
    bench_rvf_search,
    bench_memory_store_put,
);
criterion_main!(benches);
