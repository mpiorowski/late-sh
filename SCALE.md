# late.sh Scale Notes

> **Health check or capacity question? Look up `monitoring/dashboards/observability.json` first.** It is the catalogue of production queries: 98 panels with their exact PromQL and LogsQL (spans, session start and the DB pool in the Operations row, errors in Logs, frames and bytes in Rendering and Wire & Latency). Copy an `expr` from it instead of rebuilding a query, and check metric names against it:
>
> ```sh
> jq -r '.panels[] | select(.targets) | .title as $t | .targets[] | select(.expr) | "\($t): \(.expr)"' monitoring/dashboards/observability.json
> ```
>
> Reaching the data: [Reach the data](#reach-the-data). Postgres recipes: `infra/RUNBOOK.md`. Rank DB work by a two-snapshot delta: [DB Cost Ranking](#db-cost-ranking).

This document records the production capacity posture, the measured costs, and the roadmap toward roughly 1000 concurrent users. Crashes and incidents are in the log in `infra/RUNBOOK.md`.

## Current Infra Status

Cluster shape:

- Three RKE2 nodes (Kubernetes v1.34.4+rke2r1, Debian 13). Node table and join procedure: `infra/README.md`, Nodes
  - `server-1` (8 CPU, about 15.6 GiB), the only control-plane and etcd member: service-ssh, redis, the doors, ingress-nginx, ipv6-proxy, ssh-edge, LiveKit, both Postgres instances (the last two by decision, Immediate Next Work 3)
  - `agent-1` (Hetzner CX33, 4 vCPU, 8 GB, 40 GB disk), label and `NoSchedule` taint `role=support`: service-web, Icecast, Liquidsoap, otel-collector, vmagent, kube-state-metrics, VictoriaMetrics/Logs/Traces, Grafana, Minecraft
  - `agent-2` (Hetzner CAX21, arm64, 4 vCPU, about 7.6 GiB allocatable), tainted `role=games`: Vintage Story only
- Storage: every PVC uses the `local-path` (hostPath) provisioner, so any pod with a volume is pinned to the node that holds its data. This matters for any node move (door-game saves, music data, Postgres).

Reading 2026-11-04, 21 live sessions, no pressure condition set on any node:

| Node | CPU | Memory | Disk free | Requests CPU / mem | Limits CPU / mem |
|---|---|---|---|---|---|
| `server-1` | 17% | 47% | 9.3 of 37.2 GiB (images 12.8 GiB) | 52% / 49% | 320% / 184% |
| `agent-1` | 7% | 74% | 13.0 of 37.2 GiB (images 7.0 GiB) | 41% / 55% | 135% / 107% |
| `agent-2` | 3% | 29% | 29.2 of 37.2 GiB | 25% / 21% | 75% / 39% |

The limit overcommit is normal for this shape but means a synchronized spike across pods has no referee. `server-1` disk is the one trend: 16.6 GiB free on 2026-09-13, 9.3 GiB now, and kubelet image GC starts by default at 85% used, about 5.6 GiB free. Item 3, Left to do.

Operational notes:

- Changing the CNPG memory limit restarts the primary and briefly removes the `postgres-rw` endpoint.
- In-place pod resize (`kubectl patch --subresource resize`, Kubernetes 1.34, zero restart, zero dropped sessions) patches only the running pod. The Deployment template must come from Terraform or the next rollout reverts the limit.

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
  - Stays on `server-1` by decision: TURN is tied to `rtc.late.sh`, which must stay on `server-1` (Immediate Next Work 3)
  - Current Terraform limit: 1 CPU, 1 GiB. Cheap by design: it forwards packets, it does not transcode
  - No `limit:` block in the server config, so LiveKit's node defaults apply (`num_tracks` 400/CPU up to 8000, `bytes_per_sec` 1 GB/s). Both are far above what the node's NIC can serve, so LiveKit will never be the thing that says no. See Pain Point 8
- `livekit-ingress`: 1 replica, `host_network: true`
  - WHIP ingest for `/golive obs`, transcoding disabled (packet forwarder)
- `redis`: 1 replica (`valkey/valkey:8-alpine`, service name `redis-sv`)
  - LiveKit <-> ingress message bus only. The LiveKit server refuses Ingress API calls without it. Not application state, not a cache
- `postgres`: CloudNativePG, 2 instances
  - Primary: `postgres-2`, since a switchover on 2026-10-22 17:35 UTC (timeline 8). `postgres-1` exited 0, so it was not a crash; nothing records what triggered it
  - Both instances stay on `server-1` by decision (Immediate Next Work 3). Data leaves the node through daily barman-cloud base backups plus continuous WAL archiving to object storage
  - Current Terraform/live limits: 1 CPU, 4 GiB
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



## Biggest Pain Points

### 1. Render/tick CPU: the scaling unit

The before-picture, kept as the baseline: the SSH render loop and browser tunnel world tick ran every 66 ms (roughly 15 FPS) for every session regardless of activity. Measured 2026-07-22 during the HN surge: about 59 millicores per session at the 15 FPS floor (4.7 cores for 80 sessions), saturating the 8-core node at roughly 100-110 concurrent sessions.

The render-cost program (see its own section below) removed both the fixed tick and the unconditional draw:

- A dirty gate skips `terminal.draw()` entirely on clean frames; an idle session on a page with nothing animated settles to about 1 render/min.
- The fixed 66 ms interval is replaced by an adaptive wake deadline (66 ms hot to 500 ms idle floor); an idle session costs 2 cheap channel-drain ticks/sec instead of 15 full renders/sec.
- The per-frame constant factors from the 2026-07-22 audit are fixed (run-length clubhouse spans instead of per-cell `String`s, 1 Hz presence cache instead of a per-render `active_users` lock).
- The floor by product decision: a session on a Home with an animated tile paints at ~7.5 fps and never settles fully clean. Two tiles hold that edge: the bonsai sway (always) and the ambient equalizer (only while a client is paired and unmuted).

The memory failure mode is also closed: the 2026-07-22 OOM (full writeup in the `infra/RUNBOOK.md` incident log) was frames rendered into russh's uncapped per-channel output queue for clients that had stopped reading. Shipped fix: a per-session `OutputBudget` in `late-ssh/src/ssh.rs`; over 32 MB outstanding the render loop pauses, and 30 s of sustained stall disconnects the session. Metrics: `late_ssh_render_stall_{skips,disconnects}_total`.

Prod readings:

| Date | Sessions | mcores/session | MiB/session | draws/session/s | clean-skip |
|---|---|---|---|---|---|
| 2026-07-22 (before the program) | 80 | 59 | | 15 | 0% |
| 2026-07-24 | 60 | 26.5 | 18 | 5.3 | 20% |
| 2026-08-18 | 38, peak 43 | 24.9 | 18 | 7.15 | 15% |
| 2026-11-04 (7 d) | 24 avg, peak 39 | 32 | 18 | 4.0 | 21% |

Memory sits at about 18 MiB/session over a ~1 GiB per-pod baseline, with transient bursts to about 3 GiB (consistent with `OutputBudget`, 32 MB unacked per session, holding frames for stalled clients, then releasing; not a leak). The stall guard fires: 3,926 skips and 13 disconnects in the 7 days to 2026-11-04.

A 7-day regression over 5-minute samples (2026-11-04): `service-ssh` CPU is about 0.24 cores plus 5.6 mcores per draw/s (r² 0.53), so draws carry about 69% of the 0.78 cores it averages. Per session it reads 33 mcores with no real base (r² 0.46).

Ceiling at 32 mcores/session: about **225 sessions on `server-1` as shared today and 250 on a dedicated 8-core node** (memory ceiling ~450/pod, so it stays CPU-bound). Old ceiling was 100-110. Against the 7-day peak of 39 that is roughly **6x headroom**.

**Where the bytes go, 2026-11-04 (7 d): away screensavers.** Output is 215 GB, 87% of it to terminals nobody has touched for five minutes (78% of session time). Those bytes follow the screensaver a user picked. `session wire summary` over 7 days (2,225 ended sessions, 196 GB; 12 of them are 34% of the bytes, the median is 1.2 MB), joined to each user's current selection:

| Selected | Users | GB | Hours | KB/s | KB/frame | Frames/s |
|---|---|---|---|---|---|---|
| `random` | 8 | 77.7 | 797 | 27.1 | 5.0 | 5.4 |
| `randomer` | 7 | 36.2 | 304 | 33.1 | 7.9 | 4.2 |
| `aurora_fjord` | 1 | 23.3 | 92 | 70.3 | 16.1 | 4.4 |
| `aurora_fjord_pixels` | 2 | 10.6 | 81 | 36.1 | 6.5 | 5.6 |
| `alpine_dawn` | 2 | 5.1 | 44 | 32.0 | 3.9 | 8.2 |
| `off` | 186 | 22.0 | 1,077 | 5.7 | 1.6 | 3.5 |
| `earthrise` | 80 | 14.5 | 903 | 4.4 | 1.2 | 3.8 |
| `misty_forest` | 3 | 2.7 | 98 | 7.6 | 2.6 | 3.0 |
| `misty_forest_pixels` | 3 | 2.0 | 115 | 4.9 | 1.0 | 4.8 |
| `earthrise_pixels` | 2 | 1.5 | 105 | 3.9 | 1.6 | 2.4 |

**20 users, the first five rows, are 78% of all bytes.** `earthrise` and `misty_forest` cost the same as `off`, so this is the piece and not just heavy users liking screensavers, and the ranking matches the per-piece table in `late-ssh/src/app/ascii/CONTEXT.md`, Cost: 2 to 5 KB/s for those two, 130 to 194 KB/s for the aurora in pixels and alpine dawn on a 200x50 terminal. This reading was taken while `random` and `randomer` drew from all eight pieces, half of them lively, and the aurora in dots cost 107 KB/s. They now draw only from the cheap pieces (`ascii/playback.rs`, `in_random_pool`), and the aurora in dots sweeps at 3 KB/s and is one of them. The 200 KB/s budget bounds a piece's rate, and nothing bounds how long it plays to an empty room: the heaviest session ran 33 hours at 93 KB/s, 11 GB.

Not proof: the selection is today's, the rates are whole sessions, and `late_ssh_screensavers_total` counts activations per piece but not their bytes. A piece field on `session wire summary` would settle it.

At `earthrise`'s rate the 1,193 hours of `random`, `randomer` and `aurora_fjord` are about 19 GB instead of 137 GB, so the pool and the sweep should take **about -60% of all output**. Projected, not measured: read `late_ssh_output_bytes_total{presence="idle"}` before and after the deploy. What stays lively is what someone picked, `aurora_fjord_pixels` and `alpine_dawn`, 16 GB, 8%. A lively piece that yields to a slow one after some minutes away is the lever left, and how lively an unattended terminal should be is a product call.

A screensaver covers whichever screen was open, so by-screen `presence="idle"` byte rates mostly measure screensavers: do not rank screens by them.

For someone actually there, KB/s per active viewer (7 d, screens with over an hour of attention): night city 141, Artboard 114, Brogue 41, Realm 24, DCSS 21, `zen` 20, the rest under 10.

**Draws are the separate lever, for CPU: an idle session takes the quarter tier.** Past `ATTENTION_ACTIVE_WINDOW`, `wake_hint` hands the half tier's surfaces `ANIM_QUARTER_TICK` instead of `ANIM_HALF_TICK`: an equalizer or a bonsai left on screen with no key for 5 minutes goes from 7.5 fps to 3.8 fps. Only the half tier moves. The quarter tier itself stays, because its edge steps the aquarium simulation and slowing it would slow the fish. Animation speed is otherwise unaffected because state is wall-clock derived, and door output, chat and other pushes still wake through `RenderSignal`, so a spectator watching a door keeps every frame. A screensaver and a live Arcade table keep their own tiers. Projected from the regression, assuming about 80% of draws are idle (attention is 78% idle), not measured: at most about -27% CPU, less for the sessions already on the quarter tier or the idle floor, which raises the session ceiling rather than fixing anything today.

**The input queue does overflow in prod, and it is fine.** 2,306 `session input queue full; dropping inbound ssh input` warnings (`queue_cap=256`) over the 7 days to 2026-08-18, 54 in the 24 h to 2026-11-04. Before reading that as input-latency regression: the events arrive **sub-millisecond apart** in 5 tight bursts (one hour held 1,511 of them), which is a paste bomb or an escape-sequence flood filling a 256-slot queue instantly, not a user losing keystrokes while typing. This is the bounded queue doing its job instead of growing without limit. Recorded so the next person who greps for it does not go hunting a latency bug. If it ever appears spread evenly across a session instead of bunched, that *would* be the real thing.

Input latency: keystrokes land in a per-session bounded queue and the render loop wakes on input, so ordinary keystrokes never wait on the app mutex before being queued. The remaining risk under high fan-out is that `render_once` still holds the app lock across synchronous `app.tick()` + `app.render()`, so a slow tick delays that session's input-to-frame path; the input queue closed the cadence gap, not the lock-held-across-tick stall. Chat-specific row-cache, snapshot, unread-count, and scoped-loading performance notes live in `late-ssh/src/app/chat/CONTEXT.md`.

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

**The app half of the problem (started 2026-09-02).** Session ownership and pair-WS routing are the transport half; the list above is the app half, and it only shrinks if features stop adding to it. Two things now hold that line:

- **The rule.** Root CONTEXT.md, "The multi-replica rule": every new feature is designed as if several `late-ssh` processes were already running. Truth lives in Postgres, once-only actions are conditional `UPDATE ... WHERE` claims or advisory locks, cross-session fan-out is `LISTEN/NOTIFY`, background work dedupes through the DB, switches are rows. Presence is the one named exception and may not grow. The rule is bounded by "Replica-ready, not over-engineered" in the same section: one user's sessions on different replicas need not sync live, and read-on-open counts as replica-ready.
- **The per-user shape.** Root CONTEXT.md, "Per-user state", worked in `late-ssh/src/app/bonsai/CONTEXT.md`: pure rules, one writer under the row lock, a per-session render mirror, and a notify that carries only the owner's user id. Bonsai is the first migrated domain (`late-ssh/src/app/bonsai`) and the template for the pet, the tank, and per-account settings. Its cost per change is one notify per replica plus one UUID compare per local session on an existing tick, so it is nowhere near the 1000-user budget; it is the wrong shape for high-frequency shared surfaces (Artboard, house tables), which need room- or table-scoped routing instead.

The first feature built under the rule is the first-contact haunting (`late-ssh/src/app/deadchannel/`): every daily and lifetime cap a claim on the user row, the once-ever stamps claims, the AI bio screen claimed per bio text. It is the template for the migration of the list above.

The per-structure ledger. A design that cannot meet the multi-replica rule adds its row here; new features never do:

| Structure | Location | Current | To externalize |
|-----------|----------|---------|----------------|
| `active_users` / `conn_counts` | `State` | In-memory counters | Shared store (Redis or DB) |
| `SessionRegistry` | `session.rs` | In-memory `token → mpsc` | Stays local: sticky sessions route SSH + WS to same replica |
| Chat/Article events + snapshots, Profile per-user snapshots | `broadcast` / `watch` channels | In-process only | Postgres `LISTEN/NOTIFY` or Redis pub/sub for cross-replica fan-out |
| @bot + @graybeard chat | `GhostService` | Always-on presence + AI chat tasks; both are dedicated DB users with fixed fingerprints | Single-leader to avoid duplicate chat responses. During pod drain today, the old pod cancels bot tasks immediately. |
| Leaderboard data | `LeaderboardService` | DB-backed `watch` channel, 300s refresh | Already DB-backed; each replica runs its own refresh loop: duplicate work but no write conflict |
| Chat translation | `TranslationService` (`app/ai/translate.rs`) | DB-backed cache + in-memory single-flight set, daily call cap, and concurrency gate, all per replica | Cache is already shared through Postgres, so correctness holds across replicas; only the dedupe and the cap are per-process, meaning N replicas can each spend up to the cap and can duplicate one call for the same message. Both are acceptable at current scale; a shared counter would be the fix. |
| `SharedScratchpadRegistry` | `scratchpad/registry.rs` | In-memory `/pair` intents + `user_id → pairing` | Stays local by design: `/pair` pairings are explicitly ephemeral, acceptable to drop on failover |
| Drunk map | `clubhouse/drunk.rs` (`State.drunk_map`) | A per-process mirror of `user_drinks`, seeded from the table every 60s by each replica's ghost task and bumped on a local pour | Correct across replicas (the table is the truth); a pour on another replica shows here on the next seed, up to a minute late. A notify would make it instant. |
| Nightcap `SharedWall` | `state.rs` (`State.nightcap_house`) | One process-global wall (TV captions, tab board, carvings) refreshed by one task per process; a carve updates it in place | A second replica shows a carve late until its next refresh (5 min), or needs the notify shape from root `CONTEXT.md` |
| Bonsai | `app/bonsai` (`svc.rs`, `session.rs`) | Replica-clean: the row is the truth, every write runs under the row lock in `BonsaiService`, sessions mirror the row and refresh over `bonsai_changed`. The reference per-user shape (root `CONTEXT.md`). | Nothing. |
| First-contact haunting | `app/deadchannel/haunt` | Replica-clean: staff only is a code rule, not a switch; every cap and stamp is a conditional claim on the user row, the bio screen is claimed per bio text. Only the per-session dice are local, by design. | Nothing. One accepted race: two devices playing a whisper in the same window both play it, one mark lands (claiming at arming would burn it on every dropped session). |
| Presence (the tavern crowd, the Nightcap stools, the night city street) | `app/presence` (`svc.rs`, `state.rs`, `session.rs`), wire in `late-core` `models/presence.rs` | Replica-clean: one record per session (its tavern seat or cell, its stool, its street runner, its `lounge` flag for the deadchannel spawner); each replica's task batches its sessions' changed records onto `presence` (one notify per 200ms flush, a full heartbeat every 3s) and drops another replica's records 10s after its last heartbeat. Rooms derive from the records (`clubhouse/crowd.rs`, `clubhouse/nightcap/stools.rs`, `deadchannel/street/state.rs`); a session picks its own seat or stool, the earlier claim keeping a contested one. Counter `late_ssh_presence_total{beat}` (published, publish_failed, heard, rejected, expired) and gauge `late_ssh_presence_records{scope}` (local: a replica's own sessions; all: every record it holds, which should read the same on every replica), on the dashboard's Presence row. Nothing persisted but the pulse: the same service writes each replica's headcount once a five-minute bucket to `presence_samples` (`late-core` `models/presence_sample.rs`) and the read sums the replicas, so Home's sparkline is the whole house whichever pod drew it; a pod that was down leaves a gap in its rows, drawn as a gap, not a zero. `late_ssh_pulse_samples_total{outcome}`. | Nothing. The bots' online flags in the tavern still read the local `active_users`, so on a replica without the ghost bots the bar reads empty of them. |
| `StreamRegistry` | `app/stream/registry.rs` | In-memory live-stream registry: one stream per user, capability ids, watcher heartbeats | Stays local by design: a stream dies with the process (the room and its history are DB rows and survive); the streamer just runs `/golive` again |

