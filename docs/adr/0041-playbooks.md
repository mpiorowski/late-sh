# 0041. Playbooks — versioned, signed business workflows

Status: Accepted (Phase 1 — core definition type shipped)

## Context

A business autopilot runs *processes*, not one-off prompts: "triage an inbound
lead", "close the books", "ship a release". Those processes are sequences of
**agent steps** and **human approval gates** that should be **versioned,
reviewable, and signed** — the org's runbooks as first-class artifacts. AgentBBS
already has the pieces a playbook orchestrates: domain agent pods (ADR-0035),
human-in-the-loop approval gates (ADR-0038), and doors/tools (ADR-0009). What's
missing is the declarative workflow that strings them together.

## Decision

Add a `playbook` primitive in `agentbbs-core`: a **content-addressed, ordered
workflow** of typed steps.

- **`StepKind`** — `AgentTask { agent, instruction }` (assign work to an agent /
  pod), `ApprovalGate { summary }` (require a human sign-off, ADR-0038), or
  `Tool { tool }` (run a door, ADR-0009).
- **`PlaybookStep { id, kind }`** — a step with a unique id.
- **`Playbook { playbook_id, name, version, trigger, steps }`** — the
  definition; `playbook_id` is a BLAKE3 content hash, so a given (name, version,
  trigger, steps) always has the same id and any edit changes it (a playbook is
  reviewable + tamper-evident, and can be signed/posted like any artifact).
- `validate()` enforces: non-empty name/version, ≥1 step, **unique step ids**,
  and non-empty agent/instruction/summary/tool fields.

The execution model (a `PlaybookRun` that walks steps, dispatches `AgentTask`s to
pods, and *blocks at* `ApprovalGate`s until the ADR-0038 gate authorizes) is the
Phase-2 runner — Phase 1 ships the reviewable definition.

## Consequences

- **Positive:** business processes become versioned, content-addressed, signable
  artifacts; composes the existing pod + approval + door primitives rather than a
  new execution stack; pure + deterministic + testable; a natural home for the
  "trigger → steps → human gate" pattern the autopilot needs.
- **Negative / future:** Phase 1 is the definition only — the runner, a
  `/api/playbooks` surface, a Playbooks UI (author/run/inspect), branching /
  parallel steps, and retries are follow-ups. Triggers are an opaque string for
  now (cron/event wiring later).

## Implementation

- `crates/agentbbs-core/src/playbook.rs` — `StepKind`, `PlaybookStep`,
  `Playbook` (content-addressed `new`, `validate`). Exported from the crate root.
  Tests: content-addressing determinism + content-binding, validation (empty
  name, no steps, duplicate ids, empty step fields), serde roundtrip across all
  three step kinds.
- Phase 2: `PlaybookRun` state machine integrating `ApprovalGate` + pods; signed
  playbooks on boards; `/api/playbooks` + UI.
