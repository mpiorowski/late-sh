# late.sh Stream Context

## Metadata
- Domain: "watch me" streaming rooms — the `/golive` screen-share broadcast, the in-process stream registry, stream rooms, publisher/watch capability URLs, and the rail's `stream` section
- Primary audience: LLM agents working in `late-ssh/src/app/stream`, the `/golive`/`/watch` commands, the `/api/stream/*` routes, or `late-web/src/pages/live`
- Last updated: 2026-08-14 (One audio path per sound: the CLI voice runtime
  now plays human microphones only — program audio (the OBS ingress mix,
  the console's screen-share audio) and every `stream-*` publisher are
  unsubscribed, killing the streamer-hears-their-own-OBS echo and CLI
  users eating the game mix. Both consumers classify program audio by the
  `stream-*` identity; the `SCREEN_SHARE_AUDIO` label set at CreateIngress
  is advisory only, since it may not survive transcoding-off passthrough.
  The watch page defaults audio ON (autoplay permitting), grows a separate
  voices on/off toggle for CLI viewers, a volume slider, and fullscreen.
  The go-live console's browser mic and its whole `mic_live`/on-air
  pipeline were removed — macOS CLI voice landed, so voice is CLI-only
  with zero exceptions. See §4)
- Status: Active (v1)
- Parent context: `../../../../CONTEXT.md`
- Related context: `../voice/CONTEXT.md` (LiveKit grants, the ONE-room audio model), `../../../../late-web/CONTEXT.md` (watch + go-live pages), `STREAM.md` at the repo root (the design seed)

---

## 1. Scope and the core decision

A stream is **a video track published into a standard room's LiveKit voice
channel**. No second media room, no bridging: CLI voice participants talk
with the streamer through the normal voice path, the go-live page publishes
the screen share into the same LiveKit room, and watch pages subscribe to
all of it. All talking is CLI voice; the pages publish no mic. late-ssh
never touches a media byte; it moves capability ids, registry state, and
one activity line.

Owned by this domain:
- `registry.rs` — the process-global `StreamRegistry`: one stream per user,
  phase machine (`Pending -> Live -> Grace`), watcher heartbeats, publisher
  heartbeats/grace, capability ids, and the `StreamPublisher` kind
  (`Console` vs `Obs(ObsIngress)` — the publisher kinds conflict instead of
  silently rewiring; `/golive stop` switches). In-memory only, single
  replica, dies with the process (scratchpad-registry tier).
- `svc.rs` — `StreamService` orchestration: lazy stream-room creation,
  ticket minting via `VoiceService`, WHIP ingress create/reuse/delete, the
  `WentLive` announcement, the event channel back to sessions, the ingress
  status poll, the sweeper.
- `ui.rs` — the OBS handoff overlay (`/golive obs`: WHIP server URL +
  bearer token + watch link, hand-copied into OBS, dismissed by any key).
- The `/golive [title|stop]`, `/golive obs [title]`, and `/watch @user`
  composer commands (parsed in `chat/state.rs`, drained by
  `App::tick_stream` in `app/state.rs`).
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
├── svc.rs        # StreamService: DB, VoiceService tickets, ingress, events
└── ui.rs         # OBS handoff overlay (WHIP URL + bearer token modal)
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
  SFU grant level to `screen_share`/`screen_share_audio` only — no browser
  mic exists, voice is CLI-only with zero exceptions; identity
  `stream-{user_id}` so it never collides with the CLI voice identity) and
  `stream_watch_ticket` (`canPublish=false`, `hidden=true`).
  For OBS: the LiveKit Ingress API client (`create_whip_ingress`,
  `delete_ingress`, `ingress_publishing`; `ingressAdmin` Twirp calls). The
  ingress participant identity is also `stream-{user_id}`, so teardown and
  moderation reach an OBS publisher through the exact same paths; see
  `../voice/CONTEXT.md` §7.
