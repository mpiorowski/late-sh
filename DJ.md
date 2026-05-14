# DJ / Shared Music Sources

This document captures the plan for allowing new music sources on late.sh
without pretending that consumer music services can be rebroadcast through the
global Icecast stream.

## Goal

late.sh needs ways for users to bring music into the clubhouse beyond a static
curated local playlist, while keeping the project legally and operationally
safe.

There are two separate product surfaces:

- **Shared radio stream:** audio goes through Liquidsoap -> Icecast -> browser
  pairing / `late` CLI audio.
- **Provider player rooms:** late.sh controls queue/presence/sync, but the
  provider's official player serves media directly to each listener.

Do not blur these. YouTube, Spotify, Apple Music, SoundCloud embeds, and similar
consumer services must not be piped into Icecast.

## Hard Rules

These rules apply to audio that late.sh hosts or rebroadcasts. The YouTube
media-room path described in `AUDIO.md` is out of scope of these rules; in that
path, late.sh sends only metadata and the official YouTube iframe player
delivers audio directly to each listener's browser.

Never allow these as DJ Booth source audio:

- Spotify, Apple Music, Tidal, Deezer, YouTube, or other consumer service audio.
- `yt-dlp`, ripping, recording, extracting, or restreaming platform audio.
- Discord music bots that rebroadcast YouTube/Spotify/etc.
- Hidden/background YouTube playback.
- Any source that strips ads, branding, controls, links, or normal provider
  behavior.

Allowed for the Icecast stream:

- Original music by the DJ.
- CC0, CC-BY, public-domain, or similarly permissive tracks.
- Tracks with explicit written permission for public internet broadcast.
- Licensed DJ pool/library material that allows internet broadcast.
- Commercial music only if late.sh has the required webcasting/public
  performance licenses and reporting flow.

## Live DJ Booth

Live DJ Booth is the no-browser-compatible path because it feeds the existing
Icecast stream. Users can listen through the browser pairing page or `late` CLI.

### Technical Shape

Liquidsoap should expose a live source input and fall back to the normal voted
playlist when no DJ is connected.

```liquidsoap
live = input.harbor("stage", port=8081, password="ephemeral-token")
playlist_radio = switch(track_sensitive=false, [
  ({vibe() == "lofi"}, lofi_safe),
  ({vibe() == "classic"}, classic_safe),
  ({vibe() == "ambient"}, ambient_safe),
  ({vibe() == "jazz"}, jazz_safe),
  ({true}, lofi_safe),
])
radio = fallback(track_sensitive=false, [live, playlist_radio])
```

A DJ can connect with a normal Icecast/source-client tool such as Mixxx, BUTT,
ffmpeg, OBS, or another Liquidsoap process.

### Required Product Controls

Before a DJ receives stream credentials, require:

- Rights attestation: "I have rights to publicly broadcast this audio."
- Source type: original, CC/public domain, written permission, licensed library,
  or station-licensed commercial.
- Optional proof URL or license note.
- Optional artist/title metadata.

Runtime controls:

- Ephemeral stream password/token.
- One active DJ per booth or explicit queue/slot scheduling.
- Max session length.
- Admin/mod kill switch.
- Automatic fallback to normal radio on disconnect.
- Recording disabled by default.
- Permanent archiving only when rights explicitly allow it.

Audit log:

- DJ user id.
- Start/end time.
- Declared source type.
- Artist/title metadata if supplied.
- Proof/license notes.
- Mount/token id.
- Moderator actions.

## Synced YouTube Room

YouTube can work only as a browser/player-room experience. It cannot feed
Icecast, and it cannot play through `late` CLI audio unless the CLI opens a
visible webview containing the official YouTube player.

The browser page is the player. The SSH TUI is the remote control.

```text
SSH TUI
  -> search / queue / vote / skip over late.sh API or WebSocket
late.sh server
  -> authoritative room state and timeline
browser page
  -> official YouTube iframe player
YouTube
  -> actual audio/video delivery to each listener
```

### User Experience

TUI:

```text
YouTube Room
Now: HOME - Resonance
Channel: Electronic Gems
03:12 / 03:32
Listeners: 14 synced

[a] add  [/] search  [n] skip  [p] pause  [q] queue
```

Browser:

- User opens `https://late.sh/youtube/main` or a paired
  `/connect/{token}/youtube` URL.
- Page auto-connects to the YouTube room WebSocket.
- Page receives the current room state and loads the official iframe player.
- If audible autoplay is blocked, show a single "Join audio" button.
- After that first user gesture, server-driven video changes can continue.

This is still a browser surface, but it can feel automatic once joined.

### CLI Companion Role

The CLI is useful for YouTube rooms, but not as the audio decoder. For YouTube,
`late` should be the launcher, pairer, controller, and fallback manager.

When the user runs one command:

```bash
late
```

the CLI can:

- start the SSH session,
- receive the session token,
- pair through `/api/ws/pair`,
- open the YouTube player surface automatically,
- reconnect that player if it dies,
- pass local commands/control state,
- fall back to normal Icecast audio,
- show/copy/QR the paired URL if GUI/browser opening fails.