What each remaining item needs is not the same, which is why "make it a row" is the wrong instinct for most of them:

- session registry, paired client registry: stay pod-local by design under sticky ownership (the transport half, item 8 below)
- active user presence, clubhouse lobby, scratchpad pairings, stream registry: a shared presence store (Redis) or an explicit "pod-local, dies with the pod" decision per structure; the round's buyer roster is where a wrong answer is user-visible (the clubhouse frame no longer shows a headcount)
- ghost bots (@bot, @graybeard, @bartender, the door announcers): single-leader election so two replicas never answer the same mention
- translation daily call cap and single-flight set: a shared counter
- leaderboard refresh loops: already DB-backed, duplicate work only

### 3. Connect storms hit DB and service startup paths

Per-user connect/snapshot work includes:

- user lookup/create
- chat room list
- last message timestamps
- unread counts
- friends/profile/metadata
- notifications
- room/game data

The app is not continuously polling the DB for chat *messages*; message flow is event-driven and snapshots carry empty message vectors. It does continuously poll for chat *metadata*: see Pain Point 7, the largest DB consumer in the system. Connect storms and room switches hit DB-heavy paths on top of that.

Session start, 2026-11-04 (24 h, 356 starts), TCP accept to first frame:

| | auth p50 / p95 | bootstrap p50 / p95 | first frame p50 | total p50 / p95 |
|---|---|---|---|---|
| returning | 0.93 / 2.65 s | 0.59 / 1.66 s | 35 ms | 1.76 / 4.2 s |
| new | 3.5 / 13 s | 0.46 / 1.3 s | 27 ms | 4.0 / 16.5 s |

