# late.sh Voice Context

## Metadata
- Domain: late.sh voice channels — LiveKit-backed CLI voice, SSH TUI controls/status, and pair-WS voice control
- Primary audience: LLM agents working in `late-ssh/src/app/voice`, `late-cli/src/voice.rs`, or pair-WS voice messages
- Last updated: 2026-06-21
- Status: Active
- Parent context: `../../../../CONTEXT.md`
- Related context: `../../../../late-cli/CONTEXT.md`, `../audio/CONTEXT.md`

---

## 1. Scope

Owned by this domain:
- Voice channels attached to chat rooms (the only target kind since the Rooms demolition dropped `target_kind='game_room'`).
- Server-side LiveKit token minting for authenticated CLI participants.
- TUI voice state: enabled/off, participant list, current-user joined/muted/deafened state, and room controls.
- Pair WebSocket voice control messages: join, leave, mute, deafen, and client `voice_state` reports.
- CLI voice media runtime in `late-cli/src/voice.rs`: microphone capture, remote playout, mute/deafen, and LiveKit room lifecycle.

Out of scope:
- Icecast house radio, YouTube queue/fallback, Music Booth, visualizer, and audio source switching. Those live in `late-ssh/src/app/audio/CONTEXT.md`.
- Browser publishing/listening, video, screen share, and recording.
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
└── ui.rs       # borderless TUI roster + controls strip
```

Cross-crate touchpoints:
- `late-ssh/src/api.rs` — `/api/ws/pair` parses inbound `voice_state`, sends `PairControlMessage` voice events, and removes CLI participants on CLI WS close.
- `late-ssh/src/paired_clients.rs` — `PairControlMessage::{VoiceJoin,VoiceLeave,VoiceSetMuted,VoiceSetDeafened}` and `send_control_to_voice_cli`.
- `late-ssh/src/app/state.rs` — `voice_join`, `voice_leave`, `voice_toggle_join`, `voice_toggle_muted`, `voice_toggle_deafened`.
- `late-ssh/src/app/input.rs` — global voice key routing: `Ctrl+V` join/leave, `Ctrl+T` mute/unmute, with Artboard/Pinstar opting out.
- `late-ssh/src/app/chat/ui.rs` — chat and game surfaces embed `draw_voice_strip` when a voice channel is present.
- `late-ssh/src/app/render.rs` — builds `VoiceRoomView` with snapshot, current user, and CLI capability.
- `late-ssh/src/config.rs` / `main.rs` — `LATE_VOICE_*` / `LATE_LIVEKIT_*` config, `VoiceService` construction, stale participant pruning every 30s.
- `late-cli/src/voice.rs` — CLI LiveKit media runtime.
- `late-cli/src/ws.rs` — advertises `"voice"` capability, handles voice pair-control events, sends `voice_state` every 15s and on speaking-state changes.
- `late-cli/src/main.rs::run_ws_pairing` — creates one `VoiceRuntimeState` before its pair-WS retry loop and passes it into each `run_viz_ws` attempt, so pair-WS reconnects do not implicitly leave LiveKit.
- `infra/livekit.tf`, `infra/service-ssh.tf` — production LiveKit and SSH service env wiring.

Keep `mod.rs` declaration-only.

---

## 3. Server Service

`VoiceService` is an in-memory control/status service. It does not carry media and does not talk to LiveKit at runtime except by minting JWTs.

Main types:
- `VoiceConfig` — enabled flag, LiveKit URL/key/secret, and LiveKit room base name. Each voice channel uses `{LATE_VOICE_ROOM}-{voice_channel_id}`.
- `VoiceSnapshot` — `{ enabled, livekit_url, rooms }`, delivered via `watch`. `rooms` is keyed by `voice_channels.id`.
- `VoiceParticipant` — `{ user_id, username, muted, deafened, speaking, updated_at }`.
- `VoiceClientState` — inbound CLI state shape `{ joined, room, muted, deafened, speaking }`.
- `VoiceJoinTicket` — CLI join ticket with publish+subscribe grants.

Public API:
- `new(config)` — initializes an empty snapshot.
- `snapshot()` / `subscribe()` — read or watch current TUI-visible state.
- `checked_join_ticket(voice_channel_id, user_id, username, muted, deafened)` — verifies the enabled voice channel and target chat-room membership before minting a CLI ticket.
- `join_ticket(voice_channel_id, user_id, username, muted, deafened)` — low-level LiveKit JWT minting for the native CLI after callers have authorized the join. Grants: `roomJoin=true`, `canPublish=true`, `canSubscribe=true`, `canPublishData=true`, `roomCreate=false`.
- `apply_client_state(user_id, username, state)` — accepts CLI `voice_state` only for the user's most recently server-ticketed voice channel; removes the participant if `joined=false`, if `room` is missing/unrecognized or lacks the configured base-name plus UUID suffix, or if the parsed voice channel was not ticketed for that user.
- `update_local_state(...)` — optimistic server-side mirror used after TUI mute/deafen/join actions so the UI responds immediately.
- `leave(user_id)` — removes a user from the participant snapshot.
- `revoke_channel(room_id)`, `revoke_user_from_channel(room_id, user_id)`, and `revoke_user(user_id)` — clear runtime presence/last-ticketed state and return LiveKit room/user pairs for server-side `RemoveParticipant`.
- `kick(user_id)` runtime-blocks a user from all voice rooms until `allow(user_id)` or server restart, removes current/authorized presence, and returns the LiveKit room to force-disconnect; `allow(user_id)` clears that runtime block.
- `prune_stale(ttl)` — removes participants whose `updated_at` is older than `ttl`.

DMs and private rooms are created with enabled chat-room voice channels by
default. Public chat rooms are enabled by staff through `/mod room-voice`.
Daily match chats too: claiming a daily challenge creates an enabled
`target_kind='chat_room'` channel (display name `{game}: {challenger} v
{opponent}`) in the claim transaction, rendered as the usual strip above the
board's embedded chat; the daily sweeper deletes the channel with the chat
room 30 days after the match ends (see
`late-ssh/src/app/lobby/daily/CONTEXT.md`).

Snapshots are sorted by lowercase username, then `user_id`.

Token details:
- Tokens are HS256 JWTs signed with `LATE_LIVEKIT_API_SECRET`.
- `iss` is `LATE_LIVEKIT_API_KEY`.
- Participant `sub` is the late.sh user UUID string.
- `nbf = now - 5s`, `exp = now + 1h`.

---

## 4. Pair WebSocket Protocol

Voice rides the existing paired-client WebSocket:

```text
GET /api/ws/pair?token={session_token}
```

Server → CLI (`PairControlMessage`, snake_case `event`):

```json
{ "event": "voice_join", "room": "late-voice-00000000-0000-0000-0000-000000000000", "url": "wss://rtc.late.sh", "token": "...", "muted": true, "deafened": false }
{ "event": "voice_leave" }
{ "event": "voice_set_muted", "muted": true }
{ "event": "voice_set_deafened", "deafened": true }
```

CLI → server:

```json
{
  "event": "voice_state",
  "joined": true,
  "room": "late-voice-00000000-0000-0000-0000-000000000000",
  "muted": false,
  "deafened": false,
  "speaking": false
}
```

Routing rules:
- Voice controls are sent only to native CLI paired entries whose `ClientAudioState::supports_voice()` is true.
- The CLI advertises `"voice"` in `client_state.capabilities` on Linux and Windows. macOS does not advertise native voice.
- Browsers and older CLIs do not receive voice join/mute/deafen controls.
- Pair-WS close removes the participant only when the closing entry's last known `client_kind` was `Cli`. Browser/webview pair disconnects should not force voice leave.
- On a pair-WS reconnect, the CLI immediately re-sends `voice_state` if already joined.

The pair WS still carries audio/clipboard events too; voice handlers must ignore unrelated pair messages and must not change music source/mute semantics.

---

## 5. TUI Surface

Voice is embedded into whatever surface owns the active voice channel. Chat rooms and game surfaces (house tables, daily match boards) both render voice through the same chat-room-targeted channels.

Render:
- `draw_voice_strip` is borderless and titleless. It renders only two rows: the participant/status roster and compact action hints. Do not add a `Voice` title, live-count header, or border row.
- `render.rs` also draws a global right-aligned top chrome badge when the current user is actually joined to any voice channel. The badge is derived from `VoiceSnapshot::current_room`, prefers `#chat-room` or table display labels, and shows the current user's voice status. It is presence-only UI; do not route media or LiveKit control through render.
- The strip appears whenever the active surface has an enabled voice channel, regardless of whether the current paired client can publish voice.
- Chat-room voice is shown at the top of the message area, including the Home/dashboard-with-top-boxes chat path. The room rail does not append a speaker icon for voice-enabled rooms.
- Game-surface voice uses the same strip above embedded chat. The visual separator between the game board and voice/chat belongs to the surface's renderer (`app/lobby/house/ui.rs`, `app/lobby/daily/board_ui.rs`), not the chat or voice renderer.
- Participant status precedence is: `deafened`, else `muted`, else `speaking`, else `listening`.
- The current user's name is amber/bold.

