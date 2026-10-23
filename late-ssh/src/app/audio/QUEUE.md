# Audio: the YouTube queue service

`AudioService` (`svc.rs`) in detail: its constants, public API, startup, the state machine, Booth History, timers, `playback_duration`, `player_state` ingress, and the reconciliation contract that keeps memory and the DB's `playing` row in step. Part of the audio domain: channels, the live strip, the submission reward and the service invariants stay in `CONTEXT.md` §4.

## The service

### Constants (`svc.rs`)
- `QUEUE_SNAPSHOT_LIMIT = 50`
- `MAX_SUBMISSIONS_PER_WINDOW = 10` over `SUBMISSION_WINDOW = 5 minutes` — applies to un-trusted `submit_url`, which is the path reached by the Music Booth submit modal (`booth_submit_public_task`). Trusted/admin paths (`submit_trusted_url`) bypass.
- `FALLBACK_DEBOUNCE = 10s`
- `PLAYBACK_HEARTBEAT_INTERVAL = 10s` — periodic `LoadVideo` re-broadcast for the current item. Safety net: browsers already showing the right item no-op; stuck/disconnected/wrong-item browsers force-swap.
- `RECONCILE_INTERVAL = 60s` — background DB reconcile safety net. If memory drifts from the singleton `playing` row (e.g. rollout overlap), the service adopts the DB current, cancels/re-arms timers, and republishes state.
- `STREAM_CAP = 1h` — hard cap on any single playing row's wall-clock lifetime.
- `SKIP_VOTE_FRACTION = 0.3` + `SKIP_VOTE_MIN = 2` — `skip_threshold(youtube_total) = max(ceil(0.3 * youtube_total), 2)`. **Denominator is active users whose persisted `users.settings.audio_source` is `youtube`**, not paired-client/browser presence. Floor of 2 means a lone active YouTube-pref user can't solo-skip; the 30% ceil kicks in above 6 active YouTube-pref users.
- `HISTORY_LIMIT = 200` — community Booth History keeps at most 200 unique YouTube tracks.

### Public API
- `new(db, youtube_api_key)` — `main.rs`.
- `start_background_task(shutdown)` — sweeps orphan `playing` rows, then resumes from DB, then idles. `main.rs`.
- `subscribe_ws()` — `api.rs` (pair WS upgrade).
- `subscribe_events()` — `app/audio/state.rs`.
- `initial_ws_messages()` (`svc.rs`) — catch-up burst sent on every new pair-WS connect: `source_changed`, `queue_update`, and `load_video` for the current playing item or for the configured fallback.
- `snapshot()` — returns `QueueSnapshot { mode, current, queue, history }`. Type exists but no HTTP route exposes it (see `CONTEXT.md` §14).
- `submit_url` / `submit_url_task` — un-trusted, rate-limited, validates via YouTube Data API. **Called by `booth_submit_public_task`** (the in-TUI booth modal submit). Requires `LATE_YOUTUBE_API_KEY`; when unset, `booth_submit_enabled()` returns false and the modal disables the submit row. Inserted rows carry `title`, `channel`, `duration_ms`, and `is_stream` from the Data API — so booth-queued items render their `m:ss` duration in the queue list immediately.
- `booth_submit_public_task` — wraps `submit_url` for the booth modal: emits `AudioEvent::BoothSubmit{Queued,Failed}` (user-scoped banners) and shows "Disabled" if the API key is missing. **This is the user-facing submit path.**
- `submit_trusted_url` / `submit_trusted_url_task` — used by `/audio` (staff). Bypasses rate limit but still validates via YouTube Data API. Normal videos require a server-side duration; live streams set `is_stream=true` and use the 1h cap.
- `set_trusted_youtube_fallback` / `set_trusted_youtube_fallback_task` — used by `/audio fallback`. Also validates via YouTube Data API before upserting the singleton `media_sources` row.
- `report_player_state` / `report_player_state_task` — `api.rs`, ingress for browser `player_state` reports.
- `requeue_track(user_id, RequeueSource)` / `requeue_track_task` — inserts a fresh `media_queue_items` row from stored validated metadata, a History row (`RequeueSource::History`) or one of the caller's favorites (`RequeueSource::Favorite`, looked up by owner and id). Live queue votes always start at 0.
- `favorites_task(user_id, FavoritesStep, after)` — one step on the listener's favorites (`Load`, `Add { history_item_id, favorite_id }`, `Remove(favorite_id)`), chained after the session's previous step so they land in order, in an `audio.favorites_task` span. Every step ends with one reload that publishes `AudioEvent::BoothFavoritesLoaded`, whatever the write did, so a failed reload is its own failure and never reported as a failed write. A failed write is logged there and raises `BoothFavoriteFailed`; a refused add past the cap raises `BoothFavoritesFull`. `metrics::record_booth_favorite` counts every step by `step` (load, add, remove) and `outcome` (loaded, added, already_favorite, full, removed, already_gone, failed), both labels from the closed `BoothFavoriteOutcome`; `record_refused_favorite` counts the session's own `full` refusals. `add_favorite` / `remove_favorite` are the awaitable write halves, and publish nothing; `remove_favorite` returns the rows `MediaFavorite::delete_by_user_id` deleted, 0 for a favorite already gone.
- `delete_history_item` — requires centralized `Caps::DELETE_AUDIO_TRACK` via `Permissions::can_delete_audio_track(false)`.
- `toggle_unskippable` / `toggle_unskippable_task` — staff-only path that flips `media_queue_items.unskippable` only while the item is still `queued`; `u` in Booth Queue mode triggers it.

