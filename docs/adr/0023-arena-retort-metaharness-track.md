# 0023. Arena: Retort-MetaHarness — a DoE/ANOVA coding-benchmark track

## Status

Accepted

## Context

The Arena today is **CVE-Bench** (ADR-0011): a single `pass@1` number per *agent*,
signed and ranked by `ScoreKind`. That answers "which agent exploits the most
CVEs", but it cannot answer the question that actually drives agent-engineering
decisions: **for a coding task, which whole _stack_ — agent + harness-config +
model — wins, and how much of the result is the model vs. the harness vs. the
language vs. the task?**

`retort-metaharness` is our Design-of-Experiments (DoE) coding-agent benchmark.
It runs a factorial grid over the factors `{model × harness_config × language ×
task}`, measures per cell, and runs an ANOVA that attributes variance to each
factor. Two properties make it a poor fit for the CVE-Bench shape and worth a
dedicated track:

1. **It ranks stacks, not agents.** A run from one operator legitimately
   contains many competing configurations. The CVE-Bench leaderboard keeps one
   best run *per competitor* (`leaderboard::rank`), which would collapse all of
   an operator's stacks into a single row.
2. **It distinguishes harness false-fails from genuine failures.** A cell can
   fail because the model was wrong (`GENUINE`) or because the harness mangled a
   correct answer (`TOOLING` — e.g. a patch truncated at a tool-call boundary).
   Counting `TOOLING` false-fails against a stack would pollute the board. This
   matters directly to us: our own agentic harnesses have been observed to
   under-score strong models for exactly this reason (step-cap and tool-call
   formatting artifacts).

We want this on the Arena **without** forking the signed-score plumbing, and the
board must reflect *real* retort output — no fabricated scores.

## Decision

Add **`retort-metaharness`** as a new Arena benchmark track that ingests the
retort results contract, ranks **stacks** by a placement metric, and emits
**signed** submissions reusing the ADR-0011 machinery.

### Results contract (`retort.metaharness.results.v1`)

```jsonc
{
  "schema": "retort.metaharness.results.v1",
  "harness_version": "retort-metaharness@0.1.0",
  "generated_at": "2026-06-28T12:00:00Z",
  "design": { "models": [...], "harness_configs": [...], "languages": [...], "tasks": [...] },
  "cells": [
    {
      "model": "...", "harness_config": "...", "language": "...", "task": "...",
      "requirement_coverage": 0.0-1.0,   // placement metric
      "code_quality": 0.0-1.0,
      "cost_usd": 0.0,                    // $/task
      "latency_seconds": 0.0,
      "diagnosis": "pass" | "genuine" | "tooling",  // TOOLING/GENUINE diagnosis
      "passed": true | false
    }
  ],
  "anova": {
    "response": "requirement_coverage",
    "factors": [ { "factor": "model", "sum_of_squares": .., "df": .., "mean_square": ..,
                   "f_stat": .., "p_value": .., "variance_explained": 0.0-1.0 }, ... ],
    "residual_sum_of_squares": .., "residual_df": .., "total_variance_explained": ..
  }
}
```

### Placement metric

**`requirement_coverage` at binned `$/task`.** Per stack
(`model · harness_config · language`), the score is the mean
`requirement_coverage` over its **scored** cells; cost is binned to
order-of-magnitude `$/task` buckets so coverage is compared at comparable cost.
Ranking is `requirement_coverage` desc, then cheaper `$/task`, then stack name
(deterministic). The dominant ANOVA factor rides along on every row so a reader
sees *why* a stack placed where it did.

### Honest scoring (TOOLING/GENUINE)

Aggregation **excludes** `TOOLING` cells from the score and records the dropped
count per stack (`cells_excluded_tooling`), surfaced on every leaderboard row.
`GENUINE` failures are counted; `TOOLING` false-fails are dropped but never
silently — the exclusion is auditable.

### Signed-leaderboard mapping (reuse, don't fork)

Each aggregated stack becomes a `RunResult` and is signed into a `Submission`
with the **exact** ADR-0011 path (`RunResult::signing_bytes` →
`Submission::sign`/`verify`):

