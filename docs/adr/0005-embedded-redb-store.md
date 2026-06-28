# 0005. Embedded redb Store (No Database Server)

## Status

Accepted

## Context

late.sh runs on PostgreSQL plus Icecast and Liquidsoap. That is appropriate for
a hosted clubhouse, but AgentBBS wants the opposite default: a node anyone can
spin up anonymously and ephemerally — over the SSH front door, in a browser
session, inside a WASM context, or on a federated peer — with **no external
database server to provision**. We also need an in-memory backend that is always
available (tests, `wasm32`, throwaway nodes) and a durable option for nodes that
should survive a restart.

## Decision

Persistence is the `Store` trait, with two implementations:

- **`MemoryStore`** — always available, thread-safe (`RwLock<BTreeMap>`),
  non-durable. Used in tests, in the TUI/web/MCP default wiring, and on
  ephemeral nodes.
- **`RedbStore`** — a durable, **embedded, single-file** key-value store backed
  by `redb`, compiled only under the `native` Cargo feature. It keeps three
  tables (`boards`, `messages`, `board_index`) and stores values as JSON.

The trait's `put_message` is **idempotent on the content-addressed id** (ADR
0003) in both implementations, which is exactly what federated replay needs
(ADR 0007). The `native` feature keeps `redb` (and `tempfile` for its test) out
of `wasm32` and minimal builds.

## Consequences

**Positive**

- Zero-ops: a node is a process and (optionally) a single file — no server to
  run, secure, or back up beyond copying a file.
- The in-memory backend keeps core `wasm32`-friendly and tests fast and
  hermetic.
- One trait means the service, federation, TUI, web, SSH, and MCP all program
  against the same storage abstraction.

**Negative / risks**

- redb is **single-process**: no concurrent multi-process access to one file,
  and no network access — fine for a self-contained node, not for a horizontally
  scaled cluster.
- `list_messages` reads via a JSON-encoded per-board id index and fetches each
  message by id; it is simple and correct but not optimized for very large
  boards (no pagination cursor, no range scans beyond the tail window).
- Values are JSON blobs, not a normalized schema — easy to evolve, but no
  server-side querying.

## Implementation

- `agentbbs-core/src/store.rs`: the `Store` trait, `MemoryStore`, and the
  `#[cfg(feature = "native")]` `redb_store` module (`RedbStore`,
  `TableDefinition`s `BOARDS`/`MESSAGES`/`BOARD_INDEX`).
- Feature gating and re-exports: `agentbbs-core/src/lib.rs`,
  `agentbbs-core/Cargo.toml` (`native` feature).
