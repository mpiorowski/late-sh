# Radio: two sources, a station catalogue, and user-pinned slots

Design doc. Steps 1-4 of §10 are built on `mateu/music` (catalogue, two-source rail,
slots, Stations modal, Plaza / Code Radio adapters, listen-page grouping). Plaza and
Code Radio are enabled; the open items of the §9 checklist are still owed.
`late-ssh/src/app/audio/CONTEXT.md` records what exists; this file records the why and
what is still to come.

## 1. Goal

- The right-rail music widget offers **two sources**: `radio` and `youtube`. The `icecast`
  source disappears as a user-facing concept; the house mounts become radio stations.
- `radio` is backed by a **station catalogue** (server-side, code-defined) that can grow
  without touching the UI: Nightride today, Nightwave Plaza, Code Radio and others later.
- Users **pin up to four stations** from the catalogue into slots `v1`..`v4` for fast
  access, and browse / listen to the full catalogue from a modal (`v` then `r`).
- The widget gets **shorter** (19 rows to 15 on the rail) without breaking the three
  product rules the current stage locks in (every source shows now-playing, chrome never
  moves, the stage never claims audio the session cannot produce).

## 2. What exists today (for contrast)

- Three sources, `v+x` cycles radio → youtube → icecast. Rail stage = 3-row eq strip +
  16-row dock (vol 2, three dock entries 6, rule 1, detail 6, footer 1).
- `radio` = five hard-coded Nightride stations (`RadioStation` enum, `v1`..`v5`).
- `icecast` = house `chill` and `classical` mounts (`IcecastStream` enum, `v1`/`v2`),
  with Liquidsoap now-playing + progress from `NowPlayingService`.
- Settings keys: `audio_source`, `icecast_stream`, `radio_station`.
- Clients only ever receive `SetPlaybackSource { source, stream_url, station }`; the CLI
  plays whatever absolute URL it is handed. **No client protocol change is needed** for
  any of this.

## 3. Station catalogue

One static table in `late-ssh/src/app/audio/stations.rs` replaces the two enums:

```rust
pub struct Station {
    pub key: &'static str,        // settings + meta key, stable forever ("rektify" stays "rektify")
    pub label: &'static str,      // lowercase rail label ("ambient")
    pub provider: Provider,       // Nightride | House | Plaza | CodeRadio ...
    pub stream_url: StreamUrl,    // Absolute(&'static str) | HouseMount(&'static str)
    pub tags: &'static [&'static str], // "synthwave", "lofi", "classical", "vaporwave", "coding"
    pub enabled: bool,            // false = present in code, hidden everywhere until verified
}
```

- `Provider` carries the attribution line the detail area shows for the current station
  (`nightride.fm · live`, `late.sh house · cc music`, `plaza.one · live`,
  `freecodecamp.org · code radio`) and which metadata adapter feeds it (§6).
- `HouseMount("classical")` resolves against `icecast_base_url` exactly like
  `icecast_stream_url` does today, so the house streams stay Liquidsoap-served and
  third-party streams stay direct-to-client (the "never proxy" rule is unchanged).
- Launch catalogue: `chillsynth`, `nightride`, `datawave`, `spacesynth`, `rektify`
  (ambient), `classical` (house), `lofi` (house `chill` mount, relabelled), `plaza`,
  `coderadio`, plus Nightride's `darksynth`, `horrorsynth`, `ebsm`, Radio Paradise's `mellow`, FIP's `fipjazz`, and Radio Swiss's `swissjazz` and
  `swissclassic`. A new station ships as an `enabled: false` row until the verification
  checklist in §9 passes, then is flipped on in a one-line commit.
- `RadioStation` becomes a `StationKey(String)` newtype validated against the catalogue;
  `from_settings_str` falls back to `chillsynth` as today. `radio_station_url_by_key`
  stays strict (unknown or disabled key → `None`) so `/api/listen` never lists a station
  it cannot play.

## 4. Settings and migration

| Key              | Today                              | After                                                    |
|------------------|------------------------------------|----------------------------------------------------------|
| `audio_source`   | `radio` \| `youtube` \| `icecast`  | `radio` \| `youtube`; `icecast` reads as `radio`         |
| `radio_station`  | one of five Nightride keys         | any enabled catalogue key                                |
| `icecast_stream` | `chill` \| `classical`             | read once for migration, then ignored                    |
| `radio_slots`    | (new)                              | JSON array of up to 4 unique catalogue keys              |

Read-side migration, no SQL migration: `extract_audio_source` maps `icecast` → `Radio`, and
when it does, `extract_radio_station` returns the mapped house key (`chill` → `lofi`,
`classical` → `classical`) instead of `radio_station`. The next persist writes the new
shape. Nobody loses what they were listening to.