### Startup lifecycle
1. `sweep_orphan_playing` (`svc.rs`) marks any `status='playing'` row older than `now - 1h` as `failed` with `error = "orphan playing row swept at startup"`.
2. `resume_from_db` (`svc.rs`) reads the lone `playing` row (if any). If `started_at + duration` still in the future, broadcasts a fresh `LoadVideo` with the correct `offset_ms` and re-arms the playback timer. Otherwise marks it `played` and advances.
3. Service is then driven purely by inbound chat submissions, browser player_state reports, and timer fires.

### State machine
DB statuses: `queued → playing → {played | skipped | failed}`.

All transitions go through `svc.rs`:
- `queued → playing`: `mark_playing` conditional `UPDATE … WHERE id=$1 AND status='queued'`. Before promoting, `advance_to_next_with_guard` first checks for an existing DB `playing` row and adopts it. If `mark_playing` races the singleton index (`idx_media_queue_single_playing`), the service treats that as a reconcile signal instead of surfacing a submit failure.
- `playing → played`: `finish_item_due_to_timer` via `mark_played` (`WHERE status='playing'`). Client `ended` reports do not call this path. If zero rows changed, memory was stale; reconcile from DB instead of returning with the old `current_item_id`.
- `playing → failed`: reserved for server-side cleanup/sweeps. Client `player_state: error` reports are informational and do **not** fail or advance the shared queue.
- `playing → skipped`: staff `/audio skip` and threshold skip use `mark_skipped` (`WHERE status='playing'`). A stale pod cannot mutate an already-played row to `skipped`; zero-row updates reconcile and ask the caller to retry.

`advance_to_next_with_guard` is the *only* advancer. It adopts a DB current first, otherwise picks `MediaQueueItem::first_queued()`, tries to flip it, on success broadcasts `SourceChanged: youtube` + `LoadVideo` + `QueueUpdate`. If the queue is empty it tries `publish_youtube_fallback_with_guard`; if no fallback row exists, `schedule_fallback` arms the 10s debounce, after which `finish_fallback_debounce` flips `mode = Icecast` (and re-checks `current_item_id.is_none()` to avoid races).

