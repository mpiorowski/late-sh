# Audio: the visualizer

The equalizer (`viz.rs`): the paired CLI's live spectrum, the ambient fallback, smoothing, and where each source's bands come from. Part of the audio domain; the rest is `CONTEXT.md`.

The equalizer draws what the paired CLI is actually playing whenever the CLI
can hear it. The CLI decodes Icecast and radio itself, runs an FFT over its
audible output (`late-cli/src/audio/analyzer.rs`), and sends 16 log-spaced
bands plus RMS as pair-WS `viz` frames at ~15 Hz. `api.rs` routes them to the
session as `SessionMessage::Viz`, and `tick()` folds each into
`AudioState`'s `Spectrum` (`apply_viz_frame`). Frames never mark the app
dirty: the eq already repaints on the anim_half edge while visible and picks
up whatever landed since.

The pair socket takes 16 bands (`late_core::audio::VIZ_BANDS`) or the 8 that
CLIs from before the 16-band analyzer send; `api.rs::bands_from_wire` stretches
8 to 16 at the parse, so everything past the socket handles one shape. An old
server rejects a 16-band frame, so the server deploys before the CLI.
`late_ssh_pair_viz_frames_total{bands="8"|"16"}` counts accepted frames by
wire shape, so it shows how many paired CLIs still send 8.

`Spectrum` is a pure state machine in `viz.rs`. Each frame is metered against
a running level first (`Level`): a mean of the bands and their typical swing
around it, settling over `LEVEL_SETTLE_SECS` (3s) of wall time. A band at the
mean lands at `LEVEL_CENTER` (0.35) and each swing away moves it
`LEVEL_SPREAD` (0.2), with the swing floored at `LEVEL_MIN_SWING` so a
near-steady signal is not blown up into noise. That is what keeps the eq from
reading flat: the CLI's dB meter holds steady music in a narrow band high up,
and the level spreads it low with room above, lets a hit jump before the
level catches up, keeps the spectrum's shape (a band above the mean still
stands above one below), and meters a loud track and a quiet one alike once
settled. The first frame of a run lands as-is, later ones ease toward the new
bands (fast attack, slower release), and
each band keeps a peak cap: a bar at or above the cap strikes it, the cap
hangs there for `CAP_HOLD_SECS` (0.5s), then falls under `CAP_GRAVITY`
(slow, then faster) until the bar catches it. Caps age on the frame's
`Instant`, not on frame count, so the fall keeps its speed at any client
cadence. The ambient band draws the same caps statelessly
(`ambient_cap_unit`: the highest recent strike still falling, read off the
wall tick). The renderer draws the air between a bar and its cap as the
bar's ghost, a faint amber (`theme::EQ_GHOST`) that stays up where the bar
struck and sinks back onto it; the bar's head cell carries the ghost behind
its glyph only when the peak reaches past that cell (one cell holds one
background, so a peak ending inside it would read a whole cell high, a third
of the 3-row sidebar strip), and gap columns never do. Band values are clamped into 0..=1 at the boundary, since they come
off the network. A spectrum not refreshed for 750ms is dropped
(`expire_spectrum`, every tick), which is how every "no audio data" case falls
back: the client muted, switched to YouTube, lost its socket, or is a CLI
without the analyzer. The CLI sends nothing while silent, so no case needs an
explicit off message.

`viz::eq_state` is the single reading every eq surface (sidebar music stage,
Zen music and visualizer tiles) draws. The bars and caps are one renderer,
`viz::dance_lines`, at any height: the music stage and music tile at three
rows, the visualizer tile at its full height:

| `EqState` | When | Renders |
|---|---|---|
| `Live(LiveBands)` | paired, unmuted, fresh spectrum | the spectrum, bands interpolated across the bars, with peak caps |
| `Ambient` | paired, unmuted, no fresh spectrum | the synthesized band, stateless from the wall tick |
| `Muted` | paired and muted | a flat line, the meter at rest |
| `Unpaired` | nothing paired at all | `no audio here yet` / `press ? to listen` |

`Unpaired` is the raw-`ssh` case: the session has no audio surface, so a
dancing band would be claiming playback that cannot exist. The strip points
at the `?` guide instead, which is where both the CLI install and
`late.sh/listen` live. The sibling volume row already rendered `—` here.

YouTube plays inside a cross-origin iframe in the `late-webview` helper, where
the page cannot read the samples. On Linux the CLI records the helper's own
PipeWire stream instead and sends its spectrum as ordinary `viz` frames (`late-webview/CONTEXT.md`),
so nothing here tells YouTube apart from Icecast. Where that capture cannot
run (Windows, macOS, a host without `pw-dump`/`pw-record`) YouTube stays
`Ambient`. The pet's `music_playing` reads pairing and mute only, so `Live`
and `Ambient` both count as music.