- Infra: `infra/redis.tf` (LiveKit<->ingress bus; the server refuses
  Ingress API calls without redis), `infra/livekit-ingress.tf`
  (`whip.<domain>`, WHIP only — no RTMP ingest is ever minted), the
  `redis`/`livekit-ingress` docker-compose services with
  `infra/livekit/dev-config.yaml` + `infra/livekit-ingress/dev-config.yaml`
  (dev OBS pushes to `http://localhost:7888/w`).
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
  `ActivityKind::WatchingStream { streamer }` is the audience half: "bob is
  watching mat's stream", attributed to the viewer, `watching:{streamer}`
  shape key. See §3b.
- `app/notify/` — `Notification::friend_live` and
  `Notification::stream_viewer`, both on `Kind::Streams` behind one
  "Streams (friends live, your viewers)" settings row. `friend_live` is the
  one friend-shaped notification NOT on the always-on `Friends` kind: a
  nightly streamer would otherwise cost you their login pings. Opt-in
  therefore, and off for existing accounts until they toggle it; the in-app
  banners fire either way.
- `app/chat/state.rs` / `app/chat/ui.rs` — `ChatState::live_streams` (copied
  from the registry watch ~1/s in `App::tick_stream`, epoch-bumped on
  change), the rail's `RoomSection::Stream` (under Core, above
  Cyberspace/Channels, visible from `/golive` on), the `▶LIVE` author
  presence badge (live streams only), the stream header block above the
  room's chat (title, watcher count, watch-URL nudge), and the stream-room
  arm in `select_room_slot` (lazy join on first open).
- `app/voice/ui.rs::OnAirView` — the ⦿ ON AIR strip marker while the
  room's stream is live. The CLI voice roster is the complete speaker
  list: no browser mic exists, so there is no separate on-air roster line.
- `app/state.rs` — `App::tick_stream` (commands, events, snapshot),
  `open_stream_url` (paired-CLI `OpenUrl` control or the QR modal),
  `voice_toggle_join`'s one-time ON AIR confirm, `StreamQrModal`.
- `paired_clients.rs` / `late-cli/src/ws.rs` — `PairControlMessage::OpenUrl`
  + the `open_url` capability (xdg-open/open/cmd start).
- `late-cli/src/voice.rs` — the audio-only voice runtime unsubscribes from
  any remote video track (`publication.set_subscribed(false)`), so a CLI
  voice participant in a stream room never downloads the screen share. Same
  rule for audio (`keep_remote_audio`): only microphone-source tracks from
  non-`stream-*` identities play. Any `stream-*` publisher (OBS ingress,
  go-live console) is program audio whatever source label it carries, since
  the ingress label may not survive transcoding-off passthrough. One audio
  path per sound: program audio lives on the watch page; CLI voice carries
  human voices; a streamer talks through CLI voice like everyone else.
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
   hidden grant. Playback defaults on (autoplay permitting, see §4.1);
   publishing from a page is impossible by grant.
3a. OBS variant: `/golive obs [title]` runs the same registration but mints
   a WHIP ingress (reused on re-runs; a same-user race deletes the loser)
   and shows a modal with the WHIP server URL + bearer token to paste into
   OBS (Settings → Stream → Service: WHIP). There is no console page, so
   liveness comes from `StreamService::poll_obs_publishers` (the 5s sweeper
   task): `ENDPOINT_PUBLISHING` synthesizes the publisher reports the phase
   machine already understands and fires the same one #lounge line on the
   first hit. A failed poll call skips the report (never forges a stop);
   a truly dead ingress falls to grace via the publisher TTL.
