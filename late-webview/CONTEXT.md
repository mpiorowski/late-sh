# late-webview Context

The helper process that hosts the official YouTube IFrame Player for the paired `late` CLI. The server side of YouTube playback (queue, sources, booth) is `../late-ssh/src/app/audio/CONTEXT.md`; the CLI side of pairing is `../late-cli/PAIRING.md`.

## YouTube in the CLI: the embedded webview

Legal YouTube playback inside the `late` CLI without shelling out to mpv/yt-dlp/etc. The CLI hosts the official YouTube IFrame Player inside an embedded system webview; the player fetches and decodes audio identically to the `/listen` page (`../late-ssh/src/app/audio/CONTEXT.md` §9). late.sh still ships only `video_id` over the pair WS.

### Process model

- Native `late` remains the always-on SSH/audio control process.
- Native `late` opens the normal pair WS as `client_kind = "cli"`.
- Native `late` advertises `capabilities: ["clipboard_image", "youtube"]` on desktop platforms.
- `set_playback_source: youtube` spawns a helper child (token on stdin). On Linux the helper is the standalone `late-webview` binary (resolved via `LATE_WEBVIEW_BIN`, then a sibling of the `late` executable, then `$PATH`); on Windows/macOS it is `late webview-pair`, the same binary re-executed with the webview compiled in.
- The webview code lives in the `late-webview` crate. On Linux `late` must NOT link WebKitGTK/GTK — the whole point of the split is that a host without the webview libraries can still run the CLI. Missing helper binary or missing WebKitGTK/GStreamer libraries surface as helper start failures and land in the crash-loop guard below; the CLI session, radio, and icecast keep working.
- Nothing displaces the helper. If the embedded webview stack fails on a machine, YouTube is unavailable in that CLI and the user listens at `/listen` instead; there is no automatic handoff.
- `set_playback_source: icecast` kills the helper and resumes native Icecast.
- The helper opens its own pair WS and reports `client_kind = "webview"`.
- The helper's pair WS reconnects on drops (2s delay, give up after 10 consecutive failures, counter reset once a connection has lived 60s) instead of exiting the process. Helper exits therefore mean a real crash or a persistent server outage, not a routine redeploy/network blip, and the window position plus mute/volume survive reconnects.
- The parent also runs a 1s heartbeat watchdog (`WebviewPlaybackController::maintain_helper`): while the source is YouTube it reaps and respawns a dead helper, honoring the crash-loop backoff below. Do not rely on `set_playback_source` replays for respawn — the server can miss the helper's disconnect entirely on a half-open TCP drop and then never replays.

This lazy lifecycle is intentional. A normal CLI run does not open a webview. A webview window exists only while the user's persisted playback source is YouTube, avoiding tiling-window-manager noise for Icecast and Radio users.

### Source semantics

`set_playback_source` is the user's per-user preference and is the only signal that starts/stops the helper. `source_changed` is global queue/server mode and must not spawn the helper by itself. A user pinned to Icecast can still receive `source_changed: youtube` because the shared queue/fallback is globally active.

### Webview backend

The `late-webview` crate uses `wry` + `tao` (a library embedded into `late` on Windows/macOS, a standalone binary on Linux):

- Linux: WebKitGTK 4.1 dev/runtime packages plus GStreamer playback plugins. On Arch/EndeavourOS, `gst-plugins-good` is required for `autoaudiosink`; without it WebKit logs `GStreamer element autoaudiosink not found` and the YouTube iframe can remain black/unstarted even though pair/load events succeeded. `gst-libav` is also recommended for codec coverage.
- macOS: WKWebView.
- Windows: WebView2.

The helper serves `late-webview/src/page.html` from a loopback-only ephemeral HTTP listener and loads it as `http://localhost:<port>/` in the webview. Do not switch this back to `WebViewBuilder::with_html`: Wry's HTML string path gives the page a null origin, and YouTube can reject the iframe with player error 153. Do not expose the page URL as `http://127.0.0.1:<port>/` either: a real incident with `r6L-GUOAhGo` showed YouTube IFrame error 150 / "Video unavailable / Watch on YouTube" from the CLI webview while the same controlled helper worked after changing the page URL to `localhost`. The server also sends `Referrer-Policy: strict-origin-when-cross-origin`, the page declares the same policy in a `<meta name="referrer">`, and the page passes `window.location.origin` into the IFrame Player `origin` parameter. The page posts `player_state` back through wry IPC, and Rust relays those events to `/api/ws/pair` while pushing `load_video` / `source_changed` into JS via `evaluate_script`. The helper window is 200x200, gives the iframe the full viewport, disables visible YouTube controls, and does not draw any app overlay on top of the player. The helper suppresses transient `unstarted`/`cued` reports and ignores `ended` until the current item has first reached `playing`, because the IFrame can emit startup/teardown states during rapid loads. Even a valid `ended` report does not advance the queue; the server timer does.

