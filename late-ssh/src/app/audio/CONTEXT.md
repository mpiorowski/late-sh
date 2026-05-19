# late-ssh Audio Context

## Metadata
- Domain: late.sh audio — Icecast house radio, global YouTube queue, browser/CLI source arbitration, synthetic browser-pair visualizer, now-playing poller
- Primary audience: LLM agents working in `late-ssh/src/app/audio` and the touchpoints it owns in `late-cli` and `late-web/src/pages/connect`
- Last updated: 2026-05-19 (post-incident queue reconciliation from main is preserved; CLI-side embedded YouTube webview v1 is wired into the normal `late-cli` build. Native CLI advertises `youtube`, `set_playback_source` gates Icecast and drives lazy helper spawn/teardown, and the helper registers as a browser only while YouTube is selected.)
- Previously: source arbitration simplified — no `ForceMute`; CLI gates Icecast on `set_playback_source`, and browsers only play web Icecast when no CLI is paired. Booth modal surfaces track durations: queue list has a right-aligned `m:ss` column between title and submitter, and the Now Playing row shows the same `m:ss` next to the title. Streams render `live`; unknown durations are blank. Two submit paths diverge in metadata: booth (`booth_submit_public_task` → `submit_url` → Data API) inserts rows with title/channel/`duration_ms`/`is_stream` already populated; staff `/audio` (`submit_trusted_url_task`) inserts NULL metadata and the browser backfills `duration_ms` on first play via `record_browser_duration`.
- Status: Active
- Parent context: `../../../../CONTEXT.md`

---

## 1. Scope

Owned by this domain:
- Always-on Icecast house radio playback (the `<audio>` and CLI symphonia path).
- Global, DB-backed YouTube queue: submission, persistence, single-playing invariant, server-driven track switching (per-browser playback timeline), fallback debounce.
- The singleton "YouTube fallback" stream that plays when the queue is empty.
- Audio source arbitration between paired CLI and paired browser clients on the same SSH token (`set_playback_source` + browser Icecast gate).
- Synthetic browser-pair visualizer used for both Icecast and YouTube.
- Now-playing poller for the Icecast track title.
- The `/audio` and `/audio fallback` SSH chat commands (staff-only).

Out of scope here (lives elsewhere):
- Liquidsoap playlist/skip control — only called from `app/vote/svc.rs` (`liquidsoap.rs` is co-located here for historical reasons but is not used by `AudioService`).
- Icecast HTTP serving — external service, see root `CONTEXT.md` §2.7.
- CLI Icecast decode/output (`late-cli/src/audio/`) — owned by the CLI crate; this file only documents the WS/control wiring.
- The vote system that drives genre selection on Icecast.

---

## 2. File Map

```text
late-ssh/src/app/audio/
├── mod.rs                  # declarations only (booth, client_state, liquidsoap, now_playing, state, svc, viz, youtube)
├── svc.rs                  # AudioService: queue state machine, WS broadcast, resume, fallback debounce, periodic LoadVideo heartbeat, votes/skip-vote
├── state.rs                # AudioState: per-session UI shim — proxies submits/votes and turns AudioEvent into Banners
├── client_state.rs         # ClientAudioState + ClientKind/SshMode/Platform enums (the client_state WS payload)
├── liquidsoap.rs           # LiquidsoapController telnet client (NOT used by AudioService — only by app/vote/svc.rs)
├── viz.rs                  # Visualizer (procedural bars, legacy bands/RMS/beat) + ratatui render_inline
├── youtube.rs              # URL parsing + optional YouTube Data API validation client
├── booth/
│   ├── mod.rs
│   ├── state.rs            # BoothModalState: open flag, submit input, selected index, focus
│   ├── input.rs            # modal-open key dispatch (submit/queue focus, +/- vote, s skip)
│   └── ui.rs               # ratatui modal: submit row, current track, queue list with duration + score
└── now_playing/
    ├── mod.rs
    └── svc.rs              # NowPlayingService: 10s Icecast title poll, watch<Option<NowPlaying>>
```

Cross-crate touchpoints:
- `late-core/src/models/media_queue_item.rs`, `media_source.rs`,
  `media_queue_vote.rs` — DB models.
- `late-core/migrations/047_create_media_queue_items.sql`,
  `048_create_media_sources.sql`,
  `049_create_media_queue_votes.sql`.
- `late-core/src/audio.rs` — `VizFrame { bands[8], rms, track_pos_ms }` shared between server and CLI.
- `late-ssh/src/paired_clients.rs` — `PairedClientRegistry`, `PairControlMessage::SetPlaybackSource`, source/surface policy.
- `late-ssh/src/api.rs` — `/api/ws/pair` multiplexes `AudioWsMessage` + `PairControlMessage`; `/api/now-playing`.
- `late-ssh/src/app/chat/{state,input}.rs` — `/audio` and `/audio fallback` chat commands.
- `late-cli/src/ws.rs`, `late-cli/src/main.rs`, `late-cli/src/audio/output.rs` — CLI tolerates unknown audio events and gates Icecast output on `set_playback_source` without changing the user mute flag.
- `late-web/src/pages/connect/page.html` + `connect/mod.rs` — browser IFrame player, force-switch on heartbeat, per-user v+x source toggle.

---

## 3. Ownership Split

- `svc.rs` is the async boundary. It owns the DB, both broadcast channels, the queue state mutex, the playback timer (which also drives the periodic `LoadVideo` heartbeat for the current item), the fallback debounce timer, and all transitions. **Nothing else in the codebase mutates `media_queue_items.status` or `media_sources`.**
- `state.rs` is the per-session UI shim (62 lines). It clones the service, holds a per-user `AudioEvent` receiver, exposes `submit_trusted` / `set_youtube_fallback` for chat dispatch, and turns user-scoped events into banners during `tick()`.
- `client_state.rs` is type-only: the JSON shape clients send over `client_state` WS messages. No behavior.
- `youtube.rs` is pure URL/HTTP — no DB, no channels, no service state.
- `viz.rs` is pure render + signal smoothing. Lives in this domain because the data source (Icecast) is audio.
- `now_playing/svc.rs` is independent of `AudioService` — separate channel, separate task, only shares a directory.
- `liquidsoap.rs` is dead weight from this domain's perspective; kept here because the file got moved from `app/vote/` during consolidation and only `vote` re-imports it.

Keep `mod.rs` declaration-only — no `pub use` re-exports.

---

## 4. AudioService (`svc.rs`)

### Channels and state
- `ws_tx: broadcast::Sender<AudioWsMessage>` (cap 512) — server-authoritative pair-WS events, fanned out to every paired client.
- `event_tx: broadcast::Sender<AudioEvent>` (cap 256) — per-user banners (success/failure on submit, fallback set). Consumed only by `AudioState`.
- `state: Arc<Mutex<QueueState>>` — `{ mode: AudioMode, current_item_id, sequence, playback_cancel: Option<oneshot>, fallback_cancel: Option<oneshot> }`.

