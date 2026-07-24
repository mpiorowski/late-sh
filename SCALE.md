# late.sh Scale Notes

Last updated: 2026-07-24 (render-cost win measured in prod at ~26 mcores/session; sessions-per-node ceiling re-derived to ~260-300. Per-pod telemetry identity (`service.instance.id`) added in infra so otel series stop clobbering across replicas. Next infra step: add a second cluster node and move everything except `service-ssh` off `server-1`)

This document records the current production capacity posture, what was discovered during the HN-spike investigations (June 2026 and the 2026-07-22 OOM, see CONTEXT.md §10.5), the DB query findings, the shipped render-cost program, and the roadmap toward roughly 1000 concurrent users.

## Current Infra Status

Cluster shape:

- Single RKE2 node: `server-1`
- Node capacity observed: 8 CPU, about 15.6 GiB memory
- Node usage at about 80 concurrent sessions (2026-07-22, pre-render-cost-program): about 77% CPU, 43% memory
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

### 1. Render/tick CPU: was the primary 1000-user blocker, now fixed in code, pending prod verification

The before-picture, kept as the baseline: the SSH render loop and browser tunnel world tick ran every 66 ms (roughly 15 FPS) for every session regardless of activity. Measured 2026-07-22 during the HN surge: about 59 millicores per session at the 15 FPS floor (4.7 cores for 80 sessions), saturating the 8-core node at roughly 100-110 concurrent sessions.

The render-cost program (see its own section below) removed both the fixed tick and the unconditional draw:

- A dirty gate skips `terminal.draw()` entirely on clean frames; an idle session with the sidebar hidden settles to about 1 render/min.
- The fixed 66 ms interval is replaced by an adaptive wake deadline (66 ms hot to 500 ms idle floor); an idle session costs 2 cheap channel-drain ticks/sec instead of 15 full renders/sec.
- The per-frame constant factors from the 2026-07-22 audit are fixed (run-length clubhouse spans instead of per-cell `String`s, 1 Hz presence cache instead of a per-render `active_users` lock).
- The floor by product decision: a session with the right sidebar visible paints the ambient equalizer at ~7.5 fps and never settles fully clean.

The memory failure mode is also closed: the 2026-07-22 OOM (full writeup in CONTEXT.md §10.5) was frames rendered into russh's uncapped per-channel output queue for clients that had stopped reading. Shipped fix: a per-session `OutputBudget` in `late-ssh/src/ssh.rs`; over 32 MB outstanding the render loop pauses, and 30 s of sustained stall disconnects the session. Metrics: `late_ssh_render_stall_{skips,disconnects}_total`.

Measured in prod 2026-07-24 (v0.41.0, single `service-ssh` pod, 60 live sessions):

- CPU about 1591 millicores, so about **26.5 millicores/session** (down from the pre-program 59 floor; a 32h A/B against a still-draining v0.40.7 pod read 47 mcores/session for the old code on the same node at the same time).
- Memory about 1085 MiB, so about **18 MiB/session**.
- Render loop: about 5.3 draws/session/sec, **~20% clean-skip ratio**. Sessions sit in the `ANIM_HALF_TICK` (~7.5 fps) tier, not the 500 ms idle floor, because almost everyone keeps the right sidebar visible and the ambient eq paints there. The documented "1 render/min idle" case is real but rare in the wild.
- Stall guard never fired (`late_ssh_render_stall_*` has no series); 0 frame drops on this pod.

Re-derived ceiling: at ~26.5 mcores/session, `service-ssh` reaches about **260 sessions on the current shared node and about 300 on a dedicated 8-core node** (memory ceiling is ~450/pod, so it stays CPU-bound). Old ceiling was 100-110. The named knob if the eq reads expensive: move it to the quarter edge (~3.8 fps), which roughly doubles the ceiling again, not reintroducing audio-state gating.

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

The app is not continuously polling the DB for chat messages; chat message flow is event-driven. But connect storms and room switches still hit DB-heavy paths.

2026-07-22 bootstrap audit specifics (none was the OOM cause, all are burst multipliers):

- The 15-20 query bootstrap fan-out per new session has no concurrency limiter (only chat reads share an 8-permit semaphore). Still open.
- Aquarium creature/world assets are re-parsed from KDL on every session start. Still open.
- `next_available_username` + `User::create` race under same-name connect storms, rejecting auth with no backoff (the `idx_users_username_lower` error loop seen in prod logs). Still open.
- The nonogram library deep-clone per session (about 1-3 MB) is FIXED: Arc-shared since render-cost phase 0.

