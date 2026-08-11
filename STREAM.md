# STREAM.md — "watch me" streaming rooms

Status: **built (2026-08-11), v1 shipped in the working tree.** This file
remains the design rationale; the living contract is
`late-ssh/src/app/stream/CONTEXT.md` (with the voice-scope exception
recorded in `late-ssh/src/app/voice/CONTEXT.md` §7 and the web pages in
`late-web/CONTEXT.md`). Where this doc and those diverge, trust the code
and the CONTEXT files. Notable v1 deltas from the text below: the rail got
a dedicated `stream` section (under Core, above Cyberspace/Channels) whose
rows appear at `/golive` time rather than first-media time — the #lounge
announcement still waits for media — and `/watch`/`/golive` URLs open via
the paired CLI's new `open_url` capability with a QR modal fallback.

## Why (and why now)

- 30-50 people daily: quiet, they play (arcade, lobby) and listen (radio),
  they don't talk much. What works here is ambient copresence.
- The flagship game bet (deadchannel, `GAME.md`, currently only on branch
  `mateu/roguelikes_leaderboards`, commit `0bde7d30`) is months of work.
  Streaming is the cheap bonding experiment we can run first: one to two
  focused weeks, riding infra that already exists.
- The terminal-native insight: video "twitch" doesn't fit terminals, but
  "come hang out while I do the thing" does. The stream is the excuse; the
  room around it is the product.
- Not wasted if it dies: the web viewer + token grants are the spectator
  infrastructure a future deadchannel arena fight card wants anyway, and a
  public watch page is a candidate "window into the house" acquisition
  surface later.

## The doctrine (why browsers are allowed back here)

The old "drop browser support" decision removed the browser as a *paired
client*: session-holding, state-drifting, arbitrating with the CLI (see
audio CONTEXT: removing browser pairing let source alone decide the
audible surface; voice tokens went CLI-only). It did NOT remove the
browser as a passive, token-less consumption surface: `/listen` survived
and is the model.

Applied here:
- **Watching** is passive consumption, `/listen`'s sibling. Plain browser,
  no pairing, no session. Gating it behind the CLI would kill most of the
  audience for zero architectural gain.
- **Publishing** happens through a one-shot token minted from the TUI for
  a single act (the pair-handshake shape, but stateless). The browser is a
  broadcast console for the duration of one stream, then a stranger again.
- **Voice stays CLI-only** as a system. The one scoped exception is the
  streamer's own broadcast console (see below), and it must be documented
  in `voice/CONTEXT.md` as exactly that when built: room owner only, own
  stream room only, ephemeral go-live token only. Viewers never publish.
  General browser voice remains a separate, unmade decision.

## The core architecture decision: ONE LiveKit room

The stream is **a video track published into a standard room's voice
channel**. No second media room, no bridging, no split-brain audio.

- `/golive` lazily creates the streamer's permanent stream room
  (`#<user>-live`: normal `chat_rooms` row + voice channel, the seeded-room
  shape the house tables already use) or reuses it. Chat history persists
  between streams; the room only *surfaces* while live.
- The room's LiveKit voice room carries every track: CLI participants'
  mics (existing voice system, untouched), the streamer's screen video,
  optional tab/system audio from the capture picker, and the optional
  browser-mic track (streamer only).
- CLI voice participants talk with the streamer through the normal voice
  path and hear tab audio as just another audio track. Web watchers hear
  the full conversation. Everything composes because it is one room.
- late-ssh never touches a media byte: it moves tokens, registry state,
  and one activity line. Media flows browser/CLI -> LiveKit -> browser/CLI.
  Same boundary discipline voice already established.

## The one rule that kills all client-detection

**Browser pages are born silent in both directions; a human clicks to open
each direction.** There is NO server-side CLI-vs-browser detection
anywhere; CLI-ness only determines which mic/speaker the human happens to
use, and the human resolves that by clicking or not.

| page | mic | speaker (room audio) |
|---|---|---|
| go-live page | toggle, off by default | muted by default |
| watch page | none (no grant) | muted by default |

Walkthrough of every case:
- CLI streamer (in voice via CLI): shares screen, touches neither toggle.
  Page is video-out only. No echo.
- Non-CLI streamer (incl. macOS, which has no CLI voice at all): clicks
  mic on + speaker on. Full-duplex broadcast console; they hear co-hosts.
- CLI viewer in voice: opens watch page for the video, leaves it muted,
  hears via CLI. (This is the double-audio gotcha, solved by the default.)
- Non-CLI viewer: opens watch page, clicks unmute, hears everything.
  Browser autoplay policy forces the muted-until-gesture shape anyway.

## Flows

### Streamer
1. `/golive <title>` from any composer (registration like `/shop`,
   `/pair`). Creates/reuses the stream room, registers in the live
   registry (in-process, scratchpad-registry shape: one stream per user,
   single replica, dies with the process).
2. Modal in the Pair-tab style: one-time publisher URL + QR ("open this in
   your browser and pick a window"). If a native CLI is paired, optionally
   auto-open (wry is a convenience shell here, never the requirement:
   WebKitGTK capture is the flaky path and capture never runs in wry).
3. The go-live page holds the publish token, calls `getDisplayMedia`,
   publishing starts. **Announcement fires only when the page reports
   media flowing** (via late-web -> API), never at command time: no "live"
   lines pointing at black screens.
4. Ending: close the tab / stop button. Publisher disconnect starts a
   ~30s grace timer (survives refresh), then the stream is down. No
   "stream ended" feed line (noise).

### The room (what going live triggers in-app)
1. One feed line to #lounge via the normal activity pipeline
   (`ActivityKind::WentLive`, one new arm in the lounge filter, rides the
   existing 30-min repeat throttle): "· mat is live: refactoring the
   render loop".
2. LIVE tag beside the streamer's name in chat author labels (award-badge
   / name-flair pipeline shape, resolved ~1/s in `App::tick`).
