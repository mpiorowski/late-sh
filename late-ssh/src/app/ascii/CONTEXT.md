# ascii Context

## Metadata
- Scope: `late-ssh/src/app/ascii`
- Purpose: animated ascii pieces, ported from ascii.rest, for the Zen ascii tile and the away screensaver.
- Parent context: `../../../../CONTEXT.md`

---

## 1. What it is

Four colour scenes ported to Rust from ascii.rest
(https://github.com/bas3line/ascii, MIT, `LICENSE-ascii-rest` beside this
file names the commit): **earthrise** (the Earth turning over a cratered
lunar horizon in long low sunlight; a slow piece, see Cadence, and the
default), **misty forest** (pine ridges fading into morning fog, sunbeams
slanting through; the other slow piece), **aurora fjord** (aurora over a
fjord, a lit cabin) and **alpine dawn** (first light on snow peaks over a
misty lake), each drawn in one of two styles (`SceneStyle`: **dots**, the
original's halftone as braille, or **pixels**, solid half-blocks in true
colour). `late_core::models::user::AsciiPiece` (a `scene` and a `style`)
is a choice of the eight; `AsciiPiece::ALL` is the picker's order, the
screensaver's default first. It is stored by key (`as_str`: `earthrise`,
`misty_forest`, `aurora_fjord_pixels`, ...) in the Zen layout and in
`users.settings.screensaver`. A key this build does not know (the text
pieces ascii.rest also has, plasma, lava lamp and donut, were ported once
and dropped) plays the default, `AsciiPiece::DEFAULT`, earthrise in dots,
on a Zen tile (the rest of the layout kept) and as the screensaver alike.

Two surfaces draw them, both through `ui::draw_piece`:

- **The Zen ascii tile** (`../zen/CONTEXT.md`): the screensaver's default,
  earthrise in dots (one `AsciiPiece::DEFAULT` for both), until
  `[` `]` step it through the pieces or Enter picks one from the list
  (`picker/`, a popup over the page: `j` `k` or a click, Enter picks, Esc
  closes; the pick is saved with the layout); `z` zooms it over the whole
  screen, no border and no status row, as the screensaver draws it.
