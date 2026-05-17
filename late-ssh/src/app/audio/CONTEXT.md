# late-ssh Audio Context

## Metadata
- Domain: late.sh audio — Icecast house radio, global YouTube queue, browser/CLI source arbitration, Icecast visualizer, now-playing poller
- Primary audience: LLM agents working in `late-ssh/src/app/audio` and the touchpoints it owns in `late-cli` and `late-web/src/pages/connect`
- Last updated: 2026-05-17 (music-stage sidebar widget rebuilt — both surfaces always visible, see §12)
- Status: Active
- Parent context: `../../../../CONTEXT.md`

---

## 1. Scope

Owned by this domain:
- Always-on Icecast house radio playback (the `<audio>` and CLI symphonia path).
- Global, DB-backed YouTube queue: submission, persistence, single-playing invariant, server-authoritative timeline, sync seeks, fallback debounce.
- The singleton "YouTube fallback" stream that plays when the queue is empty.
- Audio source arbitration between paired CLI and paired browser clients on the same SSH token (`force_mute`).
- Icecast visualizer driven by browser-side Web Audio analysis.
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
├── svc.rs                  # AudioService: queue state machine, WS broadcast, resume, fallback debounce, sync seek, votes/skip-vote
├── state.rs                # AudioState: per-session UI shim — proxies submits/votes and turns AudioEvent into Banners
├── client_state.rs         # ClientAudioState + ClientKind/SshMode/Platform enums (the client_state WS payload)
├── liquidsoap.rs           # LiquidsoapController telnet client (NOT used by AudioService — only by app/vote/svc.rs)
├── viz.rs                  # Visualizer (Icecast bands/RMS/beat) + ratatui render_inline
├── youtube.rs              # URL parsing + optional YouTube Data API validation client
├── booth/
│   ├── mod.rs
│   ├── state.rs            # BoothModalState: open flag, submit input, selected index, focus
│   ├── input.rs            # modal-open key dispatch (submit/queue focus, +/- vote, s skip)
│   └── ui.rs               # ratatui modal: submit row, current track, queue list with score
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
- `late-web/src/pages/connect/page.html` + `connect/mod.rs` — browser IFrame player, drift correction, per-user v+x source toggle.

---

## 3. Ownership Split

- `svc.rs` is the async boundary. It owns the DB, both broadcast channels, the queue state mutex, the playback timer, the fallback debounce timer, the periodic sync-seek task, and all transitions. **Nothing else in the codebase mutates `media_queue_items.status` or `media_sources`.**
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
- `MAX_SUBMISSIONS_PER_WINDOW = 3` over `SUBMISSION_WINDOW = 5 minutes` — applies only to un-trusted `submit_url` (currently nothing reaches it).
- `FALLBACK_DEBOUNCE = 10s`
- `PLAYBACK_SYNC_INTERVAL = 10s` — periodic `Seek` to re-anchor browsers.
- `PLAYBACK_END_GRACE = 5s` — tolerance when validating a browser-reported `ended`.
- `STREAM_CAP = 1h` — hard cap on any single playing row's wall-clock lifetime.

### Public API
- `new(db, youtube_api_key)` — `main.rs:123`.
- `start_background_task(shutdown)` — sweeps orphan `playing` rows, then resumes from DB, then idles. `main.rs:360`.
- `subscribe_ws()` — `api.rs:237` (pair WS upgrade).
- `subscribe_events()` — `app/audio/state.rs`.
- `initial_ws_messages()` (`svc.rs:393-423`) — catch-up burst sent on every new pair-WS connect: `source_changed`, `queue_update`, and `load_video` for the current playing item or for the configured fallback.
- `snapshot()` — returns `QueueSnapshot { mode, current, queue }`. Type exists but no HTTP route exposes it (see §14).
- `submit_url` / `submit_url_task` — un-trusted, rate-limited, validates via YouTube Data API. **No caller today.**
- `submit_trusted_url` / `submit_trusted_url_task` — used by `/audio`. Bypasses rate limit and Data API; uses `youtube::trusted_video_from_url` to parse the ID only.
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
- **Playback timer** (`schedule_playback_timer`, `svc.rs:661-704`): one `tokio::select!` task per playing item. Sleeps `duration - elapsed` then calls `finish_item_due_to_timer`. Also runs the periodic `Seek` broadcast every `PLAYBACK_SYNC_INTERVAL = 10s` from inside the same task.
- **Fallback debounce** (`svc.rs:729-745`): one task armed when the queue drains. Cancelled by any new submission via `cancel_fallback` (`svc.rs:253`).
- Both are owned via `oneshot` cancel handles on `QueueState`; dropping the sender cancels the task.

