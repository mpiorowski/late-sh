# late.sh Voice Context

## Metadata
- Domain: late.sh voice rooms — LiveKit-backed CLI voice, SSH TUI controls/status, pair-WS voice control, and browser listen-only voice
- Primary audience: LLM agents working in `late-ssh/src/app/voice`, `late-cli/src/voice.rs`, pair-WS voice messages, or the web `/voice` listener
- Last updated: 2026-06-03 (split voice context out of `app/audio/CONTEXT.md`; current MVP uses LiveKit `PlatformAudio` in the native CLI and a subscribe-only browser listener)
- Status: Active
- Parent context: `../../../../CONTEXT.md`
- Related context: `../../../../late-cli/CONTEXT.md`, `../audio/CONTEXT.md`

---

## 1. Scope

Owned by this domain:
- One global/synthetic late.sh voice room exposed as the Home `voice` entry.
- Server-side LiveKit token minting for CLI participants and browser listen-only users.
- TUI voice state: enabled/off, participant list, current-user joined/muted/deafened state, and room controls.
- Pair WebSocket voice control messages: join, leave, mute, deafen, and client `voice_state` reports.
- CLI voice media runtime in `late-cli/src/voice.rs`: microphone capture, remote playout, mute/deafen, and LiveKit room lifecycle.
- Browser listen-only `/voice` page and `/api/voice/listen-ticket` token route.

Out of scope:
- Icecast house radio, YouTube queue/fallback, Music Booth, visualizer, and audio source switching. Those live in `late-ssh/src/app/audio/CONTEXT.md`.
- Browser publishing, video, screen share, recording, DMs, and per-room voice channels.
- A shared CLI mixer for music + voice. Current voice I/O uses LiveKit `PlatformAudio` separately from the CLI music decoder.

Product direction:
- Voice belongs to the late.sh clubhouse experience, not a standalone Discord clone.
- Prefer "voice rooms" over "calls".
- First version is CLI-joinable voice controlled from SSH TUI; raw `ssh late.sh` users can see status but cannot join because plain SSH has no local microphone/speaker bridge.

---

## 2. File Map

```text
late-ssh/src/app/voice/
├── mod.rs      # declarations only
├── svc.rs      # VoiceService, LiveKit JWT minting, participant snapshot/watch, stale prune
├── state.rs    # per-session watch receiver shim + joined/muted/deafened helpers
└── ui.rs       # TUI room body + one-line controls render
```

Cross-crate touchpoints:
- `late-ssh/src/api.rs` — `/api/voice/listen-ticket`; `/api/ws/pair` parses inbound `voice_state`, sends `PairControlMessage` voice events, and removes CLI participants on CLI WS close.
- `late-ssh/src/paired_clients.rs` — `PairControlMessage::{VoiceJoin,VoiceLeave,VoiceSetMuted,VoiceSetDeafened}` and `send_control_to_voice_cli`.
- `late-ssh/src/app/state.rs` — `voice_join`, `voice_leave`, `voice_toggle_join`, `voice_toggle_muted`, `voice_toggle_deafened`.
- `late-ssh/src/app/chat/input.rs` — voice-room key routing: `Enter` join/leave, `u` mute/unmute, `d` deafen/undeafen.
- `late-ssh/src/app/chat/ui.rs` — synthetic Home `voice` room renders `draw_voice_room` and `draw_voice_controls`.
- `late-ssh/src/app/render.rs` — builds `VoiceRoomView` with snapshot, current user, CLI capability, and browser listen URL.
- `late-ssh/src/config.rs` / `main.rs` — `LATE_VOICE_*` / `LATE_LIVEKIT_*` config, `VoiceService` construction, stale participant pruning every 30s.
- `late-cli/src/voice.rs` — CLI LiveKit media runtime.
- `late-cli/src/ws.rs` — advertises `"voice"` capability, handles voice pair-control events, sends `voice_state` every 15s.
- `late-cli/src/main.rs` — keeps one `VoiceRuntimeState` across pair-WS reconnects.
- `late-web/src/pages/voice/{mod.rs,page.html}` — public listen-only browser page.
- `infra/livekit.tf`, `infra/service-ssh.tf` — production LiveKit and SSH service env wiring.

