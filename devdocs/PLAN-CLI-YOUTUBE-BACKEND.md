# Plan: CLI YouTube player backend chain

Status: decided direction, not implemented. As the work lands, move shipped behavior into `late-cli/CONTEXT.md` (§3, §4, §6), `late-cli/DISTRIBUTION.md` and `late-webview/CONTEXT.md` and delete this file.

## Goal

Embedded YouTube on Linux should need nothing installed, and must stay within YouTube's ToS. Keep `late-webview` as a fallback, not the primary path.

## Problem

- WebKitGTK has no decoders of its own on Linux. All media goes through GStreamer, and the decoding and output plugins are optional distro packages. Users currently have to install `webkit2gtk-4.1 gst-plugins-good gst-libav`. Windows (WebView2/Edge) and macOS (WKWebView) do not have this problem.
- On Arch, `webkit2gtk-4.1` hard-depends only on `gst-plugins-base-libs` and `gst-plugins-bad-libs`; `gst-plugins-good`, `gst-plugins-bad`, and `gst-libav` are optional. The Nix build works only because the flake bundles plugins and sets `GST_PLUGIN_SYSTEM_PATH_1_0`.
- What YouTube needs from GStreamer (package ownership checked on Arch):

| Need | Plugin | Arch package |
|---|---|---|
| WebM container | `matroska` | gst-plugins-good |
| VP9 video | `vpx` | gst-plugins-good |
| MP4 container (fallback) | `isomp4` | gst-plugins-good |
| Audio output | `autodetect` + `pulseaudio` | gst-plugins-good |
| Opus audio | `opus` | gst-plugins-base (not base-libs) |
| H.264 / AAC | `libav` | gst-libav |

- `libwebkit2gtk-4.1.so` references `autoaudiosink` and `webkitaudiosink`, so without `gst-plugins-good` YouTube is silent even with every codec present.
- `gst-libav` is only the H.264/AAC path; if YouTube negotiates VP9/Opus it is never used. Unverified: that `base` + `good` alone plays YouTube in WebKitGTK. Test without uninstalling: `GST_PLUGIN_SYSTEM_PATH_1_0` pointed at a scratch dir of symlinks to only those plugins plus the `base-libs` ones, a fresh `GST_REGISTRY`, then `late webview-spike <video_id>`.

## Rejected options

- **Native audio extraction in the CLI** (resolve itag 140 AAC/MP4, decode with symphonia `aac` + `isomp4`, play through the existing pipeline; plumbing exists via `set_playback_source.stream_url` and `.m4a` support in `resolve_stream_url`). Best UX, but breaks YouTube ToS and the API Developer Policies: downloading, separating audio from video, background play without a visible player, and skipping ads. The current 200x200 window exists because 200x200 is the policy minimum player size. Concrete risks:
  - Revocation of the Data API key that booth submissions need (`LATE_YOUTUBE_API_KEY`, required in prod). This is the real lever.
  - A cease and desist. Precedent: the Groovy and Rythm Discord music bots were shut down by Google in 2021, and Invidious got a C&D in 2023. A shared community queue resembles those bots more than personal yt-dlp use.
  - Extraction breaking every few weeks as YouTube changes signature and PO-token checks.
  - A server-side proxy is worse still: redistribution plus copyright exposure, bandwidth cost, and datacenter IP blocks.
  - If ever revisited: client-side only, opt-in, iframe stays the default, and prefer shelling out to the user's own `yt-dlp` over bundling extraction code (the mpv model). News watch (§6 "Video watch") is that shape for one person watching one shared link: the user's own `mpv` and `yt-dlp`, gated by the `video` capability, so one CLI release can switch it off. The booth queue is untouched and stays on the IFrame player.