### Booth History
- History is community-wide and unique by `(media_kind, external_id)`, currently YouTube-only.
- A track is recorded when `advance_to_next_with_guard` successfully promotes a queued row to `playing`. This is the moment it "lands" in history.
- If the same YouTube video plays again, history updates `last_played_at`, `play_count`, and metadata on the existing row rather than inserting a second one.
- History is ordered by `last_played_at DESC`, so the track that just started playing is always the first row. Requeueing an old track only moves it to the top once it actually reaches Now Playing, not when it is queued.
- History pruning sorts the same way; rows after rank 200 are deleted. History is therefore a rolling window of the last 200 distinct videos: a track nobody replays eventually falls off no matter how well liked it was.
- Requeueing from history uses stored validated metadata to create a new `media_queue_items` row. The fresh queue item starts with score 0 and competes normally.
- Queue deletion and History deletion share one moderation-policy permission: `Caps::DELETE_AUDIO_TRACK`. Queue deletion passes `is_owner=true` for the submitter, so users can still delete their own queued rows; History deletion always passes `false`.
- The booth modal switches between `Queue` and `History` lists. Queue mode keeps `+/-/0`, `s`, `d`, and staff `u`; History mode has no voting keys and uses `/` to filter, Enter to requeue, `f` to favorite or unfavorite, and permission-gated `d` to delete a history row. Ctrl+Y (`^y` in the footer) copies the watch link of the track the focus points at: the playing track from the submit row, the selected row in Queue or History. It is checked before any focus or the History filter, because every printable key in the booth is already submit text, filter text, or an action on the selected row; room search's Ctrl+Y copy is the same idea. Copying only lives in the booth and the Stations modal: there is no global copy key.
- Under the current track sits its watch link (`youtube::watch_url`, the same `https://www.youtube.com/watch?v=<id>` form the CLI's MPRIS publishes) in plain text, so a terminal whose OSC 52 copy never reaches the clipboard can still select it by hand. The fallback stream gets no row: the snapshot carries no video for it, and Ctrl+Y from the submit row then reports there is nothing to copy.
- The History list compares each row's `video_id` to `QueueSnapshot.current.video_id`; the matching row renders with a play marker and amber emphasis so users can see which historical track is live now.

### Booth favorites
- Per listener, at most `BOOTH_FAVORITES_LIMIT = 20` (`late_core::models::media_favorite`), stored in `media_favorites` with their own copy of the metadata. History rows are the only way in: `f` on a History row adds it, `f` on a favorite lets it go. Past the cap the add writes nothing and the banner says to unfavorite one first.
- The History list (`BoothModalState::history_rows`) is the listener's favorites newest first, starred, then History without the tracks already shown as favorites. A favorite still in the 200 shows its play count; one that fell off shows none and stays put. The `/` filter covers both, and selection indexes the same rows the list draws.
- `f` moves the session's own list at once (`AudioState::booth_add_favorite` / `booth_remove_favorite`), the way News's `s` moves the shelf, and the write follows as a favorites step. The session names a new favorite's id (UUID v7), so the row it shows is the row the table keeps. The cursor keeps the row that came next under it (`BoothModalState::select_history_track`, by video id), so `f` reads as "keep this, move on"; with nothing after it the cursor follows the track. A list already at the cap refuses before any write.
- Enter on a favorite re-queues it through `RequeueSource::Favorite`, so it plays even after its History row is gone; it pays and rate-limits like any re-queue. `d` (staff) acts on History rows only; a favorite row is the listener's own.
- Favorites never ride the shared `QueueSnapshot`. `AudioState` mirrors them from `BoothFavoritesLoaded`, filtered by `user_id`, so every session of the listener on this replica stays in step; the load a step ends on confirms the session's own move or takes it back. `booth::input::open`, the one door every booth opener goes through, queues a `Load` on each open: a session on another replica sees the change the next time it opens the booth.

### Timers
- **Playback timer** (`schedule_playback_timer`): one `tokio::select!` task per playing item. Sleeps `duration - elapsed` then calls `finish_item_due_to_timer`. Also re-broadcasts `LoadVideo` for the current item every `PLAYBACK_HEARTBEAT_INTERVAL = 10s` from inside the same task — the safety-net heartbeat. Browsers ignore the heartbeat when they're already showing the right item; otherwise they force-swap.
- **Fallback debounce**: one task armed when the queue drains. Cancelled by any new submission via `cancel_fallback`.
- **Periodic reconcile**: every 60s the service compares memory to the DB singleton `playing` row. Reconcile is a full transition: cancel stale timers, clear skip-votes if the current changed, schedule the DB current's timer, and republish queue/load events. If memory says current but DB has none, it clears stale state and advances/fallbacks.
- Timers are owned via `oneshot` cancel handles on `QueueState`; dropping the sender cancels the task.

### `playback_duration` rules (`svc.rs`)
- `is_stream = true` → always `STREAM_CAP` (1h).
- Non-stream with known `duration_ms >= 30s` → `min(duration_ms, STREAM_CAP)` — **1h is a hard cap on every item, not a fallback.** A 2h video plays its first hour, server timer fires, queue advances.
- Non-stream with missing or implausibly short duration is rejected at submit time. Legacy rows in that shape are marked `failed` before promotion/adoption, so old client-backfilled bad durations cannot shorten the timer.
- Client `ended`, `error`, and `duration_ms` reports are not used for advancement or timer scheduling. The server timer advances at `playback_duration`, so a 2h video is capped at 1h and a known 3min video advances at its server-validated duration even if the embedded player reports noisy startup/teardown states.

### `player_state` ingress
Routed by report `state` field:
- `ended` → report-only. It does **not** advance the queue and does not backfill duration. Queue advancement is owned by the server playback timer.
- `error` → warn-only. Client/player errors are not trusted as global queue truth because one surface can fail while another surface plays the item correctly.
- `playing` / `paused` / `buffering` → logged only. `autoplay_blocked = true` logs at `warn!`.
- `unstarted` / `cued` → accepted but report-only. The YouTube IFrame emits them around `loadVideoById`; they must never fail, advance, or backfill duration.
- Unknown future player states parse as `Unknown` and are ignored.

## Queue state-drift hazards and reconciliation contract

Anything that mutates queue state follows this contract.

#### Class of bug

Every queue-state mutation is two writes: the DB row's `status`/`started_at` and the in-memory `state.current_item_id`. A DB write with no expected-old-status filter lets a stale id quietly mutate the wrong row, and treating `changed == 0` as "no-op" instead of "drift, resync" leaves memory pointing at a row whose status moved on; from there every advance races the singleton index and every booth submit fails until a restart. The prod incident that showed it is in the `infra/RUNBOOK.md` incident log (2026-05-19). The contract below makes every divergence recoverable without a restart.

#### Reconciliation contract

The service enforces these invariants. New code in this domain MUST follow them.

1. **No raw `update_status` for queue transitions.** Use `MediaQueueItem::mark_skipped(client, id, ended_at) -> u64` with `WHERE id = $1 AND status = 'playing'`. `force_skip`, the skip-vote-fired branch in `cast_skip_vote`, and `reevaluate_skip_threshold` route through it. Each caller treats `changed == 0` as drift, not success.

2. **`changed == 0` on any `mark_*` is drift, not a no-op.** `finish_item`, `fail_item`, `force_skip`, and the `mark_skipped` paths call the reconcile helper instead of returning early. That helper:
   - Cancels the existing playback timer.
   - Re-reads `MediaQueueItem::current_playing(&client)`.
   - If `Some(row)`: sets `state.current_item_id`, clears `state.skip_votes` if the id changed, reschedules the playback timer, broadcasts `SourceChanged` / `LoadVideo` / `QueueUpdate`.
   - If `None`: clears `state.current_item_id`, falls through to `advance_to_next_with_guard` (which may adopt or promote).

3. **`advance_to_next_with_guard` checks DB current first.** Before promoting a queued row, look at `current_playing` in DB. If DB already has one, adopt it (same code path as reconcile's `Some` branch) instead of trying `mark_playing` and racing the singleton index. This eliminates the singleton-violation symptom entirely — the loser of any race against the DB just adopts what's there.

4. **`mark_playing` unique-violation is recoverable.** Catch the constraint name `idx_media_queue_single_playing` in the Postgres error from `mark_playing`, treat it as "DB has a current we don't know about", route to reconcile. Never surface as a submit failure to the user.

#### Why this beats a leader lock (for now)

A Postgres advisory-lock leader (`devdocs/AUDIO-LEADER.md`) would prevent a *second pod* from also writing. The prod incident was a *single pod* corrupting its own state — a lock wouldn't have helped, and the next pod-after-handover would inherit the same bug class. The reconciliation contract makes every transition self-healing inside one pod; rule (4) also covers most of the rollout-overlap case for free (the loser of a `mark_playing` race reconciles instead of erroring at the user).

#### Regression coverage

`late-ssh/src/app/audio/svc_test.rs` covers both prod shapes:
1. DB has a `playing` row while the service memory is empty; a subsequent submit adopts the DB current instead of surfacing the singleton violation.
2. Service memory points at an already-`played` row while DB has a different `playing` row; `/audio skip` reconciles and does not mutate the played row to `skipped`.

#### What this contract does NOT cover

- **WS broadcast overlap during rolling deploys.** Two pods running for a few seconds during rollout will both broadcast `LoadVideo` / `QueueUpdate` to any browser that's connected to both. The browser receives duplicates and may visibly re-load. Cosmetic, not corrupting. If this becomes user-visible, see `devdocs/AUDIO-LEADER.md`.
- **Crash mid-transaction leaving the DB row as `playing`.** The 1h `sweep_orphan_playing` at startup is the existing safety net; reconcile shortens the window from "1h sweep" to "next time anything calls reconcile."
- **Multi-replica scale-up.** Still single-replica today. If we go multi-replica, the leader lock in `devdocs/AUDIO-LEADER.md` is the answer; the contract alone is not sufficient (followers would race on every advance and rely on the singleton index as the arbiter, which works but spams errors).