Keep `mod.rs` declaration-only.

---

## 3. Server Service

`VoiceService` is an in-memory control/status service. It does not carry media and does not talk to LiveKit at runtime except by minting JWTs.

Main types:
- `VoiceConfig` — enabled flag, LiveKit URL/key/secret, and shared room name.
- `VoiceSnapshot` — `{ enabled, room_name, livekit_url, participants }`, delivered via `watch`.
- `VoiceParticipant` — `{ user_id, username, muted, deafened, speaking, updated_at }`.
- `VoiceClientState` — inbound CLI state shape `{ joined, room, muted, deafened, speaking }`.
- `VoiceJoinTicket` — CLI join ticket with publish+subscribe grants.
- `VoiceListenTicket` — browser listen ticket with subscribe-only grants.

Public API:
- `new(config)` — initializes an empty snapshot.
- `snapshot()` / `subscribe()` — read or watch current TUI-visible state.
- `join_ticket(user_id, username, muted, deafened)` — mints a LiveKit JWT for the native CLI. Grants: `roomJoin=true`, `canPublish=true`, `canSubscribe=true`, `canPublishData=true`, `roomCreate=false`.
- `listen_ticket()` — mints an anonymous `web-listener-<uuid>` JWT. Grants: `roomJoin=true`, `canSubscribe=true`, `canPublish=false`, `canPublishData=false`, `roomCreate=false`.
- `apply_client_state(user_id, username, state)` — accepts CLI `voice_state`; removes the participant if `joined=false` or if `room` does not match `config.room_name`.
- `update_local_state(...)` — optimistic server-side mirror used after TUI mute/deafen/join actions so the UI responds immediately.
- `leave(user_id)` — removes a user from the participant snapshot.
- `prune_stale(ttl)` — removes participants whose `updated_at` is older than `ttl`.

Snapshots are sorted by lowercase username, then `user_id`.

Token details:
- Tokens are HS256 JWTs signed with `LATE_LIVEKIT_API_SECRET`.
- `iss` is `LATE_LIVEKIT_API_KEY`.
- Participant `sub` is the late.sh user UUID string; browser listener `sub` is `web-listener-<uuid>`.
- `nbf = now - 5s`, `exp = now + 1h`.

---

## 4. Pair WebSocket Protocol

Voice rides the existing paired-client WebSocket:

```text
GET /api/ws/pair?token={session_token}
```

Server → CLI (`PairControlMessage`, snake_case `event`):

```json
{ "event": "voice_join", "room": "late-voice", "url": "wss://rtc.late.sh", "token": "...", "muted": true, "deafened": false }
{ "event": "voice_leave" }
{ "event": "voice_set_muted", "muted": true }
{ "event": "voice_set_deafened", "deafened": true }
```

CLI → server:

```json
{
  "event": "voice_state",
  "joined": true,
  "room": "late-voice",
  "muted": false,
  "deafened": false,
  "speaking": false
}
```

Routing rules:
- Voice controls are sent only to native CLI paired entries whose `ClientAudioState::supports_voice()` is true.
- The CLI advertises `"voice"` in `client_state.capabilities` on Linux, macOS, and Windows.
- Browsers and older CLIs do not receive voice join/mute/deafen controls.
- Pair-WS close removes the participant only when the closing entry's last known `client_kind` was `Cli`. Browser/webview pair disconnects should not force voice leave.
- On a pair-WS reconnect, the CLI immediately re-sends `voice_state` if already joined.

The pair WS still carries audio/clipboard events too; voice handlers must ignore unrelated pair messages and must not change music source/mute semantics.

---

## 5. TUI Surface

Voice is a synthetic Home room, alongside other chat-adjacent entries such as RSS/news/mentions/work.

Render:
- `draw_voice_room` shows `Voice #<room_name>`, browser listen-only URL, and participants.
- The room title includes the current participant count as `<N> connected`.
- `draw_voice_controls` shows whether voice is configured, whether the paired CLI supports voice, and the current action hints.
- Participant status precedence is: `deafened`, else `muted`, else `speaking`, else `listening`.
- The current user's row is amber/bold.

Input when the voice room is selected:
- `Enter` — join or leave.
- `u` / `U` — mute or unmute microphone.
- `d` / `D` — deafen or undeafen.
- Room navigation keys still move to next/previous Home room before voice-specific handling.

