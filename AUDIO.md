# AUDIO.md

This document owns the design of the late.sh audio domain: the always-on
Icecast house radio, per-room YouTube playlists with voting, the visualizer
pipeline, and the source-priority logic that arbitrates between CLI and
browser audio outputs.

Active plans live in the top half. Parked ideas live at the bottom. `DJ.md`
captures original brainstorming and the hard ToS rules; this file supersedes
the YouTube-related sections of `DJ.md` once Phase 2 ships.

---

## 1. Goals

### User-facing

- Always-on house radio (Icecast) keeps playing in the background, just like
  today.
- Anyone in a media room can submit a YouTube link or stream URL.
- Submissions enter a per-room queue. Members vote. The room plays the highest
  voted item next.
- 1h cap on streams; the room auto-skips after that.
- When a media room's queue empties, the room falls back to Icecast house
  radio. When someone submits again, it flips back to YouTube.
- The browser tab plays the YouTube audio. The TUI is the remote: volume,
  mute, skip, vote, submit. The user never has to touch the browser tab after
  the initial autoplay-gesture click.
- The visualizer sidebar stays meaningful for Icecast playback; for YouTube it
  becomes a small "playing" indicator (real frequency analysis of a
  cross-origin iframe is blocked by browsers; see §6).

### Architectural

- All audio logic lives in one dedicated domain: `late-ssh/src/app/audio/`.
- `late-core` owns only DB-backed entities and pure types.
- `late-cli` stays free of `late-core` and free of any YouTube extraction
  logic. CLI only plays Icecast.
- Existing scattered audio code (Liquidsoap telnet in `vote/`, now-playing
  fetch in `main.rs`, visualizer types in three places, audio control state in
  `session.rs`) gets consolidated into the new domain.

---

## 2. Domain layout

### 2.1 What lives where

```
late-core/
└── src/
    ├── audio.rs                    # existing: shared VizFrame type (kept)
    ├── audio_config.rs             # existing: shared analyzer config (kept)
    ├── icecast.rs                  # existing: Icecast status-json parser (kept)
    ├── api_types.rs                # existing: NowPlaying, Track wire types (kept)
    └── models/
        ├── media_room.rs           # NEW: MediaRoom entity
        ├── media_room_queue.rs     # NEW: QueueItem entity
        └── media_room_vote.rs      # NEW: QueueVote entity

late-ssh/
└── src/
    ├── main.rs                     # MODIFIED: drop the inlined now-playing loop
    ├── state.rs                    # MODIFIED: add audio_service handles
    ├── session.rs                  # MODIFIED: ClientAudioState moves to audio/
    └── app/
        └── audio/                  # NEW domain folder
            ├── mod.rs              # declarations only
            ├── state.rs            # per-session audio UI state
            ├── input.rs            # m / + / - / skip / submit key routing
            ├── ui.rs               # now-playing block, viz bars, queue panel
            ├── liquidsoap.rs       # moved from app/vote/liquidsoap.rs
            ├── client_state.rs     # ClientAudioState moved from session.rs
            ├── coordinator.rs      # source priority: cli vs browser, mute arbitration
            ├── viz.rs              # Visualizer + BrowserVizFrame unified
            ├── now_playing/
            │   ├── mod.rs
            │   └── svc.rs          # NowPlayingService (Icecast poll, watch)
            └── room/               # Media-room subdomain
                ├── mod.rs
                ├── svc.rs          # MediaRoomService: CRUD + listing
                ├── manager.rs      # per-room runtime (queue, votes, playback)
                ├── state.rs        # per-session UI state for the room view
                ├── input.rs        # room-specific keys (submit, vote, skip)
                └── ui.rs           # queue panel, now-playing-in-room, submitter

late-web/
└── src/
    └── pages/
        ├── stream.rs               # existing: Icecast proxy (kept)
        └── connect/
            ├── mod.rs              # MODIFIED: serve youtube mode when room.kind=media
            ├── page.html           # MODIFIED: add hidden YT-iframe section + Alpine.js controller for IFrame API
            └── status.html         # existing
```

### 2.2 Why this split

Three concerns, three sub-folders inside `audio/`:

