# AUDIO.md

Owns the design of the late.sh audio domain: the always-on Icecast house
radio, a global YouTube queue with DB-backed persistence, the visualizer
pipeline, and the source-priority logic that arbitrates between CLI and
browser audio outputs.

Submission UI is deliberately out of scope for this document. The MVP target
is purely functional: an admin submits YouTube links with `/audio
<youtube-link>` from the SSH chat, the server stores them in a global DB
queue, paired browsers play them back to back through the official YouTube
iframe player, and the system falls back to Icecast when the queue is empty.
TUI surfaces, voting, keybindings, sidebar widgets, and public submission
flows all come later.

---

## 1. Goals

### MVP (functional only)

- Always-on Icecast house radio keeps working exactly as today.
- A staff-only `/audio <youtube-link>` SSH chat command (admin or moderator)
  accepts a YouTube URL.
- The command is trusted for MVP: it extracts the video id locally and
  does not require `LATE_YOUTUBE_API_KEY`.
- The URL is persisted in a DB table that survives pod restarts.
- When the queue has items, every paired browser plays the current video via
  the official YouTube iframe player.
- When the current video ends, reports an error, or hits the 1h fallback cap,
  the server advances to the next item.
- When the queue empties and a staff-configured YouTube fallback exists,
  browsers stay in the iframe and play that fallback stream. Otherwise the
  server emits `source_changed: icecast`.
- CLI clients continue playing Icecast regardless of queue state. They cannot
  decode YouTube, so they ignore queue events (with an internal mute when the
  server says a paired browser is the active source).

### Scope

- **Global queue.** One queue for all of late.sh. No rooms, no per-channel
  queues, no per-user opt-in. Every paired browser sees the same playback.
- **DB-backed.** Queue items live in Postgres. Pod restart loses runtime
  state (the per-track timer) but not the queue itself. The current playing
  item is restorable from `status='playing'` rows.
- **No submission UI in this iteration.** Submission is via the staff-only
  `/audio` command. The web connect page has the required playback iframe
  plumbing but no queue controls. Visibility is via DB or `GET /api/queue`.
  TUI surfacing comes later.
- **No voting in this iteration.** Strict FIFO by `created_at`. Voting is a
  later addition; the data model is shaped so adding a vote table is
  additive, not migration-heavy.

### Architectural

- All audio logic lives in one dedicated domain: `late-ssh/src/app/audio/`.
- `late-core` owns only DB-backed entities and pure types.
- `late-cli` stays free of `late-core` and free of any YouTube extraction
  logic. The CLI plays Icecast only.
- Existing scattered audio code (Liquidsoap telnet in `vote/`, now-playing
  fetch in `main.rs`, visualizer types in three places, audio control state
  in `session.rs`) has been consolidated into the new audio domain.

---

## 2. Persistence

The single hard requirement here is that the queue survives pod resets.

State that MUST persist (DB):

- `media_queue_items` rows. Every submission writes one row before being
  acknowledged. Items have a `status` so a pod restart can resume mid-queue.

State that MAY be lost on restart (in-memory):

- The current-track countdown timer. On restart, the audio service reads
  rows where `status='playing'` and re-derives "this track started at X,
  ends at Y." If X+duration is in the past, mark it `played` and advance.
- WebSocket subscriptions to paired clients. They reconnect on their own.
- Drift correction state on each browser. Each browser resyncs from
  server-computed `offset_ms` on reconnect.

Restart algorithm (on pod start, in `AudioService::start`):

1. Read `media_queue_items WHERE status IN ('queued', 'playing') ORDER BY created`.
2. If any item is `status='playing'`:
   - If `started_at + playback_duration > now()`, resume it: keep status,
     broadcast `load_video` with the current `offset_ms` so browsers seek to
     the correct offset. `playback_duration` is the known duration when
     present, otherwise the 1h fallback cap.
   - Else mark it `played`, advance to the next queued item.
3. If only `queued` items remain, mark the first one `playing` with
   `started_at = now()`, broadcast `load_video`.
4. If nothing remains, broadcast `source_changed: icecast`.

---

## 3. Domain location

### 3.1 What has been implemented for MVP

