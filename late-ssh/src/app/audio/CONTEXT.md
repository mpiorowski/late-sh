# late-ssh Audio Context

## Metadata
- Domain: late.sh audio — the radio station catalogue (house Icecast mounts and direct third-party streams), global YouTube queue, browser/CLI source arbitration, the equalizer (live CLI spectrum with an ambient fallback), and now-playing poller
- Primary audience: LLM agents working in `late-ssh/src/app/audio` and the music/audio touchpoints it owns in `late-cli` and `late-web/src/pages/listen`
- Status: Active
- Parent context: `../../../../CONTEXT.md`

---

## 1. Scope

Owned by this domain:
- Radio: the station catalogue (`late_core::radio`), user-pinned slots, the Stations modal, and per-provider now-playing metadata (`STATIONS.md`). Default source for an account that never picked one is radio on `chillsynth`.
- Global, DB-backed YouTube queue: submission, persistence, single-playing invariant, server-driven track switching (per-browser playback timeline), fallback debounce.
- Community Booth History: max 200 unique previously played YouTube tracks, ordered most recently played first, with requeue-from-history.
- Booth favorites: per listener, at most 20, on top of that listener's History list, kept outside the 200 so they never fall off.
- The singleton "YouTube fallback" stream that plays when the queue is empty.
- Audio source selection for paired clients (`set_playback_source`). No arbitration: only the CLI pairs, so the source alone picks the surface.
- The equalizer: the paired CLI's live spectrum, with the ambient band whenever no fresh spectrum arrives (`VIZ.md`).
- Now-playing poller for the Icecast track title.
- The `/audio` and `/audio fallback` SSH chat commands (staff-only).
- Third-party stations are played by the paired CLI and the listen page straight from the station's own stream URL. late.sh never proxies or restreams third-party audio through Icecast/Liquidsoap; it fetches metadata only.

Out of scope here (lives elsewhere):
- LiveKit voice rooms, CLI microphone/remote voice playout, TUI voice controls/status, and pair-WS voice messages — see `../voice/CONTEXT.md`.
- Liquidsoap playlist encoding and Icecast serving — configured in `infra/liquidsoap/`, not driven by `AudioService`; the music library, its licensing and how it ships are in root `MUSIC.md`.
- CLI Icecast decode/output (`late-cli/src/audio/`) — owned by the CLI crate; this file only documents the WS/control wiring.

---

## 2. File Map

```text
late-ssh/src/app/audio/
├── mod.rs                  # declarations only (booth, client_state, input, now_playing, radio_meta, state, stations, stations_modal, svc, thumbnail, viz, youtube)
├── svc.rs                  # AudioService: queue/history state machine, WS broadcast, resume, fallback debounce, periodic LoadVideo heartbeat, votes/skip-vote
├── state.rs                # AudioState: per-session UI shim — proxies submits/votes and turns AudioEvent into Banners
├── client_state.rs         # ClientAudioState + ClientKind/SshMode/Platform enums (the client_state WS payload)
├── input.rs                # v+* music suffix handling: booth, source cycling, stream/station selection
├── stations.rs             # server-side stream/station registry and URL resolution
├── viz.rs                  # render_eq + Spectrum: live client spectrum, wall-tick ambient fallback
├── youtube.rs              # URL parsing + watch_url + optional YouTube Data API validation client, thumbnail fetch
├── thumbnail.rs            # pure: a fetched thumbnail shrunk to what the live strip's picture column uses (168x96 pixels)
├── booth/
│   ├── mod.rs
│   ├── state.rs            # BoothModalState: open flag, submit input, queue/history selections, focus; HistoryRow (favorites on top of History)
│   ├── input.rs            # open (every door into the booth), modal-open key dispatch (submit/queue focus, +/- vote, s skip, Ctrl+Y copy, history focus, Enter requeue, f favorite)
│   ├── ui.rs               # ratatui modal: submit row, current track + watch link, queue list with duration + score, history list (starred favorites first) with duration + play count
│   └── live.rs             # a booth track on the live strip (app/live/): candidates, view, render_picture (chafa), words
├── stations_modal/
│   ├── mod.rs
│   ├── state.rs            # StationsModalState: open flag, selection over the enabled catalogue
│   ├── input.rs            # modal-open keys: move, Enter listen, 1-5 pin, 0 unpin, Ctrl+Y copy
│   └── ui.rs               # ratatui modal: pinned row, stations grouped by section with live now-playing, footer
├── now_playing/
│   ├── mod.rs
│   └── svc.rs              # NowPlayingService: 10s Icecast poll, watch<HashMap<mount, NowPlaying>>
└── radio_meta/
    ├── mod.rs
    ├── polled.rs           # PolledFeed (Plaza, CodeRadio, six Paradise* channels, FipJazz, SwissJazz, SwissClassic): endpoint URL, catalogue key, payload parser
    └── svc.rs              # RadioMetaService: Nightride SSE loop + one poll loop per PolledFeed, watch<HashMap<station, ArtistTitle>>
```

