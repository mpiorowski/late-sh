# late.sh Audio / Voice Notes

## Voice-Only CLI MVP Investigation

Last updated: 2026-05-26

These notes capture the current direction for adding voice chat to late.sh without turning it into a separate Discord-like community app.

## Product Direction

Voice should be part of the late.sh clubhouse experience, not a second chat platform.

The first version should be **CLI-only voice**, controlled from the SSH TUI:

- `late` CLI users can join voice.
- Raw `ssh late.sh` users can see voice status, but cannot join because plain SSH has no microphone or speaker access.
- No browser requirement for the MVP.
- No video.
- No screen share.
- No recording or streaming.
- No DMs at first unless the room model makes it trivial.

Prefer “late voice rooms” over “calls.” Voice belongs to the room or synthetic voice surface the user is already sitting in.

## Initial Scope

Start with one simple global/synthetic voice room.

This is intentionally smaller than per-chat-room voice:

- One synthetic `Voice` entry, similar in spirit to Mentions/News/Work.
- One shared voice room behind it.
- Functional first: join, leave, mute, deafen, show basic state.
- Room scoping, per-channel voice, screen share, and moderation controls can come later.

## Architecture

Use **LiveKit** as the SFU.

- `late-ssh` owns auth, room mapping, moderation, and TUI state.
- `late-cli` owns microphone capture and remote voice playback.
- LiveKit runs as a separate service/container, locally via Docker and later as its own deployment, likely at `rtc.late.sh`.
- Voice media must not flow through the SSH render loop.

The existing pair WebSocket should become the control channel for voice, the same way it already controls paired audio/browser/CLI behavior.

Server to CLI:

```json
{ "event": "voice_join", "room": "general", "url": "wss://rtc.late.sh", "token": "..." }
{ "event": "voice_leave" }
{ "event": "voice_set_muted", "muted": true }
{ "event": "voice_set_deafened", "deafened": true }
```

CLI to server:

```json
{
  "event": "voice_state",
  "joined": true,
  "room": "general",
  "muted": false,
  "deafened": false,
  "speaking": true
}
```

## TUI Shape

Example synthetic voice view:

```text
Voice  #general
@mat speaking
@anna muted
@lee deafened

v join/leave   u mute mic   d deafen
```

For the first version, users should start muted when they join. That avoids surprise hot microphones and makes the feature feel respectful.

## Audio Engine Decision

Avoid opening a totally separate unmanaged output path if possible.

The clean long-term version is one CLI audio engine that can mix:

- existing radio/music stream
- remote voice tracks
- local volume/mute/deafen state

That reduces device conflicts and allows future polish such as ducking music while people speak.

For the first working version, it is acceptable if LiveKit’s Rust/native audio path owns voice I/O separately, as long as this is treated as an MVP compromise and the boundary is isolated.

## Main Risks

- LiveKit Rust SDK is the right tool, but native WebRTC linking and runtime behavior may be non-trivial. Source: https://github.com/livekit/rust-sdks
- `cpal` gives cross-platform audio I/O, but echo cancellation is the hard part. Source: https://github.com/RustAudio/cpal
- For MVP, assume headset users and provide mute/deafen controls. Proper AEC/noise suppression can come after the first working version.
- WSL and Android need careful behavior because current CLI audio already has platform caveats.
- Screen sharing should not be part of the first scope. CLI screen capture is platform-specific, especially on Wayland.

## Docker / Service Direction

Voice service should run separately from `late-ssh`.

Local development:

- Add LiveKit to Docker Compose as a separate RTC service.
- `late-ssh` should mint LiveKit tokens and send them over the pair WebSocket.
- `late-cli` should connect directly to LiveKit.

Production:

- Separate deployment for LiveKit.
- Public RTC endpoint such as `rtc.late.sh`.
- Keep SSH/API/web services responsible for control/auth, not voice media.

## Recommended Implementation Path

1. Add LiveKit config to `late-ssh`.
2. Add one synthetic Voice room in Home.
3. Add pair-WS control events for voice join/leave/mute/deafen.
4. Add CLI capability advertisement for voice.
5. Add a CLI voice runtime boundary that can receive commands and report state.
6. Wire LiveKit join/playback/capture behind that boundary.
7. Keep browser, video, screen share, recording, and per-room voice out of the MVP.