```
late-core/
├── migrations/
│   └── 047_create_media_queue_items.sql  # NEW: global media queue table
└── src/
    └── models/
        └── media_queue_item.rs     # NEW: MediaQueueItem entity

late-ssh/
└── src/
    ├── api.rs                      # MODIFIED: GET /api/queue, WS audio fan-out
    ├── main.rs                     # MODIFIED: spawn AudioService
    ├── state.rs                    # MODIFIED: add audio_service handle
    ├── app/chat/state.rs           # MODIFIED: staff-only /audio command
    ├── app/chat/input.rs           # MODIFIED: dispatch /audio request
    └── app/
        └── audio/                  # NEW domain folder
            ├── mod.rs              # declarations only
            ├── svc.rs              # AudioService: queue state machine, broadcast
            ├── youtube.rs          # URL parsing + optional API validation for v2
            ├── liquidsoap.rs       # moved from app/vote/liquidsoap.rs
            ├── client_state.rs     # moved from session.rs
            ├── viz.rs              # visualizer code
            └── now_playing/svc.rs  # now-playing poll loop

late-web/
└── src/
    └── pages/
        └── connect/
            └── page.html           # MODIFIED: YouTube IFrame API hookup
```

### 3.2 Consolidation already done

The original plan treated Liquidsoap, now-playing, visualizer, and
client-audio-state consolidation as later refactor work. That consolidation
has now been done while building the MVP.

```
late-ssh/src/app/audio/
├── liquidsoap.rs              # moved from app/vote/liquidsoap.rs
├── client_state.rs            # ClientAudioState moved from session.rs
├── viz.rs                     # Visualizer moved from app/visualizer.rs
└── now_playing/
    ├── mod.rs
    └── svc.rs                 # NowPlayingService moved out of main.rs
```

Still deferred: per-session audio UI state, queue widgets, voting,
multi-tab dedupe, and stricter CLI/browser source arbitration.

### 3.3 What stays out of `late-core`

`late-core` keeps only what compiles without a tokio runtime: existing pure
types, plus the new `MediaQueueItem` model. Service state, channels, tasks,
and the optional YouTube Data API client all live in `late-ssh/src/app/audio/`.

The `late-cli` crate continues to have zero `late-core` dependency. It was
changed only so unknown WebSocket events from the global audio fan-out are
ignored instead of disconnecting the CLI. It keeps playing Icecast.

---

## 4. Data model

One table for MVP.

```sql
-- migration: media_queue_items
CREATE TABLE media_queue_items (
    id              UUID PRIMARY KEY DEFAULT uuidv7(),
    created         TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated         TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    submitter_id    UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    media_kind      TEXT NOT NULL DEFAULT 'youtube'
                    CHECK (media_kind IN ('youtube')),
    external_id     TEXT NOT NULL CHECK (length(trim(external_id)) > 0),
    title           TEXT,
    channel         TEXT,
    duration_ms     INTEGER CHECK (duration_ms IS NULL OR duration_ms >= 0),
    is_stream       BOOLEAN NOT NULL DEFAULT false,
    status          TEXT NOT NULL DEFAULT 'queued'
                    CHECK (status IN ('queued','playing','played','skipped','failed')),
    started_at      TIMESTAMPTZ,              -- set when status flips to playing
    ended_at        TIMESTAMPTZ,              -- set when status leaves playing
    error           TEXT                      -- last failure reason
);

CREATE INDEX idx_media_queue_status_created
    ON media_queue_items(status, created);

CREATE INDEX idx_media_queue_submitter_created
    ON media_queue_items(submitter_id, created DESC);

CREATE UNIQUE INDEX idx_media_queue_single_playing
    ON media_queue_items ((true))
    WHERE status = 'playing';
```

YouTube fallback is stored outside FIFO queue state:

```sql
CREATE TABLE media_sources (
    id              UUID PRIMARY KEY DEFAULT uuidv7(),
    created         TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated         TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    source_kind     TEXT NOT NULL CHECK (source_kind IN ('youtube_fallback')),
    media_kind      TEXT NOT NULL DEFAULT 'youtube',
    external_id     TEXT NOT NULL,
    title           TEXT,
    channel         TEXT,
    is_stream       BOOLEAN NOT NULL DEFAULT true,
    updated_by      UUID REFERENCES users(id) ON DELETE SET NULL
);

CREATE UNIQUE INDEX idx_media_sources_source_kind
    ON media_sources (source_kind);
```