### `playback_duration` rules (`svc.rs:803-809`)
- `is_stream = true` → always `STREAM_CAP` (1h).
- Non-stream with known `duration_ms` → use it.
- Non-stream with unknown duration → `STREAM_CAP` (1h fallback cap).
- `record_browser_duration` (`svc.rs:706-727`) is the only path that backfills `duration_ms` from the browser, conditionally on the current playing item and only when the DB value is NULL. After write, it reschedules the playback timer to the now-known end time.

### `player_state` ingress (`svc.rs:347-373`)
Routed by report `state` field:
- `ended` → `finish_item_from_player` (`svc.rs:467-513`). Drops the report if `current_item_id != report.item_id`. If duration is unknown, refuses to advance and broadcasts a `Seek` to re-anchor. If `elapsed + 5s grace < known duration`, treats `ended` as premature and broadcasts a `Seek`. Only when both gates pass does it call `finish_item`.
- `error` → `fail_item`.
- `playing` / `paused` / `buffering` → may carry `duration_ms` for `record_browser_duration`; otherwise logged. `autoplay_blocked = true` logs at `warn!`.

### Invariants
1. **Singleton playing row.** Enforced both by the partial unique index `idx_media_queue_single_playing` and by conditional `mark_playing` updates. Two racing advancers cannot both succeed.
2. **Server-authoritative timeline.** `started_at` in the DB is the truth. Browser offsets are never trusted as state — only as observations that can trigger a corrective `Seek`.
3. **Server validates `ended`.** A premature browser `ended` is rebutted with an authoritative seek, not honored.
4. **Mode is server-managed.** Browser/CLI never write `mode`; they only receive `SourceChanged`.
5. **Sequence monotonicity.** `state.sequence` is bumped before every `QueueUpdate` so clients can drop stale ones.
6. **Banners are user-scoped.** `AudioEvent` carries `user_id` and `AudioState::tick` filters on it; one user's submission failure does not leak to others.

---

## 5. WebSocket Protocol (multiplexed on `/api/ws/pair`)

`api.rs` `handle_socket` (`api.rs:231-382`) drives three sources per connection with `tokio::select!`:
- inbound `socket.recv()` — client → server
- `control_rx` — `PairControlMessage` from `PairedClientRegistry` (mute/volume/force_mute/clipboard)
- `audio_rx` — `AudioWsMessage` from `AudioService::subscribe_ws()`

On connect, `audio_service.initial_ws_messages()` emits the catch-up burst.

### Server → client `AudioWsMessage` (`svc.rs:58-79`, tagged enum, snake_case)
- `load_video { item_id, video_id, started_at_ms, offset_ms, is_stream }`
- `seek { offset_ms }`
- `source_changed { audio_mode: "icecast" | "youtube" }`
- `queue_update { current, queue, sequence }`

### Server → client `PairControlMessage` (`paired_clients.rs:22-30`)
- `toggle_mute`, `volume_up`, `volume_down`, `request_clipboard_image`, `force_mute { mute }`.

### Client → server `WsPayload` (`api.rs:39-68`)
- `heartbeat`
- `viz { position_ms, bands[8], rms }` — browser-only, drives the Icecast visualizer
- `client_state { client_kind, ssh_mode, platform, capabilities, muted, volume_percent }`
- `clipboard_image { … }`, `clipboard_image_failed { … }`
- `player_state(PlayerStateReport)` — `{ item_id, state, offset_ms?, duration_ms?, autoplay_blocked, error? }` (`svc.rs:126-138`)

There is **one global broadcast**, no room scoping. Every paired browser on every token receives the same `load_video` / `seek` / `source_changed` / `queue_update`.

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

---

## 7. Chat Commands (`/audio`, `/audio fallback`)

