# Audio: the sidebar music stage

The right-rail music stage (`common/sidebar.rs`): the fixed-height accordion, the active-source rule, the title-bar tags, and the helpers behind it. Part of the audio domain; the rest is `CONTEXT.md`, the station table is `STATIONS.md`.

Renders the audio domain into the right rail as a **fixed-height accordion**: a 3-row eq strip, then a dock that is always exactly `MUSIC_DOCK_HEIGHT = 12` rows (`MUSIC_STAGE_HEIGHT = 15` with the strip). The two sources sit in the fixed order radio → youtube (radio leads because it is the default source), and each source's rows sit directly under its own title bar. `v+x` toggles the source. Entry point: `draw_music_stage` (props bundled in `MusicStageProps`); the line builder is `music_stage_lines(width, props)`.

**Product rules (user requirements):**
1. **Both title bars always show their listener count.** That is the "what's everyone tuned to" board.
2. **The YouTube track is always visible, the radio track only while on radio.** A radio listener wants to know whether the booth is playing something worth switching for; a YouTube listener does not care what the radio is playing, so radio collapses to its title bar there.
3. **The stage height is constant.** Both sources fill exactly 12 dock rows, so the panels below never shift.
4. **The stage never claims audio the session cannot produce.** With no client paired, the eq strip shows the guide pointer rather than dancing bars, matching the volume row's `—` (`VIZ.md`).

### Layout (dock rows 0-11)

| Row | On radio | On youtube |
|-----|----------|------------|
| 0   | Volume bar: `vol  ▰▰▰▰▰▱▱▱▱▱  60%` (`muted` when muted, `—` when no client is paired) | same |
| 1   | `radio` title bar + count | `radio` title bar + count |
| 2   | current station's track | `youtube` title bar + count |
| 3   | heading: `darksynth · nightride` | youtube track |
| 4-6 | pinned slot rows `v1`..`v3` | progress, skip meter, `next ⌄` |
| 7-8 | pinned slot rows `v4`..`v5` | queue rows 1-2 |
| 9   | `youtube` title bar + count | queue row 3 |
| 10  | youtube track (dim) | queue row 4 |
| 11  | footer `v+r tune  v+x source` | footer `v+v queue  v+x source` |

Track rows (`dock_track_line`): the active source's track renders `TEXT_BRIGHT` bold, the youtube peek on radio `TEXT_DIM`. Track text per source:
- **youtube**: `youtube_track_text(queue)`: `Channel - Title` for the current item (falls back to `by <submitter> - Title`, then bare title); `fallback stream` when nothing is submitted (the fallback is the steady state, never "queue empty").
- **radio**: live `Artist - Title` (or a bare title) for the selected station from the `RadioMetaService` map (`radio_now_playing`); falls back to the station label while metadata is absent.

Detail rows:
- **YouTube** (`youtube_detail_lines`, padded to `MUSIC_YOUTUBE_DETAIL_HEIGHT = 7`): progress (`progress_line` when duration is known and not a stream, `elapsed_line` otherwise), skip meter or blank, `next ⌄` header, then up to `MUSIC_QUEUE_HEIGHT = 4` queue rows or `· fallback next`. With nothing submitted: `YouTube · 24/7` + `queue with v+v` hint.
- **Radio heading** (`station_heading_line`): the current station's label in amber, then ` · ` and its provider's short name (`Provider::label()`) as the credit, both italic. On the 21-column rail the provider is cut with `…` before the station label ever is (`horrorsynth · nightr…`). Nightride's condition is artist credit, which the track row carries; the heading keeps the provider named as well.
- **Radio** (`radio_detail_lines`, exactly `RADIO_SLOTS = 5`): one selector row per pinned slot (`v1`..`v5`; an empty slot reads `pin via v+r`). A station that is not pinned lights no slot row; the heading above still names it.

Selector rows (`selector_row_line`): `●`/`○` state glyph, station label, right-aligned `v1`..`v5` key hint in `AMBER_DIM` bold. Selected: glyph `AMBER_GLOW`, name `TEXT`; unselected: glyph `BORDER_DIM`, name `TEXT_DIM`.

### Active-source rule

The expanded source = `paired_source` (`AudioSource::{Radio, Youtube}`). Pure preference-based. Does **not** gate on whether a client is paired. The saved preference (loaded from `users.settings.audio_source` during SSH bootstrap, mirrored on `App`) is the source of truth from the first frame; pairing completing does not change the visual state. Don't add a pairing guard back: waiting for the client read as a startup glitch.

The volume row stays honest about pairing (`vol  —` when nothing paired), so users aren't misled about whether their preference is currently audible.