All PKs use DB-side `uuidv7()` per project convention.

Why no `media_rooms` table: scoping was decided as fully global. There is no
room to belong to.

Why no `media_queue_votes` table in this iteration: voting is deferred.
Adding it later is a single new table with `item_id` FK + `user_id` FK; the
queue items table does not need to change.

Why `media_kind` is a CHECK constraint with only `'youtube'`: makes it
explicit this is the YouTube-only world for now, but the column is in place
so future Spotify/SoundCloud/etc. additions are a constraint relaxation,
not a schema migration.

Why fallback is not a queue item: it should not be FIFO, should not be marked
`played`, and should be replaced in place by admin command.

---

## 5. Submission and API surface

### 5.1 Submit a link (implemented MVP path)

```
/audio https://www.youtube.com/watch?v=abc123
```

Server steps:

1. Extract `video_id` from the URL. Accept canonical
   `youtube.com/watch?v=...`, `youtu.be/...`, and
   `youtube.com/embed/...`, `youtube.com/shorts/...`, and
   `youtube.com/live/...` forms. Reject anything else immediately in the SSH
   UI.
2. Trust the admin for MVP. Do not call the YouTube Data API. Do not require
   `LATE_YOUTUBE_API_KEY`.
3. Insert `media_queue_items` row with `status='queued'`, `media_kind='youtube'`,
   `external_id=<video_id>`, and empty title/channel/duration metadata.
4. Notify the audio service in-process so it can either
   start playback immediately (if queue was empty) or just append.
5. Show a local "Queued audio" banner to the admin.

### 5.2 Set YouTube fallback stream

```
/audio fallback https://www.youtube.com/watch?v=abc123
```

Staff-only (admin or moderator). The command extracts the video id and upserts the singleton
`media_sources.source_kind = 'youtube_fallback'` row. If no queue item is
playing or queued, the service immediately broadcasts `source_changed:
youtube` and `load_video` for the fallback stream.

### 5.3 HTTP submit (deferred)

`POST /api/queue/submit` is not exposed for the MVP. The optional YouTube
Data API validation path still exists in code for a future public or
non-admin flow, but the working MVP path is `/audio`.

When revived later, HTTP submit should validate embeddability/public status,
duration, quota failure, and rate limits before inserting queue items.

### 5.4 Read the queue (deferred)

No HTTP route is exposed. `AudioService::snapshot()` returns a
`QueueSnapshot { mode, current, queue }` for in-process callers. The TUI
reads the queue via direct DB queries through `MediaQueueItem`
(`list_snapshot`, `first_queued`); the browser receives queue state via the
existing pair-WS catch-up burst (`initial_ws_messages`) and live
`queue_update` events. An external `GET /api/queue` would only matter for
non-paired observers, which we do not have. Revisit if that changes.

---

## 6. WebSocket protocol additions

The existing `/api/ws/pair` channel gains four new server-to-client events
and one new client-to-server event. No `room_id` field anywhere; events
fan out to every paired client globally.

### 6.1 Server -> client (broadcast to all paired clients)

```json
{ "event": "load_video",
  "item_id": "<uuid>",
  "video_id": "abc123",
  "started_at_ms": 1770000000000,
  "offset_ms": 90000,
  "is_stream": false }

{ "event": "seek", "offset_ms": 90000 }

{ "event": "source_changed", "audio_mode": "icecast" | "youtube" }

{ "event": "queue_update",
  "current": { ... } | null,
  "queue": [ ... ],
  "sequence": 42 }
```

`load_video` is the trigger for the browser to call
`player.loadVideoById({ videoId, startSeconds: offset })`. `source_changed`
tells the browser to swap between iframe (youtube) and `<audio>` (icecast).
`queue_update` is informational; the MVP browser does not need it (no UI),
but it's in the protocol so future TUI / browser UI can subscribe.

### 6.2 Client -> server

```json
{ "event": "player_state",
  "item_id": "<uuid>",
  "state": "playing" | "paused" | "buffering" | "ended" | "error",
  "offset_ms": 12345,
  "duration_ms": 212000,
  "autoplay_blocked": false,
  "error": "<reason>" }
```

