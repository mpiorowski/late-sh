# late.sh Stream Context

## Metadata
- Domain: "watch me" streaming rooms — the `/golive` screen-share broadcast, the in-process stream registry, stream rooms, publisher/watch capability URLs, and the rail's `stream` section
- Primary audience: LLM agents working in `late-ssh/src/app/stream`, the `/golive`/`/watch` commands, the `/api/stream/*` routes, or `late-web/src/pages/live`
- Last updated: 2026-08-11 (initial v1 build, from the STREAM.md seed doc)
- Status: Active (v1)
- Parent context: `../../../../CONTEXT.md`
- Related context: `../voice/CONTEXT.md` (LiveKit grants, the ONE-room audio model), `../../../../late-web/CONTEXT.md` (watch + go-live pages), `STREAM.md` at the repo root (the design seed)

---

## 1. Scope and the core decision

A stream is **a video track published into a standard room's LiveKit voice
channel**. No second media room, no bridging: CLI voice participants talk
with the streamer through the normal voice path, the go-live page publishes
the screen share (and optionally a browser mic) into the same LiveKit room,
and watch pages subscribe to all of it. late-ssh never touches a media byte;
it moves capability ids, registry state, and one activity line.

Owned by this domain:
- `registry.rs` — the process-global `StreamRegistry`: one stream per user,
  phase machine (`Pending -> Live -> Grace`), watcher heartbeats, publisher
  heartbeats/grace, capability ids. In-memory only, single replica, dies
  with the process (scratchpad-registry tier).
- `svc.rs` — `StreamService` orchestration: lazy stream-room creation,
  ticket minting via `VoiceService`, the `WentLive` announcement, the event
  channel back to sessions, the sweeper.
- The `/golive [title|stop]` and `/watch @user` composer commands (parsed in
  `chat/state.rs`, drained by `App::tick_stream` in `app/state.rs`).
- The `/api/stream/*` routes in `api.rs` and their late-web proxies/pages.

Out of scope (deliberate v1 boundaries, from STREAM.md):
- Viewer talk-back / viewer mics, general browser voice rooms, recording,
  past-streams pages, quality settings, multi-presenter, web-side chat.
- Routing stream media into the CLI audio path (the stream lives in a page).
- A public `late.sh/live` index. Access is **unlisted, not public, not
  authed**: watch URLs carry a random per-stream id, shown only in-app,
  dead when the stream ends.

## 2. File map and touchpoints

```text
late-ssh/src/app/stream/
├── mod.rs        # declarations only
├── registry.rs   # StreamRegistry state machine + snapshot watch
└── svc.rs        # StreamService: DB, VoiceService tickets, activity, events
```

Cross-domain touchpoints:
- `late-core/src/models/chat_room.rs::get_or_create_stream_room` — the
  permanent per-streamer room: `kind='game'`, `game_kind='stream'`, slug
  `{username}-live`, public. Chat history persists between streams; `kind='game'`
  keeps it out of the normal rail/IRC surfaces, and the public game-room
  join path (`ChatService::join_game_room_task`) lets anyone enter from the
  rail. No migration was needed (`game_kind` is free-form TEXT).
- `app/voice/svc.rs` — `stream_publish_ticket` (publish restricted at the
  SFU grant level to `screen_share`/`screen_share_audio`/`microphone`,
  identity `stream-{user_id}` so it never collides with the CLI voice
  identity) and `stream_watch_ticket` (`canPublish=false`, `hidden=true`).
- `api.rs` — `/api/stream/publish/{token}`, `/api/stream/publish/{token}/state`,
  `/api/stream/watch/{id}`, `/api/stream/watch/{id}/grant`,
  `/api/stream/watch/{id}/heartbeat`. Capability id in the URL is the whole
  auth; everything is served from registry memory.
- `late-web/src/pages/live/` — `/live/{id}` (watch page) and
  `/golive/{token}` (broadcast console), plus same-origin proxies of the
  API routes above. Pages are thin LiveKit browser clients.
- `app/activity/` — `ActivityKind::WentLive`; the lounge line fires only on
  the `Pending -> Live` transition (first media report), repeat-throttled by
  the standard 30-minute window (`went-live` shape key). No "ended" line.
- `app/chat/state.rs` / `app/chat/ui.rs` — `ChatState::live_streams` (copied
  from the registry watch ~1/s in `App::tick_stream`, epoch-bumped on
  change), the rail's `RoomSection::Stream` (under Core, above
  Cyberspace/Channels, visible from `/golive` on), the `▶LIVE` author
  presence badge (live streams only), the stream header block above the
  room's chat (title, watcher count, watch-URL nudge), and the stream-room
  arm in `select_room_slot` (lazy join on first open).