3. Conditional Home-rail row (Voice-entry shape), only while someone is
   live: `▶ #mat-live · <title> · 12 watching`. Opening it is opening a
   normal room: embedded chat, voice strip appears on its own (it already
   does for any surface with a voice channel). The only new UI is the
   stream header block above the chat: title, watcher count, watch-URL
   nudge (the nudge matters: a terminal-only person in the room is missing
   the show).
4. `/watch @user`: paired CLI gets an open-URL control message down the
   pair WS; raw SSH gets URL + QR modal.

### Viewer
- Watch page = `/listen`'s sibling: video element, subscribe-only token,
  title + streamer name, muted-by-default audio, nothing else. No web
  chat, no overlay: viewers are late.sh users with a composer already
  open, and pushing conversation into the terminal room is a feature.
- Watcher count via watch-page heartbeats -> registry -> rail row.

## Tokens and access

- Reuse the voice JWT helper (HS256, `LATE_LIVEKIT_API_SECRET`).
  - Go-live token: `canPublish` restricted to screen/video sources (plus
    mic if the streamer-mic exception ships); one-time URL; room owner
    only.
  - Watch token: subscribe-only (`canPublish=false`). Enforced at the SFU
    grant level: a tampered watch page still cannot open a mic. Viewers
    are physically eyes-and-ears only.
- Access model v1: **unlisted, not public, not authed.** The web has no
  late.sh login (identity is SSH keys), so watch URLs carry a random
  per-stream id, shown only in-app, dead when the stream ends. Public
  stable `late.sh/live` is a later acquisition move, gated on a mute/ban
  story.

## Consent and roster integrity (non-negotiable)

Web watchers hearing the voice room is browser listen-only voice,
reintroduced deliberately and scoped to stream rooms. Two requirements
make it honest:
1. **ON AIR marker.** The voice strip in a live room shows it loudly, and
   joining voice there gets one explicit confirm: you are audible to
   anonymous link-holders while the stream is up.
2. **No invisible speaker.** `VoiceService`'s roster only knows CLI
   participants; a browser-mic streamer would be audible but absent. The
   stream registry must feed "mat · on air" into the room's voice display
   state (the page reports its own mic state; nothing detects anything).
   Secret participants in a voice room are the one betrayal this design
   must never allow. Anonymous *ears* are expected here (it's a broadcast,
   the count is shown); anonymous *mouths* are forbidden.
3. **Double-mic guard** (soft): browser mic defaults off + one line on the
   go-live page ("talking through the late CLI? leave this off").

## Technical spikes before committing

1. **CLI voice runtime meets a video track.** Today's native runtime has
   only seen audio-only rooms. It must ignore the screen-share track
   gracefully (not subscribe/crash/burn bandwidth). Probably one
   selective-subscribe guard, but verify first: it touches the
   invariant-heavy voice crate.
2. **Publish-source restriction** on the go-live grant (LiveKit can
   restrict publish sources): confirm the exact grant shape.
3. **Roster feed-in**: cheapest path for "on air" state into the voice
   strip (page beacon vs LiveKit webhook; beacon is probably enough).
4. (Parked, not needed for v1: `getDisplayMedia` inside wry/WebKitGTK.
   Capture always happens in the streamer's real browser, so this only
   matters if we ever want a fully-in-wry streamer flow. Windows WebView2
   works; WebKitGTK needs 2.38+/PipeWire/portal and is flaky; WKWebView
   effectively no.)

## Deliberately NOT in v1

- Viewer talk-back / viewer mics (that is browser voice chat, a separate
  decision).
- General browser voice rooms (ditto; note this work builds most of its
  parts, so it stays a cheap deliberate follow-up, never an accident).
- Recording, past-streams page, quality settings, multi-presenter,
  web-side chat overlay.
- Routing stream media into the CLI audio path (source-arbitration
  regression; the stream lives in a page, the radio lives in the CLI,
  `m` exists if they clash).
- The tavern prop (a glowing TV in the lounge pointing at the live
  stream, jukebox-popover shape) — lovely later flourish, pure decoration.
- Streamer-mic exception ambivalence resolved: **ship the browser-mic
  toggle in v1** (macOS streamers otherwise can never voice a stream and
  silent streams are dead air), with the guards above.

## Experiment framing

Metrics (name them so it can fail honestly):
- streams started per week
- peak concurrent watchers per stream
- does a "went live" line pull people into the room (chat/voice joins
  during streams)
Kill condition: nobody streams for a month -> shelve the surface, keep the
token plumbing and web viewer (arena spectator infra later).

Known costs accepted:
- A live room siphons chat from #lounge for its duration (temporary, and
  the feed line points at where the party moved).
- Egress bandwidth on the node if a stream gets popular.
- Moderation: screen share broadcasts anything; fine at ~40 trusted users,
  revisit before any public watch page.

## Alignment checklist when building

- `voice/CONTEXT.md`: scope guard at §"Keep browser publishing... out of
  the MVP" must be amended with the streamer-console exception, precisely
  scoped.
- New local CONTEXT.md for the stream domain (registry, commands, pages).
- `help_modal/data.rs`: `/golive`, `/watch`, the rail row (few lines).
- Splash tips: one line, maybe.
- Root `CONTEXT.md`: service row for the live registry + web routes.
