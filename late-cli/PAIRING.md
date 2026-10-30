# late-cli pairing protocol

The pair WebSocket between the CLI (and its webview helper) and the SSH
session: the messages both ways, the client-state labels, pairing behavior,
latency samples, clipboard images, and the embedded YouTube helper window.
Part of `CONTEXT.md` (§6); video placement is `VIDEO.md`, the server side is
`late-ssh/src/app/audio/CONTEXT.md` and `late-web/CONTEXT.md`.

## Messages

`late-cli` pairs to the active SSH session with:

```text
GET /api/ws/pair?token={token}
```

`--api-base-url` may be `http://`, `https://`, `ws://`, or `wss://`; `src/ws.rs` rewrites HTTP schemes to WebSocket schemes.

Client to server:

```json
{ "event": "heartbeat", "position_ms": 1234 }
```

```json
{ "event": "viz", "position_ms": 1234, "bands": [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0], "rms": 0.0 }
```

```json
{
  "event": "client_state",
  "client_kind": "cli",
  "ssh_mode": "native",
  "platform": "linux",
  "capabilities": ["clipboard_image", "youtube", "voice", "open_url"],
  "video_blockers": ["not_kitty", "no_mpv"],
  "muted": false,
  "volume_percent": 30
}
```

Client to server, from the Linux desktop MPRIS surface (widget play/pause, media keys, volume slider). The server fans the matching `set_muted`/`set_volume` control back to every paired client on the token:

```json
{ "event": "set_muted", "muted": true }
{ "event": "set_volume", "volume_percent": 45 }
```

Server to client:

```json
{ "event": "toggle_mute" }
```

```json
{ "event": "volume_up" }
```

```json
{ "event": "volume_down" }
```

```json
{ "event": "set_muted", "muted": true }
{ "event": "set_volume", "volume_percent": 45 }
```

```json
{ "event": "request_clipboard_image" }
```

```json
{ "event": "set_playback_source", "source": "icecast|youtube|radio", "stream_url": "...", "station": "..." }
```

```json
{ "event": "voice_join", "room": "late-voice-00000000-0000-0000-0000-000000000000", "url": "wss://rtc.late.sh", "token": "...", "muted": true, "deafened": false }
{ "event": "voice_leave" }
{ "event": "voice_set_muted", "muted": true }
{ "event": "voice_set_deafened", "deafened": true }
```

Drain handoff, or a restart from the quit modal, only to a CLI advertising `reconnect` (see "Drain handoff" below). `resume` is opaque to the CLI:

```json
{ "event": "reconnect", "resume": "v1:arcade" }
```

Client to server, in response to `request_clipboard_image`:

```json
{ "event": "clipboard_image", "data_base64": "<base64 png bytes>" }
```

```json
{ "event": "clipboard_image_failed", "message": "clipboard does not contain an image" }
```

Client to server for voice state:

```json
{ "event": "voice_state", "joined": true, "room": "late-voice-00000000-0000-0000-0000-000000000000", "muted": false, "deafened": false, "speaking": false }
```

Client to server, native mode only (see "Latency samples"):

```json
{ "event": "latency", "kind": "ssh_rtt", "micros": 42000 }
{ "event": "latency", "kind": "terminal", "micros": 900 }
```

Client state labels:
- `ssh_mode`: `native`, `openssh`, `old`
- `platform`: `linux`, `macos`, `windows`, `android`, or `unknown`
- `capabilities`: optional list; Linux, macOS, and Windows desktop CLI builds advertise `clipboard_image`, `youtube`, `voice`, and `open_url`; Android/Termux builds leave the base list empty. `video` is appended on any build whose startup probe finds no blocker (see "Video watch" below). `reconnect` is appended in native mode only, the one mode that can redial on its own.
- `video_blockers`: present exactly when `video` is not in `capabilities`; every failing precondition by wire name: `not_native`, `platform`, `multiplexer`, `not_kitty`, `no_pixel_size`, `no_mpv`, `mpv_too_old`, `no_ytdlp`.