Parsing: `late-ssh/src/app/chat/state.rs:1356-1380`.
- Longer prefix `/audio fallback ` is matched first.
- Staff gate: `is_admin || is_moderator`. Non-staff get banner `"/audio is staff-only"`.
- Empty arg → `"Usage: /audio <youtube-url>"` or `"Usage: /audio fallback <youtube-url>"`.
- Valid requests stash URLs into `requested_audio_url` / `requested_audio_fallback_url`.

Dispatch: `late-ssh/src/app/chat/input.rs:131-136` calls `app.audio.submit_trusted(url)` or `app.audio.set_youtube_fallback(url)`, which proxy through `AudioState` to `AudioService::{submit_trusted_url_task, set_trusted_youtube_fallback_task}`.

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

---

## 8. CLI Integration

Goal: the CLI tolerates everything new the audio domain added, plays Icecast unchanged, and obeys server force-mute.

- **Unknown audio events ignored** (`late-cli/src/ws.rs:140-147`). Inbound text is parsed only as `PairControlMessage`. `load_video`, `source_changed`, `queue_update`, `seek` fail to deserialize, the CLI logs `warn!("ignoring unsupported pair websocket event")`, and the select loop continues. **The CLI does not disconnect on audio events.**
- **`force_mute` applied to shared atomic** (`late-cli/src/ws.rs:155-171` → `apply_force_mute` → `muted.swap(mute, Relaxed)`). Same atomic as the local mute keybind, so the server's force-mute and the user's manual mute coexist on one piece of state. After applying, CLI re-sends `client_state` so the server observes the new value.
- **No YouTube decoding in the CLI.** The CLI never receives audio frames for YouTube — only metadata it ignores. Icecast path: `late-cli/src/audio/decoder_thread.rs` runs a symphonia HTTP stream decoder with 2s reconnect retry.
- **CLI identifies itself.** First `client_state` emitted by `late-cli/src/ws.rs:113-131` carries `"client_kind": "cli"`. That tag is what lets the registry decide who to force-mute.

---

## 9. Web Connect Page Integration

File: `late-web/src/pages/connect/page.html`. The IFrame API and `<div id="yt-player">` are always rendered; the audio source is decided in the browser.

- **Per-user audio source (server-authoritative).** The choice is persisted in `users.settings.audio_source` (`icecast` | `youtube`, default `icecast`). TUI `v+x` flips the value via `App::toggle_paired_playback_source`: writes to DB through `AudioService::persist_audio_source`, updates the local mirror `App::paired_browser_source`, and broadcasts `PairControlMessage::SetPlaybackSource { source }` to every paired browser. On every browser pair-up (`api.rs:298` detects `previous_kind != Browser && new_kind == Browser`), the SSH session is notified via `SessionMessage::BrowserPaired` and `App::replay_paired_browser_source` re-pushes the current value, so a refreshed page lands in the right mode. The browser is a follower: `applyUserPlaybackSource(source)` stores `userOverrideMode` and applies. While the user is pinned to icecast, `loadYoutubeVideo` and `seekYoutube` early-return so server queue events do not flip the iframe back on (the current item is still stashed as `pendingYoutubeItem` so a toggle to youtube starts playing immediately).
- **IFrame API load.** `<script src="https://www.youtube.com/iframe_api">` is always included. Global `window.lateYoutubeApiReady` and `onYouTubeIframeAPIReady` hooks resolve a promise that the Alpine component awaits in `init()`.
- **`source_changed` swap** (`applySourceMode`, lines 487-528). Into `youtube`: pause `<audio>`, ensure player exists, kick playback of pending item. Into `icecast`: `ytPlayer.stopVideo()`, restart `startPlayback()` for the `<audio>` if audio is enabled. The `modeChanged` guard prevents repeated `source_changed: youtube` broadcasts during queue transitions from resetting the iframe.
- **`load_video` → `loadVideoById`** (lines 597-619). Calls `loadVideoById({ videoId, startSeconds: offsetMs/1000 })` with `expectedYoutubeOffsetMs` compensated for time since the message was received. `verifyYoutubeLoad` re-checks after 1s and reloads if the video id mismatches.
- **Drift correction** (`correctYoutubeDriftTo`, lines 770-791). Periodic loop fires every 10s; `|drift| < 2500ms` → ignore; otherwise hard `seekTo` with a 5s cooldown. Live streams (`item.isStream`) skip drift entirely; the server's 1h cap governs.
- **`player_state` reports** (`sendYoutubeState`, lines 793-821). Emits `{ event: 'player_state', item_id, state, offset_ms, duration_ms, autoplay_blocked, error }`. Triggered by YT state transitions and the 10s periodic loop.
- **Autoplay-blocked** (lines 621-638). 1.5s after `loadVideoById`, if the YT state is still `CUED`/`UNSTARTED`, sets `autoplayBlocked = true`, emits `player_state: buffering` with the flag, and the UI swaps to `[ tap to play ]`. Tap routes through `startPlayback` → `ytPlayer.playVideo()`.
- **`queue_update` is currently a no-op** in the browser (no UI to show it). The event ships so a future surface can use it.