`auth` is mostly the client: `auth_publickey` is 32 ms p50 on the server, and a new user's clock probably includes answering the host key prompt. `bootstrap` is the server-side part, and `pty_request` alone is 141 ms p50 and 395 ms p95. Not yet split further.

Burst multipliers still open, none of which has caused an incident:

- The 15-20 query bootstrap fan-out per new session has no concurrency limiter (only chat reads share an 8-permit semaphore).
- Aquarium creature/world assets are re-parsed from KDL on every session start.
- `next_generated_username` scans all usernames (`SELECT username FROM users`) per new signup and per retry; fine at current scale but O(users) during a new-user surge. Cheaper shape: one indexed probe against `idx_users_username_lower` first, full scan only on the rare collision.

`idx_users_username_lower` violations are renames to a taken name, which were 655 `failed to edit profile` ERROR lines in the 7 days to 2026-11-04, 82% of all ERROR lines. 515 of them came from one username in 20 minutes: Settings kept the refused name in its draft and sent it again with every later change, so no other setting saved either. The refused name now leaves the draft and the refusal logs at WARN (Observability Gaps).

### 4. Audio capacity is still single-pod

Icecast now allows 300 clients, but it is still one pod. The second-node move (below) gives it CPU/memory headroom away from `service-ssh`, but for 1000 audio listeners a dedicated streaming strategy is still needed:

- dedicated Icecast host with real bandwidth headroom
- CDN/edge-compatible stream distribution
- multiple relays
- or browser/client behavior that avoids duplicating streams where possible

The `service-web -> icecast-sv:8000` upstream drops seen in 2026-07 have not returned: zero `upstream stream ended` events in the 7 days to 2026-11-04, though nothing was knowingly changed to fix them. If they return, start from Icecast's client timeout and burst settings and the reconnect behavior in `late-web/src/pages/stream.rs`.

### 5. Postgres connections are bounded but not pooled externally

App pools are currently per process through deadpool, with `max_pool_size: 16` in both prod profiles (`late-ssh/src/config.rs`, `late-web/src/config.rs`).

Each `late-ssh` replica also holds exactly one LISTEN connection outside the pool (`late-ssh/src/pg_listener.rs`), so a replica costs 17 connections. PgBouncer in transaction mode cannot carry that one; it stays a direct connection.

Postgres `max_connections=100`. This is acceptable while replicas are low, but scaling app replicas will multiply pools. PgBouncer should be introduced before many app replicas.

### 6. Feed writes never fan out over the users table

The rule: never compute per-user state for every registered user and publish it on a shared broadcast channel. Each session drains its receiver only once per world tick (66 ms hot, 500 ms idle), so an O(users) burst overflows a 256/512 channel in every idle session, and each one drops its own update. Measured cost of breaking it, 48 h to 2026-09-23: ~6.2k `channel lagged by N` errors across the work, showcase, and news receivers, bursting to 1,189/min on a single work-card save.

What exists now:

- **Work and Showcase:** no fan-out at all. The badge is read once at session start (`refresh_unread_count`) and zeroed on visiting Profiles page 5. It does not move live when someone else posts; these pages are visited rarely, so that was the accepted trade.
- **News:** stays live and is replica-ready. Every `articles` write fires `articles_changed` (migration 198); each replica's `ArticleService::start_notify_worker` (fed by the one process listener) re-reads the newest 40 articles into its shared `watch`. Each session counts its own badge from that snapshot against its `article_reads` rows (`news::state::unread_in_snapshot`), so the badge saturates at `40+`. A write costs one list query per replica, independent of registered users and concurrent sessions.
- The remaining `Lagged` arms warn once per lag and keep draining instead of breaking.

Verified 2026-11-04: no `failed to receive work event` / `showcase event` / `article event` lines and no `event receiver lagged` warns in 7 days.

### 7. The chat snapshot poll is the largest DB consumer, but not a scaling threat

`ChatService::ensure_refresh_scheduler` runs one process-global task that rebuilds a full `ChatSnapshot` for **every connected session every 10 seconds** (`CHAT_REFRESH_INTERVAL`), on top of on-demand rebuilds at room switch and `request_list`. One rebuild is 5 queries in 2 pipelined rounds (7 with a live poll): `ChatRoom::list_for_user_with_state` (rooms, last message and unread count in one statement, the count capped at `UNREAD_COUNT_CAP`) plus `friend_and_ignored_user_ids`, then voice channels, polls, author metadata and owners. Details in `late-ssh/src/app/chat/CONTEXT.md`.

Measured 2026-11-04 by live differential: **77% of live DB execution time, and live DB execution is 2.3% of one core** at 22 sessions. That is **0.8 ms of DB time per session per second**, so about 0.8 cores at 1000 sessions against a 1 CPU Postgres limit. It doubled from 0.40 ms on 2026-08-18: `list_for_user_with_state` went from 2.96 ms to 5.7 ms mean as memberships grew, and JIT is 11% of its time (DB Cost Ranking). So the bundle is not the reason to rework anything today, but the extrapolation now reaches the limit: re-read it before planning a 1000-session Postgres. Render/tick CPU (Pain Point 1) is still what binds first.

| | 2026-08-18 | 2026-11-04 |
|---|---|---|
| users | 16,175 | 18,691 |
| rooms | 755 | 922 |
| `chat_room_members` rows | 492,729 | 634,827 |
| avg rooms/user | 30.5 | 34.0 |
| **max rooms/user** | 225 | **308** |
| memberships never opened | 41.5% | 42.1% |
| DB size | 495 MB | 858 MB (362 MB is `rss_entries`) |

The number to watch is **max rooms/user**: the room laterals fan out over every room a user has joined, whether or not it is on screen.

Planning was 60% of execution time for `list_for_user_with_state` on 2026-08-18 (20 parameters, two laterals). It matters only if this moves onto a hot path, and then the answer is a prepared-statement cache, not a rewrite.

Two structural facts:

- **The session loop is sequential.** `refresh_registered_sessions` is a plain `for session in sessions { ... .await }`, so cycle time is N_sessions x snapshot latency and `MissedTickBehavior::Skip` means an overrun degrades silently into a back-to-back loop. Unread-badge latency is 10 s plus the session's position in the cycle.
- **The poll is load-bearing for unread badges.** Nothing else increments `ChatState::unread_counts`, so `MessageCreated` events do not move a badge and the DB poll is the only source of truth.

Three ideas that did NOT survive measurement, recorded so nobody re-proposes them:

- *Skip rooms with no new activity* (denormalized `chat_rooms.last_message_at`): would skip about 2% of memberships, because never-opened rooms always have something newer than "never".
- *Maintain an incremental unread counter per membership*: `#lounge` has every user as a member, so every message would update about 18,000 rows. That is the Pain Point 6 disease, worse.
- *Widen `CHAT_REFRESH_INTERVAL`*: tripling it triples worst-case badge latency for every room the user is not looking at, which is the primary signal in the product.

Still open: stop the loop being sequential, so cycle time cannot exceed the interval at higher session counts. If it ever matters, the O(1) unread shape is a per-room message sequence number plus a per-membership read position, so unread is subtraction rather than counting; it needs a migration and a decision about the activity lines. No new infrastructure is needed or wanted: a broker would add a hop and remove none of these queries.

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
5. **LiveKit shares `server-1` with `service-ssh`.** It was deliberately kept
   there when the second node landed (2026-09-13, Immediate Next Work item 3):
   moving it breaks TURN until a dedicated TURN hostname with a DNS-01
   certificate exists. The fan-out still competes for the same NIC as every SSH
   session, which matters more once media traffic is real. A move to `agent-1`
   is also the clean moment to set the UDP receive-buffer sysctls on a fresh
   host.

Measured 2026-08-18: the `livekit` pod averaged 4.6% of its 1 CPU limit but
**peaked at 1.16 cores, so it touched and briefly exceeded its own ceiling**.
This section predicted LiveKit "will never be the thing that says no"; on CPU
that is now marginally false at current traffic, though the average says there
is no real pressure yet. It restarted twice in the window. Worth a look before
streams get an audience, since the 1 CPU limit was sized on the assumption that
a packet forwarder never approaches it.

### 9. Every ingress-nginx reload drops paired websockets 240 s later

SSH and IRC are out of this: public `22` and `6697` belong to the
`kube-system/ssh-edge` HAProxy (`infra/ssh-tcp.tf`), so nginx reloads and
controller rollouts never touch an SSH session. What still rides on nginx is
HTTP(S), including the paired CLI websocket on `api.late.sh`. Any ingress
change makes nginx reload: new workers take new connections, and the old ones
keep the existing ones until `worker_shutdown_timeout` (chart default `240s`),
when they are killed along with everything they hold. A paired CLI then
re-pairs through its retry loop (`PairRetryPolicy` in `late-cli`); its SSH
session stays up.

What triggers a reload:

- **cert-manager HTTP-01 renewals.** Each one adds and then removes an ACME
  solver ingress, so there are two reloads and one mass drop. Five certificates
  renew this way (`service-web-tls`, `irc-tls`, `icecast-tls`, `livekit-tls`,
  `livekit-whip-tls`), each about every 60 days, so roughly two or three drops a
  month. `kubectl get certificates -A` shows the next `renewalTime`s.
- **Any `terraform apply` that touches an Ingress or the controller's
  HelmChartConfig** (`infra/ingress.tf`). The HelmChartConfig change also rolls
  the controller pod, which is worse than a reload: every public HTTP(S) port
  refuses connections while the old pod drains.

How to recognise it: `service-ssh` logs a burst of `websocket dirty close or
error ... ResetWithoutClosingHandshake` at one instant, with 0 restarts and no
matching SSH reconnects, about 240 s after a `RELOAD` event on
`rke2-ingress-nginx-controller` in `kube-system`. A burst of `registered cli
session token` reconnects at the same instant means SSH is riding nginx again;
see the `ssh-edge` entry in CONTEXT.md.

If the re-pairs start to matter, raise `worker-shutdown-timeout` (for example
`24h`) under `controller.config` in the HelmChartConfig. It is one line, but old
workers and their memory stay around for up to a day after each reload, and the
next controller rollout still drops every websocket.