- `app/voice/ui.rs::OnAirView` — the ⦿ ON AIR strip marker plus the
  `{streamer} · on air` roster line while the go-live page reports its
  browser mic open.
- `app/state.rs` — `App::tick_stream` (commands, events, snapshot),
  `open_stream_url` (paired-CLI `OpenUrl` control or the QR modal),
  `voice_toggle_join`'s one-time ON AIR confirm, `StreamQrModal`.
- `paired_clients.rs` / `late-cli/src/ws.rs` — `PairControlMessage::OpenUrl`
  + the `open_url` capability (xdg-open/open/cmd start).
- `late-cli/src/voice.rs` — the audio-only voice runtime unsubscribes from
  any remote video track (`publication.set_subscribed(false)`), so a CLI
  voice participant in a stream room never downloads the screen share.
- `main.rs` — service construction and the 5s sweeper task.

## 3. Lifecycle

1. `/golive <title>` → `StreamService::go_live_task`: ensure room + enabled
   voice channel + streamer membership, `registry.begin` (idempotent per
   user: re-running updates the title and re-shows the modal), event back to
   the session → streamer lands in their stream room, publisher URL opens in
   the paired CLI's browser and/or shows as a QR modal.
2. The go-live page fetches its grant, the human picks a window
   (`getDisplayMedia` runs in the real browser, never in wry), and the page
   reports `publishing=true` → `Pending -> Live`, the one #lounge line
   fires. **The announcement never fires at command time**: no line ever
   points at a black screen.
3. Watch pages resolve `/live/{id}`, poll state (10s), heartbeat (15s;
   45s TTL drives the "N watching" count), and subscribe with an anonymous
   hidden grant. Pages are born silent; a human click opens each direction.
4. Ending: `/golive stop`, or close the tab / stop sharing → grace
   (~30s, survives a refresh) → the registry sweeps the stream, watch and
   publisher URLs die, the rail row disappears. A registered stream whose
   page never reports media is swept after 15 minutes.

## 4. Consent invariants (non-negotiable, from STREAM.md)

1. **No server-side client detection.** Pages are born silent in both
   directions (watch page muted, go-live mic off + room audio muted); the
   human clicks to open each direction. The page reports its own state.
2. **ON AIR is loud.** The voice strip in a live room leads with ⦿ ON AIR,
   and the first Ctrl+V there demands a second Ctrl+V to confirm you are
   audible to anonymous link-holders.
3. **No invisible speaker.** A browser-mic streamer appears in the voice
   strip as `{name} · on air`, fed by the page's own mic report. Anonymous
   *ears* are expected (the count is shown); anonymous *mouths* are
   forbidden — watch grants are `canPublish=false` at the SFU level, so a
   tampered page still cannot open a mic.
4. **Voice stays CLI-only** as a system. The streamer's own broadcast
   console is the one scoped exception (room owner, own stream room,
   per-stream token); see `../voice/CONTEXT.md` §7/§10.

## 5. Testing

- `registry_test.rs` — the phase machine: one-stream-per-user, pending
  visibility, the exactly-once `went_live` transition, grace on stop,
  heartbeat counting, mic state, teardown, username lookup, sweep.
- `api_test.rs::stream_endpoints_serve_the_watch_and_publish_flow` — the
  whole HTTP flow end to end against a real registry + DB, including the
  404s for dead capability ids.
- `late-web/src/pages/live/live_test.rs` — capability-id validation (the
  proxy-path injection gate) and page rendering (born-silent copy pinned).
- LLM agents run targeted tests via `make test-llm ARGS="-p late-ssh -E
  'test(stream)'"`; never raw cargo test.

## 6. Known gaps / follow-ups

- Metrics: no `record_stream_*` telemetry yet (streams started, watcher
  peaks — the experiment metrics in STREAM.md are currently only readable
  from logs/registry).
- The stream room keeps its slug from the username at first `/golive`; a
  renamed user gets a fresh room (old history orphaned but harmless).
- Splash tips carry no `/golive` line yet.
- Moderation: `/mod` room tools work on the chat room, but there is no
  stream-specific kill switch beyond `/golive stop` + LiveKit voice mod
  tools. Fine at ~40 trusted users; revisit before any public watch page.
- The tavern TV prop, arena spectator reuse, and a public `late.sh/live`
  stay future work (see STREAM.md).