Cross-crate touchpoints:
- `late-core/src/models/media_queue_item.rs`, `media_source.rs`,
  `media_queue_vote.rs`, `media_history_item.rs`, `media_favorite.rs` — DB models.
- `late-core/migrations/047_create_media_queue_items.sql`,
  `048_create_media_sources.sql`,
  `049_create_media_queue_votes.sql`,
  `073_create_media_history.sql`,
  `122_drop_media_history_votes.sql`,
  `123_media_queue_unique_active_track.sql`,
  `251_media_favorites.sql`.
- `late-core/src/audio.rs`: `VizFrame { bands[VIZ_BANDS], rms, track_pos_ms }` (`VIZ_BANDS` = 16) shared between server and CLI.
- `late-ssh/src/paired_clients.rs` — `PairedClientRegistry`, `PairControlMessage::SetPlaybackSource`, source/surface policy.
- `late-ssh/src/api.rs` — `/api/ws/pair` multiplexes `AudioWsMessage` + `PairControlMessage`; `/api/now-playing`.
- `late-ssh/src/app/chat/{state,input}.rs` — `/audio` and `/audio fallback` chat commands.
- `late-cli/src/ws.rs`, `late-cli/src/main.rs`, `late-cli/src/audio/output.rs` — CLI tolerates unknown audio events and gates Icecast output on `set_playback_source` without changing the user mute flag.
- `late-web/src/pages/listen/` — the public listen page and its `/listen/state` proxy.

---

## 3. Ownership Split

- `svc.rs` is the async boundary. It owns the DB, both broadcast channels, the queue state mutex, the playback timer (which also drives the periodic `LoadVideo` heartbeat for the current item), the fallback debounce timer, and all transitions. **Nothing else in the codebase mutates `media_queue_items.status` or `media_sources`.**
- `state.rs` is the per-session UI shim. It clones the service, holds a per-user `AudioEvent` receiver, exposes `submit_trusted` / `set_youtube_fallback` for chat dispatch, and turns user-scoped events into banners during `tick()`.
- `client_state.rs` is type-only: the JSON shape clients send over `client_state` WS messages. No behavior.
- `youtube.rs` is pure URL/HTTP — no DB, no channels, no service state.
- `viz.rs` is pure render + signal smoothing. Lives in this domain because the data source (Icecast) is audio.
- `now_playing/svc.rs` is independent of `AudioService` — separate channel, separate task, only shares a directory.
- `radio_meta/svc.rs` is likewise independent: its own watch channel and one task (the Nightride SSE loop joined with the polled-feed loops), started once in `main.rs` next to the now-playing poller. It only fetches third-party metadata; it never proxies third-party audio.
- This crate has no Liquidsoap control path; house streams are always-on mounts.

Keep `mod.rs` declaration-only — no `pub use` re-exports.

---

## 4. AudioService (`svc.rs`)

### Channels and state
- `ws_tx: broadcast::Sender<AudioWsMessage>` (cap 512) — server-authoritative pair-WS events, fanned out to every paired client.
- `event_tx: broadcast::Sender<AudioEvent>` (cap 256) — per-user banners (success/failure on submit, fallback set, history requeue/delete, favorites) and the listener's favorites list (`BoothFavoritesLoaded`). Consumed only by `AudioState`.
- `state: Arc<Mutex<QueueState>>` — `{ mode: AudioMode, current_item_id, sequence, playback_cancel: Option<oneshot>, fallback_cancel: Option<oneshot> }`.

### Queue internals

In `QUEUE.md`: the constants, the public API, startup, the state machine, Booth History, timers, `playback_duration`, `player_state` ingress, and the queue reconciliation contract.

