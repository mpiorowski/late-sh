# late.sh Scale Notes

Last updated: 2026-07-22 (post-OOM investigation: per-session render cost measured, output-budget guard shipped, memory limit 8 GiB, adaptive-render design drafted)

This document records the current production capacity posture, what was discovered during the HN-spike investigations (June 2026 and the 2026-07-22 OOM, see CONTEXT.md §10.5), the DB query findings, and the roadmap toward roughly 1000 concurrent users.

## Current Infra Status

Cluster shape:

- Single RKE2 node: `server-1`
- Node capacity observed: 8 CPU, about 15.6 GiB memory
- Node usage at about 80 concurrent sessions (2026-07-22): about 77% CPU, 43% memory
- All core app workloads currently run on the single node

Application deployments:

- `service-ssh`: 1 replica
  - SSH TUI server and HTTP API
  - Ports: 2222 SSH, 4000 API
  - Current Terraform/live CPU limit: 8 CPU
  - Current Terraform/live memory limit: 8 GiB, request 2 GiB (raised from 4 GiB / 512 MiB during the 2026-07-22 OOM incident)
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

Applied 2026-07-22 (OOM incident):

- Raised `service-ssh` memory limit from `4Gi` to `8Gi` and request from `512Mi` to `2Gi`, first via in-place pod resize (Kubernetes 1.34 `kubectl patch --subresource resize`, zero restart, zero dropped sessions), then persisted in Terraform
- Shipped the SSH output-budget guard and pair-WS hardening (see Pain Point 1 and CONTEXT.md §10.5)
- In-place resize caveat: it patches only the running pod; the Deployment template must come from Terraform or the next rollout reverts the limit

## Biggest Pain Points

### 1. Render/tick CPU is the primary 1000-user blocker

Each SSH/browser TUI session owns an `App`.

The SSH render loop and browser tunnel world tick run every 66 ms, roughly 15 FPS. At 1000 connected users, that is about 15,000 ticks/renders per second before considering input, chat, games, audio visualization, or room events.

This is likely the true baseline killer for "1000 connected and mostly idle" users.

Measured 2026-07-22 during the HN surge: about 59 millicores per session at the 15 FPS floor (4.7 cores for 80 sessions), which saturates the 8-core node at roughly 100-110 concurrent sessions. The June estimate held; this is now a measured ceiling, not a prediction.

The render loop is also the memory failure mode, not just CPU. The 2026-07-22 OOM (full writeup in CONTEXT.md §10.5) was frames rendered at 15-66 FPS into russh's uncapped per-channel output queue for clients that had stopped reading: the send timeout only observes russh's event queue, not delivery, so a stalled client silently pinned 1.4+ GiB until keepalive reaped it. Shipped fix: a per-session `OutputBudget` in `late-ssh/src/ssh.rs` tracks bytes handed to russh versus window credit returned by `Handler::window_adjusted`; over 32 MB outstanding the render loop pauses, and 30 s of sustained stall disconnects the session. Metrics: `late_ssh_render_stall_{skips,disconnects}_total`.

Pain multipliers:

- Large terminals. Logs showed clients with PTYs as large as about 283x72.
- Animated/live panels: visualizer, clocks, aquarium, splash, timers, games, and other tick-driven UI.
- Browser tunnel sessions also render at the same world tick.
- Every hot chat event can wake many users and trigger rendering.
- Per-frame constant factors found in the 2026-07-22 audit: the Clubhouse renderer heap-allocates one `String` per visible map cell per frame (about 9,200 tiny allocations per frame on the landing screen where idle users sit), and every render of every session locks and iterates the global `active_users` map for the online count. Both are cheap targeted fixes.

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

The pair-WS surface itself was hardened 2026-07-22 (per-token cap of 8 sockets, per-IP concurrent-socket cap, bounded control queues with drop-on-full), so it is no longer a memory amplifier, but none of that changes the ownership problem above.

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

2026-07-22 bootstrap audit specifics (none was the OOM cause, all are burst multipliers): the 15-20 query bootstrap fan-out per new session has no concurrency limiter (only chat reads share an 8-permit semaphore); the nonogram library (about 1-3 MB) is deep-cloned per session instead of Arc-shared; aquarium creature/world assets are re-parsed from KDL on every session start; and `next_available_username` + `User::create` race under same-name connect storms, rejecting auth with no backoff (the `idx_users_username_lower` error loop seen in prod logs).

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

Design (drafted 2026-07-22): deadline-driven rendering instead of a fixed tick. The key realization is that the refresh rate is not one global number; it is the minimum of the next moments anything visible actually changes. Two mechanisms, one of which already exists:

- Push for unpredictable changes. Input, resize, chat events, and pair-WS viz frames already wake the render loop through `RenderSignal` (dirty + notify). Nothing new needed; typing latency stays at the existing 15 ms input path.
- Pull for predictable changes. Each animated subsystem exposes a pure `next_frame_at() -> Option<Instant>` getter: the clock answers "next second boundary", bonsai sway "+500 ms", aquarium "+250 ms", a static screen `None`. No channels, no tasks per animation. After each render the loop asks every visible subsystem, takes the minimum, and replaces the fixed 66 ms interval with `sleep_until(min_deadline)` in the same `select!`.

A session's render rate then automatically equals the rate of the fastest thing visible on its screen: full-screen game 15 FPS, idle chat with a clock 1 FPS. Animations keep their native cadence; nothing is dragged up to a global floor.

Layered on top:

- Idle demotion: no input for a few minutes (AFK tracking exists) clamps all animation deadlines to at least 500 ms; the first keystroke restores full cadence instantly via the notify path.
- Load governor (cheap immediate lever, an afternoon of work): a global session-count atomic stretches `WORLD_TICK_INTERVAL` 66 -> 100 -> 133 ms under load, degrading smoothness gracefully instead of saturating the node.
- Visualizer policy: it is already event-fed from the pair WS, so render it on frame arrival; sessions with no paired client should not animate it, and the procedural fallback can run at 4 FPS.
- Constant-factor fixes from the 2026-07-22 audit (Clubhouse per-cell `String` allocations, per-render `active_users` lock) make each remaining frame several times cheaper on the landing screen.

Rough impact: mostly-idle sessions drop from 15 renders/s to 1-2 and each render gets cheaper, which is the order-of-magnitude that turns "100 users = 92% of the node" into "1000 users = a few sharded pods".

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