- **`now_playing/`** owns the Icecast house radio's metadata and the
  Liquidsoap genre controller. Today this is split between `vote/svc.rs` (the
  telnet call) and `main.rs:341-379` (the polling loop). The audio domain is
  the right owner; `vote/` should ask "audio, please switch to genre X" and
  not know what TCP is.

- **`room/`** owns media-room runtime: registry, queue, votes, per-room
  playback state. Modelled on the existing `rooms/` domain (which has
  `RoomsService` + per-table `BlackjackTableManager`); here it is
  `MediaRoomService` + per-room `MediaRoomManager`.

- **Top-level files** (`state.rs`, `input.rs`, `ui.rs`, `viz.rs`,
  `client_state.rs`, `coordinator.rs`) handle the cross-cutting concerns: the
  sidebar widget, the global mute/volume keys, the visualizer rendering, and
  the source-priority decisions between CLI and browser.

### 2.3 What stays out of `late-core`

`late-core` keeps only what compiles without a tokio runtime: the `VizFrame`
struct, the Icecast status-json parser, the `NowPlaying` wire type, the
analyzer config struct, and the new DB models. Everything stateful (services,
channels, tasks, the Liquidsoap telnet client) is in `late-ssh/src/app/audio/`.

The `late-cli` crate continues to have zero `late-core` dependency. The CLI
talks to the server over the same JSON WS schema it does today, plus the new
`source_changed` event so the CLI knows when to mute itself in favor of a
paired browser playing YouTube.

---

## 3. Data model

### 3.1 New chat kind

`chat_rooms.kind` gains a `'media'` value. Migration adds it to the existing
`chat_rooms_kind_check` constraint. The existing constraint that requires
`game_kind IS NOT NULL` when `kind='game'` does not apply to `'media'`.

### 3.2 New tables

```sql
-- media_rooms: top-level room registry, mirrors game_rooms shape.
CREATE TABLE media_rooms (
    id              UUID PRIMARY KEY,
    chat_room_id    UUID NOT NULL UNIQUE REFERENCES chat_rooms(id) ON DELETE CASCADE,
    media_kind      TEXT NOT NULL CHECK (length(trim(media_kind)) > 0),  -- 'youtube' for now
    slug            TEXT NOT NULL UNIQUE,
    display_name    TEXT NOT NULL CHECK (length(trim(display_name)) > 0),
    status          TEXT NOT NULL CHECK (status IN ('open', 'closed')),
    settings        JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_by      UUID REFERENCES users(id) ON DELETE SET NULL,
    created         TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated         TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- media_room_queue_items: pending and historical submissions.
CREATE TABLE media_room_queue_items (
    id              UUID PRIMARY KEY,
    room_id         UUID NOT NULL REFERENCES media_rooms(id) ON DELETE CASCADE,
    submitter_id    UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    media_kind      TEXT NOT NULL,                  -- 'youtube'
    external_id     TEXT NOT NULL,                  -- youtube video_id
    title           TEXT,
    channel         TEXT,
    duration_ms     INTEGER,                        -- null for live streams
    is_stream       BOOLEAN NOT NULL DEFAULT false,
    status          TEXT NOT NULL CHECK (status IN ('queued','playing','played','skipped','failed')),
    started_at      TIMESTAMPTZ,                    -- set when status flips to playing
    ended_at        TIMESTAMPTZ,                    -- set when status leaves playing
    error           TEXT,                           -- last failure reason
    created         TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated         TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_media_room_queue_room_status
    ON media_room_queue_items(room_id, status, created);

-- media_room_queue_votes: one row per (user, item).
CREATE TABLE media_room_queue_votes (
    item_id     UUID NOT NULL REFERENCES media_room_queue_items(id) ON DELETE CASCADE,
    user_id     UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    value       SMALLINT NOT NULL CHECK (value IN (-1, 1)),
    created     TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (item_id, user_id)
);
```

All PKs use `Uuid::now_v7()` per project convention.

### 3.3 Why not `GameKind::YouTube`