### 10. A pair websocket that drops during a drain cannot come back for up to 6 h

This is already a one-replica problem: during every `service-ssh` rollout two
pods run at once. The old pod keeps its SSH sessions for the 6 h
`termination_grace_period_seconds`, but as soon as it is terminating it leaves
the `service-ssh` Service endpoints. The pair websocket on `api.late.sh` goes
through ingress-nginx to that Service, so if a paired CLI's websocket drops
for any reason while its session drains (dirty close, network blip, a Pain
Point 9 reload), every reconnect lands on the new pod. The new pod's
`SessionRegistry` does not know the token and answers 404 (`api.rs`,
`ws pair rejected: no live session for token`,
`late_ssh_ws_pair_rejected_unknown_token_total`). The CLI's `PairRetryPolicy` keeps
retrying, once a minute after the first ten, until the old pod is killed and
the SSH session dies with it. For that whole time the user has no paired
audio, no device sync, and no visualizer, while the SSH TUI looks fine.

How to recognise it: one `token_hint` rejected about once a minute on the
new pod, starting after a rollout, with a `registered paired client session`
and then a `websocket dirty close or error` for the same token on the old pod,
and the rejections stopping at the old pod's grace deadline. Measured over
the 2026-10-15/17 window: about 1,600 rejections from 8 tokens across two
rollouts, each run ending at exactly rollout time + 6 h.

What would fix it, cheapest first:

- **Tell the user.** When a draining pod loses a session's pair websocket,
  show a banner in that session saying the server was updated and a
  reconnect brings the audio back. It stays inside `service-ssh` and fixes
  nothing, but it turns a silent 6 h loss into one keypress.
- **Forward to the owner.** A headless peer Service over the `service-ssh`
  pods with `publishNotReadyAddresses = true`, so terminating pods stay in
  its DNS. A pod that gets an unknown token asks its peers which one holds the
  session and proxies the websocket there. This is the "routes/proxies to the
  owning pod" half of Immediate Next Work 8, at the size of two pods, so
  building it here is a step toward sharding rather than a detour.

Shortening the grace period is not a fix: it trades this for cutting every
draining SSH session sooner.

### 11. Bytes on the wire become lag the server cannot see

Every server-side number can be green while users feel lag. `render_once`
hands a frame to russh in microseconds and counts it done; what the user waits
for is the network round trip, plus every byte queued ahead of that frame
divided by their bandwidth, plus their terminal parsing it. None of that shows
in kube. Three ways bytes turn into lag:

- **A keystroke's frame queues behind animation.** Frames leave in order, so
  the frame answering a key waits for any animation frames still in the pipe.
  Nothing paces frames to what the client absorbs: the `OutputBudget` guard
  (Pain Point 1) acts at 32 MB outstanding, which protects memory, not
  latency, and it cannot see most of the backlog anyway. russh queues only
  once the SSH window is full; before that the bytes sit in the kernel and
  in the edge HAProxy.
- **Slow start after idle.** By default a connection that pauses longer than
  its retransmit timeout drops back to slow start (about 14 KB per round
  trip). Idle sessions tick every 500 ms and clean passes send nothing, so a
  connection is often cold when the user switches screens, and a 30 to 60 KB
  repaint then pays two or three extra round trips: 300 to 450 ms more for a
  user 150 ms away.
- **The terminal.** True-colour frames plus the backend's per-glyph cursor
  moves (a bordered empty 160x40 frame is 3.5 KB against 1.9 KB stock,
  `terminal_backend_test.rs`) cost real parse time in slower terminals.

Measured 2026-10-20 at 33 sessions: `service-ssh` egress about 447 KB/s, so
about 13.5 KB/s per session and about 7 KB per drawn frame (61 draws/s fleet
wide). The averages are harmless; bursts and the screensaver pieces (up to
200 KB/s each, `late-ssh/src/app/ascii/CONTEXT.md`) are where the theory
would bite. The edge read `tcp_slow_start_after_idle = 1`, `cubic`,
`tcp_notsent_lowat` unlimited.

**What is measured** (dashboard row Wire & Latency):

- `late_ssh_terminal_rtt_seconds{client,backlog}`: the server's own
  end-to-end trip (`late-ssh/src/terminal_probe.rs`). Every 10 s the render
  loop follows a pass with a DSR status query; the terminal answers only
  after parsing everything before it, so the answer time is what a user
  waits for a frame. `backlog` is what the session wrote in the second
  before the query, the pass that carried it included, so animation frames
  still in the pipe count (`none`, `small` up to 4 KiB, `medium` up to
  32 KiB, `large`); a large backlog is probed as soon as the last probe is
  2 s old, so big repaints and heavy animation get sampled.
  `late_ssh_terminal_probes_total{client,outcome}` gives the answer rate.
- `late_ssh_frame_bytes{reason}` per drawn frame, and
  `late_ssh_session_output_bytes_per_second{client}`, one sample per session
  per 10 s window.
- `late_ssh_output_bytes_total{screen,place,presence,kind}`: every byte
  handed to the channel, attributed to where the session was when the pass
  drew (`place` and `presence` are the attention metrics' labels, so the two
  divide cleanly) and whether it was a frame or a terminal command (images,
  OSC). `presence="idle"` is bytes sent to a terminal nobody has touched in
  five minutes, the away screensaver included. Panels: Output Bytes by
  Place, and Output Bytes per Active Second (cost of one watching viewer).
- One `session wire summary` log line per session when it ends, in
  VictoriaLogs: user, client, `TERM`, the terminal's own name (its XTVERSION
  reply), how the session ended (`closed`, `stalled`, `failed`), bytes,
  frames, mean and peak output rate, probes answered and lost, and per
  backlog bucket the trip count, average and slowest (`rtt_none_avg_ms`,
  `rtt_large_max_ms`, ...). This is the per-user view the metrics leave out
  on purpose. `_msg:"session wire summary" username:alice` finds one user;
  `_time:24h _msg:"session wire summary" | sort by (rtt_large_max_ms desc) | limit 20`
  lists the worst sessions of the day.
- `late_ssh_cli_latency_seconds{kind,platform}`: the native `late` CLI splits
  the same trip. `ssh_rtt` is an SSH keepalive round trip (transport only),
  `terminal` is the query leaving the CLI to the answer coming back (the
  terminal only). Sent over the pair WebSocket as `latency` events.
- `late_ssh_render_pass_seconds{reason,screen}`: how long each render pass
  held the app lock (input drain, tick, draw), the server-side half the
  trip above cannot separate. One slow session pins a tokio worker while the
  fleet's CPU average stays green; a pass over the hot tick (66 ms) also logs
  `slow render pass` with the username and screen, once a minute per session.

**Shipped against slow start:** the `ssh-edge` DaemonSet's `tcp-tuning` init
container sets `net.ipv4.tcp_slow_start_after_idle=0` in `server-1`'s host
network namespace (`infra/ssh-tcp.tf`). Verified 2026-11-04:
`kubectl exec -n kube-system ds/ssh-edge -- cat /proc/sys/net/ipv4/tcp_slow_start_after_idle`
reads `0`.

How to pull every number above from a terminal, and the current reading, are
in [User Latency: Checking It Fast](#user-latency-checking-it-fast).

**How to read it:**

- `backlog="large"` p95 well above `backlog="none"` p95 (say 100 ms or more)
  means bytes on the wire are what users feel. Do the fix below.
- `none` already high means plain network distance. Nothing server-side
  shortens that.
- For CLI sessions, compare the whole trip with its halves. A large
  `terminal` half means the terminal is the bottleneck, and smaller frames
  help more than pacing.
- For one user, read their summary lines. A slow `rtt_fastest_ms` is their
  network distance. Fine `rtt_none_*` with slow `rtt_large_*` is a link our
  bytes outrun. A high `peak_bytes_per_sec` is a heavy screen. Lost or
  retired probes, or `ended=stalled`, are a terminal ignoring DSR or a link
  that stopped reading.

**Monitoring has confirmed it (reading below); this is the fix once screensaver
bytes are cut (Pain Point 1) and re-read: pace frames by acknowledgement,
the way mosh does.** A frame that answers input always goes out at once. A
world-tick frame (animation) waits while a probe is still unanswered, and
while pacing is active every frame carries a probe, so the terminal's
answers clock the session at exactly what its link and terminal absorb. A
skipped frame costs nothing: the ratatui diff folds it into the next one.
Fast links never wait, so they see no change; slow links stop building a
queue, and a keystroke never waits behind animation. A terminal that does
not answer DSR keeps today's behaviour (the probe retires after three
losses). Pair it with `tcp_notsent_lowat` (about 16 KiB) on the edge, so the
backlog sits in russh and HAProxy where late-ssh can see it rather than in
the kernel's send buffer; on its own that setting only moves the queue. The
`OutputBudget` memory guard stays as it is.

## User Latency: Checking It Fast

The runbook for Pain Point 11: what to run to answer "is user latency
measured, what are the numbers, and what is sending the bytes", from a
terminal with `kubectl` on context `late-sh-r`. Pain Point 11 says what each
metric means and how to decide; this says how to read them.

### Reach the data

```sh
kubectl get pods -n default | grep service-ssh          # pod age = how long the current code has been live
kubectl port-forward -n monitoring svc/victoriametrics 18428:8428 &   # metrics
kubectl port-forward -n monitoring svc/victorialogs 19428:9428 &      # logs
curl -s --retry 15 --retry-connrefused --retry-delay 1 localhost:18428/health   # wait for the forward
q() { curl -s 'localhost:18428/api/v1/query' --data-urlencode "query=$1" \
  | jq -r '.data.result[] | "\(.metric | del(.__name__,.instance,.job,.otel_scope_name,.service_instance_id,.service_name,.telemetry_sdk_language,.telemetry_sdk_name,.telemetry_sdk_version))  \(.value[1])"'; }
pkill -f 'kubectl port-forward -n monitoring'            # when done
```

Gotchas, each of which has cost a wrong answer once:

- **Wait for the forward.** A query sent before the port-forward is up
  returns nothing, which reads exactly like "the metric has no data".
