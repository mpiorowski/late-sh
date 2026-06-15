# late.sh Scale Notes

Last updated: 2026-06-04

This document records the current production capacity posture, what was discovered during the HN-spike investigation, the DB query findings, and the roadmap toward roughly 1000 concurrent users.

## Current Infra Status

Cluster shape:

- Single RKE2 node: `server-1`
- Node capacity observed: 8 CPU, about 15.6 GiB memory
- Node usage during investigation: about 39% CPU, 44% memory
- All core app workloads currently run on the single node

Application deployments:

- `service-ssh`: 1 replica
  - SSH TUI server and HTTP API
  - Ports: 2222 SSH, 4000 API
  - Current Terraform/live CPU limit: 8 CPU
  - Current Terraform/live memory limit: 4 GiB
  - Current Terraform/live `LATE_MAX_CONNS_GLOBAL`: 1000
  - `termination_grace_period_seconds`: 21600, so old pods can linger for up to 6 hours while sessions drain
- `service-web`: 1 replica
  - Web pages and `/stream` proxy
  - Current Terraform/live `LATE_AUDIO_URL`: `http://icecast-sv:8000`
  - Public browser users still reach `/stream` through `https://late.sh/stream`; only the web pod's upstream fetch is internal
- `icecast`: 1 replica
  - Current Terraform/live client limit: 300
  - Current Terraform/live resources: request `100m/128Mi`, limit `500m/512Mi`
- `liquidsoap`: 1 replica
  - Encodes local playlist mounts for Icecast
- `postgres`: CloudNativePG, 2 instances
  - Primary: `postgres-1`
  - Current Terraform/live memory limit: 4 GiB
  - Current status after memory rollout: healthy, 2/2 ready
  - `max_connections`: 100
  - `shared_buffers`: 256 MB

Public endpoints still required:

- `late.sh`: public web and browser `/stream`
- `api.late.sh`: browser/CLI pair WebSocket and API
- `audio.late.sh`: direct public Icecast path, especially for CLI/local audio
- `ssh late.sh`: public SSH ingress

Internal endpoints:

- `service-web -> icecast-sv:8000` for upstream `/stream` proxying
- `service-ssh/service-web -> postgres-rw:5432`

## Recent Emergency Headroom Changes

Applied in Terraform and live Kubernetes:

- Raised `service-ssh` CPU limit from `4000m` to `8000m`
- Set `LATE_MAX_CONNS_GLOBAL` to `1000`
- Changed `late-web` audio upstream from public `https://audio.late.sh` to internal `http://icecast-sv:8000`
- Raised Postgres memory limit from `2Gi` to `4Gi`
- Raised Icecast client cap from `100` to `300`
- Raised Icecast resources from `50m/64Mi` request and `200m/128Mi` limit to `100m/128Mi` request and `500m/512Mi` limit

Operational note: changing the CNPG memory limit briefly removed the `postgres-rw` endpoint while the primary restarted. It recovered and reported healthy with 2 ready instances.

## Biggest Pain Points

### 1. Render/tick CPU is the primary 1000-user blocker

Each SSH/browser TUI session owns an `App`.

The SSH render loop and browser tunnel world tick run every 66 ms, roughly 15 FPS. At 1000 connected users, that is about 15,000 ticks/renders per second before considering input, chat, games, audio visualization, or room events.

This is likely the true baseline killer for "1000 connected and mostly idle" users.

Pain multipliers:

- Large terminals. Logs showed clients with PTYs as large as about 283x72.
- Animated/live panels: visualizer, clocks, aquarium, splash, timers, games, and other tick-driven UI.
- Browser tunnel sessions also render at the same world tick.
- Every hot chat event can wake many users and trigger rendering.

### 2. `service-ssh` cannot safely scale horizontally yet

Current `service-ssh` has in-memory ownership for:

- SSH session registry
- paired client registry
- active user presence
- app state per session
- room/game managers
- artboard state
- activity fanout

Scaling `service-ssh` to multiple replicas without routing browser pair WebSockets to the owning pod will break pairing. If SSH lands on pod A and `/api/ws/pair` lands on pod B, pod B does not know that token/session.

