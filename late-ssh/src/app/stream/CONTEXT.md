# late.sh Stream Context

## Metadata
- Domain: "watch me" streaming rooms — the `/golive` screen-share broadcast, the in-process stream registry, stream rooms, publisher/watch capability URLs, and the rail's `stream` section
- Primary audience: LLM agents working in `late-ssh/src/app/stream`, the `/golive`/`/watch` commands, the `/api/stream/*` routes, or `late-web/src/pages/live`
- Last updated: 2026-08-12 (review hardening: publisher kill switch, pending
  consent gates, heartbeat caps, owner-keyed rooms)
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
  rail. No migration was needed (`game_kind` is free-form TEXT). The room
  follows the **account**, not the name: lookup is by `created_by` first, so
  a renamed streamer keeps their room (old slug) and a freed-and-reclaimed
  username never inherits another account's room; a squatted slug falls back
  to `{username}-live-{id-suffix}`.
- `app/voice/svc.rs` — `stream_publish_ticket` (publish restricted at the
  SFU grant level to `screen_share`/`screen_share_audio`/`microphone`,
  identity `stream-{user_id}` so it never collides with the CLI voice
  identity) and `stream_watch_ticket` (`canPublish=false`, `hidden=true`).
- `api.rs` — `/api/stream/publish/{token}`, `/api/stream/publish/{token}/state`,
  `/api/stream/watch/{id}`, `/api/stream/watch/{id}/grant`,
  `/api/stream/watch/{id}/heartbeat`. Capability id in the URL is the whole
  auth; everything is served from registry memory. The publish token is
  additionally **claim-once**: the first grant fetch locks it to that
  console (secret minted registry-side, carried as the
  `x-late-publish-claim` header between API and proxy, stored as an
  HttpOnly path-scoped cookie in the console's browser). A leaked publish
  URL cannot fetch a grant or forge state reports from any other browser
  (403); leaked *before* the console opens, the intruder claims first and
  the real console fails loudly instead of silently losing its stream.
  Claims die with the stream; fresh `/golive` = fresh unclaimed token.
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
   page never reports media is swept after 5 minutes. **Teardown kills the
   media, not just the URLs**: every path out of the registry (`stop`, a
   moderation voice kick, grace/pending sweep) also force-disconnects the
   go-live console from LiveKit (`VoiceService::remove_stream_publisher`,
   identity `stream-{user_id}`), and the console itself treats a 404 on its
   state report as stream-over (unpublish + disconnect), so neither side
   can keep broadcasting into the voice channel after the stream is gone.

## 4. Consent invariants (non-negotiable, from STREAM.md)

1. **No server-side client detection.** Pages are born silent in both
   directions (watch page muted, go-live mic off + room audio muted); the
   human clicks to open each direction. The page reports its own state.
2. **ON AIR is loud.** The voice strip in a live room leads with ⦿ ON AIR,
   and the first Ctrl+V there demands a second Ctrl+V to confirm you are
   audible to anonymous link-holders. The confirm arms at **registration**
   (a pending stream warns too, with softer wording), and a session already
   sitting in the voice channel gets a banner on the Pending → Live edge:
   the channel persists between streams, so a co-host can predate `/golive`.
3. **Nobody subscribes before media flows.** The watch grant is withheld
   while the stream is pending (grace still counts as live so refreshes
   reconnect); the watch page connects off the state poll's `live` flag,
   never on load. A pending stream's voice channel is not listenable.
4. **No invisible speaker.** A browser-mic streamer appears in the voice
   strip as `{name} · on air`, fed by the page's own mic report. Anonymous
   *ears* are expected (the count is shown); anonymous *mouths* are
   forbidden — watch grants are `canPublish=false` at the SFU level, so a
   tampered page still cannot open a mic.
5. **Voice stays CLI-only** as a system. The streamer's own broadcast
   console is the one scoped exception (room owner, own stream room,
   per-stream token); see `../voice/CONTEXT.md` §7/§10.

## 5. Testing

- `registry_test.rs` — the phase machine: one-stream-per-user, pending
  visibility, the exactly-once `went_live` transition, grace on stop,
  heartbeat counting (including the `WATCHERS_MAX` cap), mic state,
  teardown, username lookup, the claim-once publisher lock, and all four
  TTL transitions via the clock-injected `sweep_at` (pending expiry,
  live → grace, grace teardown, watcher pruning).
- `activity/event_test.rs` — feed titles are mention-safe: `@` is stripped
  before a `/golive` or cyberspace title lands in a #lounge body (the
  lounge feed's "bodies never contain `@`" contract).
- `chat_room_test.rs` (late-core) — the stream room follows the account
  through a rename; a reclaimed username does not inherit the old room.
- `api_test.rs::stream_endpoints_serve_the_watch_and_publish_flow` — the
  whole HTTP flow end to end against a real registry + DB, including the
  404s for dead capability ids.
- `late-web/src/pages/live/live_test.rs` — capability-id validation (the
  proxy-path injection gate), page rendering (born-silent copy pinned),
  upstream-status forwarding, and the claim cookie exchange.
- LLM agents run targeted tests via `make test-llm ARGS="-p late-ssh -E
  'test(stream)'"`; never raw cargo test.

## 6. Moderation

- `/mod voice kick @user` is the stream kill switch: it blocks future voice
  tickets (which `go_live` and `stream_publish_ticket` both check, so the
  target cannot restart), ends the target's registered stream, and
  force-disconnects the go-live console from LiveKit (`ModerationInfra`
  carries the `StreamService`; see `moderation/service.rs::voice_action`).
  `/mod voice allow @user` lifts the block.
- `/mod` room tools (ban, kick-from-room, slow mode) work on the stream
  chat room like any other room.
- A minted LiveKit token stays valid until it expires; the force-disconnect
  plus the no-new-tickets block is what makes a kick bite immediately.

## 7. Known gaps / follow-ups

- Metrics: no `record_stream_*` telemetry yet (streams started, watcher
  peaks — the experiment metrics in STREAM.md are currently only readable
  from logs/registry).
- A renamed streamer keeps their room under the old `{username}-live` slug
  (cosmetic only: the slug is not shown anywhere user-facing).
- Splash tips carry no `/golive` line yet.
- The tavern TV prop, arena spectator reuse, and a public `late.sh/live`
  stay future work (see STREAM.md).