- **Check the exported name before trusting an empty panel.** The
  collector's Prometheus translation (`monitoring/otel-collector-config.yaml`)
  appends the OTel unit to the name. It dedupes `bytes` and `seconds`, but a
  `By/s` unit on `late_ssh_session_output_bytes_per_second` came out as
  `..._per_second_per_second_*` and left its panel empty, so that histogram
  now has no unit. A new metric's real name:
  `curl -s localhost:18428/api/v1/label/__name__/values | jq -r '.data[]' | grep <stem>`.
- **Keep the range inside the pod's age** when judging a change; older
  samples are the previous build.

### The checks, in order

1. **Is it measuring?** Every answered probe records one trip, so the two
   totals match per client; an answer rate under ~99% is terminals without
   DSR or stalled links.
   `sum by (client, outcome) (increase(late_ssh_terminal_probes_total[12h]))`
   against `sum by (client) (increase(late_ssh_terminal_rtt_seconds_count[12h]))`.
2. **The trip.** `histogram_quantile(0.95, sum by (le, client) (increase(late_ssh_terminal_rtt_seconds_bucket[12h])))`,
   then the same `by (le, client, backlog)`. The `none` row is the network
   floor; `large` minus `none` is what our bytes add.
3. **Where a CLI trip goes.** `histogram_quantile(0.5, sum by (le, kind) (increase(late_ssh_cli_latency_seconds_bucket[12h])))`:
   `ssh_rtt` is transport, `terminal` the user's terminal.
4. **Ambient or typing.** `sum by (reason) (increase(late_ssh_frame_bytes_sum[12h]))`:
   `tick` is bytes nobody asked for, `input` the answers to keys.
5. **What sends the bytes.**
   `topk(15, sum by (screen, place, presence) (increase(late_ssh_output_bytes_total[12h])))`,
   and per watching viewer
   `topk(15, sum by (screen, place) (increase(late_ssh_output_bytes_total{presence="active"}[12h])) / (sum by (screen, place) (increase(late_ssh_attention_seconds_total{presence="active"}[12h])) >= 60))`.
   Images and OSC alone: add `kind="command"`.
6. **Who.** The per-session log line, heaviest first:
   ```sh
   curl -s localhost:19428/select/logsql/query --data-urlencode \
     'query=_time:12h {service.name="late-ssh"} _msg:"session wire summary" | sort by (bytes desc) | limit 15 | fields username, client, terminal, secs, bytes, frames, mean_bytes_per_sec, peak_bytes_per_sec, rtt_none_avg_ms, rtt_large_avg_ms' | jq -c .
   ```
   Bytes over frames near a full screen (25 KB and up) on every tick is a
   screen repainting whole instead of diffing.

Cross-checks: `sum(increase(late_ssh_output_bytes_total[12h]))` should
match `sum(increase(late_ssh_frame_bytes_sum[12h]))` plus the
`kind="command"` share, and roughly ten times
`sum(increase(late_ssh_session_output_bytes_per_second_sum[12h]))` (one
sample per 10 s window).

### Reading (2026-11-04, 7 d)

| | cli | ssh |
|---|---|---|
| p50 backlog `none` / `large` | 128 / 246 ms | 133 / 211 ms |
| p95 backlog `none` / `large` | 233 / 979 ms | 296 / 809 ms |
| p95 backlog `small` / `medium` | 434 / 456 ms | 381 / 285 ms |

- By the rule in Pain Point 11, bytes on the wire are what users feel: `large` p95 is 500 to 750 ms above `none`. The bucket is oversampled by design (probed as soon as the last probe is 2 s old), so its share of probes is not its share of traffic. Much of `large` is probably screensaver traffic nobody is typing on (the heaviest session answered 43,803 of its 46,575 probes in that bucket), so re-read this after the Pain Point 1 fix before treating it as felt lag.
- CLI split: `ssh_rtt` 127 ms p50, 408 ms p95; `terminal` 2.9 ms p50, 434 ms p95. At the median the trip is the network. At p95 the terminal costs as much as the network, which by the same rule says smaller and fewer frames help more than pacing.
- In 24 h, 93% of frame bytes are world-tick frames; over 7 days 87% of all bytes go to idle terminals, mostly away screensavers (Pain Point 1). By screen over 7 days: Home 135 GB, `zen` 22 GB, Arcade 19 GB, Artboard 10 GB, Leaderboard 8 GB, city 7 GB.
- The night city and Artboard are the heavy screens for someone actually there (Pain Point 1), neither root-caused. Artboard also holds 76 of 121 `slow render pass` lines in 24 h and a 59 ms p99 pass.

## Render-Cost Program (shipped 2026-07-22/23)

The canonical description of the gate contract and the adaptive tick lives in `late-ssh/CONTEXT.md`, Render loop; this section keeps the rules that must not be violated and the open follow-ups.

Shipped: per-frame constant factors, the dirty gate (`App::tick() -> bool`; `render_once` skips `terminal.draw()` on a clean pass), every domain's `tick()` under the dirty contract, and the adaptive wake (`App::wake_hint()`) replacing the fixed 66 ms interval. Result: a session on a page with nothing animated costs 2 cheap clean ticks/sec and about 1 render/min. A Home with a bonsai or equalizer tile holds ~7.5 fps by product decision.

Gating the `anim_half` edge on a paired client is NOT the knob, and would freeze the bonsai sway for the plain-`ssh` majority. The gate is whether Home draws a bonsai or equalizer tile (`ZenState::paints_on_half_edge`), and since the stock Home draws both, narrowing it would buy almost nothing. The knob is gating on presence: Pain Point 1.

### Design rules (do not violate)

- PROVE-CLEAN, NOT PROVE-DIRTY. Anything uncertain reports changed. A spurious frame costs nothing; a wrong "clean" freezes UI.
- The gate lives in `render_once` in ssh.rs, the only render loop.
- Peek receivers BEFORE draining (`has_changed()` on watches, `!is_empty()` on mpsc/broadcast). Exception: fixed-cadence publishers (chat snapshot, audio queue) report real change from the drain itself. A watch that is only `borrow()`ed at render must be marked seen (`borrow_and_update`) by whoever peeks it, or the peek latches dirty forever.
- Nothing paints at full rate. The ambient eq, pet, bonsai sway, and clubhouse ambience share the half-rate edge (`anim_half`, ~7.5 fps); aquarium steps on the quarter edge (~3.8 fps); everything else is slow/static. Marquee moves 3 columns/sec in 1 s steps so speed costs no extra frames.
- `is_multiple_of` on the tick counter is a bug pattern under sparse ticking; every edge compares its period index against the previous tick's.
- A blanket `changed = true` or fixed cadence needs a written justification at its call site (the current survivors are listed in `late-ssh/CONTEXT.md`).

### Metrics and observability

- `late_ssh_renders_total{reason=input|tick}` vs `late_ssh_renders_skipped_clean_total` (metrics.rs, `RenderReason` closed enum) observe the skip ratio in prod.
- Grafana: "Rendering" row in `monitoring/dashboards/observability.json` (render rate, clean-skip ratio, draws per session, stall guard).
- Per-session debug stats: the render loop logs drawn vs skipped_clean every 5 s at debug level; run with `RUST_LOG=late_ssh=debug` to feel the skip ratio locally.

### Test gotchas (for anyone touching the gate)

- Any test driving `tick()` without `render()` leaves `pending_terminal_commands` queued and the gate correctly stays dirty; mirror the loop with a drain_frame (render + take commands). See `app/tick_test.rs`.
- The settle tests (`idle_ticks_settle_clean_and_chat_send_marks_changed`, `open_settings_modal_settles_clean`) loop to 30 consecutive clean ticks; their failure panic dumps a state snapshot; extend that dump when debugging new dirt sources.
- Never raw cargo test; `make test-llm ARGS="-p late-ssh --lib -E 'test(...)'"`.

### Open follow-ups (all optional tightening)

- [ ] HouseTable hot tier is coarse (screen == HouseTable). Per-game "round running" predicates would let a quiet table idle.
- [ ] Artboard screen rides the 500 ms floor; remote strokes lag up to 0.5 s. Bump its tier while on-screen if it feels laggy.
- [ ] Push wakes for chat's targeted mpsc would cut the ≤500 ms idle chat latency to instant; needs the sender side to hold the RenderSignal.
- [ ] Load governor (raise the idle floor when node CPU is high) not built.

### Revert knobs

Drop the `anim_half`/`anim_quarter` gates in tick.rs and restore the aquarium's 220 ms self-throttle + draw-time reef tick to get pre-program animation behavior back. The dirty gate and adaptive deadline have no single revert switch; they are the architecture now.

## DB Cost Ranking

Each instance keeps its own `pg_stat_statements`, so the window restarted when `postgres-2` was promoted on 2026-10-22. Read as of 2026-11-04 that is a clean 13 days: about 31,700 s of execution, nothing evicted (`dealloc` 0, 1,159 of 10,000 slots). The window is cumulative and never decays, so a query shape that stopped running stays at the top. Rank "what runs now" by a **two-snapshot delta** rather than by cumulative totals; `stats_since` dates each entry, which tells deploy generations of one query apart.

**Live, two snapshots 305 s apart, investigation queries excluded: 2.3% of one core at 22 sessions, 1.4 GB of WAL a day.**

| share | query | calls/s | mean ms |
|---|---|---|---|
| 56% | `ChatRoom::list_for_user_with_state` | 2.22 | 5.7 |
| 12% | `User::list_chat_author_metadata` | 2.29 | 1.2 |
| 5% | `RssEntry::list_visible_for_user` | 0.02 | 56 |
| 4% | `voice_channels` | 2.22 | 0.44 |
| 3% | `INSERT INTO mud_world_states` | 0.07 | 10 |
| 2.5% | `owner_ids_for_rooms` | 0.81 | 0.69 |
| 2% | leaderboard refresh passes | 0.003 | 30-144 |
| 1.4% | `SELECT settings FROM users` | 2.25 | 0.14 |

**Cumulative, 13 days:**