Input:
- `Ctrl+V` — join the active voice channel, switch to it if joined elsewhere, or leave when already joined to that same channel. If no active voice channel is visible but the user is joined elsewhere, it leaves the current voice room.
- `Ctrl+T` — mute or unmute microphone.
- Artboard and Pinstar opt out of these global chords.
- Deafen is still represented in the lower-level CLI/pair protocol, but no TUI shortcut is exposed in the embedded voice UI.

Join behavior:
- Users start muted (`muted=true`, `deafened=false`).
- `App::voice_join` starts an async checked ticket task, then sends `voice_join` to a capable paired CLI after authorization succeeds.
- Entering a DM/private room or active game room does not auto-join voice. Joining or switching voice rooms is explicit through `Ctrl+V`.
- Voice membership persists across room, screen, and game navigation. Leaving a chat/game surface must not send `voice_leave`; users stay in the LiveKit room until they explicitly leave, switch to another voice channel, disconnect the native CLI pair, are pruned as stale, or moderation revokes them.
- If no capable CLI is paired, banner: `No paired CLI with voice support. Update and run \`late\`.`
- The server optimistically updates local state after sending controls so the TUI changes immediately; CLI `voice_state` remains the eventual source of truth.

Current UX gaps worth addressing:
- Participant sorting is alphabetical only; speaking-first/current-user-first would make active rooms easier to scan.

