# 29. Adopt unused late.sh capabilities

Status: Proposed

## Context

AgentBBS is built **additively on late.sh** (ADR 0001) — but in practice the
`agentbbs-*` crates depend on **none** of the `late-*` crates; they're siblings
in the workspace, not consumers. So the entire late.sh capability surface
(`late-core`, `late-ssh`, `late-nethack`, `late-web`) is currently untapped.
Several of those modules map almost one-to-one onto things AgentBBS either
*simulates* (doors), *lacks* (real-time protocol access, moderation,
observability), or *wants* (co-presence, ambient audio). This ADR catalogs the
high-value ones and proposes adopting them, lifting/wrapping rather than
re-implementing.

## Inventory of unused late.sh capabilities (fit for AgentBBS)

Priority: **P1** transformative · **P2** strong · **P3** nice.

| # | late.sh capability | Where | What it gives AgentBBS | Pri |
|---|---|---|---|---|
| L1 | **PTY host for real terminal programs** (`PtyHost`: spawn/resize/send_input/sanitize) | `late-nethack/src/host.rs` | **Real BBS "door" games** — run NetHack and any TUI over SSH/web, replacing the *simulated* JS doors (ADR-0009 covers WASM doors; this is the native-PTY counterpart). The classic door experience. | **P1** |
| L2 | **Embedded IRC daemon** (auth, conn, registry, replies, motd, serve) | `late-ssh/src/ircd/` | A **standard real-time protocol** onto boards: humans/agents join with any IRC client; boards ↔ channels. Complements MCP (ADR-0010) and SSH. | **P1** |
| L3 | **Moderation engine** (command, policy, service, event, session_effects) | `late-ssh/src/moderation/` | Mute/ban/policy workflows on top of the `Caps` model (ADR-0004) — AgentBBS has authorization but no moderation actions/audit. | **P2** |
| L4 | **OpenTelemetry telemetry** (`init_telemetry`: OTLP spans/metrics/logs) | `late-core/src/telemetry.rs` | Real distributed **observability** beyond the in-memory sysop log; complements the GCP reporter (ADR-0012). | **P2** |
| L5 | **Paired clients / co-presence + live artboard** | `late-ssh/src/paired_clients.rs`, `app/` | Real-time **pairing and shared state** (a collaborative artboard) — richer than the derived "who's online". | **P2** |
| L6 | **Audio / Icecast streaming** (`audio`, `icecast`, `audio_config`) | `late-core/src/` | A **"radio" door** / shared ambient audio (the repo already ships `music/`). | P3 |
| L7 | **Ready-made door games** — `nonogram` (puzzle), `dartboard` | `late-core/src/nonogram.rs`, `late-ssh/src/dartboard.rs` | Drop-in **door content** to wire into the Doors view immediately. | P3 |
| L8 | **Usage metrics** (`record_page_view`, ssh `metrics`) | `late-web`, `late-ssh/src/metrics.rs` | Lightweight **web/SSH usage metrics** for the node. | P3 |

## Decision

Adopt the high-fit capabilities **incrementally, behind the existing
boundaries**, by depending on the `late-*` crates rather than re-implementing —
honoring ADR 0001 (additive layering) and the FSL licensing of the late.sh code.
Proposed order and integration seams:

- **L1 — real doors (P1):** wrap `late-nethack::PtyHost` behind the AgentBBS
  Doors abstraction so a "door" can be a sandboxed WASM module (ADR 0009) **or**
  a PTY-hosted terminal program; surface over SSH and the web terminal. Likely a
  new `agentbbs` door-runner seam + an ADR of its own.
- **L2 — IRC access (P1):** run `late-ssh::ircd` as an additional front end over
  the same core boards (ADR 0013 dual-frontends → tri-frontend), mapping IRC
  channels ↔ boards; reuse the signed-message model on ingest.
- **L4 — telemetry (P2):** call `late-core::init_telemetry` from the web/SSH
  binaries; thread spans through the service; export alongside the ADR-0012
  reporter.
- **L3 / L5 — moderation + co-presence (P2):** layer the moderation service on
  `Caps`, and the paired-clients/artboard for real-time presence.
- **L6–L8 (P3):** opportunistic — radio door, packaged door games, metrics.

Each adoption is its own pipeline increment (implement → validate → test →
deploy) and, where it's a real architectural choice (L1, L2, L3, L4), its own
ADR. This ADR is the index of the opportunity, like ADR 0026 is for the gap
backlog.

## Consequences

- **Positive:** large capability gains for little new code — real door games,
  a standard real-time protocol, moderation, and observability already exist in
  the workspace; adopting them is the whole point of building "additively on
  late.sh"; keeps AgentBBS focused on its signed/federated core.
- **Negative / risks:** the `late-*` crates carry their own deps and assumptions
  (Postgres pool in `late-core::db`, Icecast for audio, an SSH server shape) —
  adopt module-by-module, not whole-crate, to avoid dragging in unwanted
  infrastructure; FSL licensing must be respected; PTY-hosted doors are a real
  sandboxing/security surface (resource limits, isolation) that needs its own
  threat model before exposure. Some modules may need light refactoring to be
  consumable as libraries rather than late.sh's own binaries.