### The live strip and thumbnails
Every track playing or queued is a candidate for the live strip (`../live/CONTEXT.md`), stamped with `QueueItemView::queued_at` (the row's `created`): a track is news when somebody brings it, and stops being a candidate when it leaves the booth. `booth/live.rs` paints it: the thumbnail in the picture column, then the title, `channel · 3:45`, and `mat queued it · up next`. `o` tunes a viewer on another source in to YouTube, and opens the booth for one already there.

- `attach_thumbnails` runs on every queue publish: tracks already fetched get their `QueueItemView::thumbnail`, a track seen for the first time starts `fetch_thumbnail_task`, and tracks that left the booth are forgotten. The fetch patches the published snapshot in place (`send_modify`), so the strip repaints when the image lands. The thumbnail map stays locked from the attach to `send_replace`, and the fetch holds the same lock from its slot write through `send_modify`, so a fetch landing mid-publish is either on the new snapshot or patches it, never lost to it.
- The image is `https://i.ytimg.com/vi/<id>/mqdefault.jpg` (16:9, no bars), capped at `THUMBNAIL_MAX_BYTES` and shrunk to 168x96 pixels (8 by 16 a cell, about 64 KB a track). Until it lands, or if the fetch fails, the strip draws a framed play mark of the same size. A failed fetch is not retried while the track sits in the booth.
- Painting is per session, not per replica: `booth/live.rs::render_picture` runs the image through the same chafa symbol picker as chat's inline images (`files/inline_image.rs::render_rgba_preview`) at 21 columns by `THUMBNAIL_ROWS` (6), with the session's `InlineImageRenderSettings`, so a terminal that draws octants or sextants gets them. `LiveState` renders it on the tick, once per track, thumbnail and settings, and the frame paints the kept lines.
- Fetched once per track per replica, in memory only: nothing is stored. Both fields are `#[serde(skip)]`, so paired clients never see them.
- Off when `LATE_YOUTUBE_API_KEY` is unset: nothing can be queued then, and tests never reach the network.
- `late_ssh_booth_thumbnails_total{outcome}` counts `fetched` and `failed`; a failure is logged with the video id.

### Submission reward
Queueing a track pays the person who brought it `SONG_QUEUE_REWARD_CHIPS`
(100), minted through `ChipMove::SongQueued`. The gate lives in
`MediaQueueItem::insert_youtube` (`late-core/src/models/media_queue_item.rs`),
which is the only path a submission may take, so booth, `/audio`, trusted
submits, and a history re-queue cannot pay different amounts or forget to pay.

- **The track is never a gate.** The same song twice, a re-queue from History,
  one somebody else put on an hour ago: all paid. Nothing looks at what was
  queued or whether the room has heard it. The one question asked is how many
  this person has already been paid for today, which is what keeps "why did
  that one not pay?" a single number the guide can state.
- `SONG_QUEUE_MAX_PAID_PER_DAY` (5) per UTC day is therefore the whole of the
  gating, and what stands between the jukebox and a chip printer:
  `MAX_SUBMISSIONS_PER_WINDOW` (10 per 5 minutes) is a rate limit for the
  room's sake, not an economic one.
- Insert, count, and credit are one transaction under a per-user advisory
  lock, the same shape as `Article::create_shared`: a credit that fails leaves
  no orphan row in the queue, and two submissions landing together cannot both
  read the same count and pay a sixth. Migration 169 indexes
  `(user_id, created_at) WHERE reason = 'song_queued'` so the count is not a
  scan over a ledger that only grows.
- `source_ref` is the video id as provenance, so a ledger row says which track
  it paid for. Nothing reads it back; it is not a key.
- `SongQueueReward` (`Paid` / `DailyCapReached`) is what the banner and
  `metrics::record_song_queued` read, never the constant, so a track that came
  in past the cap can never be reported as paid.
  `SubmitQueueResponse.reward_chips` carries it to
  `AudioEvent::BoothSubmitQueued` and `BoothHistoryRequeued`, and
  `state.rs::submitted_line` drops the chips clause entirely when nothing was
  minted.

### Invariants
1. **Singleton playing row.** Enforced both by the partial unique index `idx_media_queue_single_playing` and by conditional `mark_playing` updates. Two racing advancers cannot both succeed; losers reconcile to the DB current.
2. **Server owns track *changes*, not playback positions.** Server picks which item is `playing` and broadcasts `LoadVideo` on changes + every 10s as a heartbeat. Each browser plays its own timeline from wherever YT happens to start. There is no wall-clock-offset sync, so slow networks never audibly skip mid-track.
3. **Force-switch on heartbeat.** A browser receiving `LoadVideo` for a different `item_id` than what it's currently playing MUST swap, regardless of pause/buffer/error state. Same-`item_id` heartbeat with the right `video_id` loaded → no-op (respect a manual pause).
4. **`ended` is not trusted for queue advancement.** Browsers and the CLI webview can report `ended`, but the server treats it as diagnostics only. The playback timer is the only normal `playing → played` path, which keeps embedded-webview startup/teardown state churn from skipping tracks.
5. **Mode is server-managed.** Browser/CLI never write `mode`; they only receive `SourceChanged`.
6. **Sequence monotonicity.** `state.sequence` is bumped before every `QueueUpdate` so clients can drop stale ones.
7. **Banners are user-scoped.** `AudioEvent` carries `user_id` and `AudioState::tick` filters on it; one user's submission failure does not leak to others.
8. **DB beats memory on drift.** Any zero-row terminal transition (`mark_played` / `mark_skipped`) or singleton conflict routes through reconcile. Reconcile never blindly clears `current_item_id` while DB still has a `playing` row.
9. **One active row per track.** The partial unique index `idx_media_queue_active_track` on `(media_kind, external_id) WHERE status IN ('queued','playing')` holds the playlist to one copy of a video. Every insert path (`submit_video` for booth + `/audio`, `requeue_track` for History and favorites) checks `MediaQueueItem::youtube_is_active` first and bails with `"track is already in the queue"`, so the user sees "Already in the queue" rather than a raw constraint error; a submission that loses the race gets the same banner from the violation text. A finished track (`played`/`skipped`/`failed`) leaves the active set and can be submitted or requeued again.

---

## 5. WebSocket Protocol (multiplexed on `/api/ws/pair`)

`api.rs` `handle_socket` (`api.rs`) drives three sources per connection with `tokio::select!`:
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
- `now_playing_update { mounts: { "<mount>": Track } }` — full per-mount icecast now-playing snapshot, pushed by `AudioService::start_meta_forward_task` whenever any mount's track changes; also sent once in the connect catch-up burst.
- `radio_meta_update { stations: { "<station>": { artist, title } } }` — full radio station metadata snapshot (`svc::pair_radio_tracks`: Nightride, any enabled polled station, and the house mounts, which are radio stations to the client; a house track with no artist tag carries an empty artist), pushed on change (deduped, since the radio-meta watch ticks on every SSE event); a station is absent while its feed is down. House mounts are in this map so CLIs that only read radio tracks from it show the house track in MPRIS. Also in the catch-up burst. CLIs and the webview helper ignore both events.

### Server → client `PairControlMessage` (`paired_clients.rs`)
- `toggle_mute`, `volume_up`, `volume_down`, `request_clipboard_image`.
- `set_playback_source { source: "icecast" | "youtube" | "radio", stream_url?, station? }` — sent immediately on pair-WS connect, after persisted `v+x` source changes, and when CLI presence changes. It carries no surface-policy flags: the CLI plays direct streams, the webview helper plays YouTube.

### Client → server `WsPayload` (`api.rs`)
- `heartbeat`
- `viz { position_ms, bands[16], rms }`: the paired CLI's spectrum of what it plays (its native Icecast/radio output, or on Linux the captured YouTube helper stream); drives the equalizer (`VIZ.md`, §18). Older CLIs send `bands[8]`, which `api.rs::bands_from_wire` stretches to 16.
- `client_state { client_kind, ssh_mode, platform, capabilities, muted, volume_percent }` — older CLIs also send `icecast_output_available`, which the server now ignores: it only ever gated the browser Icecast takeover.
- `clipboard_image { … }`, `clipboard_image_failed { … }`
- `player_state(PlayerStateReport)` — `{ item_id, state, offset_ms?, duration_ms?, autoplay_blocked, error? }` (`svc.rs`)

There is **one global broadcast**, no room scoping. Every paired client on every token receives the same `load_video` / `source_changed` / `queue_update`.

---

## 6. Source selection (single audible surface)

Only the CLI pairs, so there is nothing to arbitrate. The user's
persisted `audio_source` alone decides who is audible, and `SetPlaybackSource`
carries no surface-policy flags:

| Source  | Audible surface                             |
|---------|---------------------------------------------|
| Icecast | native CLI decoder                          |
| Radio   | native CLI decoder, direct Nightride stream |
| YouTube | the CLI's embedded webview helper           |

Anyone without the CLI listens at late-web's public `/listen` page, which is a
read-only follower of the same global state and pairs with nothing.

Because the message depends only on the persisted source, **who else is paired
cannot change what a client should be playing**. `client_state` therefore does
not rebroadcast a source: the only sends are the pair-WS connect replay, a
`v+x` source change, and unregistration.

Mechanics:
- `PairControlMessage::SetPlaybackSource { source, stream_url, station }` is
  sent on pair-WS connect, on persisted `v+x` source changes, and when CLI
  presence changes for a token.
- CLI stores `source_is_icecast` as the native-output gate; despite the
  name it is true for direct stream sources (`radio`, and `icecast`) and false for `youtube`. Output emits silence when the gate is false without touching the
  user `muted` flag.
- CLI retargets the decoder thread on source changes. `icecast` restores the
  configured `LATE_AUDIO_BASE_URL`; `radio` uses the server-sent station URL
  (legacy-server fallback: `https://stream.nightride.fm/chillsynth.mp3`);
  `youtube` leaves the last direct stream connected but gates output silent.
  Switching between direct stream URLs first disables native output, bumps a
  stream generation, makes the CPAL output callback drain the queued sample
  buffer to empty, and lets the decoder thread re-enable output only after it
  has opened the new URL and observed the output-flush acknowledgement. This
  prevents a brief old-stream bleed when moving from YouTube to Radio.
- The native CLI spawns the webview helper whenever `source == Youtube`. Nothing
  else takes YouTube away from it, so if the helper fails, YouTube is simply unavailable in the CLI and the user goes
  to `/listen` (see `late-webview/CONTEXT.md`).

### Skip-vote eligibility — YouTube source preference

Skip-vote uses the persisted preference cached on active users. If an active user's `users.settings.audio_source = "youtube"`, they can cast a skip vote and count toward the threshold. Pairing shape is intentionally ignored: CLI-only, embedded-webview, and real-browser users with the same saved preference all count the same. Offline users do not count.

Helpers used by the skip-vote path:
- `User::audio_source(user_id)` — gates the caller: only users whose saved preference is `Youtube` can vote.
- `ActiveUsers[*].audio_source` — cached from the user's settings on login and updated after `v+x`; feeds both the sidebar tags and skip threshold.
- `PairedClientRegistry::set_audio_source(user_id, source)` — only mirrors the new preference to connected clients via `SetPlaybackSource`; it does not define listener counts.

Vote-strip on flip-away: when the DB value transitions from `Youtube` to `Icecast`, `AudioService::persist_audio_source` removes the user from `state.skip_votes` and runs `reevaluate_skip_threshold` (which may fire a skip if the threshold dropped to meet remaining votes).

Eligibility table:

| Saved `audio_source` | Can skip-vote? | Counts toward threshold? |
|----------------------|----------------|--------------------------|
| Radio (default)      | no             | no                       |
| Icecast              | no             | no                       |
| Youtube              | yes            | yes                      |

A user always contributes at most one vote (`HashSet<Uuid>` on `user_id`) and counts once in the denominator while active. Staff `/audio skip` (`force_skip`) bypasses the threshold entirely.

Skip-vote paths reject a current item with `unskippable=true`; staff `/audio skip` still bypasses the vote threshold and can skip it.

---

## 7. Chat Commands (`/audio`, `/audio fallback`, `/audio skip`)

Parsing: `late-ssh/src/app/chat/state.rs` around the `/audio` block.
- Exact match `/audio skip` is checked first (otherwise `strip_prefix("/audio ")` would treat `skip` as a URL).
- Longer prefix `/audio fallback ` is matched next.
- Staff gate: `is_admin || is_moderator`. Non-staff get banner `"/audio is staff-only"`.
- Empty arg → `"Usage: /audio <youtube-url>"` or `"Usage: /audio fallback <youtube-url>"`.
- Valid requests stash into `requested_audio_url` / `requested_audio_fallback_url` / `requested_audio_skip`.

Dispatch: `late-ssh/src/app/chat/input.rs` `handle_post_submit_requests` calls `app.audio.submit_trusted(url)`, `app.audio.set_youtube_fallback(url)`, or `app.audio.skip_trusted()`, which proxy through `AudioState` to `AudioService::{submit_trusted_url_task, set_trusted_youtube_fallback_task, force_skip_task}`.

Music setup and user-facing controls are documented in the Pair guide tab (`?`). There is no separate `/music` help command; don't confuse the guide with staff submission commands.

`/audio` flow:
1. `YoutubeClient::validate_url(url)` extracts the ID and calls YouTube Data API. Accepted forms: `youtube.com/watch?v=…`, `youtu.be/…`, `youtube.com/embed/…`, `youtube.com/shorts/…`, `youtube.com/live/…`, subdomains via `host.ends_with(".youtube.com")`. Anything else returns an `anyhow` error (lowercase, per repo style).
2. Validation rejects missing API key, not-found/non-public/non-embeddable videos, upcoming streams, normal videos without duration, and normal videos shorter than 30s. Live streams are accepted with `is_stream=true`.
3. `MediaQueueItem::insert_youtube` writes the row with `status='queued'`, `media_kind='youtube'`, title/channel/duration/is_stream from the server-side metadata.
4. If nothing is currently playing, `advance_to_next_with_guard` immediately flips it to `playing` and broadcasts.
5. On success, banner via `AudioEvent::TrustedSubmitQueued` — "Queued audio — up next" or "Queued audio — #N in line" depending on position. On failure (URL parse, API key/validation, DB), banner via `AudioEvent::TrustedSubmitFailed` carrying a classified message from `trusted_submit_error_message`.

`/audio fallback` flow:
1. `YoutubeClient::validate_url(url)` (same server-side validation as `/audio`).
2. `MediaSource::upsert_youtube_fallback` — `ON CONFLICT (source_kind) DO UPDATE`, always sets `is_stream=true` because fallback playback is not a queue item with a completion timer.
3. If the queue is empty *and* no item is playing, immediately broadcasts `SourceChanged: youtube` + `LoadVideo` for the fallback so paired clients start it without waiting.
4. On success, banner via `AudioEvent::YoutubeFallbackSet` — "Set YouTube fallback". On failure, banner via `AudioEvent::YoutubeFallbackFailed` carrying the classified message from `trusted_submit_error_message`.

`/audio skip` flow:
1. Routes through `AudioService::force_skip` — unconditional, bypasses the vote threshold; staff can skip directly.
2. Marks the current playing row `skipped` via `MediaQueueItem::mark_skipped` (`WHERE status='playing'`), clears `current_item_id` and any pending `skip_votes`, cancels the playback timer, and runs `advance_to_next_with_guard` to bring up the next queued item (or arm the fallback debounce).
3. If the row was already no longer `playing`, the service reconciles from DB instead of mutating the stale row and asks the caller to retry. On success, banner via `AudioEvent::TrustedSkipFired` — "Skipped audio". On failure (nothing playing, state changed, DB error), banner via `AudioEvent::TrustedSkipFailed` — "Nothing is playing" or "Failed to skip audio".

---

## 8. CLI Integration

Goal: the CLI tolerates everything new the audio domain added, plays direct stream sources when selected, and stays silent when the user selects YouTube.

- **Unknown audio events ignored** (`late-cli/src/ws.rs`). Inbound text is parsed only as `PairControlMessage`. `load_video`, `source_changed`, `queue_update` fail to deserialize, the CLI logs `warn!("ignoring unsupported pair websocket event")`, and the select loop continues. **The CLI does not disconnect on audio events.** Note: each playing track now also produces a 10s `load_video` heartbeat — the CLI log noise budget should account for that.
- **Source gate, not forced mute.** `set_playback_source` updates `source_is_icecast`; `late-cli/src/audio/output.rs` emits silence when it is false. The user-controlled `muted` atomic remains only the local mute keybind / paired mute control. `radio` sets the native-output gate true and uses the server-sent `stream_url` / `station`; Chillsynth is only the legacy fallback when an old server omits `stream_url`.
- **Embedded YouTube webview lifecycle.** The same `set_playback_source` message drives `late-cli/src/ws.rs::WebviewPlaybackController`: `youtube` spawns one helper child (the `late-webview` binary on Linux, `late webview-pair` on Windows/macOS) and writes the session token over the child's stdin pipe; `icecast` or `radio` kills the helper. Do **not** spawn the helper from global `source_changed`.
- **AT-SPI bridge isolation.** The parent CLI spawns the helper with `NO_AT_BRIDGE=1`. This scopes the workaround to the helper process and avoids `libatk-bridge-2.0.so` SIGSEGV crashes caused by stale `at-spi-bus-launcher`/dbus state on some Linux desktops.
- **Embedded webview initial seek only.** On helper open, `late-webview/src/pair.rs` uses the first `queue_update.current.started_at_ms` snapshot to apply a one-shot `startSeconds` to the first matching `load_video`. If a `load_video` arrives before the initial snapshot, the relay buffers it and flushes it when the snapshot decision is known. Once that first load is dispatched, server heartbeats and later queue track switches keep the normal no-offset behavior.
- **YouTube capability.** Native CLI `client_state.capabilities` includes `"youtube"` on desktop platforms. The server still sends `set_playback_source` to every paired entry; older/plain CLIs simply gate Icecast, while YouTube-capable CLIs also launch the helper.
- **CLI identifies itself.** First native `client_state` emitted by `late-cli/src/ws.rs` carries `"client_kind": "cli"`; the helper sends `"client_kind": "webview"`. Helpers from older releases send `"browser"` plus `"ssh_mode": "webview"`, both of which still deserialize (`ClientKind::Webview` aliases `browser`, and `ClientSshMode::Unknown` is `#[serde(other)]`), because a rejected `client_state` would leave that helper with no mute or volume for the session.

---

## 9. Public listen page

File: `late-web/src/pages/listen/`. Browsers do not pair. The
listen page is an anonymous, token-less follower of the same global state the
TUI sees, and it holds no per-user server state at all.

- **One poll, no socket.** The page fetches same-origin `/listen/state` every
  10s, which late-web fills server-side from late-ssh `GET /api/listen` over
  its profile's `ssh_internal_url`. No browser ever calls late-ssh directly, which is
  why late-ssh carries no CORS layer.
- **`/api/listen` is memory-only.** It reads `AudioService::current_snapshot()`
  (the `snapshot_tx` watch, not the DB-backed `snapshot()`), the now-playing
  watch, and the radio-meta watch. Polling it costs no DB work. Its response
  types (`PublicTrack`, `PublicAir`, `PublicStation`) are deliberately separate from
  `QueueItemView`/`QueueSnapshot` so internal fields (`submitter_id`, vote
  score, unskippable, the 200-row history, skip progress) cannot leak into a
  published contract; `api_test.rs` asserts those names never appear in the
  body.
- **Strict station filtering.** The Nightride `/meta` feed carries stations
  late.sh does not offer (rekt, rektory).
  `RadioStation::from_key` returns None for those (and for disabled
  catalogue rows) and they are dropped from the response, rather than
  resolving to the Chillsynth default the way
  `RadioStation::from_settings_str` would.
- **The page's own rules** (one audible surface, source order, joining
  mid-track, attribution) are `late-web/CONTEXT.md`, Listen page.