### Constants (`svc.rs:15-21`)
- `QUEUE_SNAPSHOT_LIMIT = 50`
- `MAX_SUBMISSIONS_PER_WINDOW = 10` over `SUBMISSION_WINDOW = 5 minutes` — applies to un-trusted `submit_url`, which is the path reached by the Music Booth submit modal (`booth_submit_public_task`). Trusted/admin paths (`submit_trusted_url`) bypass.
- `FALLBACK_DEBOUNCE = 10s`
- `PLAYBACK_HEARTBEAT_INTERVAL = 10s` — periodic `LoadVideo` re-broadcast for the current item. Safety net: browsers already showing the right item no-op; stuck/disconnected/wrong-item browsers force-swap. Replaces the old `Seek`-based sync.
- `RECONCILE_INTERVAL = 60s` — background DB reconcile safety net. If memory drifts from the singleton `playing` row (e.g. rollout overlap), the service adopts the DB current, cancels/re-arms timers, and republishes state.
- `STREAM_CAP = 1h` — hard cap on any single playing row's wall-clock lifetime.
- `SKIP_VOTE_FRACTION = 0.3` + `SKIP_VOTE_MIN = 2` — `skip_threshold(youtube_total) = max(ceil(0.3 * youtube_total), 2)`. **Denominator is YouTube listeners only** (`PairedClientRegistry::total_youtube_listeners()`) — paired browsers whose user has `audio_source = Youtube`. CLI-only or Icecast-pinned browsers don't count in either numerator or denominator. Floor of 2 means a lone listener can't solo-skip; the 30% ceil kicks in above 6 YouTube listeners.

### Public API
- `new(db, youtube_api_key)` — `main.rs:123`.
- `start_background_task(shutdown)` — sweeps orphan `playing` rows, then resumes from DB, then idles. `main.rs:360`.
- `subscribe_ws()` — `api.rs:237` (pair WS upgrade).
- `subscribe_events()` — `app/audio/state.rs`.
- `initial_ws_messages()` (`svc.rs:393-423`) — catch-up burst sent on every new pair-WS connect: `source_changed`, `queue_update`, and `load_video` for the current playing item or for the configured fallback.
- `snapshot()` — returns `QueueSnapshot { mode, current, queue }`. Type exists but no HTTP route exposes it (see §14).
- `submit_url` / `submit_url_task` — un-trusted, rate-limited, validates via YouTube Data API. **Called by `booth_submit_public_task`** (the in-TUI booth modal submit). Requires `LATE_YOUTUBE_API_KEY`; when unset, `booth_submit_enabled()` returns false and the modal disables the submit row. Inserted rows carry `title`, `channel`, `duration_ms`, and `is_stream` from the Data API — so booth-queued items render their `m:ss` duration in the queue list immediately.
- `booth_submit_public_task` — wraps `submit_url` for the booth modal: emits `AudioEvent::BoothSubmit{Queued,Failed}` (user-scoped banners) and shows "Disabled" if the API key is missing. **This is the user-facing submit path.**
- `submit_trusted_url` / `submit_trusted_url_task` — used by `/audio` (staff). Bypasses rate limit and Data API; uses `youtube::trusted_video_from_url` to parse the ID only. Inserts `title=NULL`, `channel=NULL`, `duration_ms=NULL`, `is_stream=false` — duration is backfilled by the browser on first play via `record_browser_duration` (svc.rs:1261). Until then, the booth queue list shows a blank duration for staff-queued items.
- `set_trusted_youtube_fallback` / `set_trusted_youtube_fallback_task` — used by `/audio fallback`. Upserts the singleton `media_sources` row.
- `report_player_state` / `report_player_state_task` — `api.rs:329`, ingress for browser `player_state` reports.

### Startup lifecycle
1. `sweep_orphan_playing` (`svc.rs:425-438`) marks any `status='playing'` row older than `now - 1h` as `failed` with `error = "orphan playing row swept at startup"`.
2. `resume_from_db` (`svc.rs:440-460`) reads the lone `playing` row (if any). If `started_at + duration` still in the future, broadcasts a fresh `LoadVideo` with the correct `offset_ms` and re-arms the playback timer. Otherwise marks it `played` and advances.
3. Service is then driven purely by inbound chat submissions, browser player_state reports, and timer fires.

### State machine
DB statuses: `queued → playing → {played | skipped | failed}`.

All transitions go through `svc.rs`:
- `queued → playing`: `mark_playing` conditional `UPDATE … WHERE id=$1 AND status='queued'`. Before promoting, `advance_to_next_with_guard` first checks for an existing DB `playing` row and adopts it. If `mark_playing` races the singleton index (`idx_media_queue_single_playing`), the service treats that as a reconcile signal instead of surfacing a submit failure.
- `playing → played`: `finish_item` or `finish_item_due_to_timer` via `mark_played` (`WHERE status='playing'`). If zero rows changed, memory was stale; reconcile from DB instead of returning with the old `current_item_id`.
- `playing → failed`: `fail_item` via `mark_failed`. Only fired when the browser reports `player_state: error` for the active item; zero-row updates reconcile like `mark_played`.
- `playing → skipped`: staff `/audio skip` and threshold skip use `mark_skipped` (`WHERE status='playing'`). A stale pod cannot mutate an already-played row to `skipped`; zero-row updates reconcile and ask the caller to retry.

`advance_to_next_with_guard` is the *only* advancer. It adopts a DB current first, otherwise picks `MediaQueueItem::first_queued()`, tries to flip it, on success broadcasts `SourceChanged: youtube` + `LoadVideo` + `QueueUpdate`. If the queue is empty it tries `publish_youtube_fallback_with_guard`; if no fallback row exists, `schedule_fallback` arms the 10s debounce, after which `finish_fallback_debounce` flips `mode = Icecast` (and re-checks `current_item_id.is_none()` to avoid races).

### Timers
- **Playback timer** (`schedule_playback_timer`): one `tokio::select!` task per playing item. Sleeps `duration - elapsed` then calls `finish_item_due_to_timer`. Also re-broadcasts `LoadVideo` for the current item every `PLAYBACK_HEARTBEAT_INTERVAL = 10s` from inside the same task — the safety-net heartbeat. Browsers ignore the heartbeat when they're already showing the right item; otherwise they force-swap.
- **Fallback debounce**: one task armed when the queue drains. Cancelled by any new submission via `cancel_fallback`.
- **Periodic reconcile**: every 60s the service compares memory to the DB singleton `playing` row. Reconcile is a full transition: cancel stale timers, clear skip-votes if the current changed, schedule the DB current's timer, and republish queue/load events. If memory says current but DB has none, it clears stale state and advances/fallbacks.
- Timers are owned via `oneshot` cancel handles on `QueueState`; dropping the sender cancels the task.

### `playback_duration` rules (`svc.rs:1197-1205`)
- `is_stream = true` → always `STREAM_CAP` (1h).
- Non-stream with known `duration_ms` → `min(duration_ms, STREAM_CAP)` — **1h is a hard cap on every item, not a fallback.** A 2h video plays its first hour, server timer fires, queue advances.
- Non-stream with unknown duration → `STREAM_CAP` (1h).
- `record_browser_duration` (`svc.rs:1100-1121`) is the only path that backfills `duration_ms` from the browser, conditionally on the current playing item and only when the DB value is NULL. After write, it reschedules the playback timer to `min(real_duration, STREAM_CAP)`.
- `playback_known_duration` (uncapped) is still used by `finish_item_from_player` (`svc.rs:859`) to reject premature browser `ended` reports — a 2h video that the browser claims ended at 30min is rebutted with a `Seek`, regardless of the 1h playback cap.

### `player_state` ingress
Routed by report `state` field:
- `ended` → `finish_item_from_player`. Drops the report if `current_item_id != report.item_id`. Otherwise trusts it and calls `finish_item` — no duration check, no grace gate, no seek rebuttal. Server's own playback timer is the redundant safety net for browsers that never report `ended`.
- `error` → `fail_item`.
- `playing` / `paused` / `buffering` → may carry `duration_ms` for `record_browser_duration`; otherwise logged. `autoplay_blocked = true` logs at `warn!`.

