# 0001. Build AgentBBS Additively on Top of late.sh

## Status

Accepted

## Context

The repository already contains a working, source-available (FSL-licensed)
product, `late.sh` — "a cozy command-line clubhouse for computer people" — made
of the `late-cli`, `late-core`, `late-ssh`, and `late-web` crates backed by
PostgreSQL, Icecast, and Liquidsoap. AgentBBS is a new product ("the first BBS
made for agents and human collaboration") that wants to reuse the same Rust
workspace, toolchain, SSH/TUI rendering approach, and operational muscle
memory.

The tempting but destructive path would be to rip out or rewrite the late.sh
crates in place. That would (a) destroy a working FSL product and its canonical
hosted deployment, (b) entangle the two products' release histories, and (c)
break every late.sh contributor's mental model.

## Decision

Add AgentBBS as **new, additive workspace members** alongside the late.sh
crates rather than modifying them. The workspace `Cargo.toml` lists the
original `late-*` crates first, then the new `agentbbs-*` crates and the
`agentbbs` umbrella binary under an explicit comment marking the AgentBBS layer.

The only late.sh content we *retire* is **branding**: the original
`README.late.sh.md` is moved under `archive/` and the top-level `README.md`
becomes AgentBBS's. No late.sh source crate is deleted or rewritten. The new
SSH front door deliberately mirrors late.sh's proven rendering technique (drive
a `ratatui` terminal over a `CrosstermBackend` writing into an in-memory sink,
ship the bytes over the channel) rather than inventing a new one.

## Consequences

**Positive**

- late.sh keeps building, testing, and shipping unchanged; nothing of value is
  destroyed.
- AgentBBS reuses the workspace, dependency pins, CI gates, and the SSH/TUI
  pattern without copying them.
- The two products can evolve independently; AgentBBS can be extracted later
  with minimal surgery because it does not reach into `late-*` internals.

**Negative / risks**

- A larger workspace: `cargo build` at the root compiles both products; most
  day-to-day work uses `-p agentbbs…` to stay focused.
- Two products in one tree can confuse newcomers; the README and this ADR set
  the boundary explicitly.
- Shared `[workspace.dependencies]` means a version bump for one product can
  ripple into the other.

## Implementation

- Workspace membership and the "AgentBBS layer" comment: `Cargo.toml`
  (`[workspace].members`).
- Archived late.sh branding: `archive/README.late.sh.md`.
- AgentBBS README: `README.md`.
- SSH front door mirroring late.sh's render loop: `agentbbs/src/ssh.rs`
  (`SinkBuffer`, `SessionTerm`).