---

## 10. Visualizer (`viz.rs`)

In `VIZ.md`: the live CLI spectrum, the ambient fallback, smoothing, and each source's band origin. YouTube bands on Linux come from the helper capture (`late-webview/CONTEXT.md`, "YouTube spectrum").

---

## 11. Now-Playing (`now_playing/svc.rs`)

- Shared `watch::Sender<HashMap<String, NowPlaying>>` keyed by Icecast mount name (`chill`, `classical`, …) — one entry per live mount.
- `start_poll_task` spawns a blocking thread that calls `late_core::icecast::fetch_tracks` every 10s (split into 1s sleeps to shut down quickly). `fetch_tracks` parses `/status-json.xsl`, where `icestats.source` is a single object with one mount but an ARRAY with two or more — handled by a `#[serde(untagged)]` enum; do not regress to a single-object parse. The mount key is the last path segment of each source's `listenurl`; sources without a `listenurl` are skipped.
- Per-mount change detection: a mount's `NowPlaying::new(track)` (which stamps `started_at`) is only re-created when that mount's title string changes, so an update on one mount does not reset another mount's elapsed clock. Mounts that vanish from the status page are dropped from the map. The watch only ticks when something actually changed.
- Independent of `AudioService` — does not subscribe to its channels.
- Consumers:
  - `GET /api/now-playing?mount={chill|classical}` (`api.rs`) — `mount` is optional and defaults to `chill`, which keeps late-web's existing param-less fetch working unchanged. Response shape is unchanged.
  - The sidebar music stage (`STAGE.md`), when the user's selected station is a house mount: `stations::station_now_playing` looks the mount up in the map and `sidebar.rs` receives the preformatted `radio_now_playing` text. When the mount has no entry yet, the track row shows the station label.
  - The pair WS via `AudioService::start_meta_forward_task` (§5): per-mount snapshots are broadcast as `now_playing_update` whenever the watch changes, for paired clients.

