# late.sh Scale Notes

Last updated: 2026-08-18, second entry (7-day production health check, read-only. The 2026-07-26 DB fixes are now **verified in prod**, not projected: every pre-fix query shape is idle and total live DB load is about 1.9% of one core, so the chat poll is no longer the binding constraint on the 1000-user target. The largest remaining DB item is `list_discover_public_topic_rooms`, which **regressed to 771 ms mean** and turns out to be a stale-visibility-map problem rather than a query-shape problem. New sections: "Live Production Baseline (2026-08-18)" and "Observability Gaps". Previously 2026-08-18, first entry: added Pain Point 8: stream egress is the first cost that scales with *viewers* rather than sessions, the go-live publish ceiling is now pinned in code, and the OBS/WHIP path is still uncapped by construction. Previously 2026-08-06: SCALE.md is now the single home for performance findings: absorbed root CONTEXT.md's §8.5 input-lag notes into Pain Point 1 and the discover-room CNPG CPU-saturation observation into DB Hot Queries; root context keeps only current-state contracts and routes perf here. Next infra step is unchanged: add a second cluster node and move everything except `service-ssh` off `server-1`)

This document records the current production capacity posture, what was discovered during the HN-spike investigations (June 2026 and the 2026-07-22 OOM, see CONTEXT.md §10.5), the DB query findings, the shipped render-cost program, and the roadmap toward roughly 1000 concurrent users.

## Current Infra Status

Cluster shape:

- Single RKE2 node: `server-1` (Kubernetes v1.34.4+rke2r1, Debian 13)
- Node capacity observed: 8 CPU, about 15.6 GiB memory
- Node usage at about 60 concurrent sessions (2026-07-24, post-render-cost-program): about 37% CPU, 46% memory. For contrast, the pre-program reading at 80 sessions was about 77% CPU, 43% memory
- Node usage 2026-08-18 at 38 sessions: 23% CPU, 53% memory. Scheduler requests are at 56% CPU / 56% memory; limits are overcommitted to 295% CPU / 206% memory. The overcommit is normal for this shape but means a synchronized spike across pods has no referee
- Node disk: 52% used as of 2026-08-18, down from the 75% recorded 2026-07-24 (something reclaimed, most likely containerd image GC). `DiskPressure`, `MemoryPressure`, and `PIDPressure` all `False`
- All core app workloads currently run on the single node
- Storage: every PVC uses the `local-path` (hostPath) provisioner, so any pod with a volume is pinned to the node that holds its data. This matters for any node move (door-game saves, music data, Postgres).

Application deployments:

- `service-ssh`: 1 replica
  - SSH TUI server and HTTP API
  - Ports: 2222 SSH, 4000 API
  - Current Terraform/live CPU limit: 8 CPU
  - Current Terraform/live memory limit: 8 GiB, request 2 GiB (raised from 4 GiB / 512 MiB during the 2026-07-22 OOM incident)
  - Current live `max_conns_global` (prod profile, late-ssh/src/config.rs): 1000
  - `termination_grace_period_seconds`: 21600, so old pods can linger for up to 6 hours while sessions drain
- `service-web`: 1 replica
  - Web pages and `/stream` proxy
  - Current live `audio_base_url` (prod profile, late-web/src/config.rs): `http://icecast-sv:8000`
  - Public browser users still reach `/stream` through `https://late.sh/stream`; only the web pod's upstream fetch is internal
- `icecast`: 1 replica
  - Current Terraform/live client limit: 300
  - Current Terraform/live resources: request `100m/128Mi`, limit `500m/512Mi`
- `liquidsoap`: 1 replica
  - Encodes local playlist mounts for Icecast
- `livekit`: 1 replica, `host_network: true`
  - Voice SFU and `/golive` video fan-out
  - Host ports: 7881 TCP and 7882 UDP (RTC), 3478 UDP and 5349 TCP (TURN)
  - Current Terraform limit: 1 CPU, 1 GiB. Cheap by design: it forwards packets, it does not transcode
  - No `limit:` block in the server config, so LiveKit's node defaults apply (`num_tracks` 400/CPU up to 8000, `bytes_per_sec` 1 GB/s). Both are far above what the node's NIC can serve, so LiveKit will never be the thing that says no. See Pain Point 8
- `livekit-ingress`: 1 replica, `host_network: true`
  - WHIP ingest for `/golive obs`, transcoding disabled (packet forwarder)
- `redis`: 1 replica (`valkey/valkey:8-alpine`, service name `redis-sv`)
  - LiveKit <-> ingress message bus only. The LiveKit server refuses Ingress API calls without it. Not application state, not a cache
- `postgres`: CloudNativePG, 2 instances
  - Primary: `postgres-1`
  - Current Terraform/live memory limit: 4 GiB
  - `max_connections`: 100
  - `shared_buffers`: 256 MB

Public endpoints still required:

- `late.sh`: public web and browser `/stream`
- `api.late.sh`: CLI/webview pair WebSocket and API
- `audio.late.sh`: direct public Icecast path, especially for CLI/local audio
- `ssh late.sh`: public SSH ingress

Internal endpoints:

- `service-web -> icecast-sv:8000` for upstream `/stream` proxying
- `service-ssh/service-web -> postgres-rw:5432`

## Recent Emergency Headroom Changes

Applied in Terraform and live Kubernetes:

- Raised `service-ssh` CPU limit from `4000m` to `8000m`
- Set `max_conns_global` to `1000` in the prod profile (late-ssh/src/config.rs)
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

### 1. Render/tick CPU: was the primary 1000-user blocker, fixed in code and verified in prod (2026-07-24, re-confirmed 2026-08-18). Still the scaling unit

The before-picture, kept as the baseline: the SSH render loop and browser tunnel world tick ran every 66 ms (roughly 15 FPS) for every session regardless of activity. Measured 2026-07-22 during the HN surge: about 59 millicores per session at the 15 FPS floor (4.7 cores for 80 sessions), saturating the 8-core node at roughly 100-110 concurrent sessions.

The render-cost program (see its own section below) removed both the fixed tick and the unconditional draw:

- A dirty gate skips `terminal.draw()` entirely on clean frames; an idle session with the sidebar hidden settles to about 1 render/min.
- The fixed 66 ms interval is replaced by an adaptive wake deadline (66 ms hot to 500 ms idle floor); an idle session costs 2 cheap channel-drain ticks/sec instead of 15 full renders/sec.
- The per-frame constant factors from the 2026-07-22 audit are fixed (run-length clubhouse spans instead of per-cell `String`s, 1 Hz presence cache instead of a per-render `active_users` lock).
- The floor by product decision: a session with the right sidebar visible paints at ~7.5 fps and never settles fully clean. Two panels hold that edge: the bonsai sway (always) and the ambient equalizer (only while a client is paired and unmuted).

The memory failure mode is also closed: the 2026-07-22 OOM (full writeup in CONTEXT.md §10.5) was frames rendered into russh's uncapped per-channel output queue for clients that had stopped reading. Shipped fix: a per-session `OutputBudget` in `late-ssh/src/ssh.rs`; over 32 MB outstanding the render loop pauses, and 30 s of sustained stall disconnects the session. Metrics: `late_ssh_render_stall_{skips,disconnects}_total`.

Measured in prod 2026-07-24 (v0.41.0, single `service-ssh` pod, 60 live sessions):

- CPU about 1591 millicores, so about **26.5 millicores/session** (down from the pre-program 59 floor; a 32h A/B against a still-draining v0.40.7 pod read 47 mcores/session for the old code on the same node at the same time).
- Memory about 1085 MiB, so about **18 MiB/session**.
- Render loop: about 5.3 draws/session/sec, **~20% clean-skip ratio**. Sessions sit in the `ANIM_HALF_TICK` (~7.5 fps) tier, not the 500 ms idle floor, because almost everyone keeps the right sidebar visible and the ambient eq paints there. The documented "1 render/min idle" case is real but rare in the wild.
- Stall guard never fired (`late_ssh_render_stall_*` has no series); 0 frame drops on this pod.

Re-measured in prod 2026-08-18 (7-day window, 38 sessions live, peak 43):

- CPU **24.9 millicores/session**, confirming the 26.5 figure holds. This is the number that matters and it has not drifted.
- Memory ~18 MiB/session at steady state over a ~1 GiB per-pod baseline, also confirming.
- **Zero OOM kills cluster-wide over 7 days** (`container_oom_events_total`).
- Two numbers moved the wrong way: **7.15 draws/session/sec** (was 5.3) and a **15% clean-skip ratio** (was ~20%). Small, and consistent with everyone sitting in the `ANIM_HALF_TICK` sidebar tier as documented, but it is movement against the baseline rather than sampling noise. Re-check after the next render-touching change; if it keeps climbing, the eq-to-quarter-edge knob is the response.
- Per-pod memory shows transient bursts to 2.7-3.5 GiB against the ~1 GiB baseline, but only **3 ten-minute samples in the whole week**, and they do not correlate with session count. Consistent with `OutputBudget` holding frames for stalled clients (32 MB × N sessions) and then releasing. Not a leak: the baseline is flat across a 2-day pod lifetime.

Re-derived ceiling: at ~26.5 mcores/session, `service-ssh` reaches about **260 sessions on the current shared node and about 300 on a dedicated 8-core node** (memory ceiling is ~450/pod, so it stays CPU-bound). Old ceiling was 100-110. Against the 2026-08-18 peak of 43 concurrent sessions that is roughly **7x headroom**, and no capacity event occurred in the window. The named knob if this reads expensive: move both the eq and the sway to the quarter edge (~3.8 fps), which roughly doubles the ceiling again. Not gating the render edge on audio state: the eq's *content* is now pairing-aware (`EqState`), but the `anim_half` edge itself must stay unconditional while the bonsai sway rides it.

**The input queue does overflow in prod, and it is fine.** 2,306 `session input queue full; dropping inbound ssh input` warnings (`ssh.rs:1299`, `queue_cap=256`) over the 7 days to 2026-08-18. Before reading that as input-latency regression: the events arrive **sub-millisecond apart** in 5 tight bursts (one hour held 1,511 of them), which is a paste bomb or an escape-sequence flood filling a 256-slot queue instantly, not a user losing keystrokes while typing. This is the bounded queue doing its job instead of growing without limit. Recorded so the next person who greps for it does not go hunting a latency bug. If it ever appears spread evenly across a session instead of bunched, that *would* be the real thing.

Input latency (moved here from the retired CONTEXT.md §8.5): keystrokes land in a per-session bounded queue and the render loop wakes on input, so ordinary keystrokes never wait on the app mutex before being queued. The remaining risk under high fan-out is that `render_once` still holds the app lock across synchronous `app.tick()` + `app.render()`, so a slow tick delays that session's input-to-frame path; the input queue closed the cadence gap, not the lock-held-across-tick stall. Chat-specific row-cache, snapshot, unread-count, and scoped-loading performance notes live in `late-ssh/src/app/chat/CONTEXT.md`.

### 2. `service-ssh` cannot safely scale horizontally yet

Current `service-ssh` has in-memory ownership for:

- SSH session registry
- paired client registry
- active user presence
- app state per session
- room/game managers
- artboard state
- activity fanout

Scaling `service-ssh` to multiple replicas without routing pair WebSockets to the owning pod will break pairing. If SSH lands on pod A and `/api/ws/pair` lands on pod B, pod B does not know that token/session.

The pair-WS surface itself was hardened 2026-07-22 (per-token cap of 8 sockets, per-IP concurrent-socket cap, bounded control queues with drop-on-full), so it is no longer a memory amplifier, but none of that changes the ownership problem above.

For horizontal scaling, one SSH session must stay on the same pod for its lifetime. That does not mean one pod per user. It means each pod owns many sessions, and pair traffic routes to the session owner.

Target shape for 1000 users: a handful of SSH pods after the render-cost program, not 1000 pods. How many depends on the prod re-measurement above.

### 3. Connect storms hit DB and service startup paths

Per-user connect/snapshot work includes:

- user lookup/create
- chat room list
- last message timestamps
- unread counts
- friends/profile/metadata
- notifications
- room/game data

The app is not continuously polling the DB for chat *messages*; message flow is event-driven and snapshots carry empty message vectors (`app/chat/svc.rs`, the `(chat, Vec::new())` map). It does continuously poll for chat *metadata*: see Pain Point 7, which is the single largest DB consumer in the system. Connect storms and room switches hit DB-heavy paths on top of that.

2026-07-22 bootstrap audit specifics (none was the OOM cause, all are burst multipliers):

- The 15-20 query bootstrap fan-out per new session has no concurrency limiter (only chat reads share an 8-permit semaphore). Still open.
- Aquarium creature/world assets are re-parsed from KDL on every session start. Still open.
- `next_available_username` + `User::create` race under same-name connect storms, rejecting auth with no backoff (the `idx_users_username_lower` error loop seen in prod logs). FIXED: new users get a randomly generated curated username (no longer derived from the SSH login name), and `ensure_user` retries on the `idx_users_username_lower` unique violation (bounded to 5 attempts, then auth is rejected instead of spinning). Follow-up: `next_generated_username` scans all usernames (`SELECT username FROM users`) per new signup and per retry; fine at current scale but O(users) per signup during a new-user surge. Cheaper shape is a single indexed probe against `idx_users_username_lower` first, full scan only on the rare collision.
  - Open as of 2026-07-24: `idx_users_username_lower` violations still appear in prod at about 1/hour of isolated singles, plus one 59-error burst from a single user retrying a rename to a taken name. The rename path is handled correctly (`ProfileService::edit_profile` publishes a user-visible error banner, `app/profile/svc.rs:310`), so that burst is only log noise. The isolated singles are unexplained and worth one pass over who raises them, signup or rename.
- The nonogram library deep-clone per session (about 1-3 MB) is FIXED: Arc-shared since render-cost phase 0.

### 4. Audio capacity is still single-pod

Icecast now allows 300 clients, but it is still one pod. The second-node move (below) gives it CPU/memory headroom away from `service-ssh`, but for 1000 audio listeners a dedicated streaming strategy is still needed:

- dedicated Icecast host with real bandwidth headroom
- CDN/edge-compatible stream distribution
- multiple relays
- or browser/client behavior that avoids duplicating streams where possible

**RESOLVED as of 2026-08-18.** The separate open bug seen 2026-07-24 was: the `service-web -> icecast-sv:8000` upstream fetch dropping every 10 to 50 minutes (`upstream stream ended; injecting silence until reconnect` and `ConnectionReset` in `late-web/src/pages/stream.rs`), giving browser `/stream` listeners audible silence gaps. A 7-day log search on 2026-08-18 found **zero** `upstream stream ended` events. `service-web` currently reports 0 restarts and 11 readiness blips over the window (the 8 restarts in the window belong to a pod that no longer exists). Nothing was knowingly changed to fix this, so if the symptom returns, the 2026-07-24 leads still apply: Icecast client timeout/burst settings and the proxy's reconnect behavior.

### 5. Postgres connections are bounded but not pooled externally

App pools are currently per process through deadpool, with `max_pool_size: 16` in both prod profiles (`late-ssh/src/config.rs`, `late-web/src/config.rs`).

Postgres `max_connections=100`. This is acceptable while replicas are low, but scaling app replicas will multiply pools. PgBouncer should be introduced before many app replicas.

### 6. Feed writes fan out over the entire users table (correctness, not capacity)

Found 2026-07-24 during a health check. Re-scoped 2026-07-26: measured against `pg_stat_statements`, all three fan-outs together are **307 s of DB time, 0.5% of the total**, because the per-user queries are sub-millisecond counts over small tables. The write burst is real and spikes Postgres CPU while it runs, but in aggregate this is not the capacity problem it was billed as. What it *is* is a correctness bug: every burst overflows the broadcast channel and every session silently loses its feed events.

There are **three identical copies** of this loop, not one:

- `WorkService::publish_unread_updates_for_all` (`app/chat/work/svc.rs:383`), called at `svc.rs:165`, `:237`, `:300`
- `ShowcaseService::publish_unread_updates_for_all` (`app/chat/showcase/svc.rs:340`), called at `:149`, `:209`, `:262`
- `ArticleService::publish_unread_updates_for_all` (`app/chat/news/svc.rs:168`), called at `:243`, `:383`

Each loops over `User::list_ids` (every row in `users`), runs two sequential queries per user, and publishes one or two events onto a broadcast channel (capacity 256 / 256 / 512).

The receiving side makes it worse: all three states log the `Lagged` error and `break` out of the drain with no recovery (`work/state.rs:535`, `showcase/state.rs:483`, `news/state.rs:406`). Compare `ChatState`, which handles `Lagged` by reloading the visible room tail (`chat/state.rs:3796`). So dropped feed events are simply gone until an unrelated refresh happens to fix the count.

At the 2026-07-24 table size of 14,156 users that is about 28,000 sequential round trips on one held pool connection per write, feeding a channel created with capacity 256 (`svc.rs:70`) that has one receiver per live session.

Observed cost per write, at 59 sessions:

- Postgres CPU 0.15 -> 0.45 cores (3x) for the duration
- the burst runs 1 to 5 minutes wall clock
- 800 to 950 `failed to receive work event e=channel lagged by ~230` errors/min (`app/chat/work/state.rs:535`), i.e. every session's receiver overflows and drops its work events, leaving stale unread badges until something else refreshes them
- observed at 15:25-15:29, 15:40-15:41, 20:20, and 20:22 on 2026-07-24, so several times an hour

Re-confirmed live 2026-08-18, still unfixed, exact same signature: `failed to receive work event, e=channel lagged by 450` at `work/state.rs:535`. Counts over the 7 days to 2026-08-18, split across all three copies:

| receiver | errors / 7d |
|---|---|
| `failed to receive work event` (`work/state.rs:535`) | 6,153 |
| `failed to receive showcase event` (`showcase/state.rs:483`) | 1,651 |
| `failed to receive article event` (`news/state.rs:406`, both sites) | 1,422 |
| **total feed events dropped** | **~9,226** |

Daily rate ran 614 to 3,723, so this is happening continuously, not in rare bursts. It reads lower than the 800-950/min figure from 2026-07-24 only because concurrency was lower this week (22 average vs 59), and it scales with sessions × registered users, so both directions make it worse.

Both halves scale with total registered users, not with concurrent sessions, so this gets worse with signups even if concurrency stays flat. It also gets worse per replica: every `service-ssh` pod would run its own copy of the loop.

Fix direction (not yet implemented): the per-user unread count does not need a full-table scan. Either compute it only for currently connected sessions, or publish one broad `FeedChanged` event and let each session recompute its own count on receipt. Either shape removes the O(users) query loop and the O(users) channel sends at the same time. Whatever ships must land in all three services, and the `Lagged` arms should recover rather than `break`. Raising the channel capacity alone is not a fix; it only hides the drops.

### 7. The chat snapshot poll is the largest DB consumer, but it is no longer a scaling threat

**Status changed 2026-08-18.** This was billed as "the top code-level scaling problem on this list" and "the binding DB constraint on the 1000-user target". Measured against a live differential, it is neither. It is still the largest single share of DB work, but the total is now so small that the share does not matter:

- The chat snapshot bundle is about **80% of live DB execution time**, and live DB execution time is about **1.9% of one core** at 38 sessions.
- Cost per session: **0.40 ms of DB time per session per second**. Extrapolated to 1000 sessions that is roughly **0.4 cores**. Postgres has a 4 GiB limit and averaged 0.086 cores over the week.
- Per-pass cost is about 4.0 ms of DB time across 5 queries, running at the expected 3.5 passes/sec for 38 sessions on a 10 s interval.

So the remaining work in this section is worth doing for tidiness and for latency, not because it gates the 1000-user target. **Do not plan the 1000-user roadmap around this item any more.** The render/tick CPU in Pain Point 1 is the scaling unit; this is overhead.

The historical measurement, kept for context: measured 2026-07-26 (first full `pg_stat_statements` ranking, see DB Cost Ranking below), **50-68% of all database execution time**, with `ChatRoomMember::unread_counts_for_user` alone at **43.0%**.

`ChatService::ensure_refresh_scheduler` (`app/chat/svc.rs:1255`) runs one process-global task that rebuilds a full `ChatSnapshot` for **every connected session every 10 seconds** (`CHAT_REFRESH_INTERVAL`, `svc.rs:64`), on top of on-demand rebuilds at room switch and `request_list`.

Each rebuild was 9 sequential round trips on one held pool connection, 11 when a room has a live poll:

1. `ChatRoom::list_for_user`
2. `VoiceChannel::enabled_for_chat_rooms`
3. `ChatMessage::last_message_at_for_rooms` (LATERAL, one index scan per joined room)
4. `chat_poll::list_active_polls_for_rooms` (1 query, +2 more if any poll is live)
5. `ChatRoomMember::unread_counts_for_user` (LATERAL `COUNT(*)`, one per membership row)
6. `User::friend_user_ids` -> `SELECT settings FROM users WHERE id = $1`
7. `User::list_chat_author_metadata`
8. `User::ignored_user_ids` -> **the identical `SELECT settings` query a second time** (`late-core/src/models/user.rs:985`)
9. `ChatRoom::owner_ids_for_rooms`

It is now **5 queries in 2 pipelined rounds** (7 queries when a poll is live). See "Fixed 2026-07-26" below.

Why it is so expensive: **`chat_room_members` holds 444,994 rows over 583 rooms and 14,347 users, averaging 31 rooms per user and peaking at 154** (2026-07-26). Auto-join means memberships only accumulate.

Growth since that baseline, measured 2026-08-18:

| | 2026-07-26 | 2026-08-18 | change |
|---|---|---|---|
| users | 14,347 | 16,175 | +12.7% |
| rooms | 583 | 755 | +29.5% |
| `chat_room_members` rows | 444,994 | 492,729 | +10.7% |
| avg rooms/user | 31 | 30.5 | flat |
| **max rooms/user** | 154 | **225** | **+46%** |
| memberships never opened | 46% | 41.5% | -4.5pp |
| DB size | 161 MB | 495 MB | 3x |

The number to watch is **max rooms/user, not the average**. The average held flat at 30.5 while the heaviest user gained 46% more rooms, so this is concentration in the tail rather than broad growth, and the tail is exactly what the per-room laterals fan out over. DB size tripling is mostly `chat_messages` (86k to 182k rows) and is not a concern at 495 MB. So queries 3 and 5 fan out over every room a user has ever joined, whether or not it is on screen, and the unread query returns 32.5 rows per call on average. The double `settings` read shows up as 11,070,677 calls, exactly 2.006 per snapshot.

Two structural problems on top of the raw cost:

- **The session loop is sequential.** `refresh_registered_sessions` (`svc.rs:1291`) is a plain `for session in sessions { ... .await }`, so the effective concurrency is 1 and the 8-permit `read_permits` semaphore does nothing on this path. Cycle time is N_sessions × snapshot latency, and `MissedTickBehavior::Skip` means an overrun degrades silently into a continuous back-to-back loop rather than erroring. Unread-badge latency is therefore 10 s plus the session's position in the cycle.
- **The poll is load-bearing for unread badges.** The only writes to `ChatState::unread_counts` are mark-read to 0 (`chat/state.rs:869`), wholesale replace from the snapshot (`:3746`), and remove on room deletion (`:4547`). There is no increment path, so `MessageCreated` events do not move a badge and the DB poll is the only source of truth.

**Where the cost actually was, measured 2026-07-26.** `#lounge` is auto-join with all 14,346 users as members and 127,246 messages, and it is where the activity feed posts. 46% of all memberships have `last_read_at IS NULL`, never opened once, so their count starts at the first message ever posted and walks the whole room. On top of that, the "is this an activity line" test read `users.settings->>'system'` out of JSONB per message, so Postgres sequentially scanned the entire users table and built a 5 MB hash to answer a question with one constant answer. Worst-case single user: 578 ms, 183,914 buffers, 12,613 rows removed by that filter out of 127,260 examined.

Two ideas that did NOT survive measurement, recorded so nobody re-proposes them:

- *Skip rooms with no new activity* (denormalized `chat_rooms.last_message_at`): would skip **1.9%** of memberships, because never-opened rooms always have something newer than "never".
- *Maintain an incremental unread counter per membership*: `#lounge` has 14,346 members, so every message would update 14,346 rows. That is the Pain Point 6 disease, worse.

**Fixed 2026-07-26, part one: the unread count itself.** The lateral count runs under `LIMIT ChatRoomMember::UNREAD_COUNT_CAP` (100) so it walks the index forward from `last_read_at` and stops, and the `users` join is replaced by a UUID compare against the system bot id passed in from `ChatService` (published once at startup by the #lounge feed task). 578 ms to 11.8 ms on the same user. Room ordering only tests `unread > 0` so it is unaffected, and the room tail only ever loads 500 messages anyway.

**Fixed 2026-07-26, part two: the shape of the bundle.** Two changes, no new infrastructure:

- **Three queries merged into one.** `list_for_user`, `last_message_at_for_rooms`, and `unread_counts_for_user` all keyed off the same `chat_room_members` row for the same user, so the user's membership set was index-scanned three times per pass. They are now `ChatRoom::list_for_user_with_state`, one query with two laterals, returning rooms plus both per-room maps. Measured on prod against the heaviest user in the system (157 rooms): **1,452 buffers / 5.2 ms exec / 4.1 ms planning**, against **1,760 buffers / 8.1 ms exec / 6.3 ms planning** for the three it replaces. Both laterals plan as index scans on `idx_chat_messages_room_created`. `unread_counts_for_user` and `last_message_at_for_rooms` were **deleted**, not left beside it: a second copy of that SQL is a drift hazard, and their tests moved onto the merged function.
- **The remaining queries are pipelined, not serial.** `build_chat_snapshot` is now two rounds of `tokio::join!` on one pooled connection: round one is the merged room query plus `friend_and_ignored_user_ids`; round two is voice channels, polls, author metadata, and owners, all of which need the room or friend set from round one. tokio-postgres pipelines concurrent queries on a single connection, so each round costs about one round trip rather than one per query. Same pattern as `late_core::models::leaderboard::fetch_leaderboard_data`. **This buys latency, not server CPU** (Postgres still executes them in order), which is exactly what the sequential session loop below needs.

Net: 9 sequential round trips to 5 queries in 2 pipelined rounds.

**Verified in prod 2026-08-18.** Both halves are confirmed live, not projected:

- Every pre-fix query shape is **completely idle**. A two-snapshot differential over the live system shows 0 calls for all three `unread_counts_for_user` variants, for `last_message_at_for_rooms`, and for the old `list_for_user`; only `list_for_user_with_state` runs, at 3.48 calls/sec. See DB Cost Ranking for why the cumulative table still shows the old shapes at the top.
- The merged query re-explained against the current heaviest user (**225 rooms**, up from the 157 measured when it shipped): **8.5 ms execution, 5.1 ms planning, 859 buffers on the unread lateral**. Was 5.2 ms exec / 4.1 ms planning at 157 rooms, so cost scales slightly worse than linearly with rooms-per-user, as designed. Both laterals still plan as index scans on `idx_chat_messages_room_created`.
- Note that **planning is now 60% of execution time** for this query. It carries 20 parameters and two laterals, so the planner does real work per call. At 3.5 calls/sec that is irrelevant; it would matter if this ever moved onto a hot path, and a prepared-statement cache is the answer if it does, not a query rewrite.

Still open, all much smaller now:

1. **Stop the loop being sequential**, so cycle time cannot exceed the interval at higher session counts. The pipelining above cut per-snapshot latency, which raises the ceiling, but does not remove it.
2. If it ever matters again, the O(1) shape is a per-room message sequence number plus a per-membership read position, so unread is subtraction rather than counting. It needs a migration and a decision about the activity lines, which is why it is not the fix today.

**Do not widen `CHAT_REFRESH_INTERVAL` as a cheap win.** It looks like a free 3x, and it is not: the poll is the only writer of unread badges (see the second structural problem above), so tripling the interval triples worst-case badge latency for every room the user is not looking at. That is the primary signal in the product.

No new infrastructure is needed and none is wanted here. The events already exist and already reach every session; the bug is that the client discards them and re-derives the state from Postgres on a timer. A broker (NATS, Redis pub/sub) would add a hop to a pipeline that already works and would not remove one of those 22M queries.

### 8. Stream egress scales with viewers, not sessions

Recorded 2026-08-18 after an audit prompted by a user question, not by an
incident. Nothing here has hurt yet; the point is to know where the cliff is
before streams get an audience.

Every other pain point on this list scales with concurrent *sessions*. This one
does not. LiveKit is an SFU: a publisher uploads once and the server re-sends
that stream once per subscriber, so node egress is roughly
`publisher_bitrate × viewers`. One streamer with 20 viewers costs the node more
outbound traffic than the entire TUI fleet does. Nothing else in this document
has a multiplier of that shape.

There is no transcoding anywhere in the path (`enable_transcoding: false` at
CreateIngress, `late-ssh/src/app/voice/svc.rs`), which is the right call for a
CPU-bound node: the `livekit` pod stays a packet forwarder on a 1 CPU limit.
The cost lands entirely on bandwidth and on the node's packet path.

**The browser `/golive` path is now capped.** It used to publish at the JS SDK
default, `ScreenSharePresets.h1080fps15` (1080p, 2.5 Mbps, 15 fps), because
`setScreenShareEnabled` was called with no publish options. It now pins
`screenShareEncoding` to 1.5 Mbps at 15 fps with `contentHint: 'detail'`
(`late-web/src/pages/live/golive.html`, asserted in `live_test.rs`). A terminal
share is text, not motion, so the picture is unchanged and every viewer costs
40% less. Simulcast stays on (SDK default), which adds a half-resolution layer
so a weak viewer steps down instead of dropping packets.

**The OBS/WHIP path is uncapped, by construction.** With transcoding off there
is no encoder in the path, so LiveKit can neither clamp the bitrate nor build
simulcast layers. Whatever OBS is configured to send is what every viewer gets,
all or nothing, and a viewer who cannot keep up just loses packets. A streamer
who sets 20 Mbps in OBS is a 20 Mbps per-viewer bill with no server-side
opinion about it. The fix is not to enable transcoding: that puts a per-stream
encoder on the shared node and trades a bandwidth cost we are not yet paying
for CPU that competes with `service-ssh` sessions, which is the actual scaling
unit (Pain Point 1). The cheap version is telling the streamer what to set, in
the OBS handoff modal that already shows the WHIP URL and token
(`late-ssh/src/app/stream/ui.rs`).

**The bill is not the constraint, the node is.** Hetzner EU includes about
20 TB/month with overage around EUR 1/TB. At the pinned 1.5 Mbps, ten viewers
for an hour is about 6.75 GB, so roughly 3000 viewer-hours before the first
euro. What binds first is the node's packet handling: LiveKit already logs
`UDP receive buffer is too small for a production set-up {current: 425984,
suggested: 5000000}` at every boot, and a screen share fanned to several
subscribers shows nack ratios of 0.4-0.5 in the congestion logs, on *every*
viewer rather than one bad link. That is a host sysctl
(`net.core.rmem_max`/`rmem_default`) and it cannot come from the pod, since a
hostNetwork pod may not set `net.*` sysctls. Full note in
`late-ssh/src/app/stream/CONTEXT.md` §7.

Open, in the order they would start to matter:

1. **No stream telemetry.** There is no `record_stream_*` metric family, so
   watcher counts and peaks are readable only from logs and registry state.
   The first thing to build if streams get regular viewers, because everything
   below is unmeasurable without it. Also noted in the stream context §7.
2. **The UDP receive buffer.** Already producing symptoms at current traffic.
   Host-level fix, no home in this repo today (the RKE2 script is one-shot
   bootstrap, not a reconciler).
3. **No OBS bitrate guidance or enforcement.** Copy in the handoff modal is the
   zero-cost version; detect-and-warn off the existing `poll_obs_publishers`
   sweep is the next step up.
4. **No `limit:` block in the LiveKit server config.** Worth adding as a node
   circuit breaker, but be clear about what it buys: `bytes_per_sec` is a node
   limit that refuses new tracks once saturated, not a per-stream cap, so it
   protects the box without stopping one streamer from hogging it.
5. **LiveKit shares `server-1` with `service-ssh`.** It is on the move list for
   the second node (Immediate Next Work item 3), which matters more once media
   traffic is real: today the fan-out competes for the same NIC as every SSH
   session.

Measured 2026-08-18: the `livekit` pod averaged 4.6% of its 1 CPU limit but
**peaked at 1.16 cores, so it touched and briefly exceeded its own ceiling**.
This section predicted LiveKit "will never be the thing that says no"; on CPU
that is now marginally false at current traffic, though the average says there
is no real pressure yet. It restarted twice in the window. Worth a look before
streams get an audience, since the 1 CPU limit was sized on the assumption that
a packet forwarder never approaches it.

## Render-Cost Program (shipped 2026-07-22/23)

Consolidated from RENDER_COST.md (deleted). The canonical description of the gate contract and the adaptive tick lives in CONTEXT.md §2.6; this section keeps the scale-relevant summary, the rules that must not be violated, and the open follow-ups.

### What shipped

- **Phase 0, per-frame constant factors (2026-07-22):** counter-validated chat row caches (`ChatRowsVersions`, see `late-ssh/src/app/chat/CONTEXT.md`), presence cached at 1 Hz, per-session `targeted_event_rx` for single-recipient chat events, 64 KB BufWriter frame path, run-length clubhouse spans, Arc-shared nonogram library, and the `OutputBudget` guard (32 MB unacked pause, 30 s disconnect).
- **Phase 1, dirty gate (2026-07-22):** `App::tick() -> bool`; `render_once` (ssh.rs) computes `changed = signal.dirty.swap(false) | input drained | app.tick()` and skips `terminal.draw()` entirely when clean (ratatui's diff does not advance on skip, no forced repaint needed).
- **Phase 1 tightening + domain sweep (2026-07-22/23):** every domain state exposes `tick() -> bool` under the dirty contract ("rule of three", CONTEXT.md §2.6): chat snapshot drains report real change via full compares, modals are event-driven, house tables and door games report their watch peeks and go quiet between rounds, the ultimate cooldown became minute-granularity riding the per-minute global frame. The FFT audio visualizer was replaced by a stateless synthetic ambient equalizer (`viz::render_eq`), so no audio state drives rendering at all.
- **Phase 2, adaptive world tick (2026-07-23):** the fixed 66 ms interval is gone. Each render pass returns `App::wake_hint() -> Duration` and the loop sleeps exactly that long unless input or a `RenderSignal` wake lands first. Tiers (`app/tick.rs` consts): `HOT_TICK` 66 ms (splash, 2 s post-input window, active ultimate effect, house tables, open arcade game, bonsai modals), `ANIM_HALF_TICK` 132 ms (Clubhouse, visible sidebar, pet), `ANIM_QUARTER_TICK` 264 ms (aquarium surfaces), `IDLE_TICK` 500 ms floor. Floor ticks only drain channels; worst-case latency for an unprompted event while idle is one floor interval. Enablers: `marquee_tick` is wall-clock-derived, every frame edge is a period-index compare, and bonsai passive growth was removed entirely (product decision) so no wall-time accumulator depends on tick cadence.

Result: idle sessions cost 2 cheap clean ticks/sec and about 1 render/min. A sidebar-visible session holds ~7.5 fps (about 37 draws per 5 s) by product decision: the bonsai sway animates unconditionally, and the ambient eq animates while a client is paired and unmuted.

Gating the `anim_half` sidebar edge on a paired client is NOT the knob, and would freeze the bonsai sway for the plain-`ssh` majority. A correct gate is `sidebar_visible && (eq_animating || bonsai_panel_enabled)`, and since Bonsai ships enabled by default, it would buy almost nothing. The real knob if this reads expensive in prod is moving both to the quarter edge.

### Design rules (do not violate)

- PROVE-CLEAN, NOT PROVE-DIRTY. Anything uncertain reports changed. A spurious frame costs nothing; a wrong "clean" freezes UI.
- The gate lives in `render_once` in ssh.rs, the only render loop (the browser `/play` demo and its `web_tunnel.rs` mirror loop were removed entirely on 2026-07-23; the loops used to gate identically, change-both).
- Peek receivers BEFORE draining (`has_changed()` on watches, `!is_empty()` on mpsc/broadcast). Exception: fixed-cadence publishers (chat snapshot, audio queue) report real change from the drain itself. A watch that is only `borrow()`ed at render must be marked seen (`borrow_and_update`) by whoever peeks it, or the peek latches dirty forever.
- Nothing paints at full rate. The ambient eq, pet, bonsai sway, and clubhouse ambience share the half-rate edge (`anim_half`, ~7.5 fps); aquarium steps on the quarter edge (~3.8 fps); everything else is slow/static. Marquee moves 3 columns/sec in 1 s steps so speed costs no extra frames.
- `is_multiple_of` on the tick counter is a bug pattern under sparse ticking; every edge compares its period index against the previous tick's.
- A blanket `changed = true` or fixed cadence needs a written justification at its call site (the current survivors are listed in CONTEXT.md §2.6).

### Metrics and observability

- `late_ssh_renders_total{reason=input|tick}` vs `late_ssh_renders_skipped_clean_total` (metrics.rs, `RenderReason` closed enum) observe the skip ratio in prod.
- Grafana: "Rendering" row in `monitoring/dashboards/observability.json` (render rate, clean-skip ratio, draws per session, stall guard).
- Per-session debug stats: the render loop logs drawn vs skipped_clean every 5 s at debug level; run with `RUST_LOG=late_ssh=debug` to feel the skip ratio locally.

### Test gotchas (for anyone touching the gate)

- Any test driving `tick()` without `render()` leaves `pending_terminal_commands` queued and the gate correctly stays dirty; mirror the loop with a drain_frame (render + take commands). See `app/tick_test.rs`.
- The settle tests (`idle_ticks_settle_clean_and_chat_send_marks_changed`, `open_settings_modal_settles_clean`) loop to 30 consecutive clean ticks; their failure panic dumps a state snapshot; extend that dump when debugging new dirt sources.
- Never raw cargo test; `make test-llm ARGS="-p late-ssh -E 'test(...)'"`.

### Open follow-ups (all optional tightening)

- [ ] HouseTable hot tier is coarse (screen == HouseTable). Per-game "round running" predicates would let a quiet table idle.
- [ ] Artboard screen rides the 500 ms floor; remote strokes lag up to 0.5 s. Bump its tier while on-screen if it feels laggy.
- [ ] Push wakes for chat's targeted mpsc would cut the ≤500 ms idle chat latency to instant; needs the sender side to hold the RenderSignal.
- [ ] Load governor (raise the idle floor when node CPU is high) not built.
- [ ] Viz pipeline removal: `SessionMessage::Viz` frames are dropped on arrival, but the WS/CLI/late-core pipeline still produces and ships them; remove end to end.

### Revert knobs

Drop the `anim_half`/`anim_quarter` gates in tick.rs and restore the aquarium's 220 ms self-throttle + draw-time reef tick to get pre-program animation behavior back. The dirty gate and adaptive deadline have no single revert switch; they are the architecture now.

## DB Cost Ranking

> **Read this before trusting the cumulative table.** `pg_stat_statements` was
> **never reset** after the 2026-07-26 deploy, despite this document twice
> instructing it. As of 2026-08-18 the window is 41.9 days old and blends 19
> days of pre-fix data with 23 days of post-fix data. `pg_stat_statements` is
> cumulative and never decays, so **query shapes that stopped running on
> 2026-07-26 still sit at the top of the ranking forever.** Read raw, the table
> says `unread_counts_for_user` is the #1 query in the system at 20.5% of all
> execution time. It has not executed once since July. Use the live differential
> below, or reset the window, but do not read shares off the cumulative table.

Baseline taken 2026-07-26 from a `pg_stat_statements` window opened 2026-07-07 (18.6 days, 59.05M calls, 56,625 s total execution time). Percentages are share of total execution time in that window, before the fixes landed the same day.

| # | Subsystem | Baseline | Calls | Cadence | Status |
|---|---|---|---|---|---|
| 1 | Chat snapshot poll (Pain Point 7) | 50-68% | 22.1M+ | 10 s per session | **fixed, verified 2026-08-18** |
| 2 | Leaderboard refresh loop | 13.1% | 1.1M | 30 s, process-global | **fixed, verified 2026-08-18** |
| 3 | `list_discover_public_topic_rooms` | 5.6% | 6.0k | on demand, 510 ms mean | **open, REGRESSED to 771 ms** |
| 4 | Artboard snapshot reads | 2.9% | 39k | on demand, 158 ms and 624 ms means | open, not worth it |
| 5 | Chat username list scan | 1.9% | 66k | 30 s, process-global | **fixed, verified 2026-08-18** |
| 6 | Feed fan-outs (Pain Point 6) | 0.5% | 3.6M | per feed write | open, correctness, ~9.2k drops/7d |

Notes on reading this table:

- The range on #1 is deploy-generation variants of the same statement: the conservative match gives 50.4%, counting every variant visible in the top 25 gives 67.8%. The unambiguous figure is `unread_counts_for_user` at 43.0% on its own.
- Everything not attributed above is 27.5% spread across the long tail. Nothing in the tail exceeds 0.5%; the largest oddities are `UPDATE rss_entries` (982k calls, 0.32%, the poller writes entries whether or not the content changed) and `INSERT INTO tetris_games` (328k calls, 0.16%).
- #4 is not a missing index. `artboard_snapshots` is 17 rows in 9.9 MB, about 580 KB of JSONB per row, and `board_key` is already uniquely indexed. The time is detoasting the blob. Fixing it means smaller payloads or incremental persistence, not an index.

The shape worth remembering: the two largest items were **timers, not user actions.** Nobody had to do anything for either to run.

### Fixed 2026-07-26

All shipped together, no migration, no schema change, no new infrastructure. Percentages are of the baseline total above.

| Change | Baseline | Expected after |
|---|---|---|
| `unread_counts_for_user`: count capped at 100, per-message `users` join replaced by a UUID compare | 43% | ~1% |
| Leaderboard loop: 30 s to 300 s, skipped entirely with no subscribers | 13.1% | ~1.3% |
| Leaderboard connect refresh (added 2026-07-27, see below) | — | bounded by the same 300 s |
| `list_for_user` + `last_message_at_for_rooms` + `unread_counts_for_user` merged into `ChatRoom::list_for_user_with_state` | 12.6% + the unread row above | ~8% |
| Mention-autocomplete list: read the in-memory `UsernameDirectory` instead of re-scanning `users` | 1.9% | 0 |
| `User::friend_and_ignored_user_ids`: one `SELECT settings` per snapshot instead of two | 0.9% | 0.45% |

Plus one change with no `pg_stat_statements` line of its own: `build_chat_snapshot` now issues its remaining queries as **two pipelined `tokio::join!` rounds instead of a serial chain**, which cuts per-snapshot latency without changing server CPU. That is what raises the ceiling on the sequential session loop (item 3 below).

Roughly **60% of the total workload**, projected.

### Verified 2026-08-18: the live ranking

Method, since the cumulative window is contaminated (see the warning above): two full `pg_stat_statements` snapshots 88 seconds apart, ranked by the *delta*. That measures what is actually executing now regardless of accumulated history. `dealloc` was 0 and 993 of 10,000 statement slots were in use, so nothing had been evicted and the differential is sound. Investigation queries run from the same psql session have been excluded from the totals below.

**Total live DB execution: about 1.9% of one core** at 38 concurrent sessions.

| share | query | calls/s | mean ms |
|---|---|---|---|
| 54.4% | `ChatRoom::list_for_user_with_state` | 3.48 | 2.96 |
| 14.7% | `User::list_chat_author_metadata` | 3.49 | 0.80 |
| 7.0% | leaderboard refresh passes | 0.01 | 12-46 |
| 6.2% | `voice_channels` | 3.48 | 0.34 |
| 3.4% | `INSERT INTO mud_world_states` | 0.07 | 9.36 |
| 2.7% | `owner_ids_for_rooms` | 0.61 | 0.83 |
| 0.8% | `SELECT settings FROM users` | 3.49 | 0.05 |
| 0.7% | `chat_polls` | 3.48 | 0.04 |

79 of 1,002 recorded statements were active at all. Reading this table:

- **The four fixes are verified.** Every pre-fix shape reads 0.00 calls/s: all three `unread_counts_for_user` variants, `last_message_at_for_rooms`, and the old `list_for_user`. The merged `list_for_user_with_state` (`stats_since` 2026-07-26) is the only one running.
- **The leaderboard gate works.** 0.01 calls/s is one pass in the window, consistent with the 300 s interval, down from 30 s.
- **The double `settings` read is gone**, at exactly 1 call per snapshot pass rather than 2.006.
- The chat bundle is still ~80% of the total, but the total is small enough that this is no longer a scaling item. See Pain Point 7.
- `INSERT INTO mud_world_states` appearing at 3.4% on 0.07 calls/s is a 9.4 ms write of a whole JSONB world blob on conflict. Cheap now, same detoast-cost shape as the artboard snapshots (#4 below), and worth remembering if MUD usage grows.

### Follow-up 2026-07-27: the staleness the leaderboard cut bought

Widening the loop to 300 s was correct on cost and wrong on one consumer nobody checked. The PR cleared the change against the per-session chip balance, which is event-driven and fine. It did not check the leaderboard panels themselves, and they had a latent bug that the wider interval turned from invisible into a product complaint: `watch::Sender::subscribe` marks the current value as **seen**, so the `has_changed()` gate in `app/tick.rs` never fired for the snapshot a session was handed at bootstrap. Sessions rendered *empty* panels — not stale ones — until the next timer pass, which at 30 s nobody noticed and at 300 s reads as broken.

Two fixes, neither of which touches the interval or the subscriber gate:

- `App::new` seeds `leaderboard` from `rx.borrow()` instead of `LeaderboardData::default()`.
- `subscribe` wakes the loop, and `should_refresh` grants that wake a pass only when the published snapshot is already older than `REFRESH_INTERVAL`. This handles the quiet-server case, where the subscriber gate skipped every pass and the first session back seeded from whatever the last session left behind — potentially hours old. The age bound is load-bearing: unbounded, this would fire once per connect and undo the cut above.

Added DB cost is at most one extra pass per 300 s window, and only on a process that was idle. **The lesson for the next timer that gets widened: check every consumer of the data, not just the one with a known latency requirement.** A cadence change is a correctness change for anything that was quietly relying on the old rate.

### Remaining DB work, ranked by impact

**Re-ranked 2026-08-18.** This list was written against the pre-fix baseline and its order no longer holds. Current order: **Discover (item 2) is now first**, because it regressed to 771 ms mean and is the only user-facing DB latency left, and its fix turns out to be a `VACUUM` rather than anything in the list below. The chat-poll leftovers (item 1) drop to housekeeping: verified at ~1.9% of one core total, so the percentages quoted in item 1 are shares of a baseline that no longer exists. Everything below is retained for its analysis, not its ranking.

1. **The rest of the chat snapshot poll, about 13% of baseline** (was 26% before the merge). Still the largest single item and still the only one that scales with concurrency. What is left is `list_chat_author_metadata` 9.0%, `voice_channels` 2.1%, then polls, owners, and settings under 1% each: individually cheap (0.2 ms to 0.9 ms) and expensive only because the bundle runs 5.5M times. `list_chat_author_metadata` is now the one worth looking at, and it is the odd one out because it keys off the *user* set, not the room set, so it did not merge. Remaining directions:
   - **Merge what is left.** `voice_channels` and `owner_ids_for_rooms` still key off the room set and could fold into `list_for_user_with_state`, but neither is one row per room (voice is 0-N, owners only apply to private topic rooms), so both need lateral guards to avoid making the lounge scan its 14k members. Smaller payoff than the merge already done, and more ways to get the plan wrong. Measure first.
   - **Run the bundle less often.** Raising `CHAT_REFRESH_INTERVAL` is a one-constant change but costs badge latency, because the poll is the only writer of unread badges. It is only free if unread is incremented locally first, which introduces dual-maintenance of the unread rule. Weigh that tradeoff deliberately; it was rejected once already on those grounds.
2. **`list_discover_public_topic_rooms`, 5.6%.** 510 ms mean and a 969 ms variant, on demand, so this is user-facing latency rather than background load: half a second to open Discover. Options unchanged from the DB Hot Queries section: denormalized `member_count`/`message_count`/`last_message_at` on `chat_rooms`, a short-TTL cache, or pre-aggregation.
3. **The sequential session loop, no direct DB cost.** `refresh_registered_sessions` awaits one session at a time, so cycle time is N_sessions × snapshot latency and `MissedTickBehavior::Skip` hides the overrun. Harmless at 60 sessions, and the 2026-07-26 pipelining cut per-snapshot latency further, but it is still a latent cliff at 4 figures: the loop degrades silently into a continuous back-to-back cycle rather than erroring, and unread badges just get slower. Cheap to fix now that each snapshot is fast.
4. **Feed fan-outs, 0.5%.** Fix for correctness, not capacity: three identical O(users) loops that overflow their broadcast channels and drop events in every live session. Pain Point 6.
5. **`UPDATE rss_entries`, 0.32%.** 982k writes because the poller updates every entry on every pass whether or not the content changed. A content compare before the write would remove nearly all of it. Small, but it is pure waste and the fix is local.

Below this, nothing measured exceeds 0.2%. Re-rank after the deploy rather than working further down this list from the pre-fix baseline.

## DB Investigation

`pg_stat_statements` was not enabled during the first investigation; it is now preloaded and installed in prod (used during the 2026-07-22 investigation; query recipes live in CONTEXT.md §10.2.2).

The first investigation used:

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

Both rewrites below were patched in source on 2026-06-04 and are live in prod.

### `ChatRoomMember::unread_counts_for_user`

Source: `late-core/src/models/chat_room_member.rs`

- 2026-06-04 shape: joined all memberships for a user to `chat_messages`; the planner chose a full sequential scan (about 86k messages); representative heavy user about 381 ms.
- 2026-06-04 rewrite: per-room `LEFT JOIN LATERAL` using `idx_chat_messages_room_created`; representative heavy user about 2.5 ms. That number did not age well. It was measured against a room-and-message set less than half the current size, and the lateral it introduced is per membership row, so its cost tracks rooms-per-user, which only grows. By 2026-07-26 the same query was 43% of all database execution time and 578 ms for a user who had never opened anything.
- 2026-07-26 rewrite (current): the lateral counts under `LIMIT ChatRoomMember::UNREAD_COUNT_CAP` and the per-message `users` join is replaced by a UUID compare against the system bot. Same never-read user: **11.8 ms, 3,534 buffers**, down from 578 ms and 483,271. Full story in Pain Point 7.

### `ChatMessage::list_recent_for_rooms`

Source: `late-core/src/models/chat_message.rs`

- Old shape: window function over all messages in all user rooms; a representative heavy user pulled about 82k rows, spilled about 11 MB temp, about 1.4 seconds.
- New shape: distinct room IDs, then per-room lateral index scan with `LIMIT $2`; representative heavy user about 211 ms.

### `ChatRoom::list_discover_public_topic_rooms`

Source: `late-core/src/models/chat_room.rs`

- Current shape: public topic room discovery uses lateral counts for member count and message count; representative runtime about 300-475 ms, dominated by repeated counts over `chat_room_members`.
- Confirmed 2026-07-26 at 5.6% of all DB execution time, 510 ms mean over 5,688 calls, plus a 969 ms variant. Now the **largest remaining single query** after the chat-poll fixes, and unlike those it is on demand, so the cost lands as user-facing latency: about half a second to open Discover.
- Under concurrent fan-out this query can pin the CNPG primary at its CPU limit while the node still has spare CPU (observed 2026-05-14: `pg_stat_activity` showed `service-ssh` running 8 concurrent sessions of the pre-lateral shape, which joined `chat_rooms -> chat_room_members -> chat_messages` with `COUNT(DISTINCT ...)` over an estimated ~4.48M joined rows; that shape drove the rewrite to lateral aggregates). Fix query shape first; raising the CNPG CPU limit from `1` to `2` is headroom only. Triage caveat from the same check: repeated `idx_users_username_lower` duplicate-key errors from profile updates are log noise, not the CPU source, unless active queries point there.
- **2026-08-18: regressed, and the cause is not the query shape.** The current variant (`stats_since` 2026-07-25, the one that added `r.topic`) runs at **770.8 ms mean over 4,518 calls with a 4,135 ms max**, against 509.7 ms for the shape it replaced. This is now the worst user-facing latency in the system: over three quarters of a second to open Discover, occasionally four seconds.

  `EXPLAIN (ANALYZE, BUFFERS)` on prod, 158 matching rooms, 516 ms, 98,675 buffers. The member-count lateral is 474 ms of that:

  ```
  Bitmap Heap Scan on chat_room_members m  (loops=158)
    Heap Blocks: exact=89681
    Buffers: shared hit=94407
  ```

  **It visits 89,681 heap blocks against a table that is only 4,631 pages.** `chat_room_members_pkey` is `(room_id, user_id)`, which fully covers a `WHERE room_id = ?` count, so this should be an index-only scan and never touch the heap. It is not, because **only 31.9% of the table's pages are marked all-visible** in the visibility map (`pg_class.relallvisible / relpages`). Last autovacuum was 2026-08-04, two weeks before the reading.

  Mechanism: 1.63M cumulative updates to `last_read_at` continuously clear all-visible bits, while neither autovacuum threshold ever trips to restore them. The dead-tuple threshold is `50 + 0.2 × 492,729 ≈ 98,595` against 8,145 actual dead tuples; the insert threshold is `1000 + 0.2 × 492,729 ≈ 99,546` against 26,850 `n_ins_since_vacuum`. So the table is updated constantly, is never eligible for autovacuum, and its visibility map decays indefinitely.

  **Fix, in order of cost:** run `VACUUM chat_room_members` and re-measure; if the lateral converts to an index-only scan, make it durable with `ALTER TABLE chat_room_members SET (autovacuum_vacuum_scale_factor = 0.02)` or a low `autovacuum_vacuum_insert_threshold`. No migration, no schema change, no query rewrite. This should also help the Pain Point 7 snapshot bundle, which counts against the same table. **Not yet tested:** the payoff is inferred from the plan and the visibility-map figure, not measured, because the 2026-08-18 check was read-only.

- Options for the query shape itself, now **deprioritized**: denormalized `member_count`/`message_count`/`last_message_at` on `chat_rooms`, a short-TTL cache, or pre-aggregation with a better index. All three solve a query-shape problem. Measure after vacuuming before building any of them, because the evidence says this is a vacuum problem wearing a query-shape costume.

## Observability Gaps

Found during the 2026-08-18 health check. None of these is a capacity problem; all of them limit the ability to diagnose one.

- **`otel-collector` is flapping, and it is the pipeline every other number here depends on.** 28 restarts lifetime, 5 in the 7 days to 2026-08-18, with 238 readiness and 103 liveness probe failures. Not memory (256Mi limit, 239Mi used, zero OOM events) and not a collector fault: the probes use `timeoutSeconds: 1` against a `200m` CPU limit with 64.7% peak throttling, so the health endpoint misses a 1-second deadline under its own load and liveness kills the pod. Every restart is a hole in metrics, logs, and traces. **Cheapest high-value fix on this list:** raise `timeoutSeconds` and the CPU limit in the collector's Terraform.
- **No node-exporter anywhere in the cluster.** `node_cpu_seconds_total` and the whole `node_*` family do not exist, so every host-level figure in this document is a cadvisor container sum, which cannot see the node's NIC, socket buffers, or steal time. This directly blocks Pain Point 8 item 2: the UDP receive buffer pressure is exactly what node-exporter would show and nothing currently can. Add it before the second-node work so there is a real host baseline to compare against.
- **`late_ssh_render_stall_{skips,disconnects}_total` still has no series at all.** Meanwhile `late_ssh_render_frame_drops_total` recorded 168,480 drops across 29 episodes in 7 days, peaking at 946/min, which is the documented ~909/min single-stalled-session signature. So stalls are demonstrably happening while the guard's own metrics never report. Either the 32 MB threshold is never reached before the client drops, or the metric is not wired to the code path. One pass over `late-ssh/src/ssh.rs` would settle it.
- **No stream telemetry** (no `record_stream_*` family). Already recorded as Pain Point 8 item 1; repeated here because it belongs to the same gap.

## Live Production Baseline (2026-08-18)

A 7-day read-only health check, for future comparison. Nothing here required intervention.

| | 7d |
|---|---|
| Peak concurrent sessions, fleet-wide | **43** |
| Average concurrent sessions | 22.3 |
| Sessions at time of check | 38 |
| Connections/day | 1,147 rising to **3,343** |
| Chat messages sent | 8,141 |
| Successful CLI pairs | 1,189 |
| Node CPU (all containers) | 1.60 cores avg, 2.58 peak, of 8 |
| `service-ssh` CPU | 0.62 cores avg, 1.65 peak |
| `service-ssh` per session | **24.9 mcores**, ~18 MiB |
| Postgres CPU | **0.086 cores avg**, 0.571 peak |
| Live DB execution | **~1.9% of one core** |
| OOM kills | **0** |
| Render frame drops | 168,480 in 29 stalled-client episodes |
| Feed events dropped (Pain Point 6) | ~9,226 |
| SSH inputs dropped (burst floods) | 2,306 |

Headline: **roughly 7x headroom on the binding constraint** (render/tick CPU) against the weekly peak, no capacity incident in the window, and Postgres has gone from the projected scaling threat to a rounding error. Connections roughly tripled over the week, which is the trend to keep watching, though concurrency did not follow proportionally.

Sustained CPU throttling worth noting, all in the monitoring stack rather than the app path: `vmagent` 28.6% of periods throttled on average, `postgres-1` 6.8%, `victoriametrics` 4.3%, `otel-collector` 3.3%.

## Immediate Next Work

**Re-ordered 2026-08-18** after the health check. The four 2026-07-26 DB fixes are verified and their follow-ups have dropped down the list; what rose to the top is a two-week-stale visibility map and a flapping telemetry collector, neither of which was on the list at all. Current order of payoff:

0. **`VACUUM chat_room_members`.** Cheapest real win available. Fixes the only user-facing DB latency in the system (Discover at 771 ms mean, 4.1 s max) and probably helps the chat snapshot bundle too. See DB Hot Queries, `list_discover_public_topic_rooms`. Then make it durable with a per-table autovacuum setting.
0b. **Fix the `otel-collector` probes** (`timeoutSeconds: 1` against a `200m` CPU limit). One Terraform change; protects every measurement this document depends on. See Observability Gaps.
0c. **Add node-exporter**, before the second-node work, so host-level CPU/NIC/socket-buffer pressure is visible at all. See Observability Gaps.

Then the pre-existing list below, with items 1, 2, 4, and 6 now done and item 5 unchanged and still open.

Historical note, ordered by measured payoff as of 2026-07-26: four DB fixes landed that day (see DB Cost Ranking, Fixed 2026-07-26) removing a projected 56% of the workload; items 1 and 2 below are what survived of them. That instruction to reset `pg_stat_statements` was never carried out, and the ranking was instead verified by live differential on 2026-08-18. **Resetting the window is still worth doing** so the cumulative table stops reporting retired queries at the top.

### 1. Kill the chat snapshot poll's per-room fan-out

Partly done 2026-07-26. The dominant term, `unread_counts_for_user`, is fixed: the count is capped at `ChatRoomMember::UNREAD_COUNT_CAP` (100) and the per-message `users` join is gone. Measured on prod against a never-read user: **578 ms to 11.8 ms, 483,271 buffers to 3,534**. The UI renders anything at the cap as `99+`.

Also done: the duplicate `SELECT settings` read is gone (`User::friend_and_ignored_user_ids` fetches the row once for both lists, was 11.1M calls at exactly 2.006 per snapshot).

Still open: the other queries in the bundle, which are individually cheap and expensive only because the bundle runs often, plus the sequential session loop. Ranked with the rest of the leftovers under DB Cost Ranking, Remaining DB work.

**Verified done 2026-08-18** and largely closed as a priority: the live differential shows all pre-fix shapes idle and the whole bundle costing about 0.40 ms of DB time per session per second, roughly 0.4 cores extrapolated to 1000 sessions. Finish the leftovers for latency and tidiness if you like, but this is no longer a scaling item. See Pain Point 7.

### 2. Gate the leaderboard refresh loop

Done 2026-07-26 (`app/hub/svc.rs`; the service has since moved to `app/leaderboard/svc.rs`). `REFRESH_INTERVAL` went 30 s to 300 s and each pass now skips entirely when `has_subscribers()` is false, so an empty server does no leaderboard work at all. Expected to take the loop from 13.1% of DB execution time to under 1.5%. Safe because the only latency-sensitive consumer, the per-session chip balance in `app/tick.rs:881`, is already event-driven: chip writes fire `chip_user_changed` and `ShopService` pushes the balance per user (`app/hub/shop/svc.rs:832`).

**Verified 2026-08-18:** the leaderboard passes run at 0.01 calls/sec in the live differential, consistent with the 300 s interval, and account for 7.0% of live DB time in aggregate. Working as designed.

Follow-up 2026-08-18: Late Time adds one query over indexed all-time and
current-month O(users) rollups to the subscriber-gated pass (fourteen queries
total), plus one array-upsert statement updating both rollups per five minutes
when connected time is pending. Connection and disconnection do no DB work.

### 3. Add a second node; give `service-ssh` a full node to itself

`late-ssh` render/tick CPU is the scaling unit; everything else on `server-1` is overhead stealing cores from sessions. Move the overhead to a new node so the full 8 cores serve sessions.

Plan sketch:

- Provision `server-2` and join it as an RKE2 agent (`infra/setup_rke2.sh` is the existing node bootstrap); label the nodes (for example `role=ssh` on server-1, `role=support` on server-2).
- Stays on `server-1`: `service-ssh` (the public SSH path is pinned there: ingress-nginx TCP passthrough hostPorts, the `ipv6-proxy` DaemonSet address binding, and the DNS A/AAAA records all point at server-1), plus the door-host pods (`late-nethack`, `late-dcss`, `late-usurper`, `late-dopewars`) unless their `local-path` save PVCs are migrated; their saves are hostPath-pinned to the node.
- Moves to `server-2`: `service-web`, `icecast`, `liquidsoap`, the monitoring stack, and LiveKit if its node bindings allow. Liquidsoap's music PVC is not a blocker: the data re-syncs from R2 by the `sync_music` deploy job, so a fresh PVC on the new node refills itself.
- Postgres: keep 2 CNPG instances but spread them one per node (CNPG pod anti-affinity), which upgrades the second instance from same-node standby to actual node-level HA. Note the PVC pin: the moved instance gets a fresh volume and re-clones from the primary.
- Placement enforcement in Terraform: `node_selector` on each moved Deployment. Optionally taint `server-1` afterwards so nothing new schedules next to `service-ssh`.
- Cross-node hops after the move: `service-web -> icecast-sv:8000` (~128 kbps per proxied listener) and app `-> postgres-rw` if the primary lands on server-2; both are LAN-negligible, but prefer keeping the Postgres primary on server-1 with `service-ssh` and the standby on server-2.
- Public ingress for web/audio keeps working unchanged: DNS still points at server-1, ingress-nginx forwards across the cluster network to pods on server-2.

### 4. Verify the render-cost win in prod

Done 2026-07-24 (numbers in Pain Point 1): ~26.5 mcores/session, ~20% clean-skip ratio, ~5.3 draws/session/sec, stall guard never fired. Re-derived ceiling ~260-300 sessions/node, up from 100-110. This decides the 1000-user shape needs roughly 4 SSH pods, not a large fleet. A second independent reading later the same day at 59 sessions reproduced it (26.8 mcores/session, 22% clean-skip, 6.6 draws/session/sec, 0 frame drops, node at 37% CPU and 46% memory), so the numbers are stable, not a lucky sample. Remaining watch item: re-read under a genuine 100+ concurrent surge (both readings were about 60 sessions) and after the eq-to-quarter-edge knob if it ever ships.

### 5. Fix the feed fan-outs

Open. Re-scoped 2026-07-26 from "top code-level fix" to a correctness fix: 0.5% of DB time, but every burst drops feed events in every live session and the receivers do not recover. Three copies to fix, not one. Full description in Pain Point 6.

### 6. `pg_stat_statements` tracking

Done: preloaded and installed in prod; query recipes in CONTEXT.md §10.2.2. Keep watching top total execution time, top mean, top calls, top temp bytes, and top shared/local block reads after traffic events.

**Caveat learned 2026-08-18:** the window has never been reset, so cumulative shares are meaningless across a deploy that changes query shapes (see the warning at the top of DB Cost Ranking). Two workable habits: reset the window after any deploy that rewrites SQL, or rank by a **two-snapshot delta** rather than by cumulative totals. The delta method needs no reset, preserves history, and is what verified the 2026-07-26 fixes. `stats_since` also helps: it dates each entry's creation, which is enough to tell deploy generations of the same query apart.

### 7. Cap render dimensions

A defensive clamp exists (500×200 in `late-ssh/src/terminal_size.rs`, shipped 2026-07-12 against hostile resizes). The product-level render cap is still open: a server-side maximum render area (for example 160 columns × 50 rows) so render work does not scale unbounded with legitimate large PTYs (283×72 seen in logs).

### 8. Make `service-ssh` horizontally shardable

Minimum viable design:

- On SSH session start, write `session_token -> owning pod` to Redis
- Pair WebSocket checks token ownership and either:
  - routes/proxies to the owning pod, or
  - ingress uses a deterministic sticky key that guarantees same pod
- On session end, remove token ownership

Do not scale `service-ssh` randomly before this exists.

### 9. Add PgBouncer

Before increasing app replicas substantially:

- keep Postgres `max_connections` sane
- move app pools behind PgBouncer transaction pooling
- avoid multiplying deadpool connections by replica count

## 1000-User Target Architecture

Suggested shape:

- Two nodes as the first step (see Immediate Next Work): `server-1` dedicated to `service-ssh`, `server-2` for web/audio/monitoring/DB standby
- `service-web`: 3+ stateless replicas
- `service-ssh`: multiple replicas, each owning many sessions
- Redis: token ownership, presence, pub/sub, lightweight fanout
- PgBouncer: DB connection smoothing
- Postgres: durable state
- Audio: dedicated scalable streaming path, not one small Icecast pod on the app node
- Observability: dashboard for active sessions, per-pod session count, render frames/sec, frame drops, DB pool wait, Postgres top SQL, p95 input latency. Partially exists: the Rendering row (renders, clean skips, draws/session, stall guard), `late_ssh_sessions_active`, `late_ssh_render_frame_drops_total` (a flat ~909/min per stalled session is the stalled-client signature), and `late_ssh_render_stall_{skips,disconnects}_total`; traces in VictoriaTraces (Jaeger API on `monitoring/victoriatraces:10428`)
- Per-pod telemetry identity (prerequisite for the above once replicas > 1): each app pod now sets `OTEL_RESOURCE_ATTRIBUTES=service.instance.id=$(POD_NAME)` (downward-API pod name) in Terraform (`infra/service-ssh.tf`, `infra/service-web.tf`). The SDK's env resource detector picks it up and the collector's `resource_to_telemetry_conversion` turns it into a `service_instance_id`/`instance` metric label. Before this, every pod exported an identical otel series (e.g. `late_ssh_sessions_active`) and they clobbered each other on scrape (the 32h A/B window showed the gauge alternating between the two pods' values). Query per pod with `... by (instance)`.

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
- render skip ratio and frame drops
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

Updated 2026-07-23, after an actual HN front-page surge (2026-07-22, peak about 100 sessions) and the render-cost program landing.

What held or is now in place:

- SSH cap 1000, chat query rewrites live, Postgres a non-factor (about 200 millicores at 80 TUI sessions)
- Memory: OOM root cause found (stalled-client output buffering in russh's uncapped queue) and guarded in code; limit raised to 8 GiB; pair-WS surface capped and bounded
- `pg_stat_statements` and traces available for live diagnosis
- Render cost: dirty gate + adaptive tick shipped; idle sessions no longer pay the 15 FPS floor

Residual risk (updated 2026-08-18):

- single-node cluster (second node is the next infra step)
- single `service-ssh` pod for real session ownership
- the feed fan-outs (Pain Point 6) drop feed events in every live session on each write. Measured at ~9,226 dropped events over 7 days, still unfixed, still three copies
- Discover latency is a live product problem: 771 ms mean, 4.1 s max, and it regressed rather than improved. Likely a stale visibility map, fix untested
- `otel-collector` restarts several times a week, punching holes in the telemetry that all of these judgements rest on
- no node-exporter, so host-level NIC and socket-buffer pressure is invisible (blocks Pain Point 8 item 2)
- no PgBouncer yet
- no horizontal `service-ssh` sharding yet

Retired from this list on 2026-08-18:

- ~~the four 2026-07-26 DB fixes are projected, not verified~~ **verified** by live differential
- ~~the chat snapshot poll is the binding DB constraint on the 1000-user target~~ **it is not.** Measured at ~0.4 cores extrapolated to 1000 sessions, against a Postgres instance averaging 0.086 cores
- ~~the `service-web -> icecast` upstream drops~~ **zero occurrences in 7 days**

For posts that bring about 100 active users, current state survives, proven in production. For 1000 active terminal users the remaining projects are now: add the second node, then shardable `service-ssh` (with PgBouncer before replicas multiply). **The DB is no longer on the critical path** for that target, which is the main change from the 2026-07-23 version of this assessment: both the render-cost multiplier and the DB side are now measured rather than projected, and render/tick CPU is the sole scaling unit. At the 2026-08-18 weekly peak of 43 concurrent sessions there is roughly 7x headroom before the current single node binds.