### Invariants
1. **Singleton playing row.** Enforced both by the partial unique index `idx_media_queue_single_playing` and by conditional `mark_playing` updates. Two racing advancers cannot both succeed; losers reconcile to the DB current.
2. **Server owns track *changes*, not playback positions.** Server picks which item is `playing` and broadcasts `LoadVideo` on changes + every 10s as a heartbeat. Each browser plays its own timeline from wherever YT happens to start. No more wall-clock-offset sync — slow networks no longer audibly skip mid-track.
3. **Force-switch on heartbeat.** A browser receiving `LoadVideo` for a different `item_id` than what it's currently playing MUST swap, regardless of pause/buffer/error state. Same-`item_id` heartbeat with the right `video_id` loaded → no-op (respect a manual pause).
4. **`ended` is trusted.** Server advances unconditionally when the playing item's browser reports `ended`. The own-timer is the backup for browsers that never report.
5. **Mode is server-managed.** Browser/CLI never write `mode`; they only receive `SourceChanged`.
6. **Sequence monotonicity.** `state.sequence` is bumped before every `QueueUpdate` so clients can drop stale ones.
7. **Banners are user-scoped.** `AudioEvent` carries `user_id` and `AudioState::tick` filters on it; one user's submission failure does not leak to others.
8. **DB beats memory on drift.** Any zero-row terminal transition (`mark_played` / `mark_failed` / `mark_skipped`) or singleton conflict routes through reconcile. Reconcile never blindly clears `current_item_id` while DB still has a `playing` row.

---

## 5. WebSocket Protocol (multiplexed on `/api/ws/pair`)

`api.rs` `handle_socket` (`api.rs:231-382`) drives three sources per connection with `tokio::select!`:
- inbound `socket.recv()` — client → server
- `control_rx` — `PairControlMessage` from `PairedClientRegistry` (mute/volume/source/clipboard)
- `audio_rx` — `AudioWsMessage` from `AudioService::subscribe_ws()`

On connect, `api.rs` sends the user's persisted `set_playback_source` first, then
`audio_service.initial_ws_messages()` emits the catch-up burst. This ordering keeps
the browser from briefly assuming the default Icecast preference and staging a
YouTube item without entering the switching/playback path.

### Server → client `AudioWsMessage` (tagged enum, snake_case)
- `load_video { item_id, video_id, is_stream }` — sent on track changes AND every 10s as a heartbeat. Browsers swap when `item_id` differs from what they're playing; same-item heartbeat is a no-op.
- `source_changed { audio_mode: "icecast" | "youtube" }`
- `queue_update { current, queue, sequence }`

### Server → client `PairControlMessage` (`paired_clients.rs:22-30`)
- `toggle_mute`, `volume_up`, `volume_down`, `request_clipboard_image`.
- `set_playback_source { source: "icecast" | "youtube", web_icecast_enabled: bool }` — sent immediately on pair-WS connect, after persisted `v+x` source changes, and when CLI presence changes. CLI ignores `web_icecast_enabled`; browsers use it to avoid double Icecast when a CLI is paired.

### Client → server `WsPayload` (`api.rs:39-68`)
- `heartbeat`
- `viz { position_ms, bands[8], rms }` — legacy/compat payload; the current web page does not send it
- `client_state { client_kind, ssh_mode, platform, capabilities, muted, volume_percent }`
- `clipboard_image { … }`, `clipboard_image_failed { … }`
- `player_state(PlayerStateReport)` — `{ item_id, state, offset_ms?, duration_ms?, autoplay_blocked, error? }` (`svc.rs:126-138`)

There is **one global broadcast**, no room scoping. Every paired browser on every token receives the same `load_video` / `source_changed` / `queue_update`.

---

## 6. Source Arbitration (single audible surface)

Policy lives in `late-ssh/src/paired_clients.rs` plus the browser/CLI followers. There is no `ForceMute` control message anymore; the server broadcasts `set_playback_source { source, web_icecast_enabled }` and clients gate themselves.

Rule: **Icecast belongs to the CLI when a CLI is paired; YouTube belongs to the browser.** When a CLI and browser are both paired and the user flips from YouTube back to Icecast, the browser pauses/silences YouTube and does **not** start its own Icecast `<audio>` element, preventing doubled radio streams.

| CLI paired | Browser paired | Source  | Audible surface                                      |
|------------|----------------|---------|------------------------------------------------------|
| yes        | no             | Icecast | CLI                                                  |
| yes        | no             | YouTube | silent (CLI cannot decode YouTube)                   |
| yes        | yes            | Icecast | CLI; browser web-Icecast disabled                    |
| yes        | yes            | YouTube | browser iframe; CLI source gate emits silence        |
| no         | yes            | Icecast | browser `<audio>` (`web_icecast_enabled = true`)     |
| no         | yes            | YouTube | browser iframe                                       |

Mechanics:
- `PairControlMessage::SetPlaybackSource { source, web_icecast_enabled }` is sent on pair-WS connect, on persisted `v+x` source changes, and when CLI presence changes for a token.
- CLI stores `source_is_icecast`; output emits silence when `source != Icecast` without touching the user `muted` flag.
- Browser stores `webIcecastEnabled`; `source=Icecast && webIcecastEnabled=false` pauses YouTube and stops the web Icecast element. If the CLI disconnects, the server replays the same source with `web_icecast_enabled=true` so a browser-only token can resume web Icecast.

### Skip-vote eligibility — only YouTube listeners

Each `PairControlEntry` carries `user_id: Uuid` (resolved from `SessionRegistry::user_for(token)` during the pair-WS upgrade) and `audio_source: AudioSource` (cached from `users.settings.audio_source`, read at registration time).

Helpers used by the skip-vote path:
- `has_youtube_listener(token) -> bool` — any browser on this token with `audio_source == Youtube`.
- `total_youtube_listeners() -> usize` — count of such entries across all tokens.
- `set_audio_source(user_id, source) -> bool` — updates every entry for the user; returns `true` when at least one entry transitioned **away from** `Youtube`. Called from `AudioService::persist_audio_source` after the DB write succeeds.

Vote-strip on flip-away: when `set_audio_source` returns `true`, `AudioService::persist_audio_source` removes the user from `state.skip_votes` and runs `reevaluate_skip_threshold` (which may fire a skip if the threshold dropped to meet remaining votes).

Eligibility table:

| Has paired browser | Browser's `audio_source` | Can skip-vote? | Counts toward threshold? |
|--------------------|--------------------------|----------------|--------------------------|
| no                 | n/a                      | no             | no                       |
| yes                | Icecast                  | no             | no                       |
| yes                | Youtube                  | yes            | yes                      |

A user with multiple browser tabs in YouTube mode counts each tab toward the denominator but still only contributes one vote (HashSet on `user_id`). Staff `/audio skip` (`force_skip`) bypasses the threshold entirely.

---

## 7. Chat Commands (`/audio`, `/audio fallback`, `/audio skip`)

Parsing: `late-ssh/src/app/chat/state.rs` around the `/audio` block.
- Exact match `/audio skip` is checked first (otherwise `strip_prefix("/audio ")` would treat `skip` as a URL).
- Longer prefix `/audio fallback ` is matched next.
- Staff gate: `is_admin || is_moderator`. Non-staff get banner `"/audio is staff-only"`.
- Empty arg → `"Usage: /audio <youtube-url>"` or `"Usage: /audio fallback <youtube-url>"`.
- Valid requests stash into `requested_audio_url` / `requested_audio_fallback_url` / `requested_audio_skip`.

Dispatch: `late-ssh/src/app/chat/input.rs` `handle_post_submit_requests` calls `app.audio.submit_trusted(url)`, `app.audio.set_youtube_fallback(url)`, or `app.audio.skip_trusted()`, which proxy through `AudioState` to `AudioService::{submit_trusted_url_task, set_trusted_youtube_fallback_task, force_skip_task}`.

The unrelated bare `/music` command (`state.rs:1325`) opens a help topic, not a submission. Don't confuse the two — `/music` ≠ submit.