The helper owns its own mute/volume state, starting at the same 30% default as native CLI Icecast. It registers as a browser with `ssh_mode = "webview"`, so pair-WS `toggle_mute`, `volume_up`, and `volume_down` controls must be applied inside `late-webview/src/pair.rs` and forwarded into `page.html`; changing only the native CLI Icecast atom is not enough because YouTube audio is emitted by WebKit/GStreamer.

Helper mute is session-sticky across respawns and reconnects. At spawn the parent CLI passes its current mute/volume (its atomics track the same broadcast controls) via `LATE_WEBVIEW_INITIAL_MUTED` / `LATE_WEBVIEW_INITIAL_VOLUME`; the helper seeds its audio settings from them and pushes them into the page on the page's `ready` IPC event, before the first `load_video` plays. Server-side alignment is `api::align_paired_audio`; see below.

### Mute and volume: one source of truth, stored per device

`user_ssh_keys.settings` holds this device's `audio_muted` + `audio_volume_percent` (`late-core/src/models/user_ssh_key.rs`, `KeyAudio`), and that row **is** the mute and volume. Both live on the key rather than the account because they belong to the machine with the speakers: muting a laptop must not silence the desktop. Same store and same "account default, per-device override" shape as the home rails. There is deliberately no second control anywhere: `users.settings.start_with_music_muted` is only a **read-time seed** for a device that has never reported audio. Once a device reports once, that account value is never read again.

The write path is one place: `api.rs`'s `client_state` handler persists what a paired client reports, with `api::PairAudioFlow` (pure, tested in `api_internal_test.rs`) deciding what each report means. The CLI re-sends `client_state` after applying any control (`late-cli/src/ws.rs`, `should_send_state`), so `m`, `+`/`-`, and a media key all land in the same write with no per-keybind plumbing and no special cases. Three classes of report are never persisted: anything from the webview helper (its volume-up clears mute while the CLI's does not, and the CLI is the surface of record, so a helper report must not overwrite it; `ClientKind::Unknown` is an older CLI and persists like one), reports *before* the alignment (the client is still on its own boot defaults, which is not intent), and the reports *echoing* the alignment itself (the client re-reports once per control applied; persisting those would write a transiently wrong value for a restore that changed nothing). `PairedClientRegistry::claim_audio_write` gates the rest, so the CLI's periodic heartbeats do not rewrite an unchanged row and a failed write retries on the next report; the DB write is awaited on the socket task so a token's writes land in report order.

The connect-time read (`api::read_device_audio`) distinguishes a failed read from an empty one: on a DB error the whole connection runs with alignment and persistence off, so a transient failure can neither impose fresh-boot defaults on the session nor overwrite the stored row with them; the next connection retries. The read path aligns a connecting client to the stored value, and that alignment is claimed **once per SSH session token**, not once per WebSocket (`audio_alignment_pending` / `note_alignment_applied`, retired by `forget_session` from `Drop for App` and from `ClientHandler::drop` when no App was ever built). One token is one CLI process, which is the scope the stored value describes. A mid-session pair-WS reconnect (a network change, an ingress restart, a webview helper respawn) instead reports the state the session is already running with and is left alone; re-imposing the stored value there would unmute a muted session behind the user's back.

`api::align_paired_audio` is the pure decision and returns both halves of the push. **Order matters and is load-bearing: volume goes first, because a non-zero `SetVolume` also clears mute on the CLI and on the helper**, so the mute half is decided against the state *after* the volume write. Without that, restoring a stored (muted, 60%) would come back audible. A webview helper additionally prefers the live CLI entry's mute (`cli_muted`), since the CLI takes the same controls and is the session's surface of record while YouTube plays.

Two consequences worth knowing. A session whose SSH key is unknown (`fingerprint_for` returns `None`) has no device identity, so its audio is memory-only for that session and nothing is stored. And two `late` processes on one machine share a fingerprint, so the last one to change mute or volume wins the row.