Slot defaults when `radio_slots` is absent: `[chillsynth, nightride, datawave, classical]`,
with the user's current `radio_station` swapped into slot 1 if it is not already present
(an `ambient` listener keeps `v1` = ambient). Default source stays `radio` / `chillsynth`
for new users, as today.

`RADIO_SLOTS = 4` is a constant. Four is the proposal (it is what fits a short detail area
and what the request suggested); bumping to five costs one rail row and nothing else.

## 5. Rail widget (24 columns wide)

```
 ▁▂▃▅▂▁▂▃▅▂▁▂▃▅▂▁▂▃▅▂▁   eq strip, 3 rows (unchanged)
 vol  ▰▰▰▰▰▰▱▱▱▱  60%     0  volume (unchanged)
 radio ──────────── 12    1  active tab: amber bold + listener count
 Artist - Title           2  now playing of the CURRENT station (bright)
 youtube ─────────  5     3  inactive tab: italic faint
 Channel - Title          4  youtube now playing (dim)
 ── chillsynth ───────    5  rule names the current station (or youtube)
 ● chillsynth      v1     6
 ○ nightride       v2     7  slot rows: ● = current, ○ = pinned
 ○ datawave        v3     8
 ○ classical       v4     9
 nightride.fm · live     10  attribution of the CURRENT station's provider
 v+v queue v+x src v+r   11  footer
```

- 3 + 12 = **15 rows** (today 19). Rows 0-5 and 10-11 are fixed chrome for both sources;
  rows 6-10 are the detail area (`MUSIC_DETAIL_HEIGHT` goes from 6 to 5).
- YouTube detail in 5 rows: progress/elapsed, skip meter, `next ⌄`, two queue rows
  (`MUSIC_QUEUE_HEIGHT` 3 → 2). The booth modal still has the full queue.
- The `m mute  -= vol` hint row is dropped; `m`, `-`, `=` are already in the global keys
  of the guide and the footer keeps the three music chords.
- Current station off-slot (picked from the modal, not pinned): no `●` lights up, the rule
  on row 5 still names it, row 2 still shows its track. Nothing moves.
- Row 2 falls back to the station label when metadata is absent, as the radio dock row does
  today; house stations get `Artist - Title` from the Liquidsoap now-playing map, so the
  classical/lofi progress bar moves from the detail area to nowhere: the slot rows need the
  space. (Keep it only if the user misses it; it never existed for Nightride.)
- Tab counts: `radio` tag = radio + legacy icecast preferences, `youtube` tag unchanged.
- Tests to update: `music_stage_chrome_rows_never_move` (two sources), dock tests (rows
  2/4), `radio_selector_rows_mark_selected_station` → slot rows, plus a new
  `off_slot_station_lights_no_slot_row`.

## 6. Keys

| Key          | Today                                   | After                                                   |
|--------------|-----------------------------------------|---------------------------------------------------------|
| `v` `x`      | cycle radio → youtube → icecast         | toggle radio ⇄ youtube                                  |
| `v` `1`..`5` | station/stream by index in active source| `v` `1`..`4`: play the station pinned in that slot (radio active); no-op on youtube |
| `v` `r`      | (unused)                                | open the Stations modal                                 |
| `v` `v`      | Music Booth (YouTube queue)             | unchanged                                               |
| `v` `s`      | skip vote                               | unchanged                                               |
| `v` `a/b/c`  | Home poll votes                         | unchanged, still checked first                          |

Banner copy stays sentence case: `Station: Classical`.

## 7. Stations modal (`v` then `r`)

Follows the booth modal conventions (`centered_rect`, `theme` colours, footer keybinds,
`Esc` closes). Width 80 with two blank columns inside each border, height = rows + chrome, clamped to the terminal.

```
┌ stations ───────────────────────────────────────────────────────┐
│ pinned   v1 chillsynth   v2 nightride   v3 datawave   v4 classical │
│                                                                   │
│ ▸ ● chillsynth   nightride     FM-84 - Running in the Night   v1  │
│   ○ nightride    nightride     The Midnight - Sunset           v2  │
│   ○ datawave     nightride     Com Truise - Flightwave         v3  │
│   ○ spacesynth   nightride     Dynatron - Pulse Power              │
│   ○ ambient      nightride     Stellardrone - Billions              │
│   ○ classical    late.sh       Kimiko Ishizaka - Prelude No. 1 v4  │
│   ○ lofi         late.sh       HoliznaCC0 - Autumn                 │
│   ○ plaza        plaza.one     Macintosh Plus - リサフランク420    │
│   ○ code radio   freecodecamp  Trebles and Blues - Dusk            │
│                                                                   │
│ ↑↓ move   Enter listen   1-4 pin to slot   0 unpin   / filter   Esc │
└───────────────────────────────────────────────────────────────────┘
```