---

## 12. Sidebar music-stage widget (`common/sidebar.rs`)

In `STAGE.md`: the fixed-height accordion, the active-source rule, title-bar source tags, fallback-not-empty semantics, data sources and helpers. The station table and the onboarding checklist for a new station are in `STATIONS.md`.

---

## 13. Data Model

### `media_queue_items` (migration `047`)
- `id` uuidv7, `created`/`updated` tz, `submitter_id → users ON DELETE CASCADE`.
- `media_kind` CHECK `IN ('youtube')`, `external_id` non-empty, `title`/`channel` nullable, `duration_ms ≥ 0` nullable, `is_stream BOOLEAN`.
- `status` CHECK `IN ('queued','playing','played','skipped','failed')`. `skipped` is active: staff `/audio skip` and threshold skip mark the current `playing` row as `skipped` through `mark_skipped`.
- `started_at`, `ended_at`, `error` nullable.
- `unskippable` marks queued items that normal skip votes cannot skip once playing; staff `/audio skip` still bypasses it.
- Indices: `(status, created)` for queue scans; `(submitter_id, created DESC)` for rate-limit / submitter views.
- **Singleton playing constraint:** `CREATE UNIQUE INDEX idx_media_queue_single_playing ON media_queue_items ((true)) WHERE status = 'playing'`.