`/audio` flow:
1. `youtube::trusted_video_from_url(url)` extracts the 11-char ID. Accepted forms: `youtube.com/watch?v=…`, `youtu.be/…`, `youtube.com/embed/…`, `youtube.com/shorts/…`, `youtube.com/live/…`, subdomains via `host.ends_with(".youtube.com")`. Anything else returns an `anyhow` error (lowercase, per repo style).
2. `MediaQueueItem::insert_youtube` writes the row with `status='queued'`, `media_kind='youtube'`, title/channel/duration as NULL, `is_stream=false`.
3. If nothing is currently playing, `advance_to_next_with_guard` immediately flips it to `playing` and broadcasts.
4. On success, banner via `AudioEvent::TrustedSubmitQueued` — "Queued audio — up next" or "Queued audio — #N in line" depending on position. On failure (URL parse, rate limit, DB), banner via `AudioEvent::TrustedSubmitFailed` carrying a classified message from `trusted_submit_error_message` (svc.rs:835) — one of "Invalid YouTube URL", "Slow down — too many submissions", or "Failed to queue audio".

`/audio fallback` flow:
1. `youtube::trusted_video_from_url(url)` (same parser).
2. `MediaSource::upsert_youtube_fallback` — `ON CONFLICT (source_kind) DO UPDATE`, always sets `is_stream=true`.
3. If the queue is empty *and* no item is playing, immediately broadcasts `SourceChanged: youtube` + `LoadVideo` for the fallback so paired browsers start it without waiting.
4. On success, banner via `AudioEvent::YoutubeFallbackSet` — "Set YouTube fallback". On failure, banner via `AudioEvent::YoutubeFallbackFailed` carrying the classified message from `trusted_submit_error_message`.

`/audio skip` flow:
1. Routes through `AudioService::force_skip` — unconditional, bypasses the vote threshold (the threshold is a *listener* signal; staff can skip directly).
2. Marks the current playing row `skipped` via `MediaQueueItem::mark_skipped` (`WHERE status='playing'`), clears `current_item_id` and any pending `skip_votes`, cancels the playback timer, and runs `advance_to_next_with_guard` to bring up the next queued item (or arm the fallback debounce).
3. If the row was already no longer `playing`, the service reconciles from DB instead of mutating the stale row and asks the caller to retry. On success, banner via `AudioEvent::TrustedSkipFired` — "Skipped audio". On failure (nothing playing, state changed, DB error), banner via `AudioEvent::TrustedSkipFailed` — "Nothing is playing" or "Failed to skip audio".

---

## 8. CLI Integration

Goal: the CLI tolerates everything new the audio domain added, plays Icecast when selected, and stays silent when the user selects YouTube.

- **Unknown audio events ignored** (`late-cli/src/ws.rs`). Inbound text is parsed only as `PairControlMessage`. `load_video`, `source_changed`, `queue_update` fail to deserialize, the CLI logs `warn!("ignoring unsupported pair websocket event")`, and the select loop continues. **The CLI does not disconnect on audio events.** Note: each playing track now also produces a 10s `load_video` heartbeat — the CLI log noise budget should account for that.
- **Source gate, not forced mute.** `set_playback_source` updates `source_is_icecast`; `late-cli/src/audio/output.rs` emits silence when it is false. The user-controlled `muted` atomic remains only the local mute keybind / paired mute control.
- **Embedded YouTube webview lifecycle.** The same `set_playback_source` message drives `late-cli/src/ws.rs::WebviewPlaybackController`: `youtube` spawns one `late webview-pair <token>` child, while `icecast` kills the helper. Do **not** spawn the helper from global `source_changed`.
- **YouTube capability.** Native CLI `client_state.capabilities` includes `"youtube"` on desktop platforms. The server still sends `set_playback_source` to every paired entry; older/plain CLIs simply gate Icecast, while YouTube-capable CLIs also launch the helper.
- **CLI identifies itself.** First native `client_state` emitted by `late-cli/src/ws.rs` carries `"client_kind": "cli"`. The helper sends `"client_kind": "browser"` for v1 so existing source arbitration, listener counts, and skip-vote eligibility work without a new enum.

---

## 9. Web Connect Page Integration

File: `late-web/src/pages/connect/page.html`. The audio source is decided in the browser; the YouTube API/player is lazy-loaded only when the browser actually enters YouTube mode.

- **Per-user audio source (server-authoritative).** The choice is persisted in `users.settings.audio_source` (`icecast` | `youtube`, default `icecast`). TUI `v+x` flips the value via `App::toggle_paired_playback_source`: writes to DB through `AudioService::persist_audio_source`, updates the local mirror `App.paired_browser_source`, and broadcasts `PairControlMessage::SetPlaybackSource { source, web_icecast_enabled }` to paired clients. On pair-WS connect, `api.rs` sends the persisted source before the audio catch-up burst. On browser pair-up the SSH session also replays the value; on CLI presence changes `api.rs` replays it for the token so browsers know whether web Icecast is allowed. The browser is a follower: `applyUserPlaybackSource(source, web_icecast_enabled)` stores `userOverrideMode` and applies. While the user is pinned to icecast, `loadYoutubeVideo` early-returns so server queue events do not flip the iframe back on (the current item is still stashed as `pendingYoutubeItem` so a toggle to youtube starts playing immediately). The native CLI follows the same source message: it gates Icecast locally and only spawns the embedded webview helper for `youtube`.
- **IFrame API load.** The page does not include the YouTube iframe API up front. `ensureYoutubePlayer()` calls `loadYoutubeApi()` on demand, which appends `https://www.youtube.com/iframe_api`; `window.lateYoutubeApiReady` / `onYouTubeIframeAPIReady` then create the player only if `audioMode === "youtube"`.
- **`source_changed` / `set_playback_source` swap** (`applySourceMode`). Into `youtube`: stop `<audio>`, ensure player exists, kick playback of pending item. Into `icecast`: `ytPlayer.pauseVideo()`; restart the web `<audio>` only when `webIcecastEnabled` is true. With a CLI paired, `webIcecastEnabled=false`, so the browser goes quiet and the CLI is the only Icecast surface. The `modeChanged` guard prevents repeated `source_changed: youtube` broadcasts during queue transitions from resetting the iframe.
- **Icecast-pinned resource behavior.** While pinned to Icecast, `load_video` only stashes `pendingYoutubeItem`; it does not create the YouTube iframe or pre-cue the video. A later source flip to YouTube starts from the pending item, and the server's 10s `load_video` heartbeat remains the safety net.
- **`load_video` → force-switch or no-op** (`loadYoutubeVideo`). New shape: payload is `{ item_id, video_id, is_stream }` — no offset, no started_at. Same `item_id` AND iframe is already showing the right `video_id` → no-op (this is the safety-net heartbeat path; a manual pause stays paused). Otherwise → `loadVideoById({ videoId })` from 0, swap `currentYoutubeItem`. `verifyYoutubeLoad` re-checks after 1s and reloads if the video id still mismatches.
- **No drift correction.** Each browser plays its own timeline. Slow networks just lag behind — no `seekTo` jumps. The "everyone hears the same offset" invariant is dropped on purpose.
- **`player_state` reports** (`sendYoutubeState`). Emits `{ event: 'player_state', item_id, state, offset_ms, duration_ms, autoplay_blocked, error }` on YT state transitions (PLAYING/PAUSED/BUFFERING/ENDED). No periodic loop. Server reads `duration_ms` for backfill via `record_browser_duration`; the rest is informational.
- **Autoplay-blocked**. 1.5s after `loadVideoById`, if the YT state is still `CUED`/`UNSTARTED`, sets `autoplayBlocked = true`, emits `player_state: buffering` with the flag, and the UI swaps to `[ tap to play ]`. Tap routes through `startPlayback` → `ytPlayer.playVideo()`.
- **`queue_update` is currently a no-op** in the browser (no UI to show it). The event ships so a future surface can use it.