For horizontal scaling, one SSH session must stay on the same pod for its lifetime. That does not mean one pod per user. It means each pod owns many sessions, and pair traffic routes to the session owner.

Target shape for 1000 users may be roughly 8-15 SSH pods after render throttling, not 1000 pods.

### 3. Connect storms hit DB and service startup paths

Per-user connect/snapshot work includes:

- user lookup/create
- chat room list
- last message timestamps
- unread counts
- friends/profile/metadata
- notifications
- room/game data

The app is not continuously polling the DB for chat messages; chat message flow is event-driven. But connect storms and room switches still hit DB-heavy paths.

### 4. Audio capacity is still single-pod

Icecast now allows 300 clients, but it is still one pod on one node. For 1000 audio listeners, a dedicated streaming strategy is needed:

- dedicated Icecast host with real bandwidth headroom
- CDN/edge-compatible stream distribution
- multiple relays
- or browser/client behavior that avoids duplicating streams where possible

### 5. Postgres connections are bounded but not pooled externally

App pools are currently per process through deadpool, with `LATE_DB_POOL_SIZE=16` for both `service-ssh` and `service-web`.

Postgres `max_connections=100`. This is acceptable while replicas are low, but scaling app replicas will multiply pools. PgBouncer should be introduced before many app replicas.

## DB Investigation

`pg_stat_statements` was not enabled during the first investigation. Terraform is now prepared
to enable it through CloudNativePG's managed extension path by setting
`pg_stat_statements.*` parameters in the `Cluster` spec; CloudNativePG then adds the preload
library and runs `CREATE EXTENSION IF NOT EXISTS pg_stat_statements` automatically.

Observed live settings before this change:

- `shared_preload_libraries`: empty
- `track_io_timing`: off
- installed extensions: only `plpgsql`

That means there is no reliable historical "top query by total time" table yet. The investigation used:

- `pg_stat_activity`
- `pg_stat_user_tables`
- `pg_stat_user_indexes`
- relation sizes
- `EXPLAIN (ANALYZE, BUFFERS)` on representative query shapes

Database-level stats:

- DB size: about 161 MB during investigation
- Cache hit ratio: effectively 100%
- Historical temp spill: about 4 GB temp bytes, indicating some sort/hash spill history
- `chat_messages` was the noisiest table by sequential tuple reads: about 250B seq tuples read historically

Largest relation sizes observed:

- `chat_room_members`: about 44 MB total
- `chat_messages`: about 33 MB total
- `rss_entries`: about 16 MB total
- `notifications`: about 8.5 MB total

Skew:

- General chat dominates `chat_messages`: about 67k of 86k messages
- Heavy users can be members of more than 100 rooms

## DB Hot Queries Found

### `ChatRoomMember::unread_counts_for_user`

Source: `late-core/src/models/chat_room_member.rs`

Old shape:

- joined all memberships for a user to `chat_messages`
- planner chose a full sequential scan of `chat_messages`
- representative heavy user: about 381 ms
- scanned about 86k chat messages

New shape:

- per-room `LEFT JOIN LATERAL`
- uses existing `idx_chat_messages_room_created`
- representative heavy user: about 2.5 ms

This was patched in source on 2026-06-04. It becomes live after the next `late-ssh` image deploy.

### `ChatMessage::list_recent_for_rooms`

Source: `late-core/src/models/chat_message.rs`

Old shape:

- window function over all messages in all user rooms
- representative heavy user pulled about 82k rows
- external merge sort spilled about 11 MB temp
- representative runtime: about 1.4 seconds

New shape:

- distinct room IDs
- per-room lateral index scan with `LIMIT $2`
- uses existing `idx_chat_messages_room_created`
- representative heavy user: about 211 ms

This was patched in source on 2026-06-04. It becomes live after the next `late-ssh` image deploy.

### `ChatRoom::list_discover_public_topic_rooms`

Source: `late-core/src/models/chat_room.rs`

Current shape:

- public topic room discovery uses lateral counts for member count and message count
- representative runtime: about 300-475 ms
- main cost is repeated counts over `chat_room_members`

This is not as urgent as connect/snapshot chat paths, but it should be optimized or cached before large traffic.

Possible fixes:

- maintain denormalized `member_count`, `message_count`, `last_message_at` on `chat_rooms`
- or cache discovery results in process/Redis with a short TTL
- or pre-aggregate with a better index if exact live counts remain required

## Changes Made In Code

Changed hot query SQL only; no migration required. These changes are in source and require a normal app image deploy before production uses them:

- `ChatRoomMember::unread_counts_for_user`
- `ChatMessage::list_recent_for_rooms`

The code now uses lateral per-room scans to avoid scanning/sorting the large shared chat history table for each snapshot.

Expected verification:

```bash
make check
```

LLM agents must not run the full Rust test/lint gates in this repo; the human owner runs them.

## Immediate Next Work

### Enable `pg_stat_statements`

Apply the prepared CNPG Postgres settings:

- `pg_stat_statements.max = "10000"`
- `pg_stat_statements.track = "all"`
- `track_io_timing = "on"`

CloudNativePG's managed-extension support should automatically add `pg_stat_statements`
to `shared_preload_libraries` and create the extension in databases that allow connections.

Then track:

- top total execution time
- top mean execution time
- top calls
- top temp bytes
- top shared/local block reads

This requires a Postgres restart because `shared_preload_libraries` is restart-bound.

### Add adaptive render throttling

Goal:

- active typing/gameplay: 15 FPS
- idle chat: 1-2 FPS
- fully idle/no animation: render on event/input only
- lower visualizer/sidebar animation frequency under load

This is probably the highest-impact path toward 1000 connected users.

### Cap render dimensions

Set a server-side maximum render area, for example:

- width: 160 columns
- height: 50 rows

Clients can still have larger terminals, but render work should not scale unbounded with PTY size.

### Make `service-ssh` horizontally shardable

Minimum viable design:

- On SSH session start, write `session_token -> owning pod` to Redis
- Pair WebSocket checks token ownership and either:
  - routes/proxies to the owning pod, or
  - ingress uses a deterministic sticky key that guarantees same pod
- On session end, remove token ownership

Do not scale `service-ssh` randomly before this exists.

### Add PgBouncer

Before increasing app replicas substantially:

- keep Postgres `max_connections` sane
- move app pools behind PgBouncer transaction pooling
- avoid multiplying deadpool connections by replica count

## 1000-User Target Architecture

Suggested shape:

- `service-web`: 3+ stateless replicas
- `service-ssh`: multiple replicas, each owning many sessions
- Redis: token ownership, presence, pub/sub, lightweight fanout
- PgBouncer: DB connection smoothing
- Postgres: durable state
- Audio: dedicated scalable streaming path, not one small Icecast pod on the app node
- Observability: dashboard for active sessions, per-pod session count, render frames/sec, frame drops, DB pool wait, Postgres top SQL, p95 input latency

The goal is not "1000 pods". The goal is "N SSH pods, each owning a shard of sessions".

## Load-Test Plan

Do not jump straight to 1000.

Stages:

1. 100 concurrent SSH sessions
2. 250 concurrent SSH sessions
3. 500 concurrent SSH sessions
4. 1000 concurrent SSH sessions

For each stage, record:

- service-ssh CPU/memory
- render frame drops
- input latency
- DB pool wait
- Postgres CPU/memory
- Postgres query latency from `pg_stat_statements`
- Icecast listeners and dropped clients
- node CPU/memory

Stop conditions:

- p95 input latency becomes noticeably bad
- frame drops climb steadily
- DB pool wait approaches the 5 second deadpool wait timeout
- Postgres write endpoint flaps
- node memory pressure appears
- Icecast reaches listener cap

## Current Go/No-Go For HN

Current state is safer than before the investigation:

- SSH cap is explicitly 1000
- service-ssh has more CPU headroom
- Postgres has more memory headroom
- Icecast can accept 300 clients
- web stream proxy no longer loops through public audio ingress
- two major chat snapshot queries were optimized in source; deploy required before production uses them

Residual risk remains:

- single-node cluster
- single `service-ssh` pod for real session ownership
- render loop still likely dominates at high concurrency
- no `pg_stat_statements` yet
- no PgBouncer yet
- no horizontal `service-ssh` sharding yet

For a post that may bring about 100 active users, this is much better. For 1000 active terminal users, the required next projects are adaptive rendering and shardable `service-ssh`.
