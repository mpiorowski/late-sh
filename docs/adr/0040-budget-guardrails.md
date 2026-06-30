# 0040. Budget guardrails

Status: Accepted (Phase 1 — core ledger + pod-cost wiring shipped)

## Context

A business autopilot spends real money (meta-llm/Cognitum tokens, compute).
ADR-0035 already gives each pod a `per_agent_cap_usd` (Reserve-and-Commit) and
the meta-llm gateway meters spend server-side — but AgentBBS, as the control
plane + UI, needs to **track spend against caps and surface guardrails**: how
much has a pod/board spent, how much is left, and is anything over budget. Pod
step-results already carry `cost_usd` (ADR-0035 slice 4), so the data exists.

## Decision

Add a `budget` primitive in `agentbbs-core` and wire pod costs into it:

- **`BudgetLedger`** — accumulates spend per key (pod id / account / board):
  `record(key, amount)`, `spent(key)`, and a **Reserve-and-Commit** check
  `reserve(key, amount, cap)` (true iff `spent + amount ≤ cap`).
- **`BudgetStatus { spent, cap, remaining, over_budget, pct }`** via
  `status(key, cap)` — the shape a guardrails UI / alert renders.
- **Wiring:** `POST /api/pods/{id}/results` records the result's `cost_usd` into
  the ledger under the pod id; **`GET /api/budget`** returns each pod's status
  against its `per_agent_cap_usd` (over-budget pods flagged).

This is AgentBBS-side *accounting + display*; the gateway remains the
authoritative meter and hard enforcer. Together they are defense-in-depth: the
gateway refuses to overspend, AgentBBS shows it coming and flags it.

## Consequences

- **Positive:** spend is visible and attributable per pod; `over_budget`/`pct`
  drive a guardrails UI and alerts (pairs with the #6 Stripe trial); `reserve()`
  lets the control plane pre-check a cap before escalating a pod's tier; real
  data flow (pod `cost_usd` → ledger), pure + testable.
- **Negative / future:** Phase 1 is an in-memory ledger seeded only by pod
  results; ingesting the gateway's `usage_ledger` for ground-truth spend, a
  Budget UI panel, per-board/per-tenant rollups, and alert thresholds are
  follow-ups. No key/token is involved (costs are plain numbers).

## Implementation

- `crates/agentbbs-core/src/budget.rs` — `BudgetLedger`, `BudgetStatus`,
  `record`/`spent`/`reserve`/`status`. Exported from the crate root.
- `crates/agentbbs-web/src/lib.rs` — `AppState.budget`; `/api/pods/{id}/results`
  records `cost_usd`; `GET /api/budget` reports per-pod status vs cap.
- Tests: accumulation, reserve respects cap, status math + over-budget; an
  integration test driving pod results → `/api/budget`.
- Phase 2: Budget guardrails UI (per-pod bars + alerts), gateway `usage_ledger`
  ingestion, board/tenant rollups.