- **The away screensaver**: while a session is away (`/brb`, or
  `common::away::AWAY_AFTER` without a person's input), `App::screensaver`
  names the piece from Settings, Tweaks, `Screensaver` (earthrise in dots
  by default: a slow piece, which costs an away session about what idling
  did; the lively scenes and Off are the choices; Enter on the
  row opens the settings modal's shared picker over Off and every piece,
  Left/Right cycle them), and
  `render.rs` draws it over the whole frame
  ahead of every page and modal, with no click targets. The one thing a
  person did that brings the session back is swallowed in
  `App::handle_input` (`common::away::PresenceInput::Person` says where it
  ends), so the waking key never acts on the page under it (a door game
  included); what follows it in the same chunk is the page's, and a paste
  that wakes the session is swallowed to its close across chunks
  (`App::waking_paste_open`). A bare mouse move or a focus report is not a
  person (`common::away::presence_input`): it neither holds off the away
  clock (`App::last_active_at`) nor drops the screensaver, and a report a
  chunk boundary cuts in two is held (`App::presence_held`) and read whole
  with the next chunk. `/brb` syncs away at once, so the screensaver is up
  on the frame after the Enter.
  `late_ssh_screensavers_total{trigger=idle|brb,piece}` counts each one
  put up (`App::sync_away`).

## 2. File map

```text
late-ssh/src/app/ascii/
|-- mod.rs              # module declarations only
|-- piece.rs            # ShadedFrame, the cadences (each slow piece's frame period and rate) and frame clock, the shared frame cache, the startup warm-up, JS helpers
|-- earthrise.rs        # scene: cratered ground with marched shadows, sky and the Earth's maps built once (OnceLock); the disc and four stars per frame; a slow piece, the default
|-- misty_forest.rs     # scene: forest, fog banks, cloud and beams built once (OnceLock), drifted per frame; a slow piece, its beams on a faster clock in the crawl
|-- aurora_fjord.rs     # scene: land built once (OnceLock), sky + water per frame
|-- alpine_dawn.rs      # scene: the range raymarched once (OnceLock), tinted + mirrored per frame
|-- ui.rs               # draw_piece: a scene as pixels or braille dots by its style
|-- picker/             # the piece picker a Zen ascii tile opens: state.rs, input.rs, ui.rs
|-- fixtures/           # golden frames recorded from the TS originals: <slug>_t1.txt (glyphs) and <slug>_t1.colors (palette bytes), see Porting a piece
|-- LICENSE-ascii-rest  # the MIT notice the port carries
```

## 3. Contracts

- **A piece is a pure function of play time.** `frame(t)` holds no state
  between frames, so a frame is the same for every session and every
  replica: nothing to sync, nothing to persist.
- **One clock, two cadences.** `piece::clock_now` is milliseconds since
  the process's first ask, shared by every session, so two people away at
  once watch the same frame; `piece::frame_index` turns it into a piece's
  own edge by its `Cadence`, and `seconds` an edge into play time.
  `Cadence::Half` (the aurora and alpine dawn, the lively scenes): a
  frame every `FRAME_MS` (132ms, the half tier) at the wall clock's pace.
  `Cadence::Slow { frame_ms, rate }` (earthrise and the misty forest): a
  frame every `frame_ms`, a whole number of the 1Hz edges the idle floor
  already takes, with play time at `rate` of the wall clock, a crawl;
  `piece::cadence` is the one place each slow piece's pair lives. What a
  session pays for a piece is the cells that change per frame times the
  frames per second, and `ui_test.rs`,
  `a_slow_piece_moves_a_few_cells_a_second`, holds every slow piece to a
  hundred cells a second on a 200x50 terminal in dots: about a kilobyte a
  second per away session, against tens of kilobytes a frame for a lively
  scene (every piece's numbers are in Cost). That is what lets the screensaver default to on. The two pay it
  differently. The forest plays a frame every `SLOW_FRAME_MS` (1s) at
  `SLOW_RATE` (0.01): its fog is all but still at that crawl, so it plays
  its beams seven times faster than its air (`misty_forest::crawl`,
  `BEAM_PACE`), and the sway and breathing of the sunbeams is the motion
  it shows, a few dozen cells a frame. Earthrise moves only the cells of
  the Earth's disc, but any motion at all flips the disc cells sitting on
  a tone step (a hundredth of a second of play flips as many as a tenth),
  so it plays fewer, bigger steps: a frame every `EARTH_FRAME_MS` (4s,
  every fourth 1Hz edge; the repaints between diff to nothing) at
  `EARTH_RATE` (0.08), a visible notch of the globe each frame and a turn
  in about half an hour. The dots style's rounded ink is part of that
  arithmetic: without it, most of a frame's changed cells are colours that
  wobbled by one.
- **One frame per edge for the process.** `piece::picture` serves a
  scene's frame from a process-wide cache keyed by scene and edge (both
  styles share it); it computes outside the lock, and two sessions racing
  on one edge both compute it (a frame of CPU, nothing else).
- **Drawing.** A scene is shaded per square cell (`ShadedFrame`, on its
  own ground colour; a `Shade` is the brightness the original turns into
  dot size, the raw colour, and the piece's ink for its largest dot).
  `ui::draw_pixels` scales it to cover the area (the overflow cropped
  evenly) and draws the two scene rows a terminal cell stands on as the
  halves of a `▀` (upper in the ink, lower in the background), each the
  cell's ink over the ground by its brightness (`piece::pixel`), in true
  colour: no palette, no dither, so the picture fills the cell and
  gradients stay smooth; at 200x50 it is the original's own 200x100 grid,
  cell for cell. The original's halftone (each scene's `dot`, test only:
  ordered dither, nearest palette colour) is what the golden frame checks;
  on its canvas the cells are square and a dot reaches into the next row,
  which no terminal glyph can do. `ui::draw_dots` is the dots style, the
  original's look: a square grid of dots, one per scene cell (two scene
  rows to a terminal cell), sized and lit by brightness. Braille draws the
  three sizes as one dot, a diagonal pair, or a 2x2 cluster, each anchored
  at the same place in its quarter of the glyph, so the grid stays regular
  and only the dots grow. Sizes take plain thresholds, never a dither: a
  dither between sizes turns the grid into a texture. The tone is the ink
  over the ground on a steep curve (dim sky falls to dark, fog and sun
  stand out). Size and tone read the brightness after a local contrast
  boost (an unsharp mask, `ui::sharpened`): the trees are only a little
  darker than the fog around them, and without it the halftone's few
  steps flatten both into one dim field. The tone is held to twelve steps
  and every ink rounded to eight steps a channel, so a cell changes only
  when the scene moves it a visible step.