Pairing behavior:
- The server stores one paired-client sender/state entry per token.
- If the CLI and its webview helper both pair on the same token, latest registration owns control/state until it disconnects.
- CLI WebSocket reconnects with a 2s delay for 10 consecutive failures, then keeps reconnecting every 60s. Pairing is never abandoned, so a server redeploy or a network change cannot leave a live SSH session permanently unpaired. A connection that held for 60s (`STABLE_CONNECTION`) clears the failure count, so one long session's worth of rare drops never accumulates into a give-up. Policy lives in `ws.rs`'s `PairRetryPolicy` (pure, tested in `ws_test.rs`); `main.rs`'s pair loop owns the sleeping, logging, and mute write.
- The pair WebSocket loop is selected alongside SSH session completion in the root async task, not spawned with `tokio::spawn`. This is intentional because native LiveKit voice room state is not guaranteed to be `Send` across desktop platforms.
- The first `client_state` is sent immediately after connect. It is resent after local mute/volume controls; source changes, voice, and video use their own handling (`voice_state`, `video_state`). The video probe runs once at startup, so a fixed setup is picked up on the next `late` launch.
- `/paste-image` in SSH chat depends on the paired CLI control channel. The server only sends `request_clipboard_image` after seeing `clipboard_image` in the latest paired client's `client_state.capabilities`, so older CLIs and the webview helper do not receive unsupported control events.
- Linux Wayland support for `/paste-image` depends on the workspace `arboard` dependency enabling `wayland-data-control`; Hyprland uses this path. Without it, the CLI may report that the clipboard does not contain an image even when Wayland has `image/png` content.
- Latency samples: the server times the whole trip to the user's terminal by following frames with a DSR status query (`CSI 5 n`) and waiting for `CSI 0 n` (`late-ssh/src/terminal_probe.rs`). The CLI passes both through untouched and splits the trip. `ssh_rtt` times `send_ping` (an SSH keepalive and its reply) every 10 s, the transport alone; `terminal` runs from the output loop reading the chunk that carries the query to the stdin thread reading the answer, the user's terminal alone, including any frame still being parsed ahead of it (`TerminalProbeWatch`). Both go into a 16-sample queue (`latency_queue`) that the pair loop drains as `latency` events; a full queue (socket down) drops samples rather than waiting. Native mode only: `openssh` gives the terminal to an ssh child outright, and `old` (which relays it through a local pty) is not wired, so their queue stays empty. The server records them as `late_ssh_cli_latency_seconds{kind,platform}`, and a server older than the CLI logs a parse error per sample, so it must deploy first.
- Drain handoff: when the session's pod is being replaced, the server sends `reconnect` to a quiet session's native CLI, and it sends the same at once when the user presses `r restart` in the quit modal (rules in `late-ssh/src/app/handoff/CONTEXT.md`). `main.rs`'s `run_native_session` dials and authenticates the new pod and opens the new shell there (`LATE_RESUME=<resume>` set before the pty request, output held) with the old session still on screen, and only then retires the old shell and shows the new one. The old pair socket keeps running through all of that and closes only once the move holds, so a move that fails at any step leaves the old session, its pairing and its update banner in place (a fresh pair socket on the old token would reach only the new pod and its 404, `SCALE.md` Pain Point 10). The local audio runtime is not touched, but the YouTube helper belongs to the pairing: it stops with the old pair loop and the new one starts it again, seeking to the server's position. Pairing restarts on the new token with `PairRetryPolicy::after_handoff`, which never releases the boot mute since the old server already aligned this process's audio. A second `reconnect` while one is underway is dropped.
- Clipboard images are converted to PNG in the CLI before upload. The CLI rejects zero-size images, very large decoded RGBA buffers, and PNG payloads above the upload cap before sending them over the pair socket.
- A file copied in Finder, Explorer or a Linux file manager puts paths on the clipboard, not pixels. When the clipboard has no image, the CLI reads its file list and decodes the first file as an image (any format the `image` crate reads, judged by its bytes), under the same caps plus a 64 MiB file-size cap checked before decoding (`clipboard.rs`, `png_from_file`).

## Embedded YouTube helper window