The browser reports playback state for each loaded item. The server treats
`offset_ms` and `duration_ms` as client observations. It uses `error` to
mark items failed. It only accepts `ended` when the server timeline says the
item is actually at the end; an early browser `ended` report is ignored and
the server sends a sync seek back to the authoritative offset.

### 6.3 Fan-out implementation

`AudioService` holds a `tokio::sync::broadcast::Sender<AudioEvent>`. The
existing `/api/ws/pair` handler in `late-ssh/src/api.rs` subscribes to it on
connect, multiplexes the events onto the same socket as the existing
per-token control messages, and unsubscribes on disconnect.

No new registry. No room-keyed channels. One global broadcast.

---

## 7. Audio source state machine

### 7.1 Global state

```
audio_mode = "icecast" | "youtube"
  default:  "icecast"
  flips youtube:  when AudioService advances to a queued item
  flips icecast:  when AudioService finishes the last item, with a debounce
```

### 7.2 Debounce on fallback

When the last queued item ends, do not immediately broadcast
`source_changed: icecast`. Wait 10 seconds. If a new submission arrives in
that window, cancel the pending flip; the new item starts cleanly without a
visible Icecast intermission. If the window expires, broadcast the source
change and the browser swaps to the `<audio>` element.

Implementation: one `tokio::time::sleep` task per pending flip; cancelled
via `oneshot` if a submission arrives.

### 7.3 Per-client behavior

The rule is intentionally simple: **if any browser is paired on a token, the
CLI on that same token mutes its local audio.** No "personal fallback" mode,
no per-mode special casing. The browser is the active surface whenever it is
present; the CLI is the active surface only when it is alone.