Join behavior:
- Users start muted (`muted=true`, `deafened=false`).
- `App::voice_join` first mints a ticket, then sends `voice_join` to a capable paired CLI.
- If no capable CLI is paired, banner: `No paired CLI with voice support. Update and run \`late\`.`
- The server optimistically updates local state after sending controls so the TUI changes immediately; CLI `voice_state` remains the eventual source of truth.

Current UX gaps worth addressing:
- The browser listen-only URL currently dominates the first line even when a CLI can join.
- Participant sorting is alphabetical only; speaking-first/current-user-first would make active rooms easier to scan.

Participant count surfaces:
- Home room rail shows the count in the existing badge/count slot (`voice  3`) using `VoiceSnapshot.participants.len()`.
- Room search shows the same count for the synthetic `voice` item.
- Browser listen-only users are not included in this count.

---

## 6. CLI Voice Runtime

`late-cli/src/voice.rs` owns local media.

Runtime state:
- `VoiceRuntimeState { joined, room, muted, deafened, speaking, media }`.
- `late-cli/src/main.rs` creates one `VoiceRuntimeState` outside the reconnecting pair-WS loop. This is critical: pair-WS reconnects must not implicitly leave the LiveKit room.

Join:
1. `voice.join(...)` first calls `leave()` to close any existing room.
2. On Linux/macOS/Windows, `connect_voice_media` creates `PlatformAudio`.
3. Selects the first recording device and first playout device when available.
4. Connects to LiveKit with `Room::connect`.
5. Publishes a local audio track named `"microphone"` with `TrackSource::Microphone`.
6. If the join ticket requested muted, the local track is muted before publication.

Mute/deafen:
- `set_muted(true)` mutes the `LocalTrackPublication`; unmute calls `publication.unmute()`.
- `set_deafened(true)` disables all currently subscribed remote audio tracks and causes future subscribed tracks to be disabled too.
- Deafen state is tracked with `remote_playback_enabled: AtomicBool`.

Events:
- `RoomEvent::Reconnecting` / `Reconnected` / `Disconnected` are logged.
- Disconnected sets an atomic flag. The pair-WS heartbeat checks `media_disconnected()`, then leaves and sends `voice_state`.
- `TrackSubscribed` logs remote audio and disables it immediately if deafened.
- `TrackUnsubscribed` logs the remote track id.

Unsupported platforms:
- CLI voice media is compiled only for Linux, macOS, and Windows.
- Other platforms advertise no capabilities and `join` bails with `voice media is not supported on this platform`.

Important audio-engine boundary:
- Do not reintroduce a second manual CPAL/FIFO remote-track playout path. Earlier manual output could duplicate/stutter remote voice.
- Current MVP uses LiveKit `PlatformAudio` for both capture and remote playout.
- The long-term improvement is one CLI audio engine/mixer that can combine music and voice, support ducking, and centralize device selection.

---

## 7. Browser Listen-Only

Web route:
- `late-web/src/pages/voice/mod.rs` serves `/voice`.
- Page uses LiveKit JS from jsDelivr and fetches `/api/voice/listen-ticket`.
- `api_url` is `state.config.ssh_public_url`, normalized client-side to an HTTP(S) API base.

API route:
- `GET /api/voice/listen-ticket` returns `{ room, url, token }`.
- Rate-limited by `state.voice_listen_limiter`.
- On disabled/misconfigured voice, returns `503` with `{ "message": "..." }`.

Browser behavior:
- Subscribe-only, `autoSubscribe: true`.
- Calls `room.startAudio()` after connect when available.
- Attaches remote audio tracks into hidden `#voice-audio`.
- Dedupes attachments by track SID, media track ID, or object identity so initial existing-track scan and `TrackSubscribed` cannot double-play a track.
- Detaches on `TrackUnsubscribed` and clears all attachments on disconnect.

Browser listeners are anonymous and are not included in `VoiceSnapshot.participants`; do not count them in TUI participant badges unless authenticated listen presence is added.

---

## 8. Config, Infra, and Background Tasks