- Every row shows **live now-playing** from the unified meta map (§8), so a user can "check
  the music" of nine stations without playing any of them. A station whose adapter has no
  data shows its label dimmed, never a stale title.
- `Enter` = listen now: persists `radio_station` and pushes `SetPlaybackSource` through the
  existing `persist_radio_station` path. Audible immediately on the paired CLI. It also
  flips `audio_source` to `radio` if YouTube was active, so Enter always produces sound.
- `1`..`4` = pin the highlighted station into that slot (replaces what was there, de-dupes
  if the station already sits in another slot). `0` = unpin. Persists `radio_slots`.
  The rail re-renders from the same setting, so pinning is visible behind the modal.
- `/` filter by label/provider/tag, mirroring the booth History filter.
- Why no separate "preview then revert": the paired CLI is the only audible surface, so a
  preview is a station change by another name. A revert-on-Esc state machine would add a
  second source of truth for "what is the CLI playing" (see §19 of the audio context for
  how two-writers-one-state ends). Listen is the action; pinning is the shortcut.
- `?` guide: the Pair tab's `MUSIC_PAIR_TEXT` in `help_modal/data.rs` drops the three-source
  table and documents `v+r`, slots, and the two sources. (There is no `guide.md` file;
  that text is the guide.)

## 8. Metadata adapters

`RadioMetaService` today is Nightride-only (one SSE loop → `watch<HashMap<key, ArtistTitle>>`).
It becomes the **merge point** for one adapter per provider, all writing into the same map
keyed by catalogue key:

| Provider  | Feed                                                          | Shape                     |
|-----------|---------------------------------------------------------------|---------------------------|
| Nightride | `https://nightride.fm/meta` SSE (exists)                      | push, merge per event     |
| House     | `NowPlayingService` per-mount map (exists)                    | mirror into the map       |
| Plaza     | `https://api.plaza.one/status` JSON, poll ~15s                | `song.artist`, `song.title` |
| CodeRadio | AzuraCast `…/api/nowplaying/coderadio` JSON, poll ~15s        | `now_playing.song.artist/title` |
| RadioParadise | `api.radioparadise.com/api/now_playing?chan=1` JSON, poll ~15s | `artist`, `title`       |
| RadioSwiss | `api.radioswissjazz.ch/api/v1/rsj/en/current`, `api.radioswissclassic.ch/api/v1/rsc/en/current`, poll ~15s | `channel.playingnow.current.metadata.artist/title` |
| Fip       | `api.radiofrance.fr/livemeta/live/65/webrf_webradio_player`, poll ~15s | `now.secondLine` (artist), `now.firstLine` (title) |

Rules carried over: metadata only, never audio; an adapter that fails clears only its own
keys (so the UI falls back to labels, never stale titles); Nightride reconnects with
backoff 1s → 60s, a failing poller slows from 15s to 60s. A poller does not start while
its catalogue row is disabled. Consumers
(`app/render.rs`, `radio_meta_update` on the pair WS → CLI MPRIS, `GET /api/radio-meta`,
`GET /api/listen`) keep reading one map, so the web listen page and MPRIS pick up new
stations for free. The listen page groups stations by provider with each provider's link.

## 9. Candidate stations (web research, 2026-10)

Asked for: the one or two most popular free coding / lofi / chill streams that can be used
the way Nightride is (direct stream, third-party player allowed, attribution). Outcome:

1. **freeCodeCamp Code Radio** (`coderadio.freecodecamp.org`). 24/7 lofi/chill "music to
   code to" from a non-profit, 1,250+ hand-curated tracks (Lawrence Yeo / Trebles and
   Blues), served by AzuraCast. freeCodeCamp publishes the raw stream URL for VLC/mpv use
   (`https://coderadio-admin-v2.freecodecamp.org/listen/coderadio/radio.mp3`; older
   `…/radio/8010/radio.mp3`), with `low.mp3` / `high.mp3` variants and the standard AzuraCast
   now-playing JSON. Best fit for "coding lofi", biggest brand of the bunch.
2. **Nightwave Plaza** (`plaza.one`). Ad-free 24/7 vaporwave since 2014, the station users
   already asked for. Streams at `radio.plaza.one` (`/mp3` 128k, `/ogg`, `/opus`), public
   API at `api.plaza.one` (`/status` now-playing), and the whole client is MIT on GitHub
   (`nightwaveplaza/plaza`). Third-party directories and Cyberpunk/Discord integrations
   already list the stream; a courtesy email like the Nightride one is still the right move.