### `media_sources` (migration `048`)
- `id` uuidv7, timestamps, `source_kind` CHECK `IN ('youtube_fallback')`, `media_kind` CHECK `IN ('youtube')`.
- `external_id` non-empty, `title`, `channel`, `is_stream BOOLEAN NOT NULL DEFAULT true`, `updated_by → users ON DELETE SET NULL`.
- Unique index on `source_kind` → singleton fallback row, upserted via `MediaSource::upsert_youtube_fallback`.

### `media_history_items` (migrations `073`, `122`)
- `media_history_items` stores community Booth History rows, unique by `(media_kind, external_id)`.
- Columns mirror validated queue metadata (`title`, `channel`, `duration_ms`, `is_stream`) plus `first_played_at`, `last_played_at`, `play_count`, and nullable `last_submitter_id`.
- History has no votes; it is a pure recency list.
- History is listed and pruned to `HISTORY_LIMIT = 200` rows by `last_played_at` descending, then `created` descending. `idx_media_history_last_played` covers that sort.

### `media_favorites` (migration `251`)
- One row per listener per track, unique by `(user_id, media_kind, external_id)`, `ON DELETE CASCADE` from `users`.
- Carries its own copy of the validated metadata (`title`, `channel`, `duration_ms`, `is_stream`), with no reference to `media_history_items`, so the History prune and a staff History delete never take a favorite with it.
- At most `BOOTH_FAVORITES_LIMIT = 20` per listener, enforced in `MediaFavorite::add_from_history` under a per-listener advisory lock, never by the schema.