The Stations modal's Ctrl+Y (`^y` in its footer) copies the highlighted station's track through the OSC 52 clipboard path: `Artist - Title` (or bare title), the same `App::station_now_playing` text its row shows. Radio has no per-track link, so text is all there is. A station with no metadata shows a banner and leaves the clipboard untouched.

### Title-bar source tags

Both title bars show the active users' saved source-preference count in the tag slot (`radio  ────  12` / `youtube  ────  5`). Active vs inactive is communicated by color/weight (amber bold vs italic faint), not by case (label is always lowercase) and not by tag presence. The counts come from `ActiveUsers[*].audio_source` via `AudioService::{youtube,radio}_source_count()` and ignore whether those users are currently paired/listening.

### Fallback-not-empty semantics

The widget treats "no submitted track" and "fallback playing" as the same state. When `queue.current.is_none()`:
- Title tag still shows the YouTube source count (no separate "loop"/"fallback" badge: the track row carries that information).
- The track row renders `fallback stream`; the detail rows render `YouTube · 24/7` plus a `queue with v+v` hint.
- When a track is playing but queue is otherwise empty, the trailing "next" row says `· fallback next`, not "queue ends".

No copy anywhere reads "queue empty". The user has pushed back on that wording multiple times; in their product framing the fallback is the steady state, not a placeholder.

### Data sources

- `queue_snapshot: &QueueSnapshot`: from the `AudioState::queue_snapshot()` watch channel.
- Source/station selection lives in `users.settings` (`audio_source`, `radio_station`, `radio_slots`), mirrored on `App` and threaded through `DrawContext` into `SidebarProps` as `paired_source` / `selected_radio_station` / `radio_slots`.
- `paired_client: Option<&ClientAudioState>`: for `volume_percent` and `muted` (vol row only).
- `youtube_source_count` / `radio_source_count`: counts from active users' cached `audio_source`. Pair presence is ignored; offline users are excluded.
- `radio_now_playing: Option<&str>`: preformatted track text for the selected station from the `RadioMetaService` map or the house now-playing map, looked up in `app/render.rs` via `stations::station_now_playing`.

### Internal helpers (all in `sidebar.rs`)

- `music_stage_lines(width, props)`: the whole-dock line builder; unit tests assert directly against its output.
- `stage_title_line(area_w, label, tag, active)`: shared title-bar renderer. Label is always lowercase. Active → amber bold label + amber-dim tag; inactive → italic faint label + tag.
- `dock_track_line(width, track, active, tick)`: a track row (bright bold when active, dim when not), marquee-scrolled when longer than the rail.
- `labeled_rule_line(width, label)`: the rule naming the current station.
- `selector_row_line(width, name, key, selected)`: slot row with right-aligned key hint.
- `youtube_track_text(queue)`: track-row text for youtube.
- `volume_row_line`: the vol bar.
- `keybind_row_line(width, &[(key, label), ...])`: adaptive hint renderer; drops trailing groups when the rail is too narrow rather than mid-word truncating.
- `youtube_detail_lines` / `radio_detail_lines`: the per-source detail rows.
- `skip_meter_spans(progress)`: includes a trailing `v+s` keybind hint inline.
- `queue_next_line(idx, item, width, tick)`: number flush at column 0 (no leading indent) to maximize title width.
- `sidebar_marquee_scrolling`: tells the render gate whether any marquee row is animating; it must mirror which track rows the current source renders.

Tests (`sidebar_test.rs`): `music_stage_height_is_constant`, `on_radio_the_stage_reads_radio_then_its_stations_then_the_youtube_peek`, `on_youtube_radio_collapses_to_its_title_bar`, `both_title_bars_keep_their_listener_counts`, `an_off_slot_station_lights_no_slot_row_and_credits_its_own_provider`, `the_radio_track_row_prefers_live_metadata`.

### Cross-cuts

- There is no genre-vote row.
- `v+1`..`v+5` tune to the station pinned in that slot while Radio is the source (`input.rs::handle_music_suffix`); on YouTube the digit is swallowed. The choice persists to `users.settings.radio_station` and confirms with a `Station: <label>` banner.
- `va`/`vb`/`vc` are reserved for active Home poll votes before music dispatch; numeric selectors must stay available even when a poll is visible.
- v+x toggles radio and youtube through `app/state.rs::toggle_paired_playback_source` → `AudioService::persist_audio_source`, which updates every paired registry entry for the user and broadcasts `PairControlMessage::SetPlaybackSource { source, stream_url, station }`. Radio retargets direct-stream playback to the selected station (a house station resolves against the Icecast base URL); YouTube silences native output and starts the embedded webview helper on capable CLI builds.