Config env vars (`late-ssh/src/config.rs`):
- `LATE_VOICE_ENABLED` — defaults false in config parsing.
- `LATE_LIVEKIT_URL` — required when voice is enabled.
- `LATE_LIVEKIT_API_KEY` — required when voice is enabled.
- `LATE_LIVEKIT_API_SECRET` — required when voice is enabled.
- `LATE_VOICE_ROOM` — optional; default `late-voice`.

Production infra:
- `infra/livekit.tf` manages the LiveKit deployment.
- `rtc.<domain>` is the public LiveKit signaling endpoint.
- Media ports are bound directly on the node; keep DNS/networking assumptions distinct from SSH/API/web routing.
- `infra/service-ssh.tf` wires the voice env vars into `service-ssh`.

Background tasks:
- `main.rs` prunes stale voice participants every 30s with `ttl = 90s`.
- CLI sends `voice_state` every 15s while joined.
- Limiter cleanup also cleans `voice_listen_limiter` every 300s.
- `voice_listen_limiter` is currently constructed with `ws_pair_max_attempts_per_ip` and `ws_pair_rate_limit_window_secs`, matching the pair-WS rate-limit budget.

---

## 9. Invariants

1. **LiveKit owns media.** Voice audio must not flow through SSH rendering, TUI frames, or the Music Booth/AudioService queue.
2. **late-ssh owns auth/control/status.** It mints LiveKit tokens and tracks display state, but it is not the SFU and does not relay media.
3. **Native CLI owns joinable voice.** Raw SSH and browser-only users can observe/listen, but cannot publish microphone audio in the MVP.
4. **VoiceRuntimeState survives pair-WS reconnects.** Do not move it inside `run_viz_ws`; reconnects should refresh state, not leave the LiveKit room.
5. **Periodic `voice_state` is required.** Server-side participant pruning depends on the CLI refresh cadence.
6. **Room mismatch means leave.** Inbound `voice_state.room` must match `VoiceConfig.room_name`, otherwise the participant is removed from the snapshot.
7. **Browser listeners are not participants.** They are anonymous subscribe-only LiveKit identities and currently do not appear in TUI counts.
8. **Start muted.** Join tickets requested from the TUI use `muted=true` so users enter safely.
9. **No room creation grants.** Server-minted tokens should keep `roomCreate=false`; LiveKit room lifecycle is infra/service-owned.

---

## 10. Known Gaps / Backlog

- Improve voice room UI: less prominent browser URL, speaking/current-user sort, and compact status markers.
- Speaking state is accepted and rendered, but current `late-cli/src/voice.rs` only stores `speaking=false`; real activity detection needs a future LiveKit/audio-level signal.
- Validate production NAT/firewall behavior for `rtc.<domain>` and direct UDP/TCP media ports.
- Add richer LiveKit health/metrics dashboards.
- Add authenticated browser listen presence only if we want browser listeners to appear in counts.
- Consider one CLI audio engine/mixer for music + voice once voice polish needs ducking/device unification.
- Keep browser publishing, video, screen share, recording, and per-room/channel voice out of the MVP until product scope changes.

---

## 11. Testing Guidance

Source-side unit tests are appropriate for pure logic only:
- JWT claim shape in `svc.rs` (already present).
- Participant sorting/status formatting helpers if extracted.
- TUI label/count formatting helpers if kept pure.

Integration tests belong under `late-ssh/tests/` if they need services, DB, API routes, pair-WS orchestration, or rate limiters.

LLM agents must not run `cargo test`, `cargo nextest`, or `cargo clippy` in this repo. Note expected verification commands in handoff instead.

---

## 12. References

- Root context: `../../../../CONTEXT.md`
- CLI context: `../../../../late-cli/CONTEXT.md`
- Music/audio context: `../audio/CONTEXT.md`
- Server voice service: `late-ssh/src/app/voice/svc.rs`
- TUI voice UI: `late-ssh/src/app/voice/ui.rs`
- Pair WS handler: `late-ssh/src/api.rs`
- Pair registry/control messages: `late-ssh/src/paired_clients.rs`
- CLI voice runtime: `late-cli/src/voice.rs`
- CLI pair WS: `late-cli/src/ws.rs`
- Browser listener: `late-web/src/pages/voice/page.html`
- Production LiveKit infra: `infra/livekit.tf`