Moderation revocation:
- `/mod room-voice off` revokes every known/authorized participant for that voice channel and calls LiveKit `RemoveParticipant` for each identity.
- Room kick/ban revokes the target user from that room's voice channel (all voice channels are chat-room-targeted, including game-surface ones).
- Server kick/ban revokes the target user from whichever voice channel they are currently in or most recently ticketed for.
- `/mod voice kick` is broader than room revocation: it is a runtime, server-wide voice block and is not persisted beyond restart.
- LiveKit removal failures are logged after DB/audit state is committed; they should not roll back moderation state.

---

## 6. CLI Voice Runtime

`late-cli/src/voice.rs` owns local media.

Runtime state:
- `VoiceRuntimeState { joined, room, muted, deafened, speaking, media }`.
- `late-cli/src/main.rs::run_ws_pairing` creates one `VoiceRuntimeState` before its pair-WS retry loop and passes it into each `run_viz_ws` attempt. This is critical: pair-WS reconnects must not implicitly leave the LiveKit room.

Join:
1. `voice.join(...)` first calls `leave()` to close any existing room.
2. On Linux/Windows, `connect_voice_media` creates `PlatformAudio`.
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
- `ActiveSpeakersChanged` updates the CLI runtime `speaking` flag; pair WS reports that state quickly so SSH can render the green speaking indicator.
- `TrackSubscribed` logs remote audio and disables it immediately if deafened.
- `TrackUnsubscribed` logs the remote track id.

Unsupported platforms:
- CLI voice media is compiled only for Linux and Windows.
- macOS, Android/Termux, and other platforms advertise no voice capability and `join` bails with `voice media is not supported on this platform`.
- macOS users currently cannot join voice because browser listen-only support has been removed for v1 private-room safety.

Important audio-engine boundary:
- Do not reintroduce a second manual CPAL/FIFO remote-track playout path. Earlier manual output could duplicate/stutter remote voice.
- Current MVP uses LiveKit `PlatformAudio` for both capture and remote playout.
- The long-term improvement is one CLI audio engine/mixer that can combine music and voice, support ducking, and centralize device selection.

---

## 7. Browser Listen-Only

Browser listen-only support was removed for v1. Voice tokens are now minted only
for authenticated SSH sessions with a paired native CLI, after the server checks
that the voice channel is enabled and the user is a member of the target
chat/game room.

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
- CLI sends `voice_state` every 15s while joined and also reports speaking changes promptly.

---

## 9. Invariants

1. **LiveKit owns media.** Voice audio must not flow through SSH rendering, TUI frames, or the Music Booth/AudioService queue.
2. **late-ssh owns auth/control/status.** It mints LiveKit tokens and tracks display state, but it is not the SFU and does not relay media.
3. **Native CLI owns voice.** Raw SSH and browser-only users can observe TUI status but cannot join or listen in the MVP.
4. **VoiceRuntimeState survives pair-WS reconnects.** Do not move it inside `run_viz_ws`; reconnects should refresh state, not leave the LiveKit room.
5. **Periodic `voice_state` is required.** Server-side participant pruning depends on the CLI refresh cadence.
6. **Unknown LiveKit room means leave.** Inbound `voice_state.room` must have the configured prefix and a valid voice channel UUID suffix, otherwise the participant is removed from the snapshot.
7. **Start muted.** Join tickets requested from the TUI use `muted=true` so users enter safely.
8. **No room creation grants.** Server-minted tokens should keep `roomCreate=false`; LiveKit room lifecycle is infra/service-owned.

---

## 10. Known Gaps / Backlog

- Improve embedded voice UI: speaking/current-user sort and compact status markers.
- Speaking state is accepted and rendered, but current `late-cli/src/voice.rs` only stores `speaking=false`; real activity detection needs a future LiveKit/audio-level signal.
- Validate production NAT/firewall behavior for `rtc.<domain>` and direct UDP/TCP media ports.
- Add richer LiveKit health/metrics dashboards.
- Reintroduce browser listening only with authenticated room-access checks.
- Consider one CLI audio engine/mixer for music + voice once voice polish needs ducking/device unification.
- Keep browser publishing, video, screen share, and recording out of the MVP until product scope changes.

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
- Production LiveKit infra: `infra/livekit.tf`