---

## 10. Visualizer (`viz.rs`)

- Browser-paired audio is synthetic-only for both Icecast and YouTube. The web
  page does not create a Web Audio `AudioContext`, does not run an analyzer, and
  does not send `viz` frames.
- `app/tick.rs` turns `Visualizer::procedural_active` on only when the browser
  is the audible surface: YouTube mode, or browser-only Icecast
  (`web_icecast_enabled = true`). If a CLI is paired and the user is in
  Icecast mode, the CLI owns Icecast and real CLI `VizFrame`s remain visible.
- `render_inline(frame, area)` is the borderless sidebar render. Idle shows `"no audio paired"` / `"/music in chat"` / `"P install · pair"` (last only when height ≥ 5). Real CLI frames use attack/release smoothing, idle band decay, and the same **sub-cell vertical resolution** (`▁▂▃▄▅▆▇█`, 9-step) as the procedural path; real bars use dim/normal/glow amber by intensity. Procedural live draws dim amber 1-cell-wide bars with 1-cell gaps. Bar heights come from layered sines — a primary traveling wave, a faster per-band shimmer, and a slow global breath term (incommensurate frequencies so the pattern doesn't visibly repeat in a few seconds). No spectrum-style tilt is applied on the procedural path; the wave shape is decorative, not a frequency analog.
- The `VizFrame`/`Visualizer::update` path still drives CLI Icecast
  visualization. Browser web playback no longer sends those frames, and
  procedural rendering takes priority only while the browser is the audible
  surface.

**Future unlock: OS audio loopback.** Once the CLI hosts its own playback (embedded webview track), the cross-origin constraint disappears entirely — we capture local audio output at the OS layer (PipeWire / WASAPI / ScreenCaptureKit) and feed real `VizFrame`s through the existing pipeline for every source, including YouTube. See §18 for the parked plan. Until that lands, procedural bars are the only honest YouTube-mode indicator.

---

## 11. Now-Playing (`now_playing/svc.rs`)

- Shared `watch::Sender<Option<NowPlaying>>` reflects the current Icecast track title.
- `start_poll_task` spawns a blocking thread that calls `late_core::icecast::fetch_track` every 10s (split into 1s sleeps to shut down quickly). Only emits when the title string changes.
- Independent of `AudioService` — does not subscribe to its channels.
- Consumers: `GET /api/now-playing` (`api.rs:131`), and the sidebar music-stage widget (`app/common/sidebar.rs::draw_icecast_block`) which renders `Artist - Title` plus a progress/elapsed line under the icecast title. When the watch hasn't ticked yet, the block shows `no signal` and the progress row stays blank.

---

## 12. Sidebar music-stage widget (`common/sidebar.rs`)

Renders the audio domain into the right rail. Both surfaces (YouTube + Icecast) are always visible; the active source the user is hearing gets bold amber chrome, the other gets dim italic. Entry point: `app/common/sidebar.rs:draw_music_stage`, allocated `MUSIC_STAGE_HEIGHT = 17` rows. Both blocks share the same row shape — title, track (combined on one line), progress, then surface-specific tail — so the active/inactive comparison reads naturally.

### Layout

| Row(s) | Content |
|--------|---------|
| 0      | Volume bar: `vol  ▰▰▰▰▰▱▱▱▱▱  60%`. Renders `muted` (italic faint) when muted, `—` when no client is paired. |
| 1      | Volume keybind hints: `m mute  -= vol`. |
| 2-7    | YouTube block: title bar, track (`Channel - Title` combined on one row; falls back to `by <submitter> - Title` when channel is unknown, then to bare title), progress, skip meter (with trailing `v+s` hint when active), `next ⌄` header, queue items (`Min(2)`, absorbs spare space). |
| 8      | Booth/swap keybind hints: `v+v queue  v+x swap`. |
| 9-13   | Icecast block: title bar, track (`Artist - Title` combined on one row), progress/elapsed line (uses `draw_progress_line` when `duration_seconds` is known, `draw_elapsed_line` otherwise), `vibe → next · ends` one-liner, then a 3-row vote area delegated to `app/vote/ui.rs::draw_vote_inline`. Track + progress fall back to `no signal` and a blank row when the `now_playing` watch hasn't emitted yet. |

### Active-source rule

```rust
yt_active = paired_browser_source == AudioSource::Youtube
```

Pure preference-based. Does **not** gate on `is_browser`. The saved preference (loaded from `users.settings.audio_source` via `extract_audio_source` during SSH bootstrap, `ssh.rs:883`, mirrored in `App.paired_browser_source`) is the source of truth from the first frame. Pairing-completion does not change the visual state — earlier versions waited for the browser to pair before honoring the pref, which read as a startup glitch (sidebar showed Icecast for ~1s then flipped). Don't add the `is_browser` guard back.

The volume row stays honest about pairing (`vol  —` when nothing paired), so users aren't misled about whether their preference is currently audible.

### Title-bar listener tags

Both blocks always show their live listener count in the title-bar tag slot — `youtube  ────  5` / `icecast  ────  12`. Active vs inactive is communicated by color/weight (amber bold vs italic faint), not by case (label is always lowercase) and not by tag presence. The counts are sourced live from `PairedClientRegistry::total_youtube_listeners()` / `total_icecast_listeners()` via `AudioService` accessors; both filter to paired browsers — CLI is intentionally excluded.

### Fallback-not-empty semantics

The widget treats "no submitted track" and "fallback playing" as the same state. When `queue.current.is_none()`:
- Title tag still shows the YouTube listener count (no separate "loop"/"fallback" badge anymore — the body row carries that information).
- Body renders `fallback stream` / `YouTube · 24/7` plus a `queue with v+v` hint.
- When a track is playing but queue is otherwise empty, the trailing "next" row says `· fallback next`, not "queue ends".

No copy anywhere reads "queue empty". The user has pushed back on that wording multiple times; in their product framing the fallback is the steady state, not a placeholder. See `feedback_fallback_not_empty.md` in auto-memory.

### Data sources

- `queue_snapshot: &QueueSnapshot` — from `AudioState::queue_snapshot()` watch channel.
- `vote: VoteCardView<'_>` — from the genre vote state.
- `paired_client: Option<&ClientAudioState>` — for `volume_percent` and `muted` (vol row only).
- `paired_browser_source: AudioSource` — App's per-user mirror.
- `youtube_listener_count: usize` / `icecast_listener_count: usize` — live counts from the registry via `AudioService::{youtube,icecast}_listener_count()`. Browsers only; refreshed every render tick.
- `now_playing: Option<&NowPlaying>` — Icecast title + duration source, from `NowPlayingService` (§11). Drives the icecast track and progress rows.

### Internal helpers (all in `sidebar.rs`)

- `stage_title_line(area_w, label, tag, active)` — shared title-bar renderer. Label is always lowercase. Active → amber bold label + amber-dim tag; inactive → italic faint label + tag. No `▶ ` glyph prefix on the tag (color + position read as a state badge; the prefix was eating cells on narrow rails).
- `draw_volume_row` — the vol bar.
- `draw_keybind_row(frame, area, &[(key, label), ...])` — adaptive hint renderer; drops trailing groups when the rail is too narrow rather than mid-word truncating.
- `draw_youtube_block` / `draw_icecast_block` — fixed-size block renderers.
- `skip_meter_spans(progress)` — includes a trailing `v+s` keybind hint inline.
- `queue_next_line(idx, item, width)` — number flush at column 0 (no leading indent) to maximize title width.

### Cross-cuts

- Reuses `late-ssh/src/app/vote/ui.rs::draw_vote_inline` for the icecast vote rows. That helper uses `●`/`○` glyphs (matches the `seat_dot_spans` pattern), not block bars.
- v+x dispatch goes through `app/state.rs::toggle_paired_playback_source` → persists `paired_browser_source` via `AudioService::persist_audio_source`, which updates every paired registry entry for the user and broadcasts `PairControlMessage::SetPlaybackSource { source, web_icecast_enabled }`. The preference is meaningful even with only a CLI paired: Icecast mode plays native CLI radio, while YouTube mode silences native Icecast and starts the embedded webview helper on capable CLI builds.

---

## 13. Data Model

### `media_queue_items` (migration `047`)
- `id` uuidv7, `created`/`updated` tz, `submitter_id → users ON DELETE CASCADE`.
- `media_kind` CHECK `IN ('youtube')`, `external_id` non-empty, `title`/`channel` nullable, `duration_ms ≥ 0` nullable, `is_stream BOOLEAN`.
- `status` CHECK `IN ('queued','playing','played','skipped','failed')`. `skipped` is reserved/unused.
- `started_at`, `ended_at`, `error` nullable.
- Indices: `(status, created)` for queue scans; `(submitter_id, created DESC)` for rate-limit / submitter views.
- **Singleton playing constraint:** `CREATE UNIQUE INDEX idx_media_queue_single_playing ON media_queue_items ((true)) WHERE status = 'playing'`.

### `media_sources` (migration `048`)
- `id` uuidv7, timestamps, `source_kind` CHECK `IN ('youtube_fallback')`, `media_kind` CHECK `IN ('youtube')`.
- `external_id` non-empty, `title`, `channel`, `is_stream BOOLEAN NOT NULL DEFAULT true`, `updated_by → users ON DELETE SET NULL`.
- Unique index on `source_kind` → singleton fallback row, upserted via `MediaSource::upsert_youtube_fallback`.

Model helpers (`late-core/src/models/media_queue_item.rs`, `media_source.rs`):
- `MediaQueueItem::{insert_youtube, find_by_id, list_snapshot, queued_before_count, recent_submission_count, first_queued, current_playing, mark_playing, mark_played, mark_failed, mark_skipped, set_duration_if_missing, sweep_orphan_playing}`. Status/kind constants: `STATUS_QUEUED`, `STATUS_PLAYING`, `STATUS_PLAYED`, `STATUS_SKIPPED`, `STATUS_FAILED`, `KIND_YOUTUBE`.
- `MediaSource::{youtube_fallback, upsert_youtube_fallback}`. Constants: `KIND_YOUTUBE_FALLBACK`, `MEDIA_KIND_YOUTUBE`.

---

## 14. Known Gaps and Things to Watch

- **`GET /api/queue` is intentionally not exposed.** `AudioService::snapshot()` and `QueueSnapshot` exist for in-process use only. The TUI booth modal reads the snapshot from `AudioState::queue_snapshot()` (a `watch::Receiver<QueueSnapshot>` populated by `publish_queue_update_with_guard`); browsers receive state via the `initial_ws_messages` catch-up burst and live `queue_update` events. An external route would only matter for non-paired observers, which we do not have today.
- **Booth modal renders from `watch::Receiver<QueueSnapshot>`.** `AudioService` keeps a `snapshot_tx` watch sender alongside the broadcast channels; every `publish_queue_update_with_guard` uses `send_replace` to store the latest snapshot even when zero receivers are alive (startup often publishes before any SSH booth exists), and `AudioState::queue_snapshot()` borrows the current value. Skip progress (`votes/threshold`) is folded into the snapshot before it ships.
- **`liquidsoap.rs` lives here but is only used by `app/vote/svc.rs`.** AudioService does *not* drive Liquidsoap. Treat `AudioMode::Icecast` as a hint to the browser/CLI, not a Liquidsoap state change.
- **`/music` ≠ `/audio`.** `/music` is a help-topic command. `/audio` (and `/audio fallback`) are the submit commands. Don't conflate.
- **No `GET /api/queue` HTTP route.** Submit and visibility for end users happen through the SSH booth modal (submit + queue list) and the staff `/audio` chat command. Non-paired observers have no way to see the queue today.
- **Multi-tab double audio** is unsolved. Two browser tabs on the same token both play. Deferred until UI work.
- **Region locks / embedding disabled** are not caught at submit time — `/audio` skips the YouTube Data API. The browser reports `error`, the server marks `failed`, queue advances. Pre-validation comes back with the public submit flow.
- **`LATE_YOUTUBE_API_KEY` is optional today** (`config.rs:200`, `optional()`). Required only for `submit_url` (un-trusted), which has no caller. Set it before reviving public submit.
- **Queue state-drift / singleton-violation stuck state.** Took down prod once already (2026-05-19). The class of bug is non-atomic two-write transitions (DB row status + in-memory `state.current_item_id`); any divergence is unrecoverable without a pod restart. The reconciliation contract in §19 is the active fix — any new code that flips `media_queue_items.status` or mutates `current_item_id` must route through it.

---

## 15. Design boundaries (won.t build)

These are intentional non-goals. Reopen only if the constraint that put them here changes.

- **CLI YouTube decoding via shell-out to an external player (mpv/vlc/yt-dlp wrapper).** Won't ship. The user-side ToS exposure (yt-dlp strips ads/branding) and the config burden (most users don't have a player wired up) put this firmly out of scope. The legal path for CLI-side YouTube is an embedded webview hosting the official IFrame Player.
- **Server-side YouTube fetching.** Server routes `video_id` only; the iframe is the only thing that talks to googlevideo.com.
- **Recording / persistent archive of YouTube audio.** Blocked by YouTube ToS.
- **Ad stripping.** The iframe plays whatever YouTube serves.
- **Lyrics, album art, fancy metadata.** Title + channel is enough.
- **Custom genre control per submission.** Fallback uses the global vote winner like everywhere else.
- **Real Web Audio analysis of the YouTube iframe.** Not possible — cross-origin iframe, no audio hook in the IFrame Player API. Browser-paired audio therefore uses the same synthetic visualizer for both Icecast and YouTube (§10) until OS-loopback capture exists.