| share | query | calls | mean ms | note |
|---|---|---|---|---|
| 45.0% | `list_for_user_with_state` | 2.75M | 5.17 | Pain Point 7; 11% of it JIT |
| 9.3% | `list_visible_for_user` | 24.5k | 119 | DB Hot Queries |
| 8.3% | `list_chat_author_metadata` (two generations) | 2.8M | ~1 | Pain Point 7 |
| 3.7% | `voice_channels` | 2.75M | 0.43 | seq scan of 645 rows, fine |
| 2.9% | `INSERT INTO mud_world_states` | 88k | 10.3 | whole world blob every ~14 s |
| 2.2% | `irc_history_range` | 2.0k | 364 | 126k buffers a call, 11 s max, not explained yet |
| 2.1% | `RssFeedRead::unread_count_for_user` | 15.4k | 44 | shrinks with the RSS prune |
| 2.0% | paper room editions read | 5.2k | 124 | 26k buffers a call |
| 1.3% | `artboard_snapshots` read | 2.6k | 160 | detoasting the blob, not a missing index |
| 1.3% | `list_discover_public_topic_rooms` | 893 | 449 | DB Hot Queries |

I/O and WAL, where the ranking by time misleads:

- `list_visible_for_user` is the top disk reader (10.1M blocks, 1,601 s of read time, three times the next) and writes 37 of the 42 GB of temp files.
- WAL, about 17 GB in 13 days: `mud_world_states` 4.6 GB, `UPDATE rss_entries` 3.2 GB, the RSS list's own hint bits 0.9 GB, `artboard_snapshots` 0.8 GB, online time 0.7 GB.
- **JIT:** 1,629 s of the 31,700 s (5%), 11% of `list_for_user_with_state` and 18% of Discover, on queries that are not analytical, which is why `infra/cloudnativepg.tf` sets `jit = off`.

Connections are not a constraint: pool peak 2 of 16 in use, 0 waiters, 0 failed checkouts, checkout p99 5.9 ms over 24 h.

Lessons kept from the first ranking (2026-07-26, when the chat snapshot poll was 50-68% of all execution time and the 30 s leaderboard loop 13.1%, now 300 s and subscriber-gated):

- The two largest items were **timers, not user actions.** Nobody had to do anything for either to run.
- **A cadence change is a correctness change for anything quietly relying on the old rate.** Widening the leaderboard loop turned a latent bug (`watch::Sender::subscribe` marks the current value seen, so sessions rendered empty panels until the next pass) into a product complaint. Check every consumer of the data when a timer is widened, not just the one with a known latency requirement.

### Remaining DB work, ranked by impact

1. **Discover** (DB Hot Queries): a 60 s process cache.
2. **`irc_history_range`:** explain why a `LIMIT` over `idx_chat_messages_room_created` reads 126k buffers.
3. **The sequential session loop** (Pain Point 7): no direct DB cost, a latent cliff at 4 figures.
4. **`mud_world_states`:** a quarter of WAL on a 14 s cadence. Widening it trades world rollback on a crash for less WAL; worth it only if archive cost matters.

## DB Hot Queries

### `RssEntry::list_visible_for_user` and the RSS tables

Source: `late-core/src/models/rss_entry.rs`, `late-ssh/src/app/chat/feeds/svc.rs`.

Measured 2026-11-04:

- `rss_entries` is 357,577 rows and **362 MB of an 858 MB database**, for 72 feed owners, **only 7 of them seen in the last 7 days**. There is no retention. 61 of 138 feeds hold over 1,000 rows; one holds 50,228 and grows by about 420 a day.
- The reader shows at most 20 entries per feed and 100 per user (`PER_FEED_ENTRY_LIMIT`, `ENTRY_LIMIT`), and nothing reads deeper. The newest 100 per feed is **10,599 rows, 3% of the table**.
- The list query windows over every row the user has. For the heaviest user (55,901 rows): parallel seq scan, external merge sort, about 25 MB of temp, **1.4 s**. Mean 119 ms over 24.5k calls.
- `poll_once` polled feeds whoever owned them, 64 per 30 minutes, and on new entries reran the list for an owner who may be offline. 131 of 152 feeds and 316,784 of 357,629 rows belonged to owners absent for 14 days.
- `upsert_for_feed` rewrote every entry in the feed's window on every poll whether or not it changed: 656k `UPDATE`s in 13 days against 34k new rows, 3.2 GB of WAL. The table has had no autovacuum since the promotion, 30k dead rows, and 34.5% of its pages all-visible.

What runs now, none of it needing a schema change, and none of it read in prod yet:

1. `RssFeed::list_for_poll` skips feeds whose owner has not been seen for 14 days (`users.last_seen`, set at login). A returning owner has the oldest check of all, so their feeds go first on the next poll, at most 30 minutes later. A session held open past 14 days is skipped the same way, its manual refresh included, until its next login; the longest session seen is 33 hours.
2. `RssEntry::prune_past_keep` runs after each poll: the newest 100 per feed by publish date (`created` when the feed gave none, the inbox's own order), across every feed, because the poller no longer visits the absent owners who hold most of the rows. 5,000 rows a run, so the 347k backlog drains in about 35 hours instead of one burst of WAL. An item a feed keeps listing past 100 newer ones is pruned once and then refused by `upsert_for_feed` (older than every kept row), so it never comes back as new. Dismissing only reaches back 100 per feed for the same reason. The file only shrinks with a `VACUUM FULL`, a manual step for after the drain. The heaviest user keeps 1,033 rows, so the window and the sort should fit in memory; measure before adding the `LATERAL`-per-feed shape and its index.
3. `upsert_for_feed` leaves an unchanged row alone, in one statement so a known row costs no second round trip. The insert keeps `DO NOTHING` without a conflict target because it also has to cover the `(user_id, url)` constraint.

### `ChatRoom::list_discover_public_topic_rooms`

Source: `late-core/src/models/chat_room.rs`. Public topic room discovery with lateral counts for members and messages.

The 2026-08 diagnosis was a decayed visibility map on `chat_room_members` (`relallvisible` at 11% in 2026-10, no autovacuum for two months), fixed by migration `244_chat_room_members_autovacuum.sql`. Verified 2026-11-04: the three `reloptions` are set, autovacuum has run 3 times since 2026-10-22, 79.8% of pages are all-visible, and the member lateral is now an `Index Only Scan`. **Mean fell from 843 ms to 449 ms, and it is still slow:**

```
Index Only Scan using chat_room_members_pkey (loops=180, rows=2889 per loop)
  Heap Fetches: 102638
Execution Time: 626 ms, 74,268 buffers
```

Vacuum was half of it. The rest is query shape: every open counts about 520,000 index entries. Do not expect `pct_all_visible` above 90% either: at 634k rows the 1% thresholds are about 6.4k dead or 7.3k inserted rows, the last run was 2026-10-26, nine days before this reading, and the map sags between runs, leaving 20% heap fetches.

The fix now is a cache. The result is the same for everyone (the query takes no user), so hold it for 60 s in `ChatService`; a per-process cache of a shared read is fine under the multi-replica rule. At about 69 calls a day this is latency for the person opening Discover (span p95 789 ms, 6 s max), not DB load. Denormalized counts on `chat_rooms` would be the next step and are not justified yet.

Under concurrent fan-out an older shape of this query pinned the CNPG primary at its CPU limit while the node had spare CPU (2026-05-14, 8 concurrent sessions). Fix the query shape first; raising the CNPG CPU limit from `1` to `2` is headroom only.

To re-read the visibility map:

```sql
SELECT reloptions, round(100.0 * relallvisible / nullif(relpages, 0), 1) AS pct_all_visible
FROM pg_class WHERE relname = 'chat_room_members';
SELECT n_dead_tup, n_ins_since_vacuum, last_autovacuum FROM pg_stat_user_tables WHERE relname = 'chat_room_members';
```

## Observability Gaps

None of these is a capacity problem; all of them limit the ability to diagnose one. Read 2026-11-04.

- **`otel-collector` is calmer but still OOMs, and it is the pipeline every other number here depends on.** The infra fix is applied (`500m`/`512Mi`, `GOMEMLIMIT=400MiB`, 5 s probe timeouts, spanmetrics `metrics_expiration: 15m`), and the pod deployed 2026-10-16 has restarted once, an `OOMKilled` on 2026-11-02 at the 512Mi limit. VictoriaMetrics, also at 512Mi, was `OOMKilled` on 2026-10-27. `container_oom_events_total` missed the collector's OOM, so read `lastState.terminated.reason` on the pod instead. Two sources are removed, and whether that stops the OOMs is not read yet:
  - The per-packet `data` span on the SSH input handler was **62% of the 1.73M spans a day**. No dashboard panel read it.
  - `http_telemetry_middleware` (`late-core/src/telemetry.rs`) names a request no route matched `unmatched`, where the raw path let scanner probes (`GET /.aws/credentials`, ...) mint 687 of the 1,240 `METHOD /path` names. `traces_span_metrics_duration_milliseconds_bucket` was 21.5k of VictoriaMetrics' 102k series.
- **A stalled client burned full repaints for hours, past the stall guard.** The stall metrics now have series (3,926 skips, 13 disconnects in 7 d), and `late_ssh_render_frame_drops_total` read **1,231,550 in 7 days**, seven times 2026-08-18, in long flat runs: 318,662 in the 6 h from 2026-11-03 02:00, about 885/min, the signature of one session. A frame that times out at `handle.data` is never counted by `OutputBudget`, so the guard cannot see it. The probable client is one that stopped reading, leaving russh's session loop blocked on the socket; that part is not proven. After three drops in a row `render_once` now retries at the idle tick, 15 fps down to 2 fps, without changing what a client that resumes sees.
- **WARN was 97% noise.** Of 17.9k WARN lines a day: `unknown channel request winadj@putty.projects.tartarus.org` 10,037 (russh logs it and answers `channel_failure` correctly; `late-core/src/telemetry.rs` now holds that target to ERROR), `ws pair rejected: no live session for token` 5,785 and `websocket dirty close or error` 571 (both INFO now; a metric counts the first), `token at capacity` 1,026 (the same event logged in two places, now one). Three tokens minted on 2026-10-22 account for about 4,250 of the rejections, at exactly one a minute for 13 days: `PairRetryPolicy` never gives up, and a CLI retrying a token past the 6 h drain window can never succeed. Why those clients outlived their sessions, with SSH keepalives on, is not established. ERROR is about 75 to 80 lines a week once refusals (a taken username, no action points, a dead article link, a job posting that would not read) log at WARN; each site keeps ERROR for a database failure.
- **No node-exporter anywhere in the cluster.** `node_cpu_seconds_total` and the whole `node_*` family do not exist, so every host-level figure in this document is a cadvisor container sum, which cannot see the node's NIC, socket buffers, or steal time. This directly blocks Pain Point 8 item 2. Worth adding as a DaemonSet with tolerations for both tainted nodes.
- **No stream telemetry** (no `record_stream_*` family). Already recorded as Pain Point 8 item 1; repeated here because it belongs to the same gap.

## Live Production Baseline (2026-11-04)

A 7-day read-only health check, for future comparison. Nothing here required intervention.

| | 2026-08-18 | 2026-11-04 |
|---|---|---|
| Peak concurrent sessions | 43 | **39** |
| Average concurrent sessions | 22.3 | 24.3 |
| Connections, 7 d | 15k | 18.3k (about 1,180 rejected at admission) |
| Chat messages sent, 7 d | 8,141 | 9,088 |
| Successful CLI pairs, 7 d | 1,189 | 3,346 |
| `service-ssh` CPU avg / peak | 0.62 / 1.65 cores | 0.78 / 1.64 cores |
| `service-ssh` per session | 24.9 mcores, ~18 MiB | **32 mcores**, ~18 MiB |
| Postgres CPU avg / peak | 0.086 / 0.571 cores | 0.098 / 0.211 cores |
| Live DB execution | ~1.9% of one core | **~2.3% of one core** |
| Render frame drops | 168,480 | 1,231,550 |
| OOM kills, app | 0 | 0 (`otel-collector` 1) |

Headline: **roughly 6x headroom on the binding constraint** against the weekly peak, no capacity incident in the window, Postgres a rounding error, and backups and WAL archiving clean (`failed_count` 0). The cost is in what away screensavers send (Pain Point 1).

Sustained CPU throttling, none of it on the app path: `plugin-barman-cloud` 83% of periods (WAL archiving still keeps up), `vmagent` 17%, `otel-collector` 6%, `victoriametrics` 3%, everything else under 2%.

## Small Debts (fix as touched)

Nitpicks found while reviewing, none of them a problem at today's load (about 40 concurrent sessions, 19k user rows), each cheap to fix in passing when the file is already open. Add to this list rather than to a Pain Point when the finding is "this will be wrong at 10x, not now". Remove the row when it lands.

| Where | What | Fix when touched |
|---|---|---|
| `late-core/src/models/user.rs`, `settings` jsonb | The first-contact haunting keeps ten keys on the user row (`first_contact_*`). jsonb has no in-place update, so every claim rewrites the row; harmless at a few claims per user per day, and the keys only appear once a claim wins. What actually grows on that row is the id arrays (`ignored_user_ids`, `friend_user_ids`, `favorite_room_ids`, `favorite_theme_ids`), not these scalars. | When a second funnel wants its own keys, move both into a `first_contact(user_id primary key, ...)` table with typed columns and the same conditional `UPDATE ... WHERE` claims. Leave `settings` for settings. |
| `start_notify_worker` in `app/hub/shop/svc.rs` and `app/hub/dailies/svc.rs` | Each worker handles its queue one notify at a time, and these two run a per-user query per notify (`chip_user_changed` fires on every chip move). Re-read-only workers (flags, news, runner looks) already collapse a burst into one read; the pot applies every payload in order by design. A chip-heavy burst lags only shop panels on that replica, never another domain, since each worker has its own queue. | Drain what is queued, dedupe by user id, then refresh once per distinct user. Local to each worker, no new infrastructure. |
| Any listener for a stream-shaped table (chat messages, feeds, news, notifications) built during the Pain Point 2 app-half migration | The re-read-only listeners (news, daily matches) re-read their whole table on every notify. That shape does not transfer to tables that grow. | Carry the row id in the payload as a hint, fetch rows with id greater than the last one seen (UUID v7 orders by time), and run the same query after a reconnect. The payload stays untrusted; the table stays the truth. |
| `late-core/src/models/marketplace.rs` (9 call sites), `quest.rs` (2), `pot.rs`, `chat_message_gild.rs`, `bonsai.rs` (`Tree::notify_changed`, one call site per write in `BonsaiService`) | App-side `SELECT pg_notify(...)` after the write. Migration 128 already records why this is fragile: a new write path forgets the notify and nobody notices until a replica is stale. Chips use a trigger instead. | Move each channel's notify into an `AFTER INSERT OR UPDATE` trigger as the model is touched, delete the app-side call, keep the channel name constant in the model. |
| `late-ssh/src/app/deadchannel/haunt/svc.rs`, splash whisper | Two devices of one user on the held door in the same window both play the whisper; only one mark lands. Already in the Pain Point 2 ledger. | Claim before play instead of after, at the cost of the door opening a round trip later. Only worth it if someone notices. |
| `late-web/src/pages/shared/board_png.rs`, the preview-card routes (`/profiles/{slug}/poster.png`, `/gallery/{key}/image.png`) and `/paper` | Every hit decodes the canvas and rasterises a PNG about 1536px wide (a gallery snapshot is the whole board) on `service-web`'s 250m CPU limit, unauthenticated, with only a `Cache-Control` header (renders run on the blocking pool one at a time, `board_png::render_blocking`, so they no longer stall the pod's single tokio worker; a burst now waits in an unbounded queue instead): that dedupes one client's refetches, not the fetch each Mastodon or Slack server makes on its own when a link is pasted, nor scanners. `/paper` and its feed run several uncached queries per hit; feed readers poll on a clock. Cost unmeasured; a few hits a day today. | Memoise the PNG in memory by route key with a short TTL (a saved snapshot and a painted poster change rarely; `main` is the live board, so the TTL is the bound), and read the paper's hit rate off the HTTP spanmetrics before caching it. |