---

## 10. Visualizer (`viz.rs`)

- `Visualizer { bands[8], rms, has_viz, rms_avg, beat }` consumes `late_core::audio::VizFrame { bands[8], rms, track_pos_ms }`.
- `update(&mut self, &VizFrame)` clamps bands, smooths `rms_avg` (0.95/0.05 EMA), decays `beat *= 0.9`, fires `beat = 1.0` when `frame.rms / rms_avg > 1.3`.
- `tick_idle()` decays bands/RMS/beat each tick when no frames arrive (called when `has_viz == true` only).
- `beat()` is volume-independent and drives bonsai animation.
- `render_inline(frame, area)` is the borderless sidebar render. Idle shows `"no audio paired"` / `"/music in chat"` / `"P install · pair"` (last only when height ≥ 5). Live draws 1-cell bars with 1-cell gaps using linear resample plus tilt `(0.65 + 0.35·t)·γ^1.1`.

Data flow: browser Web Audio analyzer → `WsPayload::Viz` → `api.rs:293` converts to `SessionMessage::Viz(VizFrame)` → session dispatcher → `app/tick.rs:213` feeds it through `Visualizer::update` (latest frame each tick) → `app/common/sidebar.rs:106` renders.

**Icecast-only by browser constraint.** A YouTube iframe is cross-origin; the browser cannot tap its audio. When mode is YouTube, the browser stops sending `viz` frames, `has_viz` decays to false, and the sidebar reverts to the idle panel. Do not pretend YouTube has frequency analysis — if a future UI wants a "playing" indicator for YouTube, drive it procedurally and name it as such in code (e.g. `procedural_indicator_bands`, not `viz_bands`).

---

## 11. Now-Playing (`now_playing/svc.rs`)

- Shared `watch::Sender<Option<NowPlaying>>` reflects the current Icecast track title.
- `start_poll_task` spawns a blocking thread that calls `late_core::icecast::fetch_track` every 10s (split into 1s sleeps to shut down quickly). Only emits when the title string changes.
- Independent of `AudioService` — does not subscribe to its channels.
- Consumers: `GET /api/now-playing` (`api.rs:131`). Still plumbed into `App.now_playing_rx` and on through `SidebarProps`, but the music-stage widget *explicitly ignores it* (the icecast block renders genre vibe + vote rows, not the radio track title — see §12). The plumbing is kept in case another sidebar surface wants it; remove if no caller picks it up.

---

## 12. Sidebar music-stage widget (`common/sidebar.rs`)

Renders the audio domain into the right rail. Both surfaces (YouTube + Icecast) are always visible; the active source the user is hearing gets bold amber chrome, the other gets dim italic. Entry point: `app/common/sidebar.rs:draw_music_stage`, allocated `MUSIC_STAGE_HEIGHT = 16` rows.

### Layout

| Row(s) | Content |
|--------|---------|
| 0      | Volume bar: `vol  ▰▰▰▰▰▱▱▱▱▱  60%`. Renders `muted` (italic faint) when muted, `—` when no client is paired. |
| 1      | Volume keybind hints: `m mute  -= vol`. |
| 2-8    | YouTube block: title bar, track title, channel, progress, skip meter (with trailing `v+s` hint when active), `next ⌄` header, queue items (`Min(2)`, absorbs spare space). |
| 9      | Booth/swap keybind hints: `v+v queue  v+x swap`. |
| 10-12  | Icecast block: title bar, `vibe → next · ends` one-liner, then a 3-row vote area delegated to `app/vote/ui.rs::draw_vote_inline`. |