### 4. Audio capacity is still single-pod

Icecast now allows 300 clients, but it is still one pod. The second-node move (below) gives it CPU/memory headroom away from `service-ssh`, but for 1000 audio listeners a dedicated streaming strategy is still needed:

- dedicated Icecast host with real bandwidth headroom
- CDN/edge-compatible stream distribution
- multiple relays
- or browser/client behavior that avoids duplicating streams where possible

### 5. Postgres connections are bounded but not pooled externally

App pools are currently per process through deadpool, with `LATE_DB_POOL_SIZE=16` for both `service-ssh` and `service-web`.

Postgres `max_connections=100`. This is acceptable while replicas are low, but scaling app replicas will multiply pools. PgBouncer should be introduced before many app replicas.

## Render-Cost Program (shipped 2026-07-22/23)

Consolidated from RENDER_COST.md (deleted). The canonical description of the gate contract and the adaptive tick lives in CONTEXT.md §2.6; this section keeps the scale-relevant summary, the rules that must not be violated, and the open follow-ups.

### What shipped

- **Phase 0, per-frame constant factors (2026-07-22):** counter-validated chat row caches (`ChatRowsVersions`, see `late-ssh/src/app/chat/CONTEXT.md`), presence cached at 1 Hz, per-session `targeted_event_rx` for single-recipient chat events, 64 KB BufWriter frame path, run-length clubhouse spans, Arc-shared nonogram library, and the `OutputBudget` guard (32 MB unacked pause, 30 s disconnect).
- **Phase 1, dirty gate (2026-07-22):** `App::tick() -> bool`; `render_once` (ssh.rs) computes `changed = signal.dirty.swap(false) | input drained | app.tick()` and skips `terminal.draw()` entirely when clean (ratatui's diff does not advance on skip, no forced repaint needed).
- **Phase 1 tightening + domain sweep (2026-07-22/23):** every domain state exposes `tick() -> bool` under the dirty contract ("rule of three", CONTEXT.md §2.6): chat snapshot drains report real change via full compares, modals are event-driven, house tables and door games report their watch peeks and go quiet between rounds, the ultimate cooldown became minute-granularity riding the per-minute global frame. The FFT audio visualizer was replaced by a stateless synthetic ambient equalizer (`viz::render_eq`), so no audio state drives rendering at all.
- **Phase 2, adaptive world tick (2026-07-23):** the fixed 66 ms interval is gone. Each render pass returns `App::wake_hint() -> Duration` and the loop sleeps exactly that long unless input or a `RenderSignal` wake lands first. Tiers (`app/tick.rs` consts): `HOT_TICK` 66 ms (splash, 2 s post-input window, active ultimate effect, house tables, open arcade game, bonsai modals), `ANIM_HALF_TICK` 132 ms (Clubhouse, visible sidebar, pet), `ANIM_QUARTER_TICK` 264 ms (aquarium surfaces), `IDLE_TICK` 500 ms floor. Floor ticks only drain channels; worst-case latency for an unprompted event while idle is one floor interval. Enablers: `marquee_tick` is wall-clock-derived, every frame edge is a period-index compare, and bonsai passive growth was removed entirely (product decision) so no wall-time accumulator depends on tick cadence.

Result: idle sessions cost 2 cheap clean ticks/sec and about 1 render/min. A sidebar-visible session holds ~7.5 fps (about 37 draws per 5 s) by product decision: the ambient eq is always on. The knob if that reads expensive in prod is moving the eq to the quarter edge, not reintroducing audio-state gating.

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

- Old shape: joined all memberships for a user to `chat_messages`; the planner chose a full sequential scan (about 86k messages); representative heavy user about 381 ms.
- New shape: per-room `LEFT JOIN LATERAL` using `idx_chat_messages_room_created`; representative heavy user about 2.5 ms.

### `ChatMessage::list_recent_for_rooms`

Source: `late-core/src/models/chat_message.rs`

- Old shape: window function over all messages in all user rooms; a representative heavy user pulled about 82k rows, spilled about 11 MB temp, about 1.4 seconds.
- New shape: distinct room IDs, then per-room lateral index scan with `LIMIT $2`; representative heavy user about 211 ms.

### `ChatRoom::list_discover_public_topic_rooms`

Source: `late-core/src/models/chat_room.rs`

- Current shape: public topic room discovery uses lateral counts for member count and message count; representative runtime about 300-475 ms, dominated by repeated counts over `chat_room_members`.
- Not as urgent as connect/snapshot paths, but should be optimized or cached before large traffic. Options: denormalized `member_count`/`message_count`/`last_message_at` on `chat_rooms`, a short-TTL cache, or pre-aggregation with a better index.

## Immediate Next Work

### 1. Add a second node; give `service-ssh` a full node to itself

`late-ssh` render/tick CPU is the scaling unit; everything else on `server-1` is overhead stealing cores from sessions. Move the overhead to a new node so the full 8 cores serve sessions.

Plan sketch:

- Provision `server-2` and join it as an RKE2 agent (`infra/setup_rke2.sh` is the existing node bootstrap); label the nodes (for example `role=ssh` on server-1, `role=support` on server-2).
- Stays on `server-1`: `service-ssh` (the public SSH path is pinned there: ingress-nginx TCP passthrough hostPorts, the `ipv6-proxy` DaemonSet address binding, and the DNS A/AAAA records all point at server-1), plus the door-host pods (`late-nethack`, `late-dcss`, `late-usurper`, `late-dopewars`) unless their `local-path` save PVCs are migrated; their saves are hostPath-pinned to the node.
- Moves to `server-2`: `service-web`, `icecast`, `liquidsoap`, the monitoring stack, and LiveKit if its node bindings allow. Liquidsoap's music PVC is not a blocker: the data re-syncs from R2 by the `sync_music` deploy job, so a fresh PVC on the new node refills itself.
- Postgres: keep 2 CNPG instances but spread them one per node (CNPG pod anti-affinity), which upgrades the second instance from same-node standby to actual node-level HA. Note the PVC pin: the moved instance gets a fresh volume and re-clones from the primary.
- Placement enforcement in Terraform: `node_selector` on each moved Deployment. Optionally taint `server-1` afterwards so nothing new schedules next to `service-ssh`.
- Cross-node hops after the move: `service-web -> icecast-sv:8000` (~128 kbps per proxied listener) and app `-> postgres-rw` if the primary lands on server-2; both are LAN-negligible, but prefer keeping the Postgres primary on server-1 with `service-ssh` and the standby on server-2.
- Public ingress for web/audio keeps working unchanged: DNS still points at server-1, ingress-nginx forwards across the cluster network to pods on server-2.

### 2. Verify the render-cost win in prod

Done 2026-07-24 (numbers in Pain Point 1): ~26.5 mcores/session, ~20% clean-skip ratio, ~5.3 draws/session/sec, stall guard never fired. Re-derived ceiling ~260-300 sessions/node, up from 100-110. This decides the 1000-user shape needs roughly 4 SSH pods, not a large fleet. Remaining watch item: re-read under a genuine 100+ concurrent surge (this reading was 60 sessions) and after the eq-to-quarter-edge knob if it ever ships.

### 3. `pg_stat_statements` tracking

Done: preloaded and installed in prod; query recipes in CONTEXT.md §10.2.2. Keep watching top total execution time, top mean, top calls, top temp bytes, and top shared/local block reads after traffic events.

### 4. Cap render dimensions

A defensive clamp exists (500×200 in `late-ssh/src/terminal_size.rs`, shipped 2026-07-12 against hostile resizes). The product-level render cap is still open: a server-side maximum render area (for example 160 columns × 50 rows) so render work does not scale unbounded with legitimate large PTYs (283×72 seen in logs).

### 5. Make `service-ssh` horizontally shardable

Minimum viable design:

- On SSH session start, write `session_token -> owning pod` to Redis
- Pair WebSocket checks token ownership and either:
  - routes/proxies to the owning pod, or
  - ingress uses a deterministic sticky key that guarantees same pod
- On session end, remove token ownership

Do not scale `service-ssh` randomly before this exists.

### 6. Add PgBouncer

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
- render-cost win not yet measured in prod; the sessions-per-node ceiling is currently unknown (old ceiling was 100-110, new one should be several times higher)
- no PgBouncer yet
- no horizontal `service-ssh` sharding yet

For posts that bring about 100 active users, current state survives, proven in production. For 1000 active terminal users, the remaining projects are: verify the render-cost multiplier in prod, add the second node, then shardable `service-ssh` (with PgBouncer before replicas multiply).