- The helper opens a minimal 200x200 non-resizable, undecorated webview window while the user source is YouTube. On Linux the helper is the standalone `late-webview` binary installed next to `late` (`LATE_WEBVIEW_BIN` overrides, `$PATH` is the last fallback); on Windows/macOS it is `late webview-pair`, the `late` binary re-executed with the webview compiled in. `late webview-pair`/`late webview-spike` on Linux just forward to the external binary.
- Linux invariant: `late` itself must never link WebKitGTK/GTK. A machine without the webview libraries has to run the full CLI; only embedded YouTube playback may degrade. Missing helper binary or missing WebKitGTK/GStreamer libraries fail the helper spawn/startup and land in the crash-loop backoff below — installing the distro packages later fixes playback with no CLI reinstall. The helper page gives the YouTube iframe the full viewport, disables YouTube's visible controls, and does not draw app UI over the player.
- The helper page is served from a loopback listener but loaded as `http://localhost:<port>/`, sends `Referrer-Policy: strict-origin-when-cross-origin`, and passes `window.location.origin` as the YouTube IFrame `origin`.
- By default the parent redirects helper stderr to the webview log path. For a single combined debug capture, run `LATE_WEBVIEW_DEBUG_STDERR=1 late -v 2>late-debug.log`; this captures both parent CLI tracing and helper GTK/WebKit/GStreamer output.
- The normal helper spawn sets `NO_AT_BRIDGE=1` and, on Linux, sets `WEBKIT_DISABLE_DMABUF_RENDERER=1` unless the user already set it. If `late webview-spike ...` is run directly during debugging and crashes in `libatk-bridge-2.0.so` after `dbind-WARNING`, retry as `NO_AT_BRIDGE=1 late webview-spike <video_id>` or restart stale `at-spi-bus-launcher` processes.
- The helper's pair WebSocket reconnects on drops (2s delay, up to 10 consecutive failures, counter reset once a connection has lived 60s) instead of exiting the process, so server redeploys and network blips do not tear down the window or reset mute/volume. The helper process exits only on window close, a real crash, or a reconnect give-up.
- While the source is YouTube, the parent checks the helper child from the pair-WS 1s heartbeat (`maintain_helper`) and respawns it if it died, honoring the crash backoff below. Respawn must not depend on `set_playback_source` replays: the server can miss the helper's disconnect entirely on a half-open TCP drop and then never replays.
- At spawn the parent passes its current mute/volume via `LATE_WEBVIEW_INITIAL_MUTED` / `LATE_WEBVIEW_INITIAL_VOLUME` (internal parent-to-helper env, not user config); the helper seeds its audio settings from them and pushes them into the page on the page's `ready` event. Server-side, `api.rs` aligns a connecting webview client's mute to the live CLI entry's muted state instead of `start_with_music_muted`, so a respawned or reconnected helper keeps the session's runtime mute.
- If the embedded helper exits or fails to start 3 times within 60 seconds, the parent disables embedded YouTube fallback for 5 minutes and logs the helper log path. This prevents the repeated open/close loop when a host WebKit/GStreamer install is broken. Nothing takes over: YouTube is simply unavailable in the CLI until the helper works, and users can listen at late.sh/listen meanwhile. After the backoff expires the heartbeat watchdog retries automatically.
- The helper requests no initial focus, always-on-bottom placement, and an initial top-right position on the primary monitor; on Linux it also skips the taskbar. These are best-effort window-manager hints, not a hidden/background player. On Linux/Wayland the app id/class is `sh.late.youtube`; Hyprland may ignore always-on-bottom or client-side positioning, so users who need stronger routing should use a special workspace/scratchpad instead of relying on fully off-screen placement.
- On Linux the spawn also sets `PULSE_PROP` and `PIPEWIRE_PROPS` (`application.id=sh.late.youtube`, `late.webview.owner=<late pid>`), so the helper's WebKit audio streams can be found and recorded for the equalizer. The capture (`src/audio/loopback.rs`) lives in `RunningHelper` beside the helper child and stops with it. See `CONTEXT.md` §7 and `late-webview/CONTEXT.md`, "YouTube spectrum".
- On initial helper open only, `webview-pair` uses the first `queue_update.current.started_at_ms` snapshot to apply one `startSeconds` value to the first matching `load_video`. If a `load_video` arrives before that first snapshot, the relay buffers it and flushes it when the snapshot decision is known. After that first load is dispatched, heartbeats and later track switches do not receive a seek offset and continue through the normal `loadVideoById({ videoId })` path.
- The helper page suppresses transient YouTube IFrame `unstarted`/`cued` states and only reports `ended` after the current item has reached `playing`; the server still owns queue advancement through its playback timer.
- If YouTube rejects the embedded iframe with `101`, `150`, or `153`, the helper logs the rejection and stays on its controlled bridge page. It does not navigate to the normal `youtube.com/watch` page because that would leave the local player bridge and make source switching/state harder to reason about.
