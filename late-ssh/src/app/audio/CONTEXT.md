# late-ssh Audio Context

## Metadata
- Domain: late.sh audio — Icecast house radio, global YouTube queue, browser/CLI source arbitration, synthetic browser-pair visualizer, now-playing poller
- Primary audience: LLM agents working in `late-ssh/src/app/audio` and the touchpoints it owns in `late-cli` and `late-web/src/pages/connect`
- Last updated: 2026-05-18 (browser-pair visualizer is now synthetic-only for both Icecast and YouTube; the web page no longer creates a Web Audio analyzer or sends `viz` frames. §18 remains the parked plan for future real OS-loopback FFT.)
- Previously: booth modal now surfaces track durations: queue list has a right-aligned `m:ss` column between title and submitter, and the Now Playing row shows the same `m:ss` next to the title. Streams render `live`; unknown durations are blank. Two submit paths diverge in metadata: booth (`booth_submit_public_task` → `submit_url` → Data API) inserts rows with title/channel/`duration_ms`/`is_stream` already populated; staff `/audio` (`submit_trusted_url_task`) inserts NULL metadata and the browser backfills `duration_ms` on first play via `record_browser_duration`. See §4 Public API + §2 booth/ui.rs note.
- Status: Active
- Parent context: `../../../../CONTEXT.md`

---

## 1. Scope

Owned by this domain:
- Always-on Icecast house radio playback (the `<audio>` and CLI symphonia path).
- Global, DB-backed YouTube queue: submission, persistence, single-playing invariant, server-driven track switching (per-browser playback timeline), fallback debounce.
- The singleton "YouTube fallback" stream that plays when the queue is empty.
- Audio source arbitration between paired CLI and paired browser clients on the same SSH token (`force_mute`).
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
- `late-ssh/src/paired_clients.rs` — `PairedClientRegistry`, `PairControlMessage::ForceMute`, mute-priority policy.
- `late-ssh/src/api.rs` — `/api/ws/pair` multiplexes `AudioWsMessage` + `PairControlMessage`; `/api/now-playing`.
- `late-ssh/src/app/chat/{state,input}.rs` — `/audio` and `/audio fallback` chat commands.
- `late-cli/src/ws.rs`, `late-cli/src/main.rs`, `late-cli/src/audio/output.rs` — CLI tolerates unknown audio events, applies `force_mute` to the shared mute atomic.
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
DB statuses: `queued → playing → {played | skipped | failed}`. `skipped` is reserved but never written by current code.

All transitions go through `svc.rs`:
- `queued → playing`: `mark_playing` conditional `UPDATE … WHERE id=$1 AND status='queued'`. Loses gracefully when another advancer wins the singleton slot — caller treats `None` as "someone else is playing" and schedules the fallback debounce instead of clobbering.
- `playing → played`: `finish_item` or `finish_item_due_to_timer` via `mark_played` (`WHERE status='playing'`).
- `playing → failed`: `fail_item` via `mark_failed`. Only fired when the browser reports `player_state: error` for the active item.

`advance_to_next_with_guard` (`svc.rs:547-577`) is the *only* advancer. It picks `MediaQueueItem::first_queued()`, tries to flip it, on success broadcasts `SourceChanged: youtube` + `LoadVideo` + `QueueUpdate`. If the queue is empty it tries `publish_youtube_fallback_with_guard`; if no fallback row exists, `schedule_fallback` arms the 10s debounce, after which `finish_fallback_debounce` flips `mode = Icecast` (and re-checks `current_item_id.is_none()` to avoid races).