| CLI paired | browser paired | audio source                  | CLI audio                              |
|------------|----------------|-------------------------------|----------------------------------------|
| yes        | no             | Icecast                       | CLI plays Icecast (today's flow)       |
| yes        | yes            | Icecast or YouTube (browser)  | CLI muted via ForceMute                |
| no         | yes            | Icecast or YouTube (browser)  | n/a                                    |
| no         | no             | silent                        | n/a                                    |

The audio_mode column from earlier drafts has been folded away: it only
controls *what the browser plays*. The CLI's behavior depends purely on
whether a browser is paired alongside it.

### 7.4 CLI mute coordination

Implemented via the `PairedClientRegistry` and a new `force_mute`
`PairControlMessage`:

- The registry tracks every paired client per token (CLI + browser
  coexist) keyed by registration id.
- When a browser's `client_state` first identifies it as `client_kind:
  browser`, the server broadcasts `force_mute { mute: true }` to every CLI
  entry on the same token. Same path runs if a CLI joins a token that
  already has a browser paired.
- When the last browser entry on a token disconnects, the server
  broadcasts `force_mute { mute: false }` so the CLI's Icecast audio
  resumes immediately.

The CLI applies `force_mute` to the same `muted` atomic that the local
mute keybind uses. If a user manually un-mutes via their CLI hotkey while a
browser is still paired, the server does not re-impose mute — the user has
explicitly opted into double audio. The next browser pair/unpair transition
overrides them.

---

## 8. Sync algorithm

Server is authoritative. Each `load_video` carries both `started_at_ms`
(server epoch, for observability/resume state) and the server-computed
`offset_ms`. Browser uses `offset_ms` for initial playback and advances that
value with a monotonic local timer:

```
player.loadVideoById({ videoId, startSeconds: offset_ms / 1000 })
```

The server also broadcasts periodic `seek` sync events with the current
authoritative offset. Browsers compare that offset to their iframe position
and seek only when drift crosses the correction threshold. This lets users
catch up after ads/buffering without letting a local iframe control the
global queue timeline.

Drift correction:

- Every 10s, browser compares `player.getCurrentTime() * 1000` against
  expected offset.
- `|drift| < 2500ms`: ignore.
- `|drift| >= 2500ms`: hard seek.
- Hard seek cooldown: 5s.

Live streams skip drift correction entirely; the 1h cap governs the
lifecycle.

---

## 9. Edge case ledger

Decided behaviors. The MVP only needs to handle 1-4 and 7 to work. The rest
are still captured so future-you does not relitigate.

1. **Pod restart mid-track.** Resume logic in §2. Restored items have their
   original `started_at`, so paired browsers seek to the right offset.

2. **Submission while audio_mode is icecast.** Audio service immediately
   marks the new item `status='playing'`, sets `started_at = now()`,
   broadcasts `load_video` and `source_changed: youtube`. Icecast does not
   gracefully finish its current track; the cut is abrupt. Acceptable for
   MVP.

3. **Queue ends, fallback debounce.** §7.2.

4. **1h stream/unknown-duration cap.** On `load_video` for an item with
   `is_stream=true` or unknown duration, schedule a forced-skip task at
   `started_at + 3600s`. If browsers later report a real duration, persist
   it and reschedule the server timer to the actual server-side end time.
   The server timeline remains authoritative.

5. **Min duration on submission.** Deferred for the trusted admin MVP path
   because `/audio` does not call the YouTube Data API. Add it back with
   public/non-admin submit.

6. **Per-user submission rate limit.** Deferred for the trusted admin MVP
   path. Admin `/audio` bypasses the limiter.

7. **Browser reports `ended`.** Audio service validates the report against
   server elapsed time and known/reported duration. If it is early, ignore
   it and broadcast an authoritative seek. If it is near the server-side end,
   mark item `played`, set `ended_at = now()`, and advance queue. If no
   duration is known, accept `ended` only after a server-side 30s floor so
   short/metadata-weird MVP tracks do not get stuck forever.

8. **Browser reports `error`.** Mark item `failed`, store error message,
   advance queue. The current implementation treats the first matching
   report for the active item as authoritative.

9. **All items failed.** Treated as queue empty. Falls back to Icecast with
   the standard debounce. The DB retains the failed items for postmortem.

10. **Same-Icecast double-play (CLI + browser both paired, icecast mode).**
    Existing problem, pre-dates the queue feature. Deferred.

11. **Region locks and embedding disabled.** Not caught at `/audio`
    submission time in MVP. The browser reports `error`; the server marks
    the item failed and advances. Pre-validation comes back with the public
    HTTP submit flow.

12. **YouTube Data API quota exhausted.** Not relevant to `/audio` MVP,
    because no API key is required. Relevant again for v2/public submit.

13. **Late browser join during a `playing` item.** Browser pairs, server
    immediately sends `queue_update` and `load_video` with current
    `offset_ms`. Browser seeks to correct offset on the autoplay
    gesture. No special-casing needed beyond the standard `load_video`
    flow.

14. **Multi-tab.** Two tabs on the same session token both subscribe to the
    global broadcast and both try to play. Double audio. Defer the dedupe
    until UI work; the MVP test scenario has one browser tab.

15. **Ads.** YouTube serves ads via the iframe. We do not strip them. ToS
    posture stays clean.

---

## 10. Implementation status

### Done

- `CONTEXT.md` updated to mention the audio domain.
- Global `media_queue_items` migration and model.
- `AudioService` with queue state machine, DB resume, fallback debounce,
  playback timer, server-authoritative sync seeks, browser state reports,
  and global broadcast events.
- Staff-only `/audio <youtube-link>` chat command (admin or moderator).
- Staff-only `/audio fallback <youtube-link>` command for the singleton
  YouTube fallback stream.
- Existing `/api/ws/pair` multiplexes audio events and accepts
  `player_state`.
- Browser connect page loads the YouTube IFrame API, switches between
  Icecast and YouTube, sends state/duration observations back to the server,
  corrects drift from server sync seeks, and stays in YouTube on fallback
  when a fallback stream is configured.
- CLI tolerates audio events and keeps its Icecast path.
- Audio code consolidation into `late-ssh/src/app/audio/`.
- Music Booth: `media_queue_votes` migration + model, +/- voting on queued
  items, democratic skip-vote with paired-client threshold, public submit
  path revived (un-trusted YouTube Data API validation), TUI booth modal
  with submit/list/vote keybinds.
- Booth chat keybinds: `v+v` opens the booth modal, `v+s` casts a skip
  vote, `v+x` toggles the *paired browser* between Icecast and the
  YouTube queue (per-user preference, browser-side state).

### Ready flow

1. Admin opens SSH chat.
2. Admin submits `/audio <youtube-link>`.
3. Server inserts a global queued item and starts it if nothing is playing.
4. Paired browser receives `source_changed: youtube` and `load_video`.
5. Browser plays through the official iframe.
6. Browser sends `player_state` observations.
7. Server advances only when its own timeline says the item is done, or
   marks failed on playback error.
8. Empty queue falls back to Icecast after the debounce.

### Not done / intentionally deferred

- Public/non-admin `POST /api/queue/submit` HTTP route (booth submit
  goes through the in-process service, not HTTP).
- TUI sidebar widget on Home for queue visibility.
- Drift correction tuning.
- Multi-tab dedupe.
- Region-lock partial failure UX.
- Better admin feedback if DB insert fails after local URL validation.
- Browser-side voting UI.
- Persistent per-user Icecast preference (today the `v+x` toggle is
  browser-memory only and resets on browser reload).

---

## 10b. Browser-side YouTube gate (`?youtube=true`)

The connect page (`/{token}`) accepts an optional `?youtube=true`
query parameter. Without it, the page never leaves Icecast: it ignores
`source_changed: youtube`, `load_video`, and `seek` events, and the
YouTube IFrame API script is not loaded. With it, the full YouTube path
runs.

This is a temporary admin/mod-only toggle so the v1 push can land with
the YouTube path dark for general visitors while still being testable by
copying a link with the flag. The end-state UX is an in-page Icecast /
YouTube switch; the query gate will be replaced when that ships.

The gate applies to both queue items and the YouTube fallback stream:
if the flag is absent, the page stays on Icecast even when a fallback is
configured server-side.

---

## 11. Visualizer note

Out of scope for MVP. Recorded here so the constraint is not forgotten:

- Real Web Audio analysis of the YouTube iframe is **not possible**
  (cross-origin iframe, no audio hook in the IFrame Player API). This is a
  browser security constraint, not a project decision.
- The existing real visualizer for Icecast playback keeps working unchanged.
- Any future queue UI should avoid pretending YouTube iframe playback has
  real frequency analysis. The visualizer sidebar must either (a) hide
  entirely, (b) switch to a labeled "playing" indicator driven procedurally
  (named honestly in code, e.g. `procedural_indicator_bands`, never
  `viz_bands`), or (c) stop showing bars.

---

## 12. What we deliberately do not build

- **CLI YouTube decoding.** See §13. CLI plays Icecast only.
- **Server-side YouTube fetching.** Server routes `video_id` only. The
  iframe is the only thing that speaks to googlevideo.com.
- **Recording or persistent archive of YT audio.** Not allowed by ToS.
- **Ad stripping.** The iframe plays whatever YouTube serves.
- **Lyrics, album art, fancy metadata.** Title + channel is enough.
- **Custom genre control per submission.** Fallback uses the global vote
  winner just like everywhere else.
- **Submission UI in MVP.** Staff-only `/audio` is sufficient for the
  two-link smoke test.

---

## 13. Parked: CLI external-player handoff for YouTube

### Status

Parked. Not on the active build path. Reason: the user-facing configuration
burden is too high for current scale. Most users will not have a suitable
player installed and will not want to edit a TOML config to make a clubhouse
feature work. Revisit when the audience is technical enough or large enough
to justify a setup guide.

### Summary

Instead of opening a browser for YouTube playback, `late` shells out to a
local media player (configured by the user) that already knows how to play
YouTube. late.sh never touches YouTube audio; it only sends `video_id` over
WebSocket. The CLI is a general external-player runner with no YouTube
extraction code of its own.

### The core idea

late.sh stays out of the audio path entirely. The server is a metadata
coordinator. The CLI is a thin remote control for a media player that
already runs on the user's machine.

```text
late.sh server
  -> queue state, "play video_id at offset N"
  -> sends only metadata over /api/ws/pair
late CLI (per user, on user's machine)
  -> receives play / skip / seek / pause messages
  -> spawns or controls user-configured local player
  -> reports back: position, ended, errors
local media player (mpv, mpsyt, yewtube, FreeTube, vlc, anything)
  -> actually fetches and decodes audio from YouTube
  -> belongs to the user, installed by the user
```

The CLI never bundles, distributes, or recommends any specific player. It
exposes a config slot. The user fills it in with a command they wrote
themselves.

### Two control modes

**Command mode** (simple, single-shot per track):

```toml
[player.youtube]
mode = "command"
command = "<player> <flags> {url}"
```

Server says play, CLI spawns the configured command with `{url}` substituted,
process exits when the track ends, CLI tells the server `ended`, server picks
the next track. Skip = SIGTERM. About 80 lines of Rust.

**IPC mode** (richer, for sync/seek/pause):

```toml
[player.youtube]
mode = "ipc"
launch = "<player> --idle --input-ipc-server={socket}"
protocol = "mpv"
```

Long-running player. CLI sends commands over a JSON/IPC socket. Server can do
drift-resync, mid-track seek, pause/resume. `protocol` is the only
player-specific code shipped in `late`. Start with one adapter; community
can add more.

### What we ship vs what we don't

| Safe (ship)                                          | Unsafe (don't ship)                                       |
|------------------------------------------------------|-----------------------------------------------------------|
| Config slot for external player command              | Bundled mpv or yt-dlp binaries                            |
| Template variables (`{url}`, `{socket}`)             | `late install-youtube` subcommand                         |
| Generic IPC protocol adapter (mpv first)             | Auto-download of any extraction tool                      |
| `late doctor` against a benign non-YouTube test URL  | `late doctor` testing against a real YouTube URL          |
| Clear errors when no player is configured            | Naming a specific tool inside the binary                  |
| Community-maintained `EXTERNAL_PLAYERS.md`           | Official "recommended player" in onboarding flow          |

### Legal posture (parked)

Not "legal." Risk-shifted. late.sh ships zero yt-dlp code, the CLI is a
general external-player runner, every byte of YouTube audio is fetched by the
user's own machine, by a tool the user chose and installed. The user-side
play still violates YouTube ToS (yt-dlp strips ads, branding, controls).
late.sh is not the violator; the user is. A motivated YouTube lawyer could
argue the CLI is "designed to facilitate" ToS-violating playback; the
argument weakens dramatically when the CLI is broadly an external-player
runner with no player named in code or docs.

### ToS posture

A user-side mpv-with-yt-dlp setup still violates YouTube ToS on the user's
machine (yt-dlp strips ads, branding, and controls). If this plan is ever
activated, late.sh would need to be explicit that the CLI is a generic
external-player runner and that the user, not late.sh, is responsible for
what their configured player does.

### Reactivation criteria

- The user base is large enough and technical enough that a setup guide is
  worth maintaining.
- A stable, official YouTube-API-compliant CLI player emerges (currently
  does not exist; the closest options all use yt-dlp under the hood).
- We decide to make late.sh deliberately CLI-power-user-shaped, and a
  config file with a player slot fits the product identity.

Until then, YouTube playback goes through the browser iframe path described
in §3-§8.

---

## 14. Music Booth (shipped)

Adds public YouTube submissions, per-track up/down voting, and democratic
skip on the existing global YouTube queue (§1-§13). Icecast and the genre
vote system in `app/vote/` are unchanged — this is purely additive on the
YouTube path.

### 14.1 Surface
- `v+v` in TUI chat: opens the booth modal (submit + queue + vote controls).
- `v+s` in TUI chat: casts a skip vote on the currently-playing item.
- No web UI in this iteration.

### 14.2 Voting model
- New table `media_queue_votes (user_id, item_id, value ∈ {-1,+1})`, unique
  on `(user_id, item_id)`. Removing a vote = row delete.
- Queue order: `ORDER BY (vote_score DESC, created ASC)` over
  `status='queued'`. FIFO is the tiebreaker.
- Currently-playing track is unvotable. Modal hides up/down for it; votes
  against `status='playing'` rejected with banner "voting closed — track
  started."
- Each vote bumps the queue sequence and broadcasts a fresh `queue_update`.

### 14.3 Skip-vote model
- In-memory `HashSet<UserId>` on `QueueState`, scoped to the current
  `item_id`. Cleared on every track change.
- Threshold: `ceil(0.1 × paired_clients)`, min 1. Counts both CLI and
  browser via `PairedClientRegistry`.
- On threshold, advance current item to `status='skipped'` (the enum value
  that was reserved but never written) and broadcast the next track.
- Not persisted across restart — by design. The listener set has changed.

### 14.4 Submission
- Revives `AudioService::submit_url_task` (the un-trusted path that exists
  but has no caller today). Validates via YouTube Data API: embeddable,
  public, region-allowed, min duration.
- `LATE_YOUTUBE_API_KEY` becomes effectively required for the booth. If
  missing, the booth submit field is disabled with a banner; staff
  `/audio` continues to work.
- Existing per-user rate limit (3 per 5 min) carries over.
- Staff `/audio` keeps the trusted bypass — no API, no rate limit. Ops
  escape hatch when YouTube API quota/availability is bad.

### 14.5 Eligibility
- Submit, vote, skip-vote: any authenticated SSH user. No pairing
  requirement — you can vote without listening.
- Threshold denominator counts paired clients only (CLI + browser).
  Intentional: the threshold reflects active listeners, not total
  participants.

### 14.6 Schema

Migration `049_create_media_queue_votes.sql`:

```sql
CREATE TABLE media_queue_votes (
    id        UUID PRIMARY KEY DEFAULT uuidv7(),
    created   TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated   TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    item_id   UUID NOT NULL REFERENCES media_queue_items(id) ON DELETE CASCADE,
    user_id   UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    value     SMALLINT NOT NULL CHECK (value IN (-1, 1))
);

CREATE UNIQUE INDEX idx_media_queue_votes_user_item
    ON media_queue_votes (user_id, item_id);

CREATE INDEX idx_media_queue_votes_item
    ON media_queue_votes (item_id);
```

`media_queue_items` is unchanged. Vote score is computed via LEFT JOIN +
SUM in `MediaQueueItem::first_queued` and `MediaQueueItem::list_snapshot`.

### 14.7 Service surface

`AudioService` gains:
- `cast_vote(user_id, item_id, value)` — upsert, returns new score,
  rejects non-queued items, broadcasts `queue_update`.
- `clear_vote(user_id, item_id)` — delete, returns new score, broadcasts.
- `cast_skip_vote(user_id)` — adds to in-memory set for current item,
  returns `{ votes, threshold }`. Advances queue when threshold is hit.

`QueueState` gains:
- `skip_votes: HashSet<UserId>` keyed implicitly to `current_item_id`;
  cleared on every transition that changes `current_item_id`.

`AudioState` (per-session shim) proxies these and surfaces banners.

### 14.8 Modal placement
New module `app/audio/booth/` keeps `app/chat/state.rs` from growing.
Chat opens the modal via the existing modal-stack mechanism. v+v / v+s
keybinds registered in chat's keybind table.

### 14.9 Browser WS additions
Browser doesn't vote in this iteration but should reflect reorders.
Extend the existing `queue_update` payload:
- Each item gains `vote_score: i32`.
- Current item gains `skip_progress: { votes: u32, threshold: u32 }`.

No new events. No new client→server messages.

### 14.10 Edge cases
- **Vote race with track advance.** Conditional update against
  `status='queued'`; zero rows affected → banner.
- **Threshold drops mid-track.** Re-evaluate skip threshold on every
  paired-client disconnect; fire skip if count now meets threshold.
- **Multi-tab.** Same `user_id` — votes and skip-votes dedupe.
- **Submitter votes own track.** Allowed.
- **All-negative track.** Still plays (ranks last). Skip vote is the only
  removal mechanism.
- **API key missing.** Booth submit disabled with banner; staff `/audio`
  path intact.

### 14.11 Still out of scope
- Browser-side voting UI.
- Weighted votes by role (admin/mod vote ≠ user vote).
- Vote history / reputation.
- Public `POST /api/queue/submit` HTTP route — booth uses in-process
  service calls.
- `GET /api/queue` HTTP route — see §5.4.

---

## 15. References

- Existing audio infra: root `CONTEXT.md` §2.7.
- Vote domain (closest analogue for services + channels):
  `late-ssh/src/app/vote/`.
- Paired-client WS: root `CONTEXT.md` §4.1, `late-ssh/src/session.rs`,
  `late-ssh/src/api.rs`.
- YouTube IFrame Player API:
  https://developers.google.com/youtube/iframe_api_reference
- YouTube Data API `videos.list`:
  https://developers.google.com/youtube/v3/docs/videos/list
- Browser autoplay behavior:
  https://developer.mozilla.org/en-US/docs/Web/Media/Guides/Autoplay
- mpv JSON IPC reference (for the parked plan):
  https://mpv.io/manual/master/#json-ipc