Model helpers (`late-core/src/models/media_queue_item.rs`, `media_source.rs`):
- `MediaQueueItem::{insert_youtube, find_by_id, list_snapshot, queued_before_count, recent_submission_count, first_queued, current_playing, mark_playing, mark_played, mark_failed, mark_skipped, sweep_orphan_playing}`. Status/kind constants: `STATUS_QUEUED`, `STATUS_PLAYING`, `STATUS_PLAYED`, `STATUS_SKIPPED`, `STATUS_FAILED`, `KIND_YOUTUBE`.
- `MediaSource::{youtube_fallback, upsert_youtube_fallback}`. Constants: `KIND_YOUTUBE_FALLBACK`, `MEDIA_KIND_YOUTUBE`.
- `MediaHistoryItem::{record_play_from_queue_item, list_recent, prune_to_limit, delete_by_id, find_by_id}`.
- `MediaFavorite::{add_from_history, remove, list_for_user, get_by_user_id}` (`media_favorite.rs`), every one scoped by `user_id` in SQL.

---

## 14. Known Gaps and Things to Watch

- **`GET /api/listen` is the one public read.** `AudioService::snapshot()` (DB-backed) stays in-process; the public route uses `current_snapshot()` (memory-only). The TUI booth modal reads the same watch via `AudioState::queue_snapshot()`.
- **Booth modal renders from `watch::Receiver<QueueSnapshot>`.** `AudioService` keeps a `snapshot_tx` watch sender alongside the broadcast channels; every `publish_queue_update_with_guard` uses `send_replace` to store the latest snapshot even when zero receivers are alive (startup often publishes before any SSH booth exists), and `AudioState::queue_snapshot()` borrows the current value. Skip progress (`votes/threshold`) and Booth History are folded into the snapshot before it ships.
- AudioService does *not* drive Liquidsoap. Treat `AudioMode::Icecast` as a hint to the browser/CLI, not a Liquidsoap state change.
- **Guide vs `/audio`.** The Pair guide tab (`?`) explains music setup and controls. `/audio` and `/audio fallback` are staff submit commands. Don't conflate.
- **No public submit route.** Reading the queue is public via `/api/listen`; submitting still requires the SSH booth modal or the staff `/audio` command.
- **Region locks / embedding disabled** may still be partly regional. `/audio` and booth both use the YouTube Data API now, so public/non-embeddable/upcoming/duration failures are caught at submit time. A client may still report `error`, but the server treats that as diagnostics only.
- **`LATE_YOUTUBE_API_KEY` is optional in the dev profile and required in prod** (`config.rs` profiles), but YouTube submissions and fallback updates require it at runtime. Without it, booth submit is disabled and staff `/audio` fails validation.
- **Queue state-drift / singleton-violation stuck state** (`infra/RUNBOOK.md` incident log). The class of bug is non-atomic two-write transitions (DB row status + in-memory `state.current_item_id`); outside the reconciliation contract (`QUEUE.md`) a divergence needs a pod restart to clear. Any new code that flips `media_queue_items.status` or mutates `current_item_id` must route through that contract.