---

## 16. Deferred (open backlog)

Open work that's been deliberately punted past v1. Each line is a "we know it's missing, here's the next-time hook."

- **Public `POST /api/queue/submit` HTTP route.** Booth submit goes through the in-process service. Revive when there's a non-SSH submitter (web form, third-party). YouTube Data API validation path is already in code (un-trusted route in `AudioService::submit_url_task`).
- **`GET /api/queue` HTTP route.** Snapshot exists in-process (`QueueSnapshot`); no external consumer today. See §14 first bullet.
- **TUI sidebar widget on Home for queue visibility.** Booth modal is the only surface today.
- **Heartbeat cadence tuning.** 10s `LoadVideo` re-broadcast was carried over from the old `PLAYBACK_SYNC_INTERVAL`. Could be slower (30s) once we have confidence stuck browsers don't accumulate.
- **Multi-tab dedupe.** Two browser tabs on the same token both play. Needs a "primary tab" election or a single-tab-per-token enforcement.
- **Region-lock partial failure UX.** Staff `/audio` skips the Data API; region-locked items fail at the browser via `error` → server marks `failed` → queue advances. Pre-validation would catch it at submit time.
- **Better admin feedback** when DB insert fails after local URL validation succeeds.
- **Browser-side voting UI.** Protocol already carries `vote_score` per item and `skip_progress` on the current item; no client renders them yet.
- **Weighted votes by role** (admin/mod ≠ user) — currently 1 user = 1 vote.
- **Vote history / reputation.**