Hard limit:

```text
CLI cannot legally/usefully do YouTube -> decoder -> local audio.
```

The actual media surface must remain an official visible YouTube player.

### YouTube Mini-Player

The polished path is a small floating desktop window, launched by the CLI, that
loads the paired YouTube room page.

```text
late CLI
  starts SSH
  gets session token
  starts normal /api/ws/pair
  launches late-youtube-player sidecar

late-youtube-player
  opens a native WebView window
  loads https://late.sh/connect/{token}/youtube
  contains official YouTube iframe
  sends player state back through the web page WS
```

This is not part of the terminal. It is a normal local GUI window. The terminal
stays the clubhouse/control surface; the floating window is the legal media
surface.

Suggested default window:

```text
+------------------------------------------------+
| late.sh YouTube                          _ [] x |
+------------------------------------------------+
|                                                |
|        official YouTube embedded player        |
|                                                |
+------------------------------------------------+
| Connected · synced with #youtube-main          |
| HOME - Resonance · 03:12 / 03:32               |
+------------------------------------------------+
```

Suggested behavior:

- Small default size, around `480x360`.
- Resizable.
- Remember last size and position when the platform permits it.
- Close when `late` exits, unless detached mode is enabled.
- Reconnect on WebSocket drop.
- Surface clean errors for autoplay blocked, embedding disabled, unavailable
  video, region restrictions, or age/login requirements.

First-run flow:

1. User runs `late`.
2. SSH TUI opens as usual.
3. User joins YouTube room or presses "open player".
4. CLI opens the mini-player.
5. Mini-player auto-connects to the room.
6. Mini-player may show "Click to enable audio".
7. User clicks once.
8. Future server-driven video changes can continue automatically.

The tricky distinction:

- **Auto-open** is practical: `late` can open the player URL/window.
- **Auto-play audible YouTube** is not guaranteed: browsers and webviews may
  require one user gesture before audio starts.

### WebView Sidecar Notes

Prefer a sidecar process over embedding WebView in the main terminal process.
The current CLI owns terminal raw mode, SSH lifecycle, local Icecast audio, and
pairing. Desktop WebView toolkits often want their own GUI event loop. Keeping
the player as a sidecar avoids fighting terminal state and platform GUI rules.

Possible implementation:

```text
late --youtube-player=auto
late --youtube-player=always
late --youtube-player=never
late --youtube-url
```

Suggested sidecar binary:

```text
late-youtube-player --url https://late.sh/connect/{token}/youtube
```

Technology options:

- **Wry:** low-level Rust WebView wrapper. Good for a single trusted URL and a
  tiny native window.
- **Tauri:** heavier application shell with packaging, permissions, icons,
  tray/update options, and sidecar management.

Wry/Tauri use system webviews:

```text
Windows -> Edge WebView2
macOS   -> WKWebView
Linux   -> WebKitGTK
```

This means WebView is polished but not universal. Things that can fail:

- Linux system has no WebKitGTK runtime or media codecs/GStreamer plugins.
- Headless SSH/container has no `WAYLAND_DISPLAY` or `DISPLAY`.
- Older Windows install needs WebView2 runtime.
- Linux WebKitGTK behaves differently from Chrome/Firefox for YouTube.
- Wayland compositors may tile the window unless the user configures a float
  rule.

On Wayland, the sidecar is still viable because it only needs to create a normal
local app window and play audio through the desktop audio stack. It should not
depend on screen capture, fake input, global key capture, or forcing exact window
placement.

Set stable window identifiers so tiling users can manage it:

```text
app_id: late-youtube-player
title: late.sh YouTube
```

### Universal Browser Fallback

There is no single opener that works everywhere. The robust approach is a
fallback chain:

```text
1. Try WebView mini-player.
2. Try default browser via Rust `webbrowser` crate.
3. Try platform opener.
4. Copy URL to clipboard if possible.
5. Print URL and show QR in TUI.
```

Rust default-browser option:

```rust
webbrowser::open("https://late.sh/connect/TOKEN/youtube")?;
```

Platform fallback examples:

```text
macOS:   open <url>
Linux:   xdg-open <url>
GNOME:   gio open <url>
WSL:     wslview <url>
Termux:  termux-open-url <url>
Windows: ShellExecute/default URL handler
```

If GUI open fails, the UX should still be clear:

```text
YouTube player URL:
https://late.sh/connect/abc123/youtube

Copied to clipboard. Scan QR from the TUI if opening failed.
```

### Terminal Browser Mode

There are terminal browsers, but most are not useful for YouTube:

- `lynx`
- `w3m`
- `links` / `elinks`

These generally lack the full JavaScript/video/browser stack needed for YouTube
iframe playback.

Interesting power-user options:

- **Browsh:** terminal UI backed by headless Firefox.
- **Carbonyl:** Chromium rendered into a terminal; reports support for modern
  web APIs, audio, video, and running through SSH.

Potential command:

```bash
carbonyl https://late.sh/connect/{token}/youtube
```

This is still a browser engine, just terminal-rendered. It should be optional,
not default.

Reasons not to default to terminal-browser mode:

- Not commonly installed.
- Not as reliable as Chrome/Firefox for YouTube.
- It may fight with the SSH TUI because both want full terminal control.
- It likely needs a second terminal window, tmux pane, or split.
- YouTube playback, ads, codecs, login, and autoplay can still fail.
- Audio still comes from the local machine, not the remote SSH server.

Possible future flag:

```text
late --youtube-player=terminal
```

Fallback order for that mode:

```text
1. If `carbonyl` exists, launch Carbonyl with paired URL.
2. Else if `browsh` exists, launch Browsh with paired URL.
3. Else fall back to WebView/browser/URL.
```

### Server State

Suggested room state:

```text
room_id
current_video_id
current_title
current_channel
duration_ms
started_at_server_ms
paused_at_offset_ms
playback_state
queue[]
last_sequence
```

The server is authoritative. Clients do not decide what is playing globally.

### WebSocket Messages

Server -> browser:

```json
{
  "type": "room_state",
  "room_id": "main",
  "video_id": "abc123",
  "title": "Track title",
  "channel": "Channel",
  "duration_ms": 212000,
  "started_at_ms": 1770000000000,
  "playback_state": "playing",
  "sequence": 42
}
```

```json
{
  "type": "load",
  "video_id": "def456",
  "started_at_ms": 1770000212000,
  "sequence": 43
}
```

```json
{
  "type": "seek",
  "offset_ms": 90000,
  "sequence": 44
}
```

Browser -> server:

```json
{
  "type": "client_state",
  "video_id": "abc123",
  "player_state": "playing",
  "offset_ms": 94500,
  "muted": false,
  "autoplay_blocked": false
}
```

### Sync Algorithm

On join or room update:

```text
offset_ms = server_now_ms - started_at_server_ms
```

Then call the YouTube IFrame API:

```js
player.loadVideoById({
  videoId,
  startSeconds: Math.floor(offset_ms / 1000),
})
```

Periodically compare player time with expected room time. If drift is small,
ignore it. If drift is large, seek. If autoplay is blocked, surface a join
button and keep the WebSocket connected.

### Queue And Auto-Switch

The server auto-switches when:

- the active video reaches its duration,
- the browser reports `ended`,
- a moderator skips,
- a vote-skip threshold is reached,
- the current video errors or becomes unavailable.

Before queueing a video, validate it with YouTube Data API:

- `videos.list(part=snippet,contentDetails,status&id=...)`
- reject missing videos,
- reject `status.embeddable == false`,
- record title, channel, duration, thumbnails, and region/age errors when known.

### TUI Controls

Potential chat commands or room shortcuts:

```text
/youtube open
/youtube search <query>
/youtube add <url-or-video-id>
/youtube queue
/youtube skip
/youtube pause
/youtube resume
```

Keyboard shortcuts can be added once the room UI exists.

### CLI Behavior

The current `late` CLI cannot legally decode YouTube audio directly. If we want
a no-browser-tab experience, the CLI can launch a small visible webview that
hosts the official YouTube iframe page. That is acceptable only if the player is
visible and behaves like the normal embedded player.

## Open-License Importer

Separate from live DJ and YouTube rooms, users can submit open-license tracks.
This should feed the Icecast stream only after validation.

Minimum requirements:

- Accept source URL and declared license.
- Fetch metadata from supported sources where possible.
- Allow by default: CC0, CC-BY, public domain, direct permission.
- Reject or require admin review: NC, ND, unclear, all-rights-reserved.
- Store attribution/proof.
- Add approved files to a moderated playlist or request queue.

## Legal / Policy References

- YouTube API developer policies:
  https://developers.google.com/youtube/terms/developer-policies
- YouTube IFrame Player API:
  https://developers.google.com/youtube/iframe_api_reference
- YouTube Data API `videos.list`:
  https://developers.google.com/youtube/v3/docs/videos/list
- YouTube `status.embeddable` field:
  https://developers.google.com/youtube/v3/docs/videos
- Browser autoplay behavior:
  https://developer.mozilla.org/en-US/docs/Web/Media/Guides/Autoplay
- Spotify public/commercial use:
  https://support.spotify.com/article/spotify-public-commercial-use/
- Spotify user guidelines:
  https://www.spotify.com/us/legal/user-guidelines/
- Discord copyright policy:
  https://support.discord.com/hc/en-us/articles/4410339349655-Discord-s-Copyright-IP-Policy
- SoundExchange licensing overview:
  https://www.soundexchange.com/service-provider/licensing-101/
- U.S. Copyright Office sections 112/114:
  https://www.copyright.gov/licensing/sec_112.html

## Recommended Build Order

1. Build Synced YouTube Room as a browser-only provider room.
2. Add TUI queue/search/control surface.
3. Add YouTube Data API validation and metadata caching.
4. Add browser autoplay-block handling and drift resync.
5. Build Live DJ Booth for rights-attested Icecast input.
6. Add Open-License Importer for user-submitted tracks.

The fastest legally useful path is the Synced YouTube Room. The fastest
no-browser-compatible path is Live DJ Booth with strict source rules.