Investigated separately (see investigation notes archived in commit message).
Summary: `GameKind` is a closed enum that infects every dispatch site;
`RoomGameManager` demands seats/pace/stakes/chip-balance plumbing that has no
meaning for a YT room; the lobby UI columns (`Name | Game | Seats | Pace |
Stakes | Status`) become a string of `—` placeholders; and the chat kind
constraint forces every `game` chat to have a `game_kind`. A separate
`media_rooms` table is ~50 lines of duplicate-but-honest CTE rather than a
trait-overloaded lie.

---

## 4. WebSocket protocol additions

### 4.1 Existing channel (kept as-is)

The per-session-token channel in `PairedClientRegistry` keeps owning personal
audio: mute, volume, viz frames, clipboard. No changes to those messages.

### 4.2 New room-scoped channel

A new server-side `RoomAudioRegistry: HashMap<MediaRoomId, broadcast::Sender<RoomAudioEvent>>`
lives in `audio/coordinator.rs`. When a paired client declares it's in a media
room (via a new `join_media_room` message), the WS task subscribes to that
room's broadcast channel and multiplexes the events out on the same socket.

The single WS connection now carries two streams of events: per-token (the
existing personal stuff) and per-room (the new playback stuff). They are
distinguished by the `event` tag in the JSON envelope, not by separate
sockets.

### 4.3 New message types

**Client → server:**

```json
{ "event": "join_media_room", "room_id": "<uuid>" }
{ "event": "leave_media_room", "room_id": "<uuid>" }
{ "event": "submit_link",
  "room_id": "<uuid>", "url": "https://www.youtube.com/watch?v=..." }
{ "event": "vote_queue",
  "room_id": "<uuid>", "item_id": "<uuid>", "value": 1 }
{ "event": "skip_request", "room_id": "<uuid>", "item_id": "<uuid>" }
{ "event": "player_state",
  "room_id": "<uuid>", "item_id": "<uuid>",
  "player_state": "playing" | "paused" | "buffering" | "ended" | "error",
  "offset_ms": 12345, "autoplay_blocked": false }
```

`player_state` is the browser's heartbeat for its iframe player. The server
uses it for drift correction, "this video died" handling, and the
`source_changed` flip to icecast on `ended` when the queue is empty.

**Server → client (room-scoped):**

```json
{ "event": "room_state",
  "room_id": "<uuid>",
  "audio_mode": "icecast" | "youtube",
  "current_item": { "id": "<uuid>", "video_id": "...", "title": "...",
                    "duration_ms": 212000, "started_at_ms": 1770000000000,
                    "is_stream": false },
  "queue": [ { "id": "<uuid>", "video_id": "...", "title": "...",
               "submitter": "<username>", "score": 4, "user_vote": 1 } ],
  "sequence": 42 }
{ "event": "load_video", "room_id": "<uuid>",
  "item_id": "<uuid>", "video_id": "...", "started_at_ms": 1770000000000 }
{ "event": "seek", "room_id": "<uuid>", "offset_ms": 90000 }
{ "event": "source_changed", "room_id": "<uuid>",
  "audio_mode": "icecast" | "youtube" }
{ "event": "queue_update", "room_id": "<uuid>",
  "queue": [...], "sequence": 43 }
```

`room_state` is the full snapshot, sent on join. `queue_update` is the
incremental diff for vote/submit/reorder events. `load_video` is the trigger
for the browser to call `player.loadVideoById`. `source_changed` is what the
CLI listens to so it can mute itself when audio_mode flips to youtube.

### 4.4 Auth model

Each room-scoped message is checked against `chat_room_members` for the
underlying `chat_room_id`. The user must be a member of the room to submit,
vote, or skip. Room presence (subscription to the broadcast) is allowed for
any member; non-members get a 403-equivalent close on `join_media_room`.

---

## 5. Audio source state machine

### 5.1 Per-room state

```
room.audio_mode = "icecast" | "youtube"
  default:        "icecast"
  flips youtube:  on queue becoming non-empty
  flips icecast:  on queue becoming empty, after a debounce
```

### 5.2 Per-session derived state

Tracked in the `SourceCoordinator` (`audio/coordinator.rs`), derived from
existing `PairedClientRegistry` data plus the new room subscriptions:

- `current_room` (which media room the session is in, if any)
- `cli_paired`, `browser_paired` (already tracked today)
- `browser_audio_active` (NEW: browser has done its autoplay gesture and is
  producing sound; reported via `player_state` once it transitions to
  `playing` for the first time after a `load_video`)

### 5.3 Decision table

| room mode | CLI paired | browser audio active | result                                  |
|-----------|-----------|----------------------|-----------------------------------------|
| icecast   | yes       | no                   | CLI plays Icecast (today's flow)        |
| icecast   | yes       | yes                  | CLI mutes, browser plays Icecast        |
| icecast   | no        | yes                  | browser plays Icecast                   |
| icecast   | no        | no                   | silent                                  |
| youtube   | yes       | yes                  | browser plays YouTube, CLI silent       |
| youtube   | yes       | no                   | TUI prompts "open paired browser"       |
| youtube   | no        | yes                  | browser plays YouTube                   |
| youtube   | no        | no                   | TUI shows "no audio client connected"   |

The rule of thumb: **browser wins when present**, because it's the only
client that can play both sources without drift.

### 5.4 Mute coordination

The CLI's existing mute control is preserved. When the coordinator decides
the CLI should be silent (because the browser is the active source), it
sends the existing `toggle_mute` to the CLI iff the CLI is not already muted.
The TUI surfaces *why* it's silent ("muted because browser is active") so it
does not look like a bug.

---

## 6. Visualizer

### 6.1 What stays real

Icecast playback through either the browser or the CLI still flows through a
real `AnalyserNode` / FFT pipeline. The existing visualizer code path is
unchanged for the icecast `audio_mode`.

### 6.2 What is not possible

YouTube audio reaches the user via a cross-origin iframe. The parent page
cannot route iframe audio through `AnalyserNode` (Same Origin Policy applies
to media element CORS too). YouTube's IFrame Player API exposes no audio
analysis hooks. Tab-share via `getDisplayMedia({audio:true})` works but
requires a permission and shows a persistent "tab is being shared" banner,
which is unacceptable UX for a vibe feature.

### 6.3 What ships for YouTube

A "playing indicator" rather than a visualizer. The browser uses the IFrame
API's `getCurrentTime` / `getPlayerState` / `getVolume` to drive a procedural
animation:

- bars rise and fall on a slow time-keyed sine pattern,
- hard-stop when `playerState != playing` or volume is 0,
- amplitude scales with current volume.

The browser still sends the same `viz` payload over the WS so the TUI
rendering code is unchanged. Internally it's procedural noise, not music
analysis. UI copy must NOT call this a visualizer; the sidebar label switches
to "playing" or "now playing" while the room is in youtube mode.

This is documented honestly in the code: the function generating the YT
playing-indicator bands is named `procedural_indicator_bands`, not
`fake_viz_bands` or `viz_bands`.

---

## 7. Edge case ledger

These are decided behaviors. Implement only when the relevant phase ships,
but write the test cases now so we do not relitigate later.

### 7.1 Queue / playback

1. **Flap on near-instant resubmit.** Queue ends, fallback timer starts (10s
   debounce), someone submits again within the window. The pending
   `source_changed` to icecast is cancelled. The next item starts cleanly.
   Implementation: one timer per room, owned by `MediaRoomManager`.

2. **1h stream cap.** On `load_video` for an item with `is_stream=true`, the
   manager schedules a forced-skip task at `started_at + 3600s`. If the item
   is replaced before that fires, the task is cancelled.

3. **Min duration on submission.** Reject items with `duration_ms < 30_000`
   at submission time. Live streams (where duration is null) are exempt but
   subject to the 1h cap.

4. **Per-user submission rate limit.** Max 3 submissions per user per
   rolling 5 minutes per room. Enforced by `MediaRoomService`. Returns a
   user-targeted `MediaRoomEvent::Error` for banner display.

5. **All-unavailable queue.** If every item in the queue reports
   `unavailable`/`error` from every browser, treat queue as empty and
   fallback to icecast. Surface the queue's failures in the UI so users see
   what got dropped and can resubmit.

### 7.2 Source / pairing

6. **Same-Icecast double-play.** Never let CLI and browser both decode
   Icecast simultaneously. CLI auto-mutes when `browser_audio_active`. The
   TUI surfaces "muted because browser is the active audio source" in the
   sidebar.

7. **Browser closes mid-YouTube.** Browser WS drops. Coordinator marks
   `browser_audio_active = false`. Room stays in youtube mode (other members
   are still listening). TUI surfaces "browser disconnected, reopen to
   resume audio" for that user. CLI does not try to fill in because it can't
   decode YouTube.

8. **CLI-only user in YouTube room.** Cannot hear. Two paths considered: (a)
   personal fallback to icecast for them, (b) silence with prompt. Chosen:
   **(b)**, because (a) breaks the "we're listening together" promise. TUI
   prompt links to the paired browser URL.

9. **Audio source flips while user is in the room.** Same as case 8:
   browser-less users get a prompt. Browser users keep playing. No abrupt
   silence without explanation.

10. **Late browser pair.** User joined a YT room without a browser, opens
    the paired URL now. Browser pairs, sends `client_state`, then the
    coordinator immediately tells it to load the current item at the
    correct offset. Sync algorithm catches it up (see §8).

11. **Multi-tab dedupe.** A second browser tab on the same token: reject
    the second WS subscription to room broadcasts with a clear close
    reason. The first tab continues playing. (Personal mute/volume control
    still works on the second tab because it uses the per-token channel,
    but it does not subscribe to room audio events.)

### 7.3 YouTube specifics

12. **Region locks and embedding disabled.** Validate at submission time via
    YouTube Data API `videos.list(part=snippet,contentDetails,status)`:
    reject `status.embeddable == false`, reject if `regionRestriction`
    blocks one of a known set of viewer regions. Partial in-room failures
    (some browsers report unavailable, others not) continue for the
    majority; the failed-for users hear nothing on that item.

13. **YouTube Data API quota exhaustion.** When the validator fails to
    reach the API (or hits quota), submissions are accepted but flagged
    `validation_pending`. The browser's first `player_state` report
    confirms or kills the item.

14. **Ads.** YouTube serves ads through the iframe player. We do not block
    them and we do not strip them. If a user-submitted track plays an ad,
    that's YouTube's player doing its job; this keeps our ToS posture
    clean (the iframe is the legal media surface).

### 7.4 Fallback

15. **Which Icecast genre during fallback?** The global vote winner. Media
    rooms do not own their own genre vote; they borrow the house default.

16. **Empty room.** If a media room has zero subscribers (no one is
    listening), the playback timer still ticks (queue still advances)? No:
    pause the playback advancement when the room broadcast has zero
    subscribers. Resume on the first new subscriber. Saves API quota and
    avoids "track 5 is already 12 minutes in when I join."

---

## 8. Sync algorithm

Server is authoritative on the timeline. Each `load_video` carries
`started_at_ms` (server epoch). Browsers compute:

```
offset_ms = server_now_ms - started_at_server_ms
player.loadVideoById({ videoId, startSeconds: offset_ms / 1000 })
```

Server-now is approximated client-side from a periodic clock-skew probe (the
existing heartbeat carries `position_ms` already; a paired server response
with a server timestamp is added). Drift correction:

- Every 10s, browser compares its `player.getCurrentTime() * 1000` against
  the expected offset.
- If `|drift| < 500ms`, ignore.
- If `|drift| > 2000ms`, hard seek to expected offset.
- If `|drift|` between, soft drift correction by nudging playback rate
  (avoids audible seeks). Use 1.02x or 0.98x for up to 5s.

Live streams skip drift correction entirely; they play "live" and the 1h cap
governs the lifecycle.

---

## 9. Phase plan

### Phase 0: Documentation + decision freeze

- This file.
- Update `DJ.md` to scope its hard rules to **server-side restreaming and
  Icecast input** explicitly. The new rules section reads: "These rules
  apply to audio that late.sh hosts or rebroadcasts. The YouTube media-room
  path described in `AUDIO.md` is out of scope of these rules; in that
  path, late.sh sends only metadata and the official YouTube iframe player
  delivers audio directly to each listener's browser." Do not move the
  line silently.
- Update root `CONTEXT.md` §2.7 ("Audio infrastructure") to add a pointer
  to this file.

### Phase 1: Refactor existing audio code into the new domain (no behavior change)

- Create `late-ssh/src/app/audio/` skeleton.
- Move `vote/liquidsoap.rs` → `audio/liquidsoap.rs`. Inject the controller
  into `VoteService` rather than `VoteService` owning the file.
- Move now-playing poll loop from `main.rs:341-379` into
  `audio/now_playing/svc.rs`. Expose `subscribe_state()` watch.
- Move `ClientAudioState` from `session.rs` to `audio/client_state.rs`.
  Keep `PairedClientRegistry` in `session.rs` but have it import the type
  from the audio domain.
- Consolidate `BrowserVizFrame` and `late_core::audio::VizFrame` into one
  type. Remove the manual conversion in `tick.rs:220-230`.
- Move `app/visualizer.rs` rendering helpers into `audio/viz.rs`.
- Delete the dead `run_analyzer` server-side block in `main.rs:428-440`.
- Tests: existing test suite must pass with no behavior change.

### Phase 2: Media-room data model and service

- Migration: new chat kind `'media'`, new tables (§3.2).
- `MediaRoom`, `QueueItem`, `QueueVote` models in `late-core/src/models/`.
- `MediaRoomService` in `audio/room/svc.rs` (CRUD, listing, snapshot
  channel, event broadcast).
- `MediaRoomManager` registry in `audio/room/manager.rs` (per-room runtime,
  one tokio task per active room).
- YouTube Data API validation at submission time (using existing AI/HTTP
  patterns). API key env var: `LATE_YOUTUBE_API_KEY`.
- Tests: integration tests for queue ordering, vote tally, submission
  rate-limit, min-duration rejection, region/embedding rejection.

### Phase 3: WebSocket protocol additions

- New `RoomAudioRegistry` in `audio/coordinator.rs`.
- `handle_socket` in `api.rs` parses `join_media_room` / `leave_media_room`
  and subscribes/unsubscribes the WS task to the room broadcast.
- New `WsPayload` variants for §4.3 messages.
- Auth check: `chat_room_members` membership required.
- Tests: smoke tests for the new WS messages, room-scoped fan-out.

### Phase 4: Browser YouTube player

- Update `/connect/{token}` page to detect when the user's current room is
  a media room (via `client_state` reporting `current_room`).
- Add a YouTube iframe element to the page (hidden when audio_mode is
  icecast, shown when youtube).
- IFrame Player API integration (`onYouTubeIframeAPIReady`, `Player`,
  `loadVideoById`, `seekTo`, `setVolume`, `mute`, `unMute`).
- One-time autoplay-gesture UI: a "Click to enable audio" overlay shown the
  first time the page connects.
- Procedural playing-indicator (§6.3).
- Sync algorithm (§8).

### Phase 5: TUI media-room screen

- New entry point: a media room shows up as a normal room in the existing
  Rooms screen, alongside game rooms but visually distinguished.
- Per-room TUI view: queue, now-playing, submitter, vote counts, submit
  input field, skip key.
- Source priority surfacing: when the user is in a YT room without a
  browser, the sidebar/banner shows "open paired browser to hear audio"
  with the paired URL prefilled in clipboard.

### Phase 6: Polish

- Min duration + rate limit UX polish (clear error banners with reasoning).
- Drift correction tuning (visible vs invisible threshold).
- Multi-tab dedupe surfacing.
- Region-lock partial failure UX.

---

## 10. Cross-domain interactions

- **`VoteService` after Phase 1**: still owns vote tally and round
  switching, but delegates "tell Liquidsoap to switch genre" to the audio
  domain via an injected `LiquidsoapController` handle.
- **`RoomsService` and `MediaRoomService`**: independent services. They both
  use the chat-room CTE pattern to create a linked `chat_rooms` row, but
  they don't share code beyond that. If we ever add a third room family
  (Spotify? screenshare?), the shared CTE can be lifted into a small helper
  in `late-core::models::chat_room`.
- **`ChatService`**: extended with one new `is_chat_list_room` exclusion
  for `kind='media'` (same pattern as the existing exclusion for
  `kind='game'`). Media-room chat is shown embedded under the room view,
  not in the global chat list.
- **`ActivityService`**: new `ActivityKind` variants: `MediaRoomCreated`,
  `MediaTrackSubmitted`, `MediaTrackPlayed`. Same pattern as existing
  activity events.
- **`HubService`**: future Hub leaderboard surface for "top submitters" or
  "tracks played this week" can read from `MediaRoomService`'s snapshot
  without owning state.

---

## 11. What we deliberately do not build

- **CLI YouTube decoding.** See §12 below. The CLI plays Icecast only.
- **Server-side YouTube fetching.** Server only routes `video_id`. The
  iframe is the only thing that ever speaks to googlevideo.com.
- **Recording / persistent archive of YT tracks.** Not allowed by ToS, not
  useful.
- **YouTube ads stripping.** The iframe plays whatever YouTube serves.
- **Cross-room sync.** Each media room is its own island.
- **Lyrics / album art / fancy metadata.** Title + channel + thumbnail is
  enough for v1.
- **Custom genre vote per media room.** Fallback uses the house Icecast
  genre.

---

## 12. Parked: CLI external-player handoff for YouTube

### Status

Parked. Not on the active build path. Reason: the user-facing configuration
burden is too high for current scale. Most users will not have a suitable
player installed and will not want to edit a TOML config to make a clubhouse
feature work. Revisit when the audience is technical enough or large enough
to justify a setup guide.

### Summary

Instead of opening a browser for YouTube rooms, `late` shells out to a local
media player (configured by the user) that already knows how to play
YouTube. late.sh never touches YouTube audio; it only sends `video_id` over
WebSocket. The CLI is a general external-player runner with no YouTube
extraction code of its own.

### The core idea

late.sh stays out of the audio path entirely. The server is a metadata
coordinator. The CLI is a thin remote control for a media player that
already runs on the user's machine.

```text
late.sh server
  -> room state, queue, votes, "play video_id at offset N"
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

### Conflict with `DJ.md` hard rules

`DJ.md` lines 25 to 32 forbid `yt-dlp`, restreaming, ad-stripping, and
extracting platform audio. A user-side mpv-with-yt-dlp setup violates both
clauses on the user's machine. The Phase 0 `DJ.md` update for the active
browser plan does not solve this; if the parked plan is ever activated,
`DJ.md` needs a second, separate update to allow CLI handoffs explicitly.

### Reactivation criteria

- The user base is large enough and technical enough that a setup guide is
  worth maintaining.
- A stable, official YouTube-API-compliant CLI player emerges (currently
  does not exist; the closest options all use yt-dlp under the hood).
- We decide to make late.sh deliberately CLI-power-user-shaped, and a
  config file with a player slot fits the product identity.

Until then, YouTube rooms go through the browser path described in §2-§9.

---

## 13. References

- Existing audio infra: root `CONTEXT.md` §2.7.
- Rooms domain (model to follow): `late-ssh/src/app/rooms/CONTEXT.md`.
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

---

## 14. Open questions (decide before Phase 2)

These do not block Phase 0/1 but must be resolved before the schema migration
in Phase 2:

1. **Voting model.** Up-only (+1) like Reddit, or up/down (-1/+1)? The
   `media_room_queue_votes.value` column supports both; the manager logic
   chooses. Recommend +1 only for v1 (less drama, simpler UX).

2. **Queue order tiebreak.** When two items have the same vote score, which
   plays first? Earliest submission, or random? Recommend earliest
   submission (deterministic, encourages early submitters).

3. **Currently-playing visible in queue?** Probably yes, marked as "now
   playing," not in the votable list.

4. **Skip threshold.** Single-user skip allowed? Vote-skip threshold (e.g.
   30% of subscribed listeners)? Admin override? Recommend: submitter +
   admin always-allowed, others need a vote-skip with threshold 50% of
   listeners present in the last 30s.

5. **Room creation policy.** Anyone can create a media room (with the
   3-per-user-per-kind cap from `RoomsService`)? Or admin-only at first?
   Recommend: anyone, with the cap, matching game rooms.

6. **Default room.** Should there be one always-on `#youtube-main` media
   room similar to `#general` chat? Recommend: yes, seeded by migration,
   `auto_join=false` so users opt in.