3b. Audience signals, both alerting a person, both once-per-edge:
   - **A friend went live.** `App::tick` already subscribes to the global
     activity broadcast to edge-detect friend logins; the `WentLive` arm
     rides the same drain into `ChatState::note_friend_went_live` (banner +
     `Streams` notification, skipped for non-friends and for yourself).
     Nothing stream-side is involved: the feed event *is* the edge, so this
     inherits the "never before media flows" guarantee for free.
   - **A named viewer arrived.** Watch pages are anonymous by design (§4),
     so the "N watching" count can never be named. The two identified ways
     in are `/watch @user` and opening the streamer's stream room from the
     rail; both funnel into `StreamService::note_viewer`, which asks
     `StreamRegistry::note_viewer` (a per-stream `viewers: HashSet<Uuid>`,
     separate from the anonymous `watchers` heartbeat map) whether this is a
     first arrival. On a first arrival: the `WatchingStream` #lounge line,
     plus a `StreamEvent::ViewerJoined` broadcast that only the streamer's
     own session acts on (banner + `Streams` notification). Quiet while
     the stream is `Pending` (same no-black-screen rule), on a re-open, and
     for the streamer's own room; the set dies with the stream, so a regular
     is announced again at the next broadcast. `ChatState` records the room
     open as `opened_stream_room` and `App::tick_stream` consumes it,
     matching the `/golive` and `/watch` command plumbing: the composer and
     the rail record intent, `App` owns the service.
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
   Every one of those paths funnels through `StreamService::end_stream_task`,
   which logs one `stream ended` line before the disconnect: `reason` (the
   `EndReason` enum: `command`, `moderation`, `pending_expired`,
   `grace_expired`), `phase`, `went_live`, `watching`, and
   `since_publisher_report_ms`. That last field is the diagnostic one: a
   console that reported a stop shows a fresh report age, a console that went
   silent shows a stale one. Without it a streamer's "it just ended" is
   unanswerable after the fact. An OBS stream's `EndedStream` additionally
   carries the ingress id and the same funnel deletes the ingress:
   participant removal alone leaves the stream key valid and OBS
   auto-reconnects through it. The delete retries with backoff (an ingress
   is a LiveKit-side resource; a failed delete is a still-valid stream key
   with no page to stop itself), and `reconcile_ingresses` runs at boot to
   delete every ingress the registry does not know, since a restart wipes
   the registry while LiveKit keeps the keys.

## 4. Consent invariants (non-negotiable, from STREAM.md)

1. **No server-side client detection.** The *publishing* direction is born
   silent (the human clicks share; the go-live page's room audio starts
   muted), and the page reports its own state. The watch page — ears only,
   `canPublish=false` — defaults audio ON, falling back to the unmute click
   when browser autoplay blocks it; it splits audio by track source
   (mic = voices, everything else = stream) behind a voices on/off toggle,
   so a CLI viewer keeps the game audio without hearing the room's voices
   twice, plus a volume slider and fullscreen.
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
4. **No invisible speaker.** Every mouth in the room is a CLI voice
   participant on the strip's roster: no browser mic exists, so the roster
   is complete by construction. Anonymous *ears* are expected (the count is
   shown); anonymous *mouths* are forbidden — watch grants are
   `canPublish=false` at the SFU level, so a tampered page still cannot
   open a mic.
5. **Voice is CLI-only, zero exceptions.** The publish grant carries
   `screen_share`/`screen_share_audio` only; a streamer talks through CLI
   voice like everyone else (macOS included, now that mac CLI voice
   exists). One audio path per sound: program audio lives on the watch
   page, voices live in CLI voice; see `../voice/CONTEXT.md` §7/§10.

## 5. Testing

- `registry_test.rs` — the phase machine: one-stream-per-user, pending
  visibility, the exactly-once `went_live` transition, grace on stop,
  heartbeat counting (including the `WATCHERS_MAX` cap),
  teardown, username lookup, the claim-once publisher lock, and all four
  TTL transitions via the clock-injected `sweep_at` (pending expiry,
  live → grace, grace teardown, watcher pruning). The teardown tests pin
  the `EndReason` and the report age each path hands the log line. OBS
  side: ingress stored/reused on re-runs, publisher-kind conflicts both
  ways, `report_obs` phase transitions, and
  `EndedStream.ingress_id` on stop and sweep. Audience side: `note_viewer`
  announces each named viewer once per stream (repeat visits, the streamer's
  own room, and an unknown streamer stay quiet; a fresh stream re-announces
  the same regular) and stays quiet while pending without burning the
  announcement.
- `ui_test.rs` — the OBS overlay renders every hand-copied value unclipped
  and survives a tiny terminal.
- `chat/state_internal_test.rs` — `/golive` parse routing (console vs `obs`
  vs `stop`) and the title clamp.