### Timers
- **Playback timer** (`schedule_playback_timer`): one `tokio::select!` task per playing item. Sleeps `duration - elapsed` then calls `finish_item_due_to_timer`. Also re-broadcasts `LoadVideo` for the current item every `PLAYBACK_HEARTBEAT_INTERVAL = 10s` from inside the same task — the safety-net heartbeat. Browsers ignore the heartbeat when they're already showing the right item; otherwise they force-swap.
- **Fallback debounce**: one task armed when the queue drains. Cancelled by any new submission via `cancel_fallback`.
- Both are owned via `oneshot` cancel handles on `QueueState`; dropping the sender cancels the task.

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
1. **Singleton playing row.** Enforced both by the partial unique index `idx_media_queue_single_playing` and by conditional `mark_playing` updates. Two racing advancers cannot both succeed.
2. **Server owns track *changes*, not playback positions.** Server picks which item is `playing` and broadcasts `LoadVideo` on changes + every 10s as a heartbeat. Each browser plays its own timeline from wherever YT happens to start. No more wall-clock-offset sync — slow networks no longer audibly skip mid-track.
3. **Force-switch on heartbeat.** A browser receiving `LoadVideo` for a different `item_id` than what it's currently playing MUST swap, regardless of pause/buffer/error state. Same-`item_id` heartbeat with the right `video_id` loaded → no-op (respect a manual pause).
4. **`ended` is trusted.** Server advances unconditionally when the playing item's browser reports `ended`. The own-timer is the backup for browsers that never report.
5. **Mode is server-managed.** Browser/CLI never write `mode`; they only receive `SourceChanged`.
6. **Sequence monotonicity.** `state.sequence` is bumped before every `QueueUpdate` so clients can drop stale ones.
7. **Banners are user-scoped.** `AudioEvent` carries `user_id` and `AudioState::tick` filters on it; one user's submission failure does not leak to others.

---

## 5. WebSocket Protocol (multiplexed on `/api/ws/pair`)

`api.rs` `handle_socket` (`api.rs:231-382`) drives three sources per connection with `tokio::select!`:
- inbound `socket.recv()` — client → server
- `control_rx` — `PairControlMessage` from `PairedClientRegistry` (mute/volume/force_mute/clipboard)
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
- `toggle_mute`, `volume_up`, `volume_down`, `request_clipboard_image`, `force_mute { mute }`.
- `set_playback_source { source: "icecast" | "youtube" }` — sent immediately on
  pair-WS connect and re-sent by the SSH session on browser-pair notification.

### Client → server `WsPayload` (`api.rs:39-68`)
- `heartbeat`
- `viz { position_ms, bands[8], rms }` — legacy/compat payload; the current web page does not send it
- `client_state { client_kind, ssh_mode, platform, capabilities, muted, volume_percent }`
- `clipboard_image { … }`, `clipboard_image_failed { … }`
- `player_state(PlayerStateReport)` — `{ item_id, state, offset_ms?, duration_ms?, autoplay_blocked, error? }` (`svc.rs:126-138`)

There is **one global broadcast**, no room scoping. Every paired browser on every token receives the same `load_video` / `source_changed` / `queue_update`.

---

## 6. Source Arbitration and `force_mute`

Policy lives entirely in `late-ssh/src/paired_clients.rs`. The audio domain does not own the registry; it only consumes the resulting per-token mute state via the browser's `client_state` reports.

Rule: **if any browser is paired on a token, every CLI on that token is force-muted.** The browser is the audio surface when present; the CLI is the audio surface only when alone.

| CLI paired | Browser paired | Browser hears        | CLI behavior                          |
|------------|----------------|----------------------|---------------------------------------|
| yes        | no             | n/a                  | plays Icecast normally                |
| yes        | yes            | Icecast or YouTube   | force-muted via `ForceMute { true }`  |
| no         | yes            | Icecast or YouTube   | n/a                                   |
| no         | no             | silent               | n/a                                   |

Triggers (`paired_clients.rs:217-297`, `:88-150`):
- Browser appears on a token, or CLI joins a token already holding a browser → broadcast `ForceMute { mute: true }` to every CLI sender on that token.
- Last browser on a token disconnects → broadcast `ForceMute { mute: false }`.
- The CLI's `!new_muted` guard preserves a user-initiated *unmute* across WS reconnect — the server does not re-impose mute on a still-paired browser if the user has manually opted into double audio.

Both decisions run under the same `PairedClientRegistry` lock to close the TOCTOU window where a new browser could register between removal and sender collection.