The CLI holds up its end of this when the socket dies. Its pair-WS retry loop (`late-cli`'s `PairRetryPolicy`, `../late-cli/PAIRING.md`) may release its boot mute only when the server never registered the session at all; once the server has sent the session a frame (which it does right after `register`, before reading the buffered `client_state`), the running mute is the user's and a reconnect outage leaves it alone. A socket this handler accepts and then drops unread (the per-IP pair limit and the per-token capacity are both checked after the upgrade) never sends a frame, so the CLI still counts it as not established. Silence is the safe failure mode: unmuting because the socket dropped would start music mid-SSH in a muted session. The loop also never abandons pairing, it slows to a 60s retry, so a redeploy cannot strand a live session unpaired and uncontrollable, and a session that did release its boot mute still receives the stored value on the reconnect, since the once-per-token alignment was never claimed.

### Runtime support / troubleshooting

This feature is a real browser media stack inside a tiny helper process. Pair-WS protocol bugs show up in server logs; webview/browser/runtime bugs show up first in the per-user helper log (`$XDG_STATE_HOME/late/webview.log` or `~/.local/state/late/webview.log` on Unix, `%LOCALAPPDATA%\late\webview.log` on Windows). Interactive `late --verbose` parent logs go to the parent CLI log (`$XDG_STATE_HOME/late/late.log` or `~/.local/state/late/late.log`) so tracing does not corrupt the TUI; set `LATE_LOG_STDERR=1` for old stderr behavior.

- **Manual fallback:** if the embedded webview fails on a machine, open `/listen` in any browser. That is a plain listener, not a handoff: the CLI does not learn about it and simply has no YouTube until the helper works again.
- **Arch/EndeavourOS + Wayland/Hyprland is proven** with WebKitGTK 4.1 plus GStreamer plugins. Known host package set:
  `sudo pacman -S --needed webkit2gtk-4.1 gst-plugins-good gst-libav`.
- **DMABUF renderer failures:** the Linux helper now sets `WEBKIT_DISABLE_DMABUF_RENDERER=1` unless the user already provided a value. This is intentionally scoped to the helper process because some WebKitGTK/Wayland stacks fail or hang on the DMABUF renderer path.
- **Crash loop guard:** if the helper exits or fails to start 3 times within 60 seconds, the native CLI disables embedded YouTube fallback for 5 minutes and logs the helper log path. This stops repeated open/close loops while preserving the normal browser connect fallback.
- **Hyprland window routing.** The helper requests an undecorated 200x200 window, no initial focus, always-on-bottom placement, and initial top-right placement. Wayland compositors may ignore client-side positioning, so the app id/class is still `sh.late.youtube` for float/scratchpad rules:
  `windowrulev2 = float, class:^(sh.late.youtube)$`,
  `windowrulev2 = size 200 200, class:^(sh.late.youtube)$`,
  `windowrulev2 = move 100%-200 0, class:^(sh.late.youtube)$`,
  `windowrulev2 = workspace special:late silent, class:^(sh.late.youtube)$`,
  plus a bind like `bind = SUPER, Y, togglespecialworkspace, late`.
- **Linux X11** should be less fragile than Wayland because Wry's raw-handle path supports X11, but we still use the GTK builder on Linux so one code path covers both. WebKitGTK/GStreamer packages remain the main risk.
- **Ubuntu/Debian/Fedora** are expected to work once package names and WebKitGTK versions line up. Older distros may not ship the WebKitGTK 4.1 stack this branch expects.
- **NixOS** should use the flake package, not a random Linux binary. Required runtime/build inputs are `webkitgtk_4_1`, `pkg-config`, `glib-networking`, and GStreamer packages (`gstreamer.out`, `gst-plugins-base/good/bad/ugly`, `gst-libav`). The package builds `gst-plugins-bad` with `-Dlv2=disabled`, wraps `late` and `late-webview` with a fixed `GST_PLUGIN_SYSTEM_PATH_1_0`, `GST_PLUGIN_SCANNER`, `GIO_EXTRA_MODULES`, and `LATE_WEBKIT_GSTREAMER_SANDBOX_PATHS`, and the Linux helper adds those GStreamer paths to WebKitGTK's web-process sandbox before creating the webview. If this fails, the supported fallback is `late.sh/listen` in any browser; nothing takes over automatically.
- **macOS** uses WKWebView and does not need GStreamer. Main risks are autoplay policy and ordinary macOS audio routing.
- **Windows** uses WebView2. Modern Windows usually has the runtime; the Windows volume mixer may expose the helper as its own app stream.
- **WSL/headless/container** are not supported unless there is a real desktop/webview runtime and working audio bridge.

Failure signatures:

- **No window:** check the per-user helper log. Past failures included Wayland raw-window-handle rejection and invalid GTK app id. Linux must use Wry's GTK build path (`build_gtk`) with Tao's `default_vbox()` container, and the GTK app id must remain valid reverse-DNS (`sh.late.youtube`).
- **White/blank window with GTK warning about `GtkApplicationWindow` already containing `GtkBox`:** the webview was mounted into the GTK window instead of Tao's default vbox. Use `window.default_vbox()` as the GTK container.
- **YouTube error 150 with "Video unavailable / Watch on YouTube" in CLI webview:** first verify the helper page URL is `http://localhost:<port>/`, not `127.0.0.1`, and that the response/meta referrer policy is `strict-origin-when-cross-origin` while `playerVars.origin = window.location.origin`. Some 150/101 failures are still true YouTube embed-policy rejections and will only work in the normal browser/YouTube surface.
- **YouTube error 153:** the IFrame Player rejected embed identity. The page must load from loopback HTTP as `localhost`, pass `window.location.origin`, and keep the explicit referrer policy; do not use `with_html`.
- **Black/unstarted player:** often missing GStreamer plugins. `GStreamer element autoaudiosink not found` means `gst-plugins-good` is absent.
- **Video moves but no sound:** verify helper mute/volume handling first (`m`, `+`, `-` should hit `late-webview/src/pair.rs`, then `page.html`). If needed, click once inside the webview to satisfy an autoplay gesture. Also check the desktop mixer for a WebKit/late.sh stream.
- **First run plays through laptop speakers:** PipeWire/WirePlumber may treat the helper as a new app stream. Moving it once to headphones in the mixer usually teaches the session manager for later launches.

### Window UX

Current v1 opens a small undecorated companion window. Hidden/offscreen mode is not the default because embedded browser engines can throttle or unload hidden/minimized views, and the YouTube iframe's ads/branding/autoplay posture is cleaner with a visible surface. On compositors such as Hyprland, prefer routing the helper to a special workspace over manually parking it fully off-screen. A future config can add `youtube_webview = "window" | "hidden" | "disabled"` once hidden-mode behavior is validated per platform.

### What this does NOT change

- Server queue state machine and YouTube `load_video` protocol.
- `/listen` page behavior.
- Native stream decoder path when `audio_source = radio`.
- External-player shell-outs remain out of scope; do not revive mpv/yt-dlp handoff unless the product/legal posture changes explicitly.

---

## YouTube spectrum: OS capture of the helper's audio

**Status: Linux implemented; Windows and macOS not started.** Icecast and radio get real bars from the CLI's own decoded output (`../late-ssh/src/app/audio/VIZ.md`). YouTube plays in the `late-webview` helper's cross-origin iframe, so the CLI captures the helper's audio at the OS layer and runs it through the same analyzer (`late-cli/src/audio/loopback.rs`). Frames go out on the CLI's existing pair socket as ordinary `viz` events. The server is unchanged.

### Linux (PipeWire)

1. **Tag at spawn.** `WebviewPlaybackController` spawns the helper with `PULSE_PROP` and `PIPEWIRE_PROPS` setting `application.id = sh.late.youtube` and `late.webview.owner = <late pid>`. WebKitGTK plays through its PulseAudio sink (via pipewire-pulse), and the tag lands on `WebKitWebProcess`'s stream props (verified on Arch, PipeWire 1.6).
2. **Find.** A worker thread runs `pw-dump` once a second (about 11ms) and picks the `Stream/Output/Audio` node tagged with this owner: a running one before an idle one, then the newest `object.serial`. Re-reading every second is what picks up a stream WebKit replaces between videos.
3. **Record.** `pw-record --target <serial> --raw` (mono f32 at 44.1 kHz, with `node.dont-reconnect` and `node.dont-fallback` so a vanished stream never falls back to the microphone) pipes into a ring feeding its own `spawn_playback_analyzer_thread` on the shared `analyzer_tx`.
4. **Lifetime.** The capture lives in `RunningHelper` beside the helper `Child`, so it stops whenever the helper is stopped, dies, or is respawned.

Why these choices:
- **A tag, not PID matching.** For PulseAudio clients `pipewire.sec.pid` is `pipewire-pulse` itself, and `application.process.id` is client-reported (and namespaced if WebKit ever sandboxes its web process). The env tag is on the stream either way.
- **An owner key.** Two `late` sessions on one desktop each tag their own helper; without the owner each could record the other's player.
- **Shelling out to `pw-dump`/`pw-record`, not linking libpipewire.** Same rule as WebKitGTK: `late` must start on hosts without the library. Missing tools log one warning and the eq stays `Ambient`.
- **The stream, not the sink monitor.** A monitor would also pick up everything else the user plays.
- **A silence gate.** Chunks of exact digital zero (a paused or muted player) are dropped before analysis, so silence sends no frames and the TUI falls back instead of drawing flat live bars.

### Windows and macOS

Not started. The helper runs in-process there (`late webview-pair`), so candidates are per-process capture of `late` itself: WASAPI process loopback on Windows, ScreenCaptureKit audio filtered to the app on macOS (which prompts for permission). Until then YouTube uses the ambient band (`../late-ssh/src/app/audio/VIZ.md`) on those platforms.