- **Cadence in the tick.** A drawn Zen ascii tile, or the screensaver,
  repaints on its cadence's edge (`App::ascii_edge`: the half edge for a
  lively piece, the 1Hz edge for a slow one, whose frame cache hands back
  the same frame until its own edge) and a lively one asks `wake_hint` for
  `ANIM_HALF_TICK` (`App::lively_ascii_visible`); a slow piece asks for
  nothing, the idle floor carries its edge. The screensaver
  returns its tier ahead of every other check, since it covers everything
  and a pointer moving over it must not open the hot window.

## 4. Cost

Every piece is measured, never guessed, and a new one is measured
against this table before its cadence is picked. The report is
`ui_test.rs`, `the_cost_report` (ignored: it prints, the budget test
asserts):

```
make test-llm ARGS="-p late-ssh --run-ignored all --no-capture -E 'test(the_cost_report)'"
```

It draws each piece in both styles on a 200x50 terminal at its own
cadence and counts the cells that change between consecutive frames
(what the wire carries: a changed cell is on the order of ten to twenty
bytes with its colour), plus one frame's compute in the unoptimised test
profile (read it against the other rows, not the clock).

| piece                  | frame ms | cells/frame | worst | cells/s | frame us (debug) |
|------------------------|---------:|------------:|------:|--------:|-----------------:|
| earthrise · dots       |     4000 |         355 |   372 |      88 |             8879 |
| earthrise · pixels     |     4000 |         575 |   580 |     143 |             8176 |
| misty forest · dots    |     1000 |          55 |    62 |      55 |             8003 |
| misty forest · pixels  |     1000 |         588 |   661 |     588 |             7470 |
| aurora fjord · dots    |      132 |        2522 |  2558 |   19106 |            16160 |
| aurora fjord · pixels  |      132 |        6920 |  7118 |   52424 |            11681 |
| alpine dawn · dots     |      132 |         501 |   622 |    3795 |            13110 |
| alpine dawn · pixels   |      132 |        3014 |  3357 |   22833 |            11250 |

What the numbers decide:

- **The screensaver's default holds the budget**: a hundred cells a
  second in dots (`a_slow_piece_moves_a_few_cells_a_second`), so a
  session that is away costs about what one that idles did. Any piece
  offered as a slow one must pass that test at its cadence.
- **A lively piece's cost is its cells per second**, and the table is the
  scale: alpine dawn in dots is a tenth of the aurora in pixels at the
  same fps. Raising a piece's fps is welcome when the report says it is
  cheap: the cost of a piece at a new rate is its cells/frame times the
  rate, so a piece under a thousand cells a frame can run at `HOT_TICK`
  (66ms, `tick.rs`, the fastest tier the loop has) for what the aurora
  costs at the half tier. Doing it is a cadence: a `frame_ms` on the
  piece (`piece::cadence`), the tick tier it rides (`App::ascii_edge`,
  `App::wake_hint`, `App::lively_ascii_visible`), and a re-run of the
  report with the new row in this table.
- **Compute is not the limit** at these sizes (a frame is a few
  milliseconds in debug, shared by every session through the cache); the
  wire is. A piece that needs the frame cache to miss (a per-session
  parameter) would change that, and does not exist yet.
- Measured on the wire (`late_ssh_render*`, the output-budget metrics):
  not yet, for any piece; the cells are the proxy until then.

## 5. Porting a piece

