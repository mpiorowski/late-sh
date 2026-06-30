# 0043. Web-of-trust federation

Status: Accepted (Phase 1 — core graph shipped)

## Context

G5 gave federation **peer discovery**: a node learns peers via `PeerExchange`
and adds them at `TrustLevel::Unknown` (discovery never grants trust, ADR-0026).
But promoting every discovered peer by hand doesn't scale. The missing piece is
**transitive trust**: "I trust nodes my trusted peers vouch for." That's a
web-of-trust — the same idea as PGP key-signing or credential issuer-trust
(ADR-0042), applied to federation peers.

## Decision

Add a `webtrust` primitive in `agentbbs-federation`:

- **`Endorsement { endorser, subject, created_at, signature }`** — an
  Ed25519-signed statement that `endorser` vouches for `subject` as a trustworthy
  peer. `sign()` / `verify()` (forged/tampered rejected), same self-authenticating
  model as the federation envelope.
- **`WebOfTrust`** — `add` (verify-on-ingest) + `trusted_from(roots, max_depth)`:
  a BFS from a caller-chosen **root set** following endorsement edges, returning
  each reachable node with its trust *depth* (1 = directly endorsed by a root).
  `is_trusted(node, roots, max_depth)` is the gate.

Trust is **rooted in a caller-chosen set** (your own trusted peers), bounded by
`max_depth` (closer endorsements count, distant ones can be excluded) — no global
authority. It composes with G5: a discovered `Unknown` peer can be auto-promoted
when `is_trusted` holds, and with ADR-0042 (issuer trust is the same shape).

## Consequences

- **Positive:** trust scales without manual promotion of every peer; bounded
  (depth-limited) and rooted (no CA); fully offline-verifiable; deterministic +
  testable; reuses the Ed25519 stack (no new deps). Natural hook for "auto-trust
  peers endorsed within depth N of my trusted set."
- **Negative / future:** Phase 1 is the graph + endorsement type; wiring
  auto-promotion into the `Federator`/`PeerBook`, gossiping endorsements over a
  `FederationPayload`, revocation, and weighted/decayed trust are follow-ups.
  Sybil endorsers are inherent to anonymous identities — depth + rooting bound
  the blast radius, they don't eliminate it.

## Implementation

- `crates/agentbbs-federation/src/webtrust.rs` — `Endorsement` (sign/verify),
  `WebOfTrust` (add/trusted_from/is_trusted, BFS depth). Exported from the crate
  root. Tests: sign/verify/tamper, transitive reach with depth bound, rooting
  (unrooted nodes untrusted), forged-not-added.
- Phase 2: auto-promote discovered peers via web-of-trust; endorsement gossip;
  revocation + decay.
