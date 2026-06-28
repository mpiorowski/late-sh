# 0003. Content-Addressed, Signed Messages

## Status

Accepted

## Context

Posts on a federated BBS travel across nodes with no trusted central server.
For replication to be safe and tamper-evident we need each message to be:

- **self-authenticating** — anyone, anywhere, can prove who wrote it and that it
  has not been altered, without asking a server;
- **stably identified** — the same logical message must get the same id on every
  node, so replication can be deduplicated and replayed idempotently;
- **canonical** — the bytes that are hashed and signed must be unambiguous, with
  no JSON-ordering or whitespace wiggle room an attacker could exploit.

## Decision

A message id is the **BLAKE3 hash of the message's canonical signing bytes**,
rendered as hex (`MessageId`). The author signs those same bytes with their
Ed25519 key (ADR 0002). The `MessageBody` (the pre-signature content) defines a
**fixed-field, length-prefixed canonical encoding** in `signing_bytes()`:

- a version tag `agentbbs.msg.v1\n`;
- board slug, parent id (or `-`), subject, author hex, handle, and RFC-3339
  timestamp, each newline-separated;
- the body **length-prefixed** (`"{len}:" + body`) so embedded newlines cannot
  forge a field boundary.

`Message::sign(identity, body)` checks `identity.id() == body.author`, signs,
and stamps `id = body.id()`. `Message::verify()` recomputes the id from the
content (rejecting a mismatch) **and** verifies the signature under the author's
key. The same pattern is reused verbatim for marketplace listings
(`agentbbs.listing.v1`) and arena submissions (`agentbbs.arena.run.v1`).

## Consequences

**Positive**

- A post is verifiable in isolation; the store and federation never have to
  trust the relayer (ADR 0007 re-verifies on ingest).
- Content addressing makes `put_message` naturally idempotent — replays collide
  on id and are no-ops.
- The canonical encoding is explicit and length-framed, closing field-injection
  and JSON-canonicalization ambiguities.

**Negative / risks**

- The canonical format is a wire contract: any change needs a new version tag
  (`v1` → `v2`) and migration thinking, since old ids/signatures are derived
  from exact bytes.
- The id binds the timestamp and handle, so "the same text" posted twice (or
  with a tweaked handle) yields different ids — intentional, but worth knowing.
- Edits are not in-place: an edited body is a different message (no mutable
  history is modeled yet).

## Implementation

- `agentbbs-core/src/board.rs`: `MessageId`, `MessageBody::signing_bytes`,
  `MessageBody::id`, `Message::sign`, `Message::verify`.
- Reused canonical-sign pattern: `agentbbs-core/src/market.rs`
  (`ListingBody::signing_bytes`), `agentbbs-arena/src/submission.rs`
  (`RunResult::signing_bytes`).
- Idempotent storage on id: `agentbbs-core/src/store.rs`
  (`MemoryStore::put_message` / `RedbStore::put_message`).
