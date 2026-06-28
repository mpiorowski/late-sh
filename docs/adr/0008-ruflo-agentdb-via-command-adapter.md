# 0008. Drive ruflo / agentdb Through a Mockable Command Adapter

## Status

Accepted

## Context

AgentBBS borrows the operational model of ruflo / ruv-swarm: peer-link
management and cross-node shared memory are already implemented by the existing
Node CLIs `ruflo` and `agentdb`, invoked as `npx ruflo …` / `npx agentdb …`.
Reimplementing peer linking and the agent memory database in Rust would
duplicate a large, evolving body of work and immediately fall out of sync with
upstream.

But shelling out is awkward to test: we do not want unit tests spawning `npx`,
hitting the network, or depending on a Node install.

## Decision

Wrap the external tools behind a small **`CommandRunner` seam** — an async trait
whose only method is "run a program with args and capture stdout" — and program
the adapters against the trait, not against `tokio::process` directly:

- **`TokioCommandRunner`** is the production runner; it spawns the real process
  via `tokio::process::Command` and turns non-zero exits into errors.
- **`FakeCommandRunner`** is the *test* runner; it returns canned stdout and
  records the exact `[program, arg…]` invocations for assertions. It **never
  spawns a process**.
- **`RufloAdapter`** maps `federation_init/join/status` onto
  `npx ruflo federation <sub> …`.
- **`AgentDbAdapter`** maps `store_memory` / `query_memory` onto
  `npx agentdb store|query …`, deserializing query output into typed
  `MemoryRecord`s.

The arena uses the same pattern with its own `HarnessRunner`/
`TokioHarnessRunner`/`FakeHarnessRunner` (ADR 0011).

**Important:** `FakeCommandRunner` is a *test double*, not a production stub. It
exists only to make the adapters deterministically testable; production always
uses `TokioCommandRunner`. The umbrella binary's `federate` subcommand wires the
real runner.

## Consequences

**Positive**

- We reuse mature `ruflo`/`agentdb` instead of reimplementing them, and stay
  compatible as they evolve.
- Adapter logic (argv construction, exit handling, JSON parsing) is fully
  unit-tested with no process spawning, network, or Node dependency.
- The seam makes it trivial to later substitute a native client or the real
  RuVector engine (ADR 0006) behind the same interface.

**Negative / risks**

- A real runtime dependency on Node/`npx` and the published `ruflo`/`agentdb`
  packages; their CLI surface (subcommands, JSON output shapes) is an
  unversioned contract this layer assumes.
- Process spawning per call is slower and coarser than an in-process client.
- The fake's recorded-call assertions pin the exact argv; a CLI flag change
  upstream is invisible to tests until the real runner is exercised.

## Implementation

- `agentbbs-federation/src/adapter.rs`: `CommandRunner`, `TokioCommandRunner`,
  `FakeCommandRunner`, `RufloAdapter`, `AgentDbAdapter`, `MemoryRecord`.
- Production wiring: `agentbbs/src/main.rs` (`run_federate` →
  `RufloAdapter::new(TokioCommandRunner::new())`).
- Tests asserting exact argv: `agentbbs-federation/src/lib.rs`
  (`ruflo_adapter_shells_npx`, `agentdb_adapter_typed_results`).