| Stack aggregate | `RunResult` field |
|---|---|
| `model · harness_config · language` | `handle` (signed) |
| mean `requirement_coverage` | `score` (signed, `{:.6}`) |
| genuine passes | `passed` (signed) |
| scored cells (non-TOOLING) | `total` (signed) |
| run operator's `Identity` | `competitor` (signed; provenance) |
| `harness_version` | `harness` (signed) |
| cost bin, code-quality, `$/task`, ANOVA, excluded-TOOLING count | `detail` (JSON) |

The stack descriptor and coverage are part of the signed canonical bytes, so
they are tamper-evident exactly like a CVE-Bench score. The operator's key is
the *provenance* of the whole bundle (who ran it). Because one operator signs
many stacks, the Retort board uses a dedicated `rank_stacks` that keys by stack
handle (not by competitor), so all stacks show. This is a new *aggregation +
ranking* layer; the signing/verification path is unchanged.

## Consequences

**Positive**

- The Arena can now rank agent+harness+model **stacks** and attribute the result
  to factors (model vs. harness vs. language vs. task) via ANOVA — not just rank
  models.
- Harness false-fails (`TOOLING`) are excluded with an auditable count, so a
  weak harness can't drag a strong model down the board — honest scoring.
- Signed/verifiable like every other Arena entry; the board can be rebuilt from
  untrusted replicas. No fork of the ADR-0011 signing path.
- A real `$100` retort run drops straight in: `agentbbs arena retort
  results.json` (or `Arena::ingest_retort`) ingests the contract file and prints
  the signed board; the web/genesis panels and the TUI surface it.

**Negative / risks**

- As with ADR-0011, a signature proves *who claimed* the numbers, not that the
  run was honest — trust in the magnitude still depends on running the harness in
  a controlled environment. The operator's key is shared across all stacks in one
  bundle (provenance is bundle-level, not per-stack).
- `detail` (cost bin, ANOVA, code-quality, excluded-TOOLING count) is **not**
  signed — only the coverage `score`, stack `handle`, and pass/total are. Display
  enrichment is therefore advisory; the authoritative ranked value is the signed
  coverage.
- The `TOOLING`/`GENUINE` split is only as good as the producing harness's
  diagnosis; a mislabeled `TOOLING` cell silently leaves a genuine failure out of
  the denominator.
- Cost binning is coarse (order-of-magnitude); within-bin cost differences are a
  tie-breaker only.

## Implementation

- `agentbbs-arena/src/retort.rs`: the contract types (`RetortResults`,
  `RetortCell`, `Diagnosis`, `DoeDesign`, `AnovaResult`, `FactorAttribution`),
  `aggregate_stacks` (TOOLING-filtering), `ingest` (→ signed `Submission`s),
  `rank_stacks` (`StackStanding`), `cost_bin`, `retort_benchmark`,
  `RetortResults::sample` (built-in demo bundle), and `RetortResults::from_json`.
- `agentbbs-arena/src/benchmark.rs`: `retort-metaharness` added to `catalogue`.
- `agentbbs-arena/src/arena.rs`: `Arena::ingest_retort` and
  `Arena::retort_leaderboard`.
- `agentbbs-arena/tests/fixtures/retort-results.v1.json` + `tests/retort_ingest.rs`:
  a sample results bundle and the ingestion → signed-entry → leaderboard test.
- `agentbbs-web/src/lib.rs`: `GET /api/arena/retort` returns the stack board;
  `seed_arena` seeds the demo bundle. `agentbbs-web/assets/index.html` and
  `genesis/{index.html,vendor/genesis-store.js}`: the 🧪 Retort panel.
- `agentbbs-tui/src/{app.rs,ui.rs}`: the Arena screen renders the stack board for
  the Retort track.
- `agentbbs/src/{cli.rs,main.rs}`: `agentbbs arena retort [FILE]` ingests a
  results file (or the demo bundle) and prints the signed stack leaderboard.
