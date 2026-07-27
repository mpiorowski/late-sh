# late.sh Scale Notes

Last updated: 2026-07-27 (the leaderboard's 300s cadence exposed a latent seeding bug: sessions rendered *empty* panels rather than stale ones, because `watch::Sender::subscribe` marks the current value seen and the `has_changed()` gate never fired for it. Sessions now seed from `borrow()`, and a connect refreshes a snapshot already older than `REFRESH_INTERVAL`. Cadence and subscriber gate unchanged. Next infra step is unchanged: add a second cluster node and move everything except `service-ssh` off `server-1`)

This document records the current production capacity posture, what was discovered during the HN-spike investigations (June 2026 and the 2026-07-22 OOM, see CONTEXT.md §10.5), the DB query findings, the shipped render-cost program, and the roadmap toward roughly 1000 concurrent users.

## Current Infra Status

Cluster shape:

- Single RKE2 node: `server-1`
- Node capacity observed: 8 CPU, about 15.6 GiB memory
- Node usage at about 60 concurrent sessions (2026-07-24, post-render-cost-program): about 37% CPU, 46% memory. For contrast, the pre-program reading at 80 sessions was about 77% CPU, 43% memory
- Node disk: about 28 of 37 GiB used (75%). Not urgent, but it has no automatic reclaim
- All core app workloads currently run on the single node
- Storage: every PVC uses the `local-path` (hostPath) provisioner, so any pod with a volume is pinned to the node that holds its data. This matters for any node move (door-game saves, music data, Postgres).

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

### 1. Render/tick CPU: was the primary 1000-user blocker, now fixed in code, pending prod verification

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

Re-derived ceiling: at ~26.5 mcores/session, `service-ssh` reaches about **260 sessions on the current shared node and about 300 on a dedicated 8-core node** (memory ceiling is ~450/pod, so it stays CPU-bound). Old ceiling was 100-110. The named knob if this reads expensive: move both the eq and the sway to the quarter edge (~3.8 fps), which roughly doubles the ceiling again. Not gating the render edge on audio state: the eq's *content* is now pairing-aware (`EqState`), but the `anim_half` edge itself must stay unconditional while the bonsai sway rides it.

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

Separate open bug, seen 2026-07-24: the `service-web -> icecast-sv:8000` upstream fetch drops every 10 to 50 minutes (`upstream stream ended; injecting silence until reconnect` and `ConnectionReset` in `late-web/src/pages/stream.rs`), and `service-web` failed one readiness probe the same day. Browser `/stream` listeners hear silence gaps at those moments. Not capacity related at 59 sessions; look at Icecast's client timeout/burst settings and the proxy's reconnect behavior.

### 5. Postgres connections are bounded but not pooled externally

App pools are currently per process through deadpool, with `LATE_DB_POOL_SIZE=16` for both `service-ssh` and `service-web`.

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

Both halves scale with total registered users, not with concurrent sessions, so this gets worse with signups even if concurrency stays flat. It also gets worse per replica: every `service-ssh` pod would run its own copy of the loop.

Fix direction (not yet implemented): the per-user unread count does not need a full-table scan. Either compute it only for currently connected sessions, or publish one broad `FeedChanged` event and let each session recompute its own count on receipt. Either shape removes the O(users) query loop and the O(users) channel sends at the same time. Whatever ships must land in all three services, and the `Lagged` arms should recover rather than `break`. Raising the channel capacity alone is not a fix; it only hides the drops.

### 7. The chat snapshot poll is the largest DB consumer in the system

Measured 2026-07-26 (first full `pg_stat_statements` ranking, see DB Cost Ranking below). **50-68% of all database execution time**, and `ChatRoomMember::unread_counts_for_user` alone is **43.0%**. This is the top code-level scaling problem on this list.

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

Why it is so expensive: **`chat_room_members` holds 444,994 rows over 583 rooms and 14,347 users, averaging 31 rooms per user and peaking at 154** (2026-07-26). Auto-join means memberships only accumulate. So queries 3 and 5 fan out over every room a user has ever joined, whether or not it is on screen, and the unread query returns 32.5 rows per call on average. The double `settings` read shows up as 11,070,677 calls, exactly 2.006 per snapshot.

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

Still open, all much smaller now:

1. **Stop the loop being sequential**, so cycle time cannot exceed the interval at higher session counts. The pipelining above cut per-snapshot latency, which raises the ceiling, but does not remove it.
2. If it ever matters again, the O(1) shape is a per-room message sequence number plus a per-membership read position, so unread is subtraction rather than counting. It needs a migration and a decision about the activity lines, which is why it is not the fix today.

**Do not widen `CHAT_REFRESH_INTERVAL` as a cheap win.** It looks like a free 3x, and it is not: the poll is the only writer of unread badges (see the second structural problem above), so tripling the interval triples worst-case badge latency for every room the user is not looking at. That is the primary signal in the product.

No new infrastructure is needed and none is wanted here. The events already exist and already reach every session; the bug is that the client discards them and re-derives the state from Postgres on a timer. A broker (NATS, Redis pub/sub) would add a hop to a pipeline that already works and would not remove one of those 22M queries.

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

Baseline taken 2026-07-26 from a `pg_stat_statements` window opened 2026-07-07 (18.6 days, 59.05M calls, 56,625 s total execution time). Percentages are share of total execution time in that window, before the fixes landed the same day.

| # | Subsystem | Baseline | Calls | Cadence | Status |
|---|---|---|---|---|---|
| 1 | Chat snapshot poll (Pain Point 7) | 50-68% | 22.1M+ | 10 s per session | partly fixed |
| 2 | Leaderboard refresh loop | 13.1% | 1.1M | 30 s, process-global | fixed |
| 3 | `list_discover_public_topic_rooms` | 5.6% | 6.0k | on demand, 510 ms mean | open |
| 4 | Artboard snapshot reads | 2.9% | 39k | on demand, 158 ms and 624 ms means | open, not worth it |
| 5 | Chat username list scan | 1.9% | 66k | 30 s, process-global | fixed |
| 6 | Feed fan-outs (Pain Point 6) | 0.5% | 3.6M | per feed write | open, correctness |

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

Roughly **60% of the total workload**, projected. Not yet verified in prod: reset `pg_stat_statements` after the deploy and re-run this ranking before trusting these numbers or planning against them.

### Follow-up 2026-07-27: the staleness the leaderboard cut bought

Widening the loop to 300 s was correct on cost and wrong on one consumer nobody checked. The PR cleared the change against the per-session chip balance, which is event-driven and fine. It did not check the leaderboard panels themselves, and they had a latent bug that the wider interval turned from invisible into a product complaint: `watch::Sender::subscribe` marks the current value as **seen**, so the `has_changed()` gate in `app/tick.rs` never fired for the snapshot a session was handed at bootstrap. Sessions rendered *empty* panels — not stale ones — until the next timer pass, which at 30 s nobody noticed and at 300 s reads as broken.

Two fixes, neither of which touches the interval or the subscriber gate:

- `App::new` seeds `leaderboard` from `rx.borrow()` instead of `LeaderboardData::default()`.
- `subscribe` wakes the loop, and `should_refresh` grants that wake a pass only when the published snapshot is already older than `REFRESH_INTERVAL`. This handles the quiet-server case, where the subscriber gate skipped every pass and the first session back seeded from whatever the last session left behind — potentially hours old. The age bound is load-bearing: unbounded, this would fire once per connect and undo the cut above.

Added DB cost is at most one extra pass per 300 s window, and only on a process that was idle. **The lesson for the next timer that gets widened: check every consumer of the data, not just the one with a known latency requirement.** A cadence change is a correctness change for anything that was quietly relying on the old rate.

### Remaining DB work, ranked by impact

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
- Options unchanged: denormalized `member_count`/`message_count`/`last_message_at` on `chat_rooms`, a short-TTL cache, or pre-aggregation with a better index.

## Immediate Next Work

Ordered by measured payoff as of 2026-07-26. Four DB fixes landed that day (see DB Cost Ranking, Fixed 2026-07-26) removing a projected 56% of the workload; items 1 and 2 below are what survived of them. **Re-run the ranking against a reset `pg_stat_statements` after that deploy before planning further DB work from this list.**

### 1. Kill the chat snapshot poll's per-room fan-out

Partly done 2026-07-26. The dominant term, `unread_counts_for_user`, is fixed: the count is capped at `ChatRoomMember::UNREAD_COUNT_CAP` (100) and the per-message `users` join is gone. Measured on prod against a never-read user: **578 ms to 11.8 ms, 483,271 buffers to 3,534**. The UI renders anything at the cap as `99+`.

Also done: the duplicate `SELECT settings` read is gone (`User::friend_and_ignored_user_ids` fetches the row once for both lists, was 11.1M calls at exactly 2.006 per snapshot).

Still open: the other eight queries in the bundle, which are individually cheap and expensive only because the bundle runs 5.5M times, plus the sequential session loop. Ranked with the rest of the leftovers under DB Cost Ranking, Remaining DB work. Do not work further down that list from the pre-fix baseline; re-measure first.

### 2. Gate the leaderboard refresh loop

Done 2026-07-26 (`app/hub/svc.rs`). `REFRESH_INTERVAL` went 30 s to 300 s and each pass now skips entirely when `has_subscribers()` is false, so an empty server does no leaderboard work at all. Expected to take the loop from 13.1% of DB execution time to under 1.5%. Safe because the only latency-sensitive consumer, the per-session chip balance in `app/tick.rs:881`, is already event-driven: chip writes fire `chip_user_changed` and `ShopService` pushes the balance per user (`app/hub/shop/svc.rs:832`). Verify against `pg_stat_statements` after the next deploy.

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

Residual risk:

- single-node cluster (second node is the next infra step)
- single `service-ssh` pod for real session ownership
- the chat snapshot poll (Pain Point 7) is still the only DB load that scales with concurrency. Its worst term is fixed, but the remaining bundle is about 26% of the pre-fix baseline and grows with sessions × rooms-per-user, both of which only grow. This is the binding DB constraint on the 1000-user target
- the four 2026-07-26 DB fixes are projected, not verified. `pg_stat_statements` has not been re-read since they landed
- the feed fan-outs (Pain Point 6) drop feed events in every live session on each write, several times an hour
- no PgBouncer yet
- no horizontal `service-ssh` sharding yet

For posts that bring about 100 active users, current state survives, proven in production. For 1000 active terminal users, the remaining projects are: verify the 2026-07-26 DB fixes in prod, finish the chat snapshot bundle, add the second node, then shardable `service-ssh` (with PgBouncer before replicas multiply). The render-cost multiplier is verified; the DB side is now measured, and the largest single item on it has been cut.