3. (Reserve) **Radio Paradise Mellow Mix**. Listener-supported, public `now_playing` API,
   widely integrated in third-party players (Volumio, moOde, Music Assistant). Chill rather
   than lofi; add if a third slot of "calm" is wanted.

4. **Radio Paradise Mellow Mix** (`radioparadise.com`). Enabled as `mellow`: stream
   `stream.radioparadise.com/mellow-192` (192k MP3), now-playing
   `api.radioparadise.com/api/now_playing?chan=1`. No written third-party-player terms
   were found; the courtesy email is still owed. The Main Mix is `mp3-192` / `chan=0`.
5. **FIP Jazz** (Radio France). Enabled as `fipjazz`: stream
   `icecast.radiofrance.fr/fipjazz-midfi.mp3` (128k MP3), now-playing from the live
   metadata endpoint Radio France's own web player polls (not a documented API, so it
   may change without notice). No permission asked yet. Also probed on 2026-10-03, not
   added: WQXR and Venice Classic Radio answer over HTTPS with CORS.
6. **Radio Swiss Jazz / Radio Swiss Classic** (SRG SSR). Enabled as `swissjazz` and
   `swissclassic`: streams `stream.srg-ssr.ch/srgssr/rsj/mp3/128` and `…/rsc_de/mp3/128`
   (128k MP3; the shorter `/m/rsj/mp3_128` form redirects to plain `http://` and must not
   be used). Now-playing from the endpoints their own sites poll, undocumented. No
   permission asked yet.
7. (Candidate, nothing built) **KEXP** (`kexp-mp3-128.streamguys1.com/kexp128.mp3`;
   `api.kexp.org/v2/plays/?limit=1`). Answered with `audio/mpeg` and browser CORS on
   2026-10-03. No written third-party-player terms were found; ask first.

Explicitly **out**:

- **SomaFM** (Groove Salad, Drone Zone): asked by email and declined. Their royalty rates
  jump steeply past a listener threshold, so they do not want more listeners. Do not add.
- **Ship FM** (`shipfm.online`): offered by the station itself (stream
  `https://stream.shipfm.online/radio.mp3`, now-playing `https://shipfm.online/api/nowplaying`
  with `artist` / `track`), but on 2026-10-03 the stream host timed out and the API answered
  Cloudflare 522 on every probe. Revisit when it is back up.
- **Lofi Girl, Chillhop, lofi.cafe**: YouTube live streams only, no audio stream. They
  belong in the YouTube fallback / queue, not the radio catalogue.

Verification checklist before flipping an `enabled: false` row on. Plaza and Code Radio
were enabled ahead of the unchecked items, which are still owed:

- [x] `curl` the stream URL: both answer `audio/mpeg` (128k) and echo the request
      `Origin` in `Access-Control-Allow-Origin` (the web listen page plays direct).
- [ ] CLI decoder plays it. `resolve_stream_url` appends `/stream` to any URL without an
      audio extension, which would break Plaza's `/mp3` path on every CLI already
      installed, so the catalogue row carries a `#.mp3` fragment (never sent to Plaza).
      Not yet listened to through a paired CLI.
- [x] Now-playing endpoint returns artist/title; adapter parses it (`radio_meta/polled.rs`).
- [ ] Terms / FAQ read, courtesy email sent, attribution line agreed.
- [ ] Row added to `MUSIC.md`'s external-stations note and the Pair guide.

## 10. Delivery order

Each step ships on its own and leaves the product working:

1. ✅ **Catalogue**: `Station` table, `StationKey`, house mounts as stations, read-side
   `icecast` migration, `radio_slots` setting + defaults. Rail and keys unchanged (radio
   detail keeps listing the first five catalogue rows). Tests: catalogue strictness,
   migration mapping, slot defaults.
2. ✅ **Two sources + compact rail**: drop the icecast tab, `v+x` toggles, new 15-row stage,
   slot rows `v1`..`v4`, per-provider attribution row. Update the sidebar tests and the
   Pair guide text.
3. ✅ **Stations modal** (`v+r`): list, live metadata, listen, pin/unpin, filter.
4. ✅ **Adapters**: Plaza and Code Radio pollers behind `enabled`, listen page grouping,
   MPRIS covered by the unified map.
5. **Docs**: `audio/CONTEXT.md` §6, §12, "Nightride direct-radio source" → "Station
   catalogue"; `MUSIC.md` external stations note; delete this file's "proposal" framing
   once built, or delete the file.

## 11. Decisions taken (revisit if they feel wrong in use)

- **Slot count**: 4 (`RADIO_SLOTS`); five costs one rail row and nothing else.
- **House progress bar**: dropped from the rail; the slot rows need the space.
- **Off-slot listening**: allowed from the modal; the rule row names the station.
- **Footer**: `v+r tune  v+x source` on radio (both groups fit a 21-column rail).