---

## 17. CLI Embedded Webview for YouTube

**Status: v1 wired into the normal `late-cli` build.** Goal: legal YouTube playback inside the `late` CLI without shelling out to mpv/yt-dlp/etc. The CLI hosts the official YouTube IFrame Player inside an embedded system webview; the player fetches and decodes audio identically to today's connect page (§9). late.sh still ships only `video_id` over the pair WS.

### Process model

- Native `late` remains the always-on SSH/audio control process.
- Native `late` opens the normal pair WS as `client_kind = "cli"`.
- Native `late` advertises `capabilities: ["clipboard_image", "youtube"]` on desktop platforms.
- `set_playback_source: youtube` spawns a helper child: `late webview-pair <token>`.
- `set_playback_source: icecast` kills the helper and resumes native Icecast.
- The helper opens its own pair WS and reports `client_kind = "browser"` for v1, so existing browser force-mute, listener counts, and skip-vote eligibility work.

This lazy lifecycle is intentional. A normal CLI run does not open a webview. A webview window exists only while the user's persisted playback source is YouTube, avoiding tiling-window-manager noise for Icecast users.

### Source semantics

`set_playback_source` is the user's per-user preference and is the only signal that starts/stops the helper. `source_changed` is global queue/server mode and must not spawn the helper by itself. A user pinned to Icecast can still receive `source_changed: youtube` because the shared queue/fallback is globally active.

### Webview backend

`late-cli` uses `wry` + `tao`:

- Linux: WebKitGTK 4.1 dev/runtime packages plus GStreamer playback plugins. On Arch/EndeavourOS, `gst-plugins-good` is required for `autoaudiosink`; without it WebKit logs `GStreamer element autoaudiosink not found` and the YouTube iframe can remain black/unstarted even though pair/load events succeeded. `gst-libav` is also recommended for codec coverage.
- macOS: WKWebView.
- Windows: WebView2.

The helper serves `late-cli/src/webview/page.html` from a loopback-only ephemeral HTTP origin (`http://127.0.0.1:<port>/`) and loads that URL in the webview. Do not switch this back to `WebViewBuilder::with_html`: Wry's HTML string path gives the page a null origin, and YouTube can reject the iframe with player error 153. The page passes `window.location.origin` into the IFrame Player `origin` parameter, posts `player_state` back through wry IPC, and Rust relays those events to `/api/ws/pair` while pushing `load_video` / `source_changed` into JS via `evaluate_script`.

The helper owns its own mute/volume state. It registers as a browser, so pair-WS `toggle_mute`, `volume_up`, and `volume_down` controls must be applied inside `late-cli/src/webview/pair.rs` and forwarded into `page.html`; changing only the native CLI Icecast atom is not enough because YouTube audio is emitted by WebKit/GStreamer.

### Window UX

Current v1 opens a small companion window. Hidden/offscreen mode is not the default because embedded browser engines can throttle or unload hidden/minimized views, and the YouTube iframe's ads/branding/autoplay posture is cleaner with a visible surface. A future config can add `youtube_webview = "window" | "hidden" | "disabled"` once hidden-mode behavior is validated per platform.

### What this does NOT change

- Server queue state machine and YouTube `load_video` protocol.
- Browser connect page behavior.
- Native Icecast decoder path when `audio_source = icecast`.
- External-player shell-outs remain out of scope; do not revive mpv/yt-dlp handoff unless the product/legal posture changes explicitly.

---

## 18. Parked: OS audio loopback for CLI-side visualization

**Status: parked, not on the active build path.** Premised on the embedded-webview CLI playback work — when the CLI hosts its own audio output (not just decoding Icecast), the iframe cross-origin constraint that blocks all real YouTube viz today simply goes away. Captured here so the design unlock doesn't get lost when that track is picked up.

### Idea

Tap the CLI's own audio output at the OS layer, run FFT locally, emit `VizFrame { bands[8], rms, track_pos_ms }` through the existing pipeline. Works uniformly for YouTube, Icecast, and anything else the user plays through `late`. The current browser-pair synthetic visualizer (§10) can retire — viz becomes CLI-owned across every source, and the pair-WS `viz` fan-in can be removed.

### Per-platform capture

- **Linux**: PipeWire stream linked to the CLI's output sink's monitor source. PulseAudio monitor source as fallback for non-PipeWire systems.
- **Windows**: WASAPI loopback on the default render endpoint (`IAudioClient::Initialize` with `AUDCLNT_STREAMFLAGS_LOOPBACK`).
- **macOS**: ScreenCaptureKit audio (14+) for the modern path; CoreAudio aggregate / virtual-device plugin for older OS versions. Triggers a system-audio permission prompt the first time.

A single trait inside `late-cli/src/audio/` abstracts the platform-specific capture; one Linux backend can ship first and unblock the other two per-PR.

### What it unlocks

- Real reactive bars in YouTube mode — no procedural placeholder needed once embedded-CLI playback is the default surface.
- Single viz pipeline regardless of source. `procedural_indicator_bands` (§10) stays meaningful only for the **browser-pair** YouTube path — i.e. for users who haven't moved to the embedded CLI yet.
- Server no longer needs to fan out browser viz frames over the pair WS. Each CLI generates its own.

### Open questions

- **Per-process vs system-wide capture.** System-wide picks up whatever the user is playing outside `late`; per-process is more honest but requires extra plumbing (PipeWire per-app routing, CoreAudio AudioObject scoping). Reasonable starting point: per-process where the OS supports it, fall back to system-wide.
- **macOS permission UX.** First-launch prompt has to be explained somewhere (onboarding banner, `late doctor`, etc.).
- **Ordering vs procedural bars.** Procedural bars (§10) ship first and cover the current browser-pair surface; OS-loopback lands later and coexists. Both paths stay live until the browser-pair YouTube surface is retired (if ever).

### Reactivation criteria

- Embedded-webview CLI playback work is on the active roadmap or already shipped.
- We're willing to take on platform-specific audio code (the LATE bar to clear is one Linux backend).

Until then, browser-paired audio uses procedural bars for both Icecast and
YouTube (§10).

---

## 19. Queue state-drift hazards and reconciliation contract

**Status: active and implemented.** Anything new that mutates queue state must follow this contract.

### What went wrong (2026-05-19 incident)

Production stuck for ~1h45m. One row sat at `status='playing'` in DB. Every booth submit returned `db error: duplicate key value violates unique constraint "idx_media_queue_single_playing"`. Users couldn't add tracks or vote-skip.

Reconstruction from logs:

1. Pod restart at 09:44 UTC. `resume_from_db` adopted the lone playing row (Cyberpunk theme, started 09:40). Within seconds the playback timer fired (track was within ~1min of its real end). `finish_item` marked it `played`, `advance_to_next_with_guard` promoted the next queued row. Fine so far.
2. At some later point (logs don't pinpoint), `finish_item` was called for a row whose status was no longer `playing`. `mark_played`'s `WHERE status='playing'` returned 0 rows, and `finish_item` then early-returned `Ok(())` **without clearing `state.current_item_id`**. From this moment, `state.current_item_id` pointed at a row whose DB status had already moved on.
3. At 10:18:34 staff ran `/audio skip`. `force_skip` read the stale id and called `MediaQueueItem::update_status(stale_id, 'skipped')`. `update_status` has no `WHERE status=…` filter, so it cheerfully mutated the row from `played` → `skipped`. `state.current_item_id` was then set to `None` and `advance` was called.
4. From 10:19 onward, `state.current_item_id` was `None` in memory while the DB had `status='playing'` rows. Every `advance_to_next_with_guard` tried to promote a queued row via `mark_playing`, which violated the singleton index. Every booth submit failed.

The pod kept running this way for ~1h45m until manually restarted.

### Class of bug

Every queue-state mutation is two writes — the DB row's `status`/`started_at` column plus the in-memory `state.current_item_id` — and the old code:

- Issued the DB skip write *unconditionally* (`update_status` with no expected-old-status filter), so a stale id could quietly mutate the wrong row.
- Issued the in-memory write *conditionally* on the DB write returning rows changed, but treated `changed == 0` as "no-op, return early" instead of "drift detected, resync".

The current code makes those divergences recoverable without a pod restart.

### Reconciliation contract

The service now enforces these invariants. New code in this domain MUST follow them.

1. **No raw `update_status` for queue transitions.** Use `MediaQueueItem::mark_skipped(client, id, ended_at) -> u64` with `WHERE id = $1 AND status = 'playing'`. `force_skip`, the skip-vote-fired branch in `cast_skip_vote`, and `reevaluate_skip_threshold` route through it. Each caller treats `changed == 0` as drift, not success.

2. **`changed == 0` on any `mark_*` is drift, not a no-op.** `finish_item`, `fail_item`, `force_skip`, and the `mark_skipped` paths call the reconcile helper instead of returning early. That helper:
   - Cancels the existing playback timer.
   - Re-reads `MediaQueueItem::current_playing(&client)`.
   - If `Some(row)`: sets `state.current_item_id`, clears `state.skip_votes` if the id changed, reschedules the playback timer, broadcasts `SourceChanged` / `LoadVideo` / `QueueUpdate`.
   - If `None`: clears `state.current_item_id`, falls through to `advance_to_next_with_guard` (which may adopt or promote).

3. **`advance_to_next_with_guard` checks DB current first.** Before promoting a queued row, look at `current_playing` in DB. If DB already has one, adopt it (same code path as reconcile's `Some` branch) instead of trying `mark_playing` and racing the singleton index. This eliminates the singleton-violation symptom entirely — the loser of any race against the DB just adopts what's there.

4. **`mark_playing` unique-violation is recoverable.** Catch the constraint name `idx_media_queue_single_playing` in the Postgres error from `mark_playing`, treat it as "DB has a current we don't know about", route to reconcile. Never surface as a submit failure to the user.

### Why this beats a leader lock (for now)

A Postgres advisory-lock leader (§20) would prevent a *second pod* from also writing. The prod incident was a *single pod* corrupting its own state — a lock wouldn't have helped, and the next pod-after-handover would inherit the same bug class. The reconciliation contract makes every transition self-healing inside one pod; rule (4) also covers most of the rollout-overlap case for free (the loser of a `mark_playing` race reconciles instead of erroring at the user).

### Regression coverage

`late-ssh/tests/audio_queue_reconcile.rs` covers both prod shapes:
1. DB has a `playing` row while the service memory is empty; a subsequent submit adopts the DB current instead of surfacing the singleton violation.
2. Service memory points at an already-`played` row while DB has a different `playing` row; `/audio skip` reconciles and does not mutate the played row to `skipped`.

### What this contract does NOT cover

- **WS broadcast overlap during rolling deploys.** Two pods running for a few seconds during rollout will both broadcast `LoadVideo` / `QueueUpdate` to any browser that's connected to both. The browser receives duplicates and may visibly re-load. Cosmetic, not corrupting. If this becomes user-visible, escalate to §20.
- **Crash mid-transaction leaving the DB row as `playing`.** The 1h `sweep_orphan_playing` at startup is the existing safety net; reconcile shortens the window from "1h sweep" to "next time anything calls reconcile."
- **Multi-replica scale-up.** Still single-replica today. If we go multi-replica, the leader lock in §20 is the answer; the contract alone is not sufficient (followers would race on every advance and rely on the singleton index as the arbiter, which works but spams errors).

---

## 20. Parked: Advisory-lock audio leader

**Status: parked.** The reconciliation contract in §19 is the active fix and covers the realistic failure modes for a single-replica deployment. This is the next-step option *if* rolling-deploy WS overlap becomes user-visible OR we scale `service-ssh` past one replica.

### Idea

`AudioService` acquires a Postgres session-level advisory lock on a fixed key at startup. Only the lock-holder is the audio leader: it owns timer scheduling, queue mutation, and WS broadcasting. Followers (other replicas, the draining-out pod) keep serving SSH sessions but reject every audio-mutating call with a typed `NotLeader` error that surfaces as "audio is moving — reconnect" in the booth/sidebar.

```text
pod with lock     = audio leader, can mutate queue/timers, broadcasts ws events
pod without lock  = read-only follower; submit/skip/vote/advance return NotLeader
draining old pod  = releases lock + cancels timers in begin_drain()
new pod           = acquires lock + runs resume_from_db
old user sessions = stay connected but audio actions are rejected until they
                    reconnect to the new pod (k8s service routes new WS to leader)
```

### Sketch

- Pin a single pool connection for the lock. `pg_advisory_lock` is session-scoped; if the connection dies the lock releases, which is exactly the recovery behavior we want.
- Expose leader status as `watch::Receiver<bool>` so the sidebar/booth react to transitions, not just poll at action time. UI banner can react proactively rather than waiting for the user to press a key.
- A `LeaderGuard` zero-cost token returned by `acquire_for_mutation()`, required by every mutating method's signature. The compiler enforces the check instead of human discipline; trivial to miss otherwise given the surface (see below).
- `begin_drain()` releases the lock and cancels timers, letting the new pod take leadership before the old pod finishes draining its SSH sessions.

### Mutation surface that has to honor the gate

`submit_url`, `submit_trusted_url`, `submit_video`, `set_trusted_youtube_fallback`, `force_skip`, `cast_skip_vote`, `cast_vote`, `clear_vote`, `delete_queue_item`, `toggle_unskippable`, `record_browser_duration`, `report_player_state`, `finish_item`, `fail_item`, plus all the `*_task` spawners that call them. A `LeaderGuard` parameter on the inner sync methods catches this at compile time.

### Why not yet

- The prod incident was single-pod state drift, not multi-pod contention. The reconciliation contract is the minimum viable fix; the lock is layered safety, not the bug fix.
- Big audit surface (above). Easy to miss one, and the failure mode of missing one is a hard-to-debug "this one path bypasses leader" inconsistency. Worth doing only when we know we need it.
- Leader-handover UX during rolling deploys needs design work (banner copy, reconnect timing, what the booth modal does when the lock moves mid-modal). Premature without the demand.

### Reactivation criteria

- The reconciliation contract in §19 is in place and stable.
- We see real WS-broadcast overlap symptoms (browsers double-loading items during rollouts) OR we want to scale `service-ssh` past one replica.
- We have a story for the "audio is moving" UX that's not just a banner the user sees mid-action.

Until then: §19 is the contract; one replica is the deploy.

---

## 21. References

- Root context: `../../../../CONTEXT.md` — §2.7 (audio infra), §4.1 (paired-client WS).
- Pair WS handler: `late-ssh/src/api.rs` (look for `handle_socket`).
- Pair registry / mute policy: `late-ssh/src/paired_clients.rs`.
- CLI WS + audio: `late-cli/src/ws.rs`, `late-cli/src/audio/`.
- Web connect page: `late-web/src/pages/connect/page.html`, `late-web/src/pages/connect/mod.rs`.
- YouTube IFrame Player API: https://developers.google.com/youtube/iframe_api_reference
- YouTube Data API `videos.list`: https://developers.google.com/youtube/v3/docs/videos/list
- Browser autoplay: https://developer.mozilla.org/en-US/docs/Web/Media/Guides/Autoplay
- `wry` (webview): https://github.com/tauri-apps/wry
- `tao` (windowing): https://github.com/tauri-apps/tao