### Root-file thinning (sizes 2026-11-04, function sizes 2026-09-02)

The four root files of the TUI (`late-ssh/src/app/{input,state,render,tick}.rs`) are 14.5k lines together, and the GUIDE rule for them is "hold a domain's state, delegate to it, done". Measured against that rule they are half glue and half domain logic that drifted up. Not a scaling problem; a "every feature touches these files" problem, which is what makes them the merge-conflict and review-cost hot spot. Sizes:

| File | Lines | Biggest function | What the excess is |
|---|---|---|---|
| `app/input.rs` | 4,500 | `handle_parsed_input_inner` 512, `handle_dedicated_screen_input` 337, `dispatch_escape` 288 | 237 direct `app.chat.` touches; whole domain handlers living here |
| `app/state.rs` | 4,725 | `App::new` 634, `App` struct 210 fields (lines 432-882) | 22 door-game enter/leave/launch fns, 12 voice fns, stream/on-air consent logic |
| `app/render.rs` | 3,534 | `render` 916, `draw` 697, `app_frame_title` 323, `push_quit_confirm_sayonara_placement` 220 | per-screen title logic, quit-confirm placement (a domain with its own `ui.rs`) |
| `app/tick.rs` | 1,726 | `tick` 1,015 | image-upload result handling, daily-match room join, door-game idle reaps, all inline |

The target shape already exists in the tree and is the thing to copy: `app/deadchannel/haunt` is one line in `tick.rs` (`haunt::svc::tick(self)`), one arm in input, one call in render, and everything else lives in the domain. Most domains already have `input.rs` / `ui.rs` / `svc.rs` (chat, artboard, clubhouse, directory, leaderboard, every modal); the drift is code that never moved into them, not missing structure. Moves, each self-contained and safe to do in passing when the feature is touched:

| Move | From | To | Size |
|---|---|---|---|
| Door-game enter/leave, idle reaps, "return to hub on exit" (the hub's keys and launch are in `app/door/hub/input.rs` already) | `state.rs` (22 fns), `tick.rs` | `app/door/hub/{state,svc}.rs`, one `door::svc::tick(app)` | ~350 lines |
| Voice join/leave/mute/deafen, on-air consent, `voice_toggle_intent`, `handle_voice_join_result` | `state.rs` (12 fns) | `app/voice/{state,svc}.rs` (`voice/state.rs` is 54 lines today; the logic is at the root) | ~300 lines |
| Image upload result, inline image polls, mod outputs, sheet save, requested-profile-open, debounced username click | `tick.rs` | `chat::svc::tick(app)` returning a `ChatTick { banner, screen_switch, open_profile, open_sheet }` | ~120 lines |
| Daily-match room join on board load, lobby sync | `tick.rs` | `lobby::daily::svc::tick(app)` | ~30 lines |
| `handle_chat_scroll_click`, `handle_chat_composer_click`, `chat_room_list_view`, `start_slash_command_composer` | `input.rs` (4 fns, ~280 lines) | `app/chat/input.rs`, which already exists | ~280 lines |
| `handle_icon_picker_input`, `apply_icon_selection` | `input.rs` (~140 lines) | `app/icon_picker/input.rs` (domain has no `input.rs` yet) | ~140 lines |
| `handle_directory_catalog_input` | `input.rs` (66 lines) | `app/directory/input.rs`, which already exists | ~70 lines |
| `handle_tour_gate` | `input.rs` (32 lines) | wherever the tour state lives (clubhouse tutorial) | ~30 lines |
| `push_quit_confirm_sayonara_placement` | `render.rs` (220 lines) | `app/quit_confirm/ui.rs`, which already exists | ~220 lines |
| `app_frame_title` per-screen branches, `status_hud_title` | `render.rs` (480 lines) | a `title()` per domain `ui.rs`, root keeps the `match screen` | ~400 lines |
| `App::new` field seeding | `state.rs` (634 lines) | each domain's `State::new(config)`; root becomes a list of constructor calls | ~400 lines |

Rules for the moves, so they do not become a rewrite:

- One domain per PR, tests move with the code (`input_flow_test.rs` and `tick_test.rs` cover much of this; the moved test goes beside the moved file).
- The root keeps: the `Screen` match, the tick order comment block (the 1Hz and animation edges are shared clocks, not domain logic), and the field. Anything that reads a domain's internals to decide something moves.
- The 210-field `App` struct shrinks only by grouping, not by removal: `app.darkroom_state`, `app.greendragon_state`, `app.rebels_state`, ... become `app.doors: DoorStates`, one field per family, which is also what makes `App::new` collapse.
- No new abstraction. A domain `svc::tick(app: &mut App) -> bool` that reaches into `app` is the contract; do not introduce traits for it.

## Immediate Next Work

Ranked by payoff against size, from the 2026-11-04 readings. The item numbers further down are stable because other files cite them; items 1, 2, 4, 5 and 6 landed and are gone.

| | Win | Evidence | Size |
|---|---|---|---|
| I | **Discover cache** (DB Hot Queries) | 449 ms mean, 6 s max | Small |
| J | **One `rke2-server` restart** (`infra/RUNBOOK.md`, RKE2 certificates and etcd snapshots) | RKE2 certs expire 2027-02-27; etcd snapshots only on `server-1` | Ops |

Then re-read Wire & Latency and decide item 10.

### 3. Second node: landed 2026-09-13; what stays where and why

`late-ssh` render/tick CPU is the scaling unit; everything else on `server-1` is overhead stealing cores from sessions, so the overhead moved. Procedure in `infra/README.md`, Nodes. Not `infra/setup_rke2.sh`: it bootstraps a whole cluster and would reinstall `server-1` and overwrite kubeconfig and the `KUBE_CONFIG` GitHub secret.

**Agent, not a second server.** etcd needs a majority of members up. With two servers both must be up, so a second control-plane node adds a failure mode without adding safety. Control-plane HA starts at three servers, all schedulable, behind a stable join and API address (DNS or a Hetzner load balancer) instead of `server-1`'s IP. Converting `agent-1` later means uninstalling the agent and reinstalling as a server with the same token; take an etcd snapshot first.

**Placement.** `agent-1` carries label and `NoSchedule` taint `role=support`. A workload lands there only with both the node selector and the toleration (`support_node_*` locals in `infra/defaults.tf`); the taint is what keeps `service-ssh` and everything else on `server-1`. CoreDNS needed its own toleration (`infra/coredns.tf`): its autoscaler wants one replica per node with required anti-affinity. local-path helper pods are bound by `nodeName`, which bypasses the scheduler, so the taint does not block volume creation on `agent-1`.

**Moving a `local-path` volume means recreating it empty.** A selector alone leaves the pod unschedulable. Scale the workload to 0, delete the PVC, confirm it is gone, apply, restore. Deleting a PVC while its pod runs leaves it Terminating and breaks the apply; applying first leaves new pods Pending and times out the rollout wait. Radio has no fallback beyond the files on its PVC, so `music-data` needs `sync_music` afterwards. All metric, log and trace history from before 2026-09-13 is gone.

**Stays on `server-1`, and why:**

- `service-ssh`, `redis`: the point of the move.
- Door hosts: save PVCs are pinned to `server-1`.
- ingress-nginx, `ipv6-proxy`, `ssh-edge`: public ports and the DNS A/AAAA records point at `server-1`. Ingress still serves pods on `agent-1` across the cluster network.
- **Postgres, both instances, by decision.** A standby on `agent-1` looks like node-level HA but is not: the CNPG operator and the whole control plane also live on `server-1`, so if `server-1` dies nothing is left to promote the standby. Data already leaves the node through daily barman-cloud base backups plus continuous WAL archiving to object storage, so a cross-node standby would only be a faster restore path. Keeping the primary next to `service-ssh` also keeps every query off the network. Revisit with three control-plane nodes.
- **LiveKit and `livekit-ingress`, by decision.** Both run with `host_network`. On `agent-1`, signaling (`rtc.late.sh`) and WHIP (`whip.late.sh`) would still work through ingress, and direct media would work because `use_external_ip` advertises the node's own public IP. TURN would break: its domain is `rtc.late.sh`, which resolves to `server-1`, and that name cannot move because signaling and its HTTP-01 certificate renewal both depend on ingress-nginx on `server-1`. A clean move needs a dedicated TURN hostname (for example `turn.late.sh`) pointing at `agent-1`, with a DNS-01 certificate. Payoff: stream CPU and packet fan-out off `server-1`, and a fresh host for the UDP receive-buffer sysctls (Pain Point 8).
- Cluster add-ons (cert-manager, the CNPG operator and barman plugin, local-path provisioner, metrics-server): untouched, no reason to move.

**Left to do:**

- Rotate the RKE2 certificates before they expire on 2027-02-27 (`CertificateExpirationWarning` on `server-1`, inside the 120-day window since 2026-10-30): restarting `rke2-server` rotates them, briefly interrupting the API but not running pods. Rolled into one restart with the next item, by the steps in `infra/RUNBOOK.md`.
- Offsite etcd snapshots. RKE2 takes one every 12 hours and keeps them only on `server-1`'s own disk (`kubectl get etcdsnapshotfile` shows `file://` locations). The S3 upload, reusing the bucket Postgres backups go to, is in the same runbook section.
- Reclaim `server-1` disk: prune unused images and vacuum the journal (images take 12.8 GiB of the 37.2 GiB disk). Commands in the same runbook section.
- node-exporter on the nodes (Observability Gaps).
- Add the `_minecraft._tcp.late.sh` SRV record (priority 0, weight 5, port 25565, target `mc.late.sh`); `mc.late.sh` itself resolves, the SRV record still does not exist (`door/minecraft/CONTEXT.md`).
- Optional: taint `server-1` so nothing new schedules next to `service-ssh`. Needs tolerations on everything that stays.
- LiveKit move, once the TURN prerequisites above exist.

### 7. Cap render dimensions

A defensive clamp exists (500×200 in `late-ssh/src/terminal_size.rs`, shipped 2026-07-12 against hostile resizes). The product-level render cap is still open: a server-side maximum render area (for example 160 columns × 50 rows) so render work does not scale unbounded with legitimate large PTYs (283×72 seen in logs).

### 8. Make `service-ssh` horizontally shardable

Minimum viable design:

- On SSH session start, write `session_token -> owning pod` to Redis
- Pair WebSocket checks token ownership and either:
  - routes/proxies to the owning pod, or
  - ingress uses a deterministic sticky key that guarantees same pod
- On session end, remove token ownership

Do not scale `service-ssh` randomly before this exists. The proxy half already pays for itself at one replica: it closes Pain Point 10.

`make start REPLICAS=2` reproduces the misrouting locally: two pods behind a copy of the edge, the pair websocket round-robined apart from the SSH connection. The Docker name the edge resolves (`ssh-pod`) lists draining pods too, as the peer Service with `publishNotReadyAddresses` would, so the forwarding can be built and checked there (root `CONTEXT.md`, Build, run, test).

The app half runs in parallel and is the longer job: work the Pain Point 2 ledger row by row, in the order the transport design needs them (presence first, since sticky sessions alone do not make a round's roster right; ghost single-leader second, since duplicate bot replies are the first thing users would notice; the translation cap last). New features never add rows to that table (root CONTEXT.md, multi-replica rule); a switch still held in process memory becomes a row as it is touched (Pain Point 2).

### 9. Add PgBouncer

Before increasing app replicas substantially:

- keep Postgres `max_connections` sane
- move app pools behind PgBouncer transaction pooling
- avoid multiplying deadpool connections by replica count

### 10. Pace frames by acknowledgement, once screensaver bytes are cut

The week of data is in, and its trigger reads as met (User Latency, Reading): `large` backlog p95 sits 500 to 750 ms above `none`. But most of those bytes are away screensavers nobody is typing on, and at p95 the terminal costs as much as the network, so do win A first and re-read. Design in Pain Point 11.

## 1000-User Target Architecture

Suggested shape:

- Today (Immediate Next Work 3): `server-1` for `service-ssh`, Postgres, LiveKit, and the doors; `agent-1` for web, audio, monitoring, and Minecraft; `agent-2` for Vintage Story. Node-level Postgres HA waits for three control-plane nodes
- `service-web`: 3+ stateless replicas
- `service-ssh`: multiple replicas, each owning many sessions
- Redis: token ownership, presence, pub/sub, lightweight fanout
- PgBouncer: DB connection smoothing
- Postgres: durable state
- Audio: dedicated scalable streaming path, not one small Icecast pod on the app node
- Observability: dashboard for active sessions, per-pod session count, render frames/sec, frame drops, DB pool wait, Postgres top SQL, p95 input latency. Partially exists: the Rendering row (renders, clean skips, draws/session, stall guard), `late_ssh_sessions_active`, `late_ssh_render_frame_drops_total` (a flat ~909/min per stalled session is the stalled-client signature for its first three drops, ~120/min once the backoff has it at the idle tick), and `late_ssh_render_stall_{skips,disconnects}_total`; DB pool wait as `late_db_checkout_seconds` (every `Db::get`, by outcome) with `late_db_pool_connections{state}`, `late_db_pool_waiters` and `late_ssh_chat_read_permits_in_use`, and session start as `late_ssh_session_start_seconds{stage,user}` (accept to first frame), all in the Operations row; what a user waits to see a frame in the Wire & Latency row (terminal round trip by the second of output before it, frame size, per-session output rate, the native CLI's split of the trip, render pass time by screen; Pain Point 11); traces in VictoriaTraces (Jaeger API on `monitoring/victoriatraces:10428`)
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

An actual HN front-page surge (2026-07-22, peak about 100 sessions) is the last real load test, and it came before the render-cost program landed.

What is in place:

- SSH cap 1000, admission ahead of the handshake, `OutputBudget` guarding memory, the pair-WS surface capped by token, `pg_stat_statements` and traces for live diagnosis
- Render cost: dirty gate and adaptive wake, about 225 sessions of ceiling, 6x the weekly peak
- Postgres a rounding error: 0.1 cores average against a 1 CPU limit

Residual risk:

- single control-plane node: `agent-1` and `agent-2` carry support workloads and game servers, but etcd, the CNPG operator and both Postgres instances live on `server-1`, so losing `server-1` stops the control plane and the database together (running pods elsewhere keep serving, nothing reschedules). etcd snapshots stay on `server-1`'s disk until the S3 upload is configured
- single `service-ssh` pod for real session ownership
- `otel-collector` and VictoriaMetrics both still OOM at 512Mi now and then, so telemetry can have holes when it matters most
- no node-exporter, so host-level NIC and socket-buffer pressure is invisible (blocks Pain Point 8 item 2)
- no PgBouncer yet
- no horizontal `service-ssh` sharding yet

For posts that bring about 100 active users, current state survives. For 1000 active terminal users the remaining projects are win D, then shardable `service-ssh` (with PgBouncer before replicas multiply). Render/tick CPU is the scaling unit; the chat snapshot extrapolation (Pain Point 7) is the DB number to re-read on the way there.
