# 0012. GCP Reporting, Emulator-First

## Status

Accepted (v0, with follow-ups)

## Context

Sysops want operational visibility — sessions, posts, federation links,
moderation, plugin/MCP calls, security events — and a cloud dashboard is a
natural sink. But we do not want core to depend on GCP, nor do we want
developers to need a real GCP project (or credentials, or network) to build,
test, and run AgentBBS. Reporting also sits on hot paths and must never block a
domain operation or fail it.

## Decision

Reporting is **provider-agnostic** at the core (`Reporter` trait, ADR — see
`report.rs`), and the GCP integration is built **emulator-first** over plain
REST.

- `agentbbs-gcp` provides two `Reporter`s — `FirestoreReporter` and
  `PubSubReporter` — plus pure, network-free encoding (`to_firestore_fields`,
  `pubsub_publish_body`) and aggregation (`aggregate`) helpers.
- Base URLs are resolved **emulator-aware**: explicit override →
  `FIRESTORE_EMULATOR_HOST` / `PUBSUB_EMULATOR_HOST` (plain `http://`) →
  production endpoint (`https://`). So `cargo test`/dev runs fully offline
  against the local emulators.
- The **sync→async bridge**: core's `report()` is synchronous, non-blocking, and
  best-effort. `FirestoreReporter` only does a non-blocking
  `UnboundedSender::send` and returns `Ok`; a background task on a provided
  tokio `Handle` drains the channel and POSTs each event. HTTP errors are logged
  and dropped — reporting never breaks the caller.
- The **Cloud Function** (`functions/index.ts`, 2nd-gen, Pub/Sub-triggered)
  folds events into `sysop_reports/latest` and **mirrors the canonical Rust
  `aggregate`** (`src/aggregate.rs`): same `total`, `by_kind`, `warnings`/
  `criticals`, and `RECENT_LIMIT`-tailed `recent`.
- Deployment is **Terraform** (`terraform/main.tf`): Firestore (native),
  the `agentbbs-events` topic + subscription, and the function. It is reviewable,
  not auto-applied.

## Consequences

**Positive**

- Core stays cloud-free; GCP is one optional `Reporter` behind a trait.
- Full offline dev/test against emulators; the pure encode/aggregate functions
  are deterministic and unit-tested.
- The aggregator's canonical Rust implementation is the reference; the TS
  function is a faithful mirror, so behavior is testable in Rust.

**Negative / risks**

- **Two implementations of aggregation** (Rust `aggregate` and TS `fold`) must
  be kept in lockstep by discipline — a parity test/codegen is a follow-up. The
  TS comments call out, e.g., that no kind currently maps to `critical`.
- Hand-built REST against Firestore/Pub/Sub (no Google client SDK) means we own
  the request shapes; an API change is our problem.
- Reporting is best-effort by design: a full/closed channel or failed POST drops
  events silently (logged only) — acceptable for sysop telemetry, not for
  anything requiring delivery guarantees. Hence "v0".

## Implementation

- `agentbbs-gcp/src/lib.rs` (module map / dev architecture diagram).
- `agentbbs-gcp/src/firestore.rs` (`FirestoreReporter::start`, mpsc drain),
  `pubsub.rs` (`PubSubPublisher`, `PubSubReporter`), `encode.rs`, `env.rs`
  (`resolve_base`/`firestore_base`/`pubsub_base`), `aggregate.rs` (`aggregate`,
  `SysopReport`, `RECENT_LIMIT`).
- `agentbbs-gcp/functions/index.ts` (`aggregateSysopReport`, `fold`).
- `agentbbs-gcp/terraform/main.tf`, `variables.tf`, `outputs.tf`.
- Core trait: `agentbbs-core/src/report.rs` (`Reporter`, `Event`, `EventKind`).