---

## 15. Design boundaries (won.t build)

These are intentional non-goals. Reopen only if the constraint that put them here changes.

- **CLI YouTube decoding via shell-out to an external player (mpv/vlc/yt-dlp wrapper).** Won't ship. The user-side ToS exposure (yt-dlp strips ads/branding) and the config burden (most users don't have a player wired up) put this firmly out of scope. The legal path for CLI-side YouTube is an embedded webview hosting the official IFrame Player.
- **Server-side YouTube fetching.** Server routes `video_id` only; the iframe is the only thing that talks to googlevideo.com.
- **Recording / persistent archive of YouTube audio.** Blocked by YouTube ToS.
- **Ad stripping.** The iframe plays whatever YouTube serves.
- **Lyrics, album art, fancy metadata.** Title + channel is enough.
- **Custom genre control per submission.** Fallback uses the global vote winner like everywhere else.
- **Real Web Audio analysis of the YouTube iframe.** Not possible: cross-origin iframe, no audio hook in the IFrame Player API. The CLI captures the helper's output at the OS layer instead (§18).

---

## 16. Deferred (open backlog)

Open work that's been deliberately punted past v1. Each line is a "we know it's missing, here's the next-time hook."

- **Public `POST /api/queue/submit` HTTP route.** Booth submit goes through the in-process service. Revive when there's a non-SSH submitter (web form, third-party). YouTube Data API validation path is already in code (un-trusted route in `AudioService::submit_url_task`).
- **Expanded queue management outside Booth.** The sidebar music stage already shows current/fallback, skip progress, and up to four next YouTube items; richer queue actions remain Booth-only.
- **Heartbeat cadence tuning.** The 10s `LoadVideo` re-broadcast has never been tuned against real drift. Could be slower (30s) once we have confidence stuck browsers don't accumulate.
- **Region-lock partial failure UX.** Data API validation catches public/embeddable metadata but not every playback-region failure. Client errors are warn-only today because one surface can fail while another succeeds.
- **Better admin feedback** when DB insert fails after local URL validation succeeds.
- **Weighted votes by role** (admin/mod ≠ user) — currently 1 user = 1 vote.
- **Vote history / reputation.**

---

## 17. CLI Embedded Webview for YouTube

The helper that plays YouTube for the paired CLI, its lifecycle, mute/volume, troubleshooting, and the Linux spectrum capture: `late-webview/CONTEXT.md` (repo root).

## 18. YouTube spectrum

See `late-webview/CONTEXT.md`, "YouTube spectrum".

---

## 19. Queue state-drift hazards and reconciliation contract

In `QUEUE.md`. Any new code that flips `media_queue_items.status` follows that contract.

---

## 20. Voice Context Split

Voice rooms (LiveKit grants, the pair-WS voice protocol, the CLI voice runtime, authenticated CLI-only joins) are `../voice/CONTEXT.md`; this file stays on music.

---

## 21. References

- Root context: `../../../../CONTEXT.md`; music library and Liquidsoap: root `MUSIC.md`; the pair WS shell: `late-ssh/CONTEXT.md`.
- Voice context: `../voice/CONTEXT.md`.
- Pair WS handler: `late-ssh/src/api.rs` (look for `handle_socket`).
- Pair registry / mute policy: `late-ssh/src/paired_clients.rs`.
- CLI WS + audio: `late-cli/src/ws.rs`, `late-cli/src/audio/`.
- Public listen page: `late-web/src/pages/listen/page.html`, `late-web/src/pages/listen/mod.rs`.
- YouTube IFrame Player API: https://developers.google.com/youtube/iframe_api_reference
- YouTube Data API `videos.list`: https://developers.google.com/youtube/v3/docs/videos/list
- Browser autoplay: https://developer.mozilla.org/en-US/docs/Web/Media/Guides/Autoplay