### Active-source rule

```rust
yt_active = paired_browser_source == AudioSource::Youtube
```

Pure preference-based. Does **not** gate on `is_browser`. The saved preference (loaded from `users.settings.audio_source` via `extract_audio_source` during SSH bootstrap, `ssh.rs:883`, mirrored in `App.paired_browser_source`) is the source of truth from the first frame. Pairing-completion does not change the visual state — earlier versions waited for the browser to pair before honoring the pref, which read as a startup glitch (sidebar showed Icecast for ~1s then flipped). Don't add the `is_browser` guard back.

The volume row stays honest about pairing (`vol  —` when nothing paired), so users aren't misled about whether their preference is currently audible.

### Fallback-not-empty semantics

The widget treats "no submitted track" and "fallback playing" as the same state. When `queue.current.is_none()`:
- Title tag is `loop` (was `fallback` — didn't fit on narrow rails after dropping `▶ ` prefix).
- Body renders `fallback stream` / `YouTube · 24/7` plus a `queue with v+v` hint.
- When a track is playing but queue is otherwise empty, the trailing "next" row says `· fallback next`, not "queue ends".

No copy anywhere reads "queue empty". The user has pushed back on that wording multiple times; in their product framing the fallback is the steady state, not a placeholder. See `feedback_fallback_not_empty.md` in auto-memory.

### Data sources

- `queue_snapshot: &QueueSnapshot` — from `AudioState::queue_snapshot()` watch channel.
- `vote: VoteCardView<'_>` — from the genre vote state.
- `paired_client: Option<&ClientAudioState>` — for `volume_percent` and `muted` (vol row).
- `paired_browser_source: AudioSource` — App's per-user mirror.
- `now_playing` — plumbed but ignored, see §11.

### Internal helpers (all in `sidebar.rs`)

- `stage_title_line(area_w, label, tag, active)` — shared title-bar renderer. Active → uppercase amber bold + amber tag; inactive → lowercase italic faint + dropped tag. No `▶ ` glyph prefix on the tag (color + position read as a state badge; the prefix was eating cells on narrow rails).
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
- **No `GET /api/queue` and no submit UI** means visibility for MVP is via DB inspection or browser/CLI logs.
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
- **Real Web Audio analysis of the YouTube iframe.** Not possible — cross-origin iframe, no audio hook in the IFrame Player API. The Icecast visualizer (§10) keeps working; any future YouTube-mode visualizer must either hide, switch to a labeled "playing" indicator driven procedurally (name it honestly in code — `procedural_indicator_bands`, never `viz_bands`), or stop showing bars.

---

## 16. Deferred (open backlog)

Open work that's been deliberately punted past v1. Each line is a "we know it's missing, here's the next-time hook."

- **Public `POST /api/queue/submit` HTTP route.** Booth submit goes through the in-process service. Revive when there's a non-SSH submitter (web form, third-party). YouTube Data API validation path is already in code (un-trusted route in `AudioService::submit_url_task`).
- **`GET /api/queue` HTTP route.** Snapshot exists in-process (`QueueSnapshot`); no external consumer today. See §14 first bullet.
- **TUI sidebar widget on Home for queue visibility.** Booth modal is the only surface today.
- **Drift correction tuning.** Current thresholds (2.5s drift, 5s seek cooldown) work but were picked by feel.
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

## 18. References

- Root context: `../../../../CONTEXT.md` — §2.7 (audio infra), §4.1 (paired-client WS).
- Pair WS handler: `late-ssh/src/api.rs` (look for `handle_socket`).
- Pair registry / mute policy: `late-ssh/src/paired_clients.rs`.
- CLI WS + audio: `late-cli/src/ws.rs`, `late-cli/src/audio/`.
- Web connect page: `late-web/src/pages/connect/page.html`, `late-web/src/pages/connect/mod.rs`.
- YouTube IFrame Player API: https://developers.google.com/youtube/iframe_api_reference
- YouTube Data API `videos.list`: https://developers.google.com/youtube/v3/docs/videos/list
- Browser autoplay: https://developer.mozilla.org/en-US/docs/Web/Media/Guides/Autoplay
- mpv JSON IPC (for the parked plan): https://mpv.io/manual/master/#json-ipc