CLI side: `late-cli/src/ws.rs:155-171` swaps the shared mute atomic — `Arc::clone(&audio.muted)` (`late-cli/src/main.rs:148`) — the same atomic used by the local mute keybind (`late-cli/src/audio/output.rs:166-193`). After applying it, the CLI re-sends `client_state` so the server sees the new `muted` value.

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
2. Marks the current playing row `skipped` via `MediaQueueItem::update_status`, clears `current_item_id` and any pending `skip_votes`, cancels the playback timer, and runs `advance_to_next_with_guard` to bring up the next queued item (or arm the fallback debounce).
3. On success, banner via `AudioEvent::TrustedSkipFired` — "Skipped audio". On failure (nothing playing, DB error), banner via `AudioEvent::TrustedSkipFailed` — "Nothing is playing" or "Failed to skip audio".

---

## 8. CLI Integration

Goal: the CLI tolerates everything new the audio domain added, plays Icecast unchanged, and obeys server force-mute.

- **Unknown audio events ignored** (`late-cli/src/ws.rs`). Inbound text is parsed only as `PairControlMessage`. `load_video`, `source_changed`, `queue_update` fail to deserialize, the CLI logs `warn!("ignoring unsupported pair websocket event")`, and the select loop continues. **The CLI does not disconnect on audio events.** Note: each playing track now also produces a 10s `load_video` heartbeat — the CLI log noise budget should account for that.
- **`force_mute` applied to shared atomic** (`late-cli/src/ws.rs:155-171` → `apply_force_mute` → `muted.swap(mute, Relaxed)`). Same atomic as the local mute keybind, so the server's force-mute and the user's manual mute coexist on one piece of state. After applying, CLI re-sends `client_state` so the server observes the new value.
- **No YouTube decoding in the CLI.** The CLI never receives audio frames for YouTube — only metadata it ignores. Icecast path: `late-cli/src/audio/decoder_thread.rs` runs a symphonia HTTP stream decoder with 2s reconnect retry.
- **CLI identifies itself.** First `client_state` emitted by `late-cli/src/ws.rs:113-131` carries `"client_kind": "cli"`. That tag is what lets the registry decide who to force-mute.

---

## 9. Web Connect Page Integration

File: `late-web/src/pages/connect/page.html`. The audio source is decided in the browser; the YouTube API/player is lazy-loaded only when the browser actually enters YouTube mode.

- **Per-user audio source (server-authoritative).** The choice is persisted in `users.settings.audio_source` (`icecast` | `youtube`, default `icecast`). TUI `v+x` flips the value via `App::toggle_paired_playback_source`: writes to DB through `AudioService::persist_audio_source`, updates the local mirror `App::paired_browser_source`, and broadcasts `PairControlMessage::SetPlaybackSource { source }` to every paired browser. On pair-WS connect, `api.rs` sends the persisted source before the audio catch-up burst. On every browser pair-up (`api.rs` detects `previous_kind != Browser && new_kind == Browser`), the SSH session is also notified via `SessionMessage::BrowserPaired` and `App::replay_paired_browser_source` re-pushes the current value. The browser is a follower: `applyUserPlaybackSource(source)` stores `userOverrideMode` and applies. While the user is pinned to icecast, `loadYoutubeVideo` and `seekYoutube` early-return so server queue events do not flip the iframe back on (the current item is still stashed as `pendingYoutubeItem` so a toggle to youtube starts playing immediately).
- **IFrame API load.** The page does not include the YouTube iframe API up front. `ensureYoutubePlayer()` calls `loadYoutubeApi()` on demand, which appends `https://www.youtube.com/iframe_api`; `window.lateYoutubeApiReady` / `onYouTubeIframeAPIReady` then create the player only if `audioMode === "youtube"`.
- **`source_changed` swap** (`applySourceMode`). Into `youtube`: pause `<audio>`, ensure player exists, kick playback of pending item. Into `icecast`: `ytPlayer.pauseVideo()`, restart `startPlayback()` for the `<audio>` if audio is enabled. The `modeChanged` guard prevents repeated `source_changed: youtube` broadcasts during queue transitions from resetting the iframe.
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
- `app/tick.rs` turns `Visualizer::procedural_active` on whenever this SSH
  session has a paired browser (`paired_client_state().client_kind == Browser`).
  This is source-agnostic: Icecast and YouTube render the same procedural bars.