- `activity/filter_test.rs` — the `is watching` line ships to #lounge and
  reads `bob is watching mat's stream`.
- `activity/event_test.rs` — feed titles are mention-safe: `@` is stripped
  before a `/golive` or cyberspace title lands in a #lounge body (the
  lounge feed's "bodies never contain `@`" contract).
- `chat_room_test.rs` (late-core) — the stream room follows the account
  through a rename; a reclaimed username does not inherit the old room.
- `svc_test.rs` — the stream-ban gate on `go_live`: a banned user is refused
  with no half-registered stream left behind, an expired row does not block
  (expiry is read-time only), and lifting the ban restores `/golive`.
- `chat/svc_test.rs::mod_stream_ban_ends_the_live_stream_and_persists_the_block`
  — `/mod ban stream` tears a live stream out of the registry and writes the
  row; `/mod unban stream` clears it.
- `api_test.rs::stream_endpoints_serve_the_watch_and_publish_flow` — the
  whole HTTP flow end to end against a real registry + DB, including the
  404s for dead capability ids.
- `late-web/src/pages/live/live_test.rs` — capability-id validation (the
  proxy-path injection gate), page rendering (audio-on defaults, voices
  toggle, volume, fullscreen pinned; the go-live page has no browser mic),
  upstream-status forwarding, and the claim cookie exchange.
- `late-cli/src/voice_test.rs` — the `keep_remote_audio` policy: other
  users' mics play; `stream-*` publishers (any source label) and all
  program audio never do.
- LLM agents run targeted tests via `make test-llm ARGS="-p late-ssh -E
  'test(stream)'"`; never raw cargo test.

## 6. Moderation

Every path below runs through `StreamService::stop`, which is what actually
kills a broadcast: registry teardown drops the watch and publisher URLs, and
`VoiceService::remove_stream_publisher` force-disconnects the console
(identity `stream-{user_id}`, invisible to a plain participant removal by
user id). `ModerationInfra` carries the `StreamService` for all of it.

- `/mod kick stream @user` ends the current broadcast and stops there.
  Nothing persists, so `/golive` works again immediately: this is the tool
  for the wrong window shared by accident, not for a repeat offender. CLI
  voice is untouched, so the streamer can keep talking in the room.
- `/mod ban stream @user [duration] [reason]` does the same and writes a
  `stream_bans` row (`late-core/src/models/stream_ban.rs`, one active row per
  user, expiry checked at read time, no sweeper). `go_live` refuses on it, so
  unlike the runtime-only voice block the ban survives a restart.
  `/mod unban stream @user` lifts it; `/mod view bans stream` lists them, and
  `/mod view @user` reports `stream_banned`.
- `/mod kick voice @user` remains the wider hammer: it blocks future voice
  tickets (`go_live` and `stream_publish_ticket` both check it), ends the
  stream, and cuts CLI voice too. Runtime-only, lifted by
  `/mod unban voice @user`. Reach for it when the problem is the person in
  the room, not the broadcast.
- `/mod kick server` and `/mod ban server` also stop the stream. Terminating
  SSH sessions is not enough on its own: the console lives in a browser, so
  without this a banned user keeps broadcasting to anonymous link-holders.
- `/mod` room tools (ban, kick-from-room, slow mode) work on the stream
  chat room like any other room, but they do not touch the media: the
  publisher's grant comes from the per-stream token, not room membership.
- A minted LiveKit token stays valid for an hour; the force-disconnect plus
  the refusal on the next `/golive` is what makes any of these bite now.

## 7. Known gaps / follow-ups

- Metrics: no `record_stream_*` telemetry yet (streams started, watcher
  peaks — the experiment metrics in STREAM.md are currently only readable
  from logs/registry). The `stream ended` line carries the fields a
  teardown-reason counter would want.
- A renamed streamer keeps their room under the old `{username}-live` slug
  (cosmetic only: the slug is not shown anywhere user-facing).
- Splash tips carry no `/golive` line yet.
- The tavern TV prop, arena spectator reuse, and a public `late.sh/live`
  stay future work (see STREAM.md).