- **Bundled GStreamer codec pack** (the 6 plugins above, statically linked against libvpx and libopus, built against an old GStreamer 1.x and old glibc, a few MB, shipped next to `late-webview`, loaded via `GST_PLUGIN_PATH_1_0` (additive, not `GST_PLUGIN_SYSTEM_PATH_1_0`, which replaces the system path), with `late-webview/src/lib.rs` sandbox path collection extended to that var). It would remove `gst-plugins-good` and `gst-libav` but not `webkit2gtk-4.1` (about 130 MB, a system engine that can't be vendored small). Still a valid improvement for the webview fallback later.
- **AppImage of WebKitGTK plus GStreamer**: 150 to 250 MB; WebKit helper processes, the bubblewrap sandbox, and mesa/GPU ABI make it fragile.
- **CEF or Electron**: self-contained Chromium and ToS-clean, but a 100 MB+ download, a rewrite, and Chromium's sandbox is blocked by AppArmor unprivileged-userns restrictions on Ubuntu 24.04+ without a shipped profile or disabling the sandbox.
- **Qt WebEngine** (needs the Qt stack), **Servo** (uses GStreamer on Linux, not production-ready), **Ultralight / Sciter** (cannot play YouTube).

## Decided design

Borrow a browser the user already has, falling back through a chain. Every option runs the official IFrame player in a real browser engine, so ToS stays clean.

```
Chromium family --app  ->  Firefox (own profile)  ->  late-webview  ->  point at late.sh/listen
```

1. **Chromium family** (`google-chrome`, `chromium`, `brave`, `microsoft-edge`, `vivaldi`):
   `<bin> --app=http://localhost:<port>/ --user-data-dir=<late profile dir> --window-size=200,200 --autoplay-policy=no-user-gesture-required`.
   `--app` gives a chromeless window. `--user-data-dir` is mandatory: without it the launch hands off to the user's running browser and exits immediately, which the watchdog would read as a crash. With it, `late` owns the child process, so the heartbeat watchdog and respawn keep working. The separate profile is logged out of YouTube, so ads show; acceptable and ToS-friendly.
2. **Firefox**: `firefox --profile <late profile dir> --no-remote <url>` starts an owned instance. Firefox bundles VP9/AV1/Opus decoding, so YouTube needs no system libs. No app mode; seed the profile on first launch with `user.js` (`media.autoplay.default=0`, skip welcome, default-browser check, telemetry) and a `userChrome.css` hiding tabs and toolbars. Window may be larger than 200x200 (minimum size not checked).
3. **`late-webview`**: current helper, kept as the fallback.
4. All backends exhausted: pause and point at late.sh/listen, as today.

**One bridge for every backend.** Today the page talks to Rust through wry IPC (`window.ipc.postMessage` in `page.html`, `evaluate_script` in `late-webview/src/lib.rs`). Replace both directions with one WebSocket from the page to the existing loopback page server (`lib.rs` already binds a `TcpListener` to serve `page.html`). Then:
- `late` runs the page server and the relay (`late-webview/src/pair.rs`, about 830 lines, plus `page.html`), one copy for all backends. The relay and page code must move somewhere wry-free so `late` never links WebKitGTK on Linux.
- Each backend only opens a window on the URL. `late-webview` shrinks to "show this URL in a 200x200 window".
- The relay keeps its own pair WebSocket as `client_kind: "webview"`, so the server protocol does not change.

**Detection and fallback:**
- Closed enum, no catch-all: `PlayerBackend { Chromium(PathBuf), Firefox(PathBuf), Webview(PathBuf) }`. Detection returns an ordered list.
- Look up browser binaries on `PATH`; snap browsers appear under `/snap/bin`; Flatpak browsers need `flatpak run <app id>`; webview keeps the existing `LATE_WEBVIEW_BIN` / sibling / `PATH` lookup.
- Crash backoff becomes per backend: 3 exits or failed starts within 60s moves to the next backend instead of disabling YouTube. Only when every backend is exhausted does the 5-minute pause apply.
- User choice of backend, wanted as a real setting, not only an env var. Values: `auto` (the chain above), `chromium`, `firefox`, `webview`. An explicit choice that fails to start should fall back to `auto` only if the user opts into that; otherwise report the failure. Where it lives is still open:
  - **TUI setting stored per device** (preferred). The backend depends on what is installed on that machine, so it belongs with the per-device settings row that already holds mute/volume (`user_ssh_keys.settings`, see `late-ssh/src/app/audio/CONTEXT.md` "Mute and volume: one source of truth, stored per device"), not on the account. The server would send it over the pair WS on connect and on change (a new server-to-client event), and the CLI could list the backends it detected in `client_state` so the setting only offers ones that exist.
  - **CLI config file / env** as a local override: `youtube-player` key in `config.toml`, `LATE_YOUTUBE_PLAYER` env, plus an explicit browser path. §4 says in-app preferences belong server-side, so this is the override for scripting and debugging, not the main surface.
- Report the active backend in `client_state`, so "YouTube doesn't work" reports show which path is in use.
- Windows can use Edge `--app` (always installed); macOS uses an installed Chromium browser if any. This could eventually retire wry on those platforms too.

**Gotchas:**
- Snap Chromium and snap Firefox are Ubuntu defaults and cannot write hidden dirs under `$HOME`, so a profile under `~/.local/share/late/...` fails. Pick a snap-reachable dir.
- Detected does not mean working (for example, an AppArmor-blocked Chromium sandbox). The per-backend backoff catches that, not detection.
- Tiling WMs (Hyprland) can route the `--app` window by class, same as the current `sh.late.youtube` helper.

**Cheap stopgap, independent of the chain:** when the webview helper hits its backoff, show the exact distro install command in the TUI instead of failing silently.

## Work order

1. Move the page/Rust bridge to the loopback WebSocket with the webview as the only backend. Proves the relay move with no new backends.
2. Add the Chromium backend.
3. Add the Firefox backend.
4. Add the per-backend fallback chain, override env var, and `client_state` backend label plus detected backends; update installer and docs.
5. Add the per-device "YouTube player" setting in the TUI and its pair-WS event.