- `render_inline(frame, area)` is the borderless sidebar render. Idle shows `"no audio paired"` / `"/music in chat"` / `"P install · pair"` (last only when height ≥ 5). Procedural live draws dim amber 1-cell-wide bars with 1-cell gaps at **sub-cell vertical resolution** (`▁▂▃▄▅▆▇█`, 9-step). Bar heights come from layered sines — a primary traveling wave, a faster per-band shimmer, and a slow global breath term (incommensurate frequencies so the pattern doesn't visibly repeat in a few seconds). No spectrum-style tilt is applied on the procedural path; the wave shape is decorative, not a frequency analog.
- The old `VizFrame`/`Visualizer::update` path is still present for compatibility
  with existing payload parsing and tests, but browser web playback no longer
  drives it.

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
- v+x dispatch goes through `app/state.rs::toggle_paired_playback_source` → persists `paired_browser_source` via `AudioService::persist_audio_source` and broadcasts `PairControlMessage::SetPlaybackSource`. Early-returns `None` (skipping local update + persist) when no browser is paired; the "No paired browser" banner is the user-visible feedback. The sidebar still reflects the saved preference from the DB at SSH bootstrap regardless, so the toggle silently no-op'ing doesn't desync the visual.

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
- `MediaQueueItem::{insert_youtube, find_by_id, list_snapshot, queued_before_count, recent_submission_count, first_queued, current_playing, mark_playing, mark_played, mark_failed, set_duration_if_missing, update_status, sweep_orphan_playing}`. Status/kind constants: `STATUS_QUEUED`, `STATUS_PLAYING`, `STATUS_PLAYED`, `STATUS_SKIPPED`, `STATUS_FAILED`, `KIND_YOUTUBE`.
- `MediaSource::{youtube_fallback, upsert_youtube_fallback}`. Constants: `KIND_YOUTUBE_FALLBACK`, `MEDIA_KIND_YOUTUBE`.

---

## 14. Known Gaps and Things to Watch

- **`GET /api/queue` is intentionally not exposed.** `AudioService::snapshot()` and `QueueSnapshot` exist for in-process use only. The TUI booth modal reads the snapshot from `AudioState::queue_snapshot()` (a `watch::Receiver<QueueSnapshot>` populated by `publish_queue_update_with_guard`); browsers receive state via the `initial_ws_messages` catch-up burst and live `queue_update` events. An external route would only matter for non-paired observers, which we do not have today.
- **Booth modal renders from `watch::Receiver<QueueSnapshot>`.** `AudioService` keeps a `snapshot_tx` watch sender alongside the broadcast channels; every `publish_queue_update_with_guard` pushes a snapshot into it, and `AudioState::queue_snapshot()` borrows the current value. Skip progress (`votes/threshold`) is folded into the snapshot before it ships.
- **`liquidsoap.rs` lives here but is only used by `app/vote/svc.rs`.** AudioService does *not* drive Liquidsoap. Treat `AudioMode::Icecast` as a hint to the browser/CLI, not a Liquidsoap state change.
- **`/music` ≠ `/audio`.** `/music` is a help-topic command. `/audio` (and `/audio fallback`) are the submit commands. Don't conflate.
- **No `GET /api/queue` HTTP route.** Submit and visibility for end users happen through the SSH booth modal (submit + queue list) and the staff `/audio` chat command. Non-paired observers have no way to see the queue today.
- **Multi-tab double audio** is unsolved. Two browser tabs on the same token both play. Deferred until UI work.
- **Region locks / embedding disabled** are not caught at submit time — `/audio` skips the YouTube Data API. The browser reports `error`, the server marks `failed`, queue advances. Pre-validation comes back with the public submit flow.
- **`LATE_YOUTUBE_API_KEY` is optional today** (`config.rs:200`, `optional()`). Required only for `submit_url` (un-trusted), which has no caller. Set it before reviving public submit.

---

## 15. Design boundaries (won.t build)

These are intentional non-goals. Reopen only if the constraint that put them here changes.

- **CLI YouTube decoding.** CLI plays Icecast only. The YouTube path is browser-iframe-only. See §17 for the parked external-player alternative.
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

## 17. Parked: CLI external-player handoff for YouTube

**Status: parked, not on the active build path.** Reason: the user-facing configuration burden is too high for current scale — most users won't have a suitable player installed and won't want to edit a TOML config. Revisit when the audience is technical enough or large enough to justify a setup guide.

### Idea

Instead of opening a browser for YouTube playback, `late` shells out to a local media player (mpv, vlc, FreeTube, mpsyt, anything) that already knows how to play YouTube. late.sh never touches YouTube audio; the CLI is a general external-player runner that the user wires up. Server still ships only `video_id` over `/api/ws/pair`.

```text
server  → "play video_id at offset N" (WS, metadata only)
late CLI → spawns or controls user-configured local player
player  → fetches and decodes audio from YouTube (belongs to the user)
```

### Two control modes

**Command mode** (~80 LOC of Rust):
```toml
[player.youtube]
mode = "command"
command = "<player> <flags> {url}"
```
Server says play → CLI spawns the command with `{url}` substituted → process exits when the track ends → CLI tells server `ended`. Skip = SIGTERM.

**IPC mode** (richer — sync/seek/pause):
```toml
[player.youtube]
mode = "ipc"
launch = "<player> --idle --input-ipc-server={socket}"
protocol = "mpv"
```
Long-running player. CLI sends commands over a JSON/IPC socket. `protocol` is the only player-specific code shipped in `late`. Start with one adapter; community can add more.

### Ship / don't ship boundary

| Safe (ship)                                          | Unsafe (don't ship)                                       |
|------------------------------------------------------|-----------------------------------------------------------|
| Config slot for external player command              | Bundled mpv or yt-dlp binaries                            |
| Template variables (`{url}`, `{socket}`)             | `late install-youtube` subcommand                         |
| Generic IPC protocol adapter (mpv first)             | Auto-download of any extraction tool                      |
| `late doctor` against a benign non-YouTube test URL  | `late doctor` testing against a real YouTube URL          |
| Clear errors when no player is configured            | Naming a specific tool inside the binary                  |
| Community-maintained `EXTERNAL_PLAYERS.md`           | Official "recommended player" in onboarding flow          |

### Posture

late.sh ships zero yt-dlp code; every byte of YouTube audio is fetched by the user's machine, by a tool the user chose. A user-side mpv-with-yt-dlp setup still violates YouTube ToS on the user's machine (yt-dlp strips ads, branding, controls). If this is ever activated, docs must be explicit that the CLI is a generic external-player runner and that the user — not late.sh — is responsible for what their configured player does.

### Reactivation criteria

- User base is large/technical enough that a setup guide is worth maintaining.
- A stable, official YouTube-API-compliant CLI player emerges (none currently exists; closest options all use yt-dlp underneath).
- We decide to make late.sh deliberately CLI-power-user-shaped, and a player slot fits the product identity.

Until then, YouTube playback goes through the browser iframe path (§4-§9).

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

## 19. References

- Root context: `../../../../CONTEXT.md` — §2.7 (audio infra), §4.1 (paired-client WS).
- Pair WS handler: `late-ssh/src/api.rs` (look for `handle_socket`).
- Pair registry / mute policy: `late-ssh/src/paired_clients.rs`.
- CLI WS + audio: `late-cli/src/ws.rs`, `late-cli/src/audio/`.
- Web connect page: `late-web/src/pages/connect/page.html`, `late-web/src/pages/connect/mod.rs`.
- YouTube IFrame Player API: https://developers.google.com/youtube/iframe_api_reference
- YouTube Data API `videos.list`: https://developers.google.com/youtube/v3/docs/videos/list
- Browser autoplay: https://developer.mozilla.org/en-US/docs/Web/Media/Guides/Autoplay
- mpv JSON IPC (for the parked plan): https://mpv.io/manual/master/#json-ipc