1. Port the TS scene line by line into `<slug>.rs` with a `frame` that
   returns a `ShadedFrame`: its `ground`, and per cell the level, the raw
   colour, and the ink the original gives its largest dot, `want = 1` in
   its colour step; keep the original's `dot` and palette under
   `#[cfg(test)]` for the golden frame. Keep
   JavaScript's semantics where they decide a glyph: `Math.round` is
   `piece::js_round` (halves up), `x | 0` is `piece::js_i32`, a literal
   `6.28` is `piece::ROUGH_TAU` (not `TAU`), `Math.imul`
   and `>>>` are `u32` wrapping arithmetic, and a `Float32Array` is a
   `Vec<f32>` (store rounded, read back as `f64`).
2. Add the variant to `Scene` (late-core: key, label, `ALL`), its two
   entries to `AsciiPiece::ALL` and `AsciiPiece::as_str`, and arms in
   `piece::cadence` (a slow piece's frame period and rate go here, tuned
   to the cell budget) and `piece::picture`; the compiler finds the rest
   (the cycle test's list). The pickers and the Tweaks row read `ALL` and
   `label`.
3. Record the golden frame from the original, with its nearest-colour
   cache removed (see Gotchas). In a scratch directory, clone
   https://github.com/bas3line/ascii at the commit `LICENSE-ascii-rest`
   names, copy `src/pieces/<slug>.ts` and `src/types.ts` beside each
   other, point the piece's `import type` at `./types.ts`, and in its
   `nearest` delete the line `if (lut[k] !== 255) return lut[k];` and
   make `return (lut[k] = best);` a plain `return best;`. Then run, with
   node 23+ (it strips the types):

   ```js
   import { writeFileSync } from "node:fs";
   import piece from "./<slug>.ts";
   const color = new Uint8Array(200 * 100);
   const text = piece()(1, { color });
   writeFileSync("<slug>_t1.txt", text + "\n");
   writeFileSync("<slug>_t1.colors", color);
   ```

   `fixtures/<slug>_t1.txt` is the 100 lines of 200 glyphs the original
   returns, each line newline-terminated; `<slug>_t1.colors` is the 20000
   palette indices, one byte per cell in row order. The test
   (`alpine_dawn_test.rs` is the shape, `earthrise_test.rs` where the
   original's `dot` takes a floor and a cap) runs the port at `t = 1`
   through its `dot` and compares both files byte for byte, and checks
   every palette colour is its own nearest. A mismatch is a port bug, not
   a tolerance: never loosen it, find the JavaScript semantic that
   differs (step 1).
4. Name the file in `LICENSE-ascii-rest`, and declare the test module at
   the bottom of `<slug>.rs` (`#[cfg(test)] #[path = "<slug>_test.rs"]
   mod <slug>_test;`): a golden test that is never declared never runs.
5. Run the cost report (Cost) and add the piece's two rows to the table;
   pick its cadence from them, not from how it looks: a slow candidate
   goes into the budget test's list, a lively one gets the tier its cells
   a second earn.

## 6. Gotchas

- ascii.rest's scenes cache their nearest-colour lookup by 5-bit colour
  bucket, filled by whichever colour asks first, so their colours depend on
  draw order (about a tenth of lit cells differ from an exact lookup). The
  ports look up exactly; the `fixtures/*_t1.colors` were recorded from the
  originals with that cache removed. The dots do not depend on it.
- Alpine dawn raymarches its range once per process (`OnceLock`), about
  half a second of one core; earthrise marches its ground's shadows toward
  the sun, under a second. `piece::warm` builds them and the other scenes'
  land on a blocking thread at startup (`main.rs`), so no session pays it
  under the app lock; a test binary pays it on the first scene test.
- ascii.rest's `paper` (a light page flipping the ramp) is not ported:
  every piece draws its dark-ground ramp.
- A lively scene as the screensaver (a full-screen picture at ~7.5fps per
  away session, up to seven thousand cells a frame, Cost) has not had its
  bandwidth measured on the wire; `late_ssh_render*` and the output-budget
  metrics are where it would show. The default is a slow piece for that
  reason: its cost is pinned by the cell budget test, not estimated.
