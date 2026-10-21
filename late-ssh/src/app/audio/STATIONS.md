# Audio: the station catalogue

The guest and house radio stations (`late-core/src/radio.rs`), who is on the table, and the checklist a new station passes before it ships enabled. Part of the audio domain; the rest is `CONTEXT.md`.

`late-core/src/radio.rs` is one static table of stations (`key`, lowercase
`label`, `Provider`, `Section`, stream, `enabled`). The key is the settings and
metadata key and is stable forever (`rektify` stays `rektify` though its label
is `ambient`). A stream is an absolute URL or a `HouseMount` (`chill`,
`classical`) resolved against the Icecast base URL, so house stations stay
Liquidsoap-served and third-party ones stay direct-to-client. A station ships
as `enabled: false` until the checklist below passes; a disabled or unknown key
resolves to nothing, so `/api/listen` never lists a station it cannot play.

- **Who is on it:** Nightride (`chillsynth`, `nightride`, `datawave`,
  `spacesynth`, `rektify`, `darksynth`, `horrorsynth`, `ebsm`), the house
  `chill` (labelled `lofi`) and `classical`, Nightwave Plaza (`plaza`),
  freeCodeCamp Code Radio (`coderadio`), Radio Paradise (`paradise`, `mellow`,
  `rock`, `global`, `beyond`, `kfat`; Serenity is out, AAC only), FIP Jazz
  (`fipjazz`), Radio Swiss Jazz and Classic (`swissjazz`, `swissclassic`).
  Nightride, Plaza and Radio Paradise gave permission; Nightride's condition is
  artist credit on screen. FIP and Radio Swiss were not asked yet, and their
  now-playing endpoints are undocumented.
- **Who is out:** SomaFM declined by email (royalties jump past a listener
  threshold; do not add). Lofi Girl, Chillhop, lofi.cafe are YouTube live
  streams with no audio stream (they belong in the queue). Ship FM's host was
  down when probed. KEXP answers with CORS but has no written third-party terms:
  ask first.
- **Before flipping a row on:** the stream answers `audio/mpeg` and echoes the
  request `Origin` (the listen page plays direct); the paired CLI decodes it
  (`resolve_stream_url` appends `/stream` to a URL without an audio extension,
  which is why Plaza's row carries a `#.mp3` fragment); the now-playing adapter
  parses it; terms read and attribution agreed; a row in the Pair guide. Plaza
  and Code Radio were enabled before their CLI listen test and courtesy email,
  which are still owed.
- **Slots.** `RADIO_SLOTS` (5) pinned stations per user in
  `users.settings.radio_slots`, defaulting to `[chillsynth, nightride,
  datawave, mellow, plaza]` so a slot never moves because the user retuned. A
  pin writes one slot against the stored array (`User::set_radio_slot`), so two
  sessions pinning different slots both land. A shorter stored array reads with
  the missing slots empty. Each extra slot would cost a rail row.
- **There is no `icecast` source to pick**: a stored `audio_source: icecast`
  reads as `radio` on the house station for its mount (`chill` or
  `classical`), and `icecast_stream` in settings is ignored.
- **Stations modal** (`v` then `r`): the catalogue grouped by section (one
  faint heading per group, which the cursor skips), every row showing live
  now-playing from the shared map (a station with no data shows its label
  dimmed, never a stale title). `Enter` listens now and switches the source to
  radio if YouTube was active (it always produces sound); `1`-`5` pin to a
  slot, de-duplicating; `0` unpins; `Ctrl+Y` copies the highlighted track.
  There is no preview-then-revert: the paired CLI is the only audible surface,
  and a revert state machine would be a second writer for "what is playing".

**Metadata.** `radio_meta/svc.rs::RadioMetaService` is the merge point, one adapter per provider writing into one map keyed by catalogue key; a background audio-domain service, never the render loop:
- One `tokio::spawn` SSE loop per process, started in `main.rs` next to the now-playing poller, shut down via the shared `CancellationToken`.
- Connects to `https://nightride.fm/meta` with `accept: text/event-stream`. Each event is one `data:` line containing a JSON array of station records (`station`, `artist`, `title`, plus fields we ignore: `album`, `comment`, sometimes `dj`). Stations observed include `chillsynth`, `nightride`, `datawave`, `spacesynth`, `rektify` (surfaced as the `ambient` station), `darksynth`, `horrorsynth`, and `ebsm`.
- `parse_meta_line` skips records with an empty station/artist/title; valid records merge into the `watch<HashMap<String, ArtistTitle>>` via `send_modify` (merge, not replace, so a partial event doesn't blank other stations).
- Reconnect with backoff: 1s doubling to 60s, reset after a received event. On disconnect the Nightride keys are cleared (`clear_nightride`, which keeps polled stations) so the UI falls back to station display names instead of showing stale tracks.
- Polled providers (`radio_meta/polled.rs::PolledFeed`): Plaza (`https://api.plaza.one/status`, `song.artist`/`song.title`), Code Radio (AzuraCast `/api/nowplaying/coderadio`, `now_playing.song.artist`/`title`), Radio Paradise, one feed per channel (`https://api.radioparadise.com/api/now_playing?chan=N`, `artist`/`title`, a null `title` between tracks is `None` like FIP's programme blurb below; Main 0, Mellow 1, Rock 2, Globe 3, Beyond 5, KFAT 945; Serenity, 42, is not carried because it streams AAC only and the CLI decodes MP3 only) and FIP Jazz (`https://api.radiofrance.fr/livemeta/live/65/webrf_webradio_player`, `now.secondLine` as artist and `now.firstLine` as title; a `now` with a null `songUuid` is the programme blurb between songs: `parse` returns `None`, the key is removed, the poll counts as `outcome="no_track"` and the 15s pace is kept). Radio Swiss Jazz and Classic use the endpoint their sites poll (`https://api.radioswissjazz.ch/api/v1/rsj/en/current`, `https://api.radioswissclassic.ch/api/v1/rsc/en/current`; `channel.playingnow.current.metadata.artist`/`title`, where Classic's `artist` is the composer). `RadioMetaService::start_task` starts one `run_poll_loop` per `PolledFeed::ALL` entry; each fetches every 15s and writes the track under the feed's catalogue key; a track needs a title but may have an empty artist (shown as the bare title); a failed poll removes only that key and doubles the delay up to 60s. A loop does not start while its catalogue row is `enabled: false`, so a disabled station sends its provider nothing. Each poll counts into `late_ssh_radio_meta_polls_total{feed, outcome}` (`metrics::record_radio_meta_poll`).
- Consumers: `app/render.rs` formats `Artist - Title` for the user's selected station and threads it to the sidebar as `radio_now_playing` (`STAGE.md`); the pair WS broadcasts the map, with the house mounts merged in (`svc::pair_radio_tracks`), as `radio_meta_update` via `AudioService::start_meta_forward_task` (§5, consumed by the CLI's MPRIS publisher); and `GET /api/radio-meta` (`api.rs`) exposes it over HTTP for non-paired consumers. A missing/absent entry falls back to the station display name.

Stream URL notes:
- We use the `/<station>.mp3` URLs directly. The site advertises `.m4a` URLs, but those are a 302 to `.mp3` , and the CLI decoder only aligns MP3 streams; pointing at `.mp3` removes the dependency on that redirect.
- The server includes `Access-Control-Allow-Origin: *`, so browser direct playback should be viable.
