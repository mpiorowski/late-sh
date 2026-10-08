# ascii Context

## Metadata
- Scope: `late-ssh/src/app/ascii`
- Purpose: animated ascii pieces, ported from ascii.rest, for the Zen ascii tile and the away screensaver.
- Parent context: `../../../../CONTEXT.md`

---

## 1. What it is

Six pieces ported to Rust from ascii.rest (https://github.com/bas3line/ascii,
MIT, `LICENSE-ascii-rest` beside this file names the commit): three colour
scenes, **misty forest** (pine ridges fading into morning fog, sunbeams
slanting through; the slow piece, see Cadence), **aurora fjord** (aurora
over a fjord, a lit cabin) and **alpine dawn** (first light on snow peaks
over a misty lake), each drawn in one of two styles (`SceneStyle`:
**dots**, the original's halftone as braille, or **pixels**, solid
half-blocks in true colour), and three text pieces, **plasma**, **lava
lamp**, and **donut**. `late_core::models::user::AsciiPiece`
(`Scene(Scene, SceneStyle)` or `Text(TextPiece)`) is the closed list of
choices, nine today; `AsciiPiece::ALL` is the picker's order, the
screensaver's default first. It is stored by key (`as_str`: `misty_forest`,
`aurora_fjord_pixels`, `donut`, ...) in the Zen layout and in
`users.settings.screensaver`.

Two surfaces draw them, both through `ui::draw_piece`:

- **The Zen ascii tile** (`../zen/CONTEXT.md`): the aurora in dots until
  `[` `]` step it through the pieces or Enter picks one from the list
  (`picker/`, a popup over the page: `j` `k` or a click, Enter picks, Esc
  closes; the pick is saved with the layout); `Z` zooms it over the page.
- **The away screensaver**: while a session is away (`/brb`, or
  `common::away::AWAY_AFTER` without a person's input), `App::screensaver`
  names the piece from Settings, Tweaks, `Screensaver` (the misty forest
  in dots by default: the slow piece, which costs an away session about
  what idling did; the lively pieces and Off are the choices; Enter on the
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
|-- piece.rs            # TextFrame / ShadedFrame / Picture, the frame clock, the shared frame cache, the startup warm-up, JS helpers
|-- misty_forest.rs     # scene: forest, fog banks, cloud and beams built once (OnceLock), drifted per frame; the slow piece, its beams on a faster clock in the crawl
|-- aurora_fjord.rs     # scene: land built once (OnceLock), sky + water per frame
|-- alpine_dawn.rs      # scene: the range raymarched once (OnceLock), tinted + mirrored per frame
|-- plasma.rs           # the field, drawn at whatever size it is given
|-- lava_lamp.rs        # 30x27 text art, the glass built once
|-- donut.rs            # 40x22 text art
|-- ui.rs               # draw_piece: a scene as pixels or braille dots by its style, centring for text art
|-- picker/             # the piece picker a Zen ascii tile opens: state.rs, input.rs, ui.rs
|-- fixtures/           # golden frames recorded from the TS originals
|-- LICENSE-ascii-rest  # the MIT notice the port carries
```

## 3. Contracts

- **A piece is a pure function of play time.** `frame(t)` (plasma:
  `frame(t, cols, rows)`) holds no state between frames, so a frame is the
  same for every session and every replica: nothing to sync, nothing to
  persist.
- **One clock, two cadences.** `piece::clock_now` is milliseconds since
  the process's first ask, shared by every session, so two people away at
  once watch the same frame; `piece::frame_index` turns it into a piece's
  own edge by its `Cadence`, and `seconds` an edge into play time.
  `Cadence::Half` (every lively piece): a frame every `FRAME_MS` (132ms,
  the half tier) at the wall clock's pace. `Cadence::Slow` (the misty
  forest): a frame every `SLOW_FRAME_MS` (1s, the 1Hz edge the idle floor
  already takes) with play time at `SLOW_RATE` (0.01) of the wall clock,
  a crawl. What a session pays for a piece is the cells that change per
  frame times the frames per second. At the crawl the forest's fog is all
  but still, so the forest plays its beams ten times faster than its air
  (`misty_forest::crawl`, `BEAM_PACE`): the sway and breathing of the
  sunbeams is the motion the screensaver shows, and it is cheap, a few
  dozen cells a frame on a 200x50 terminal (`ui_test.rs`,
  `the_slow_piece_moves_a_few_cells_a_frame`, holds the budget at a
  hundred): about a kilobyte a second per away session, against tens of
  kilobytes a frame for a lively scene. That is what lets the screensaver
  default to on. The dots style's rounded ink is part of that arithmetic:
  without it, most of a frame's changed cells are colours that wobbled by
  one.
- **One frame per edge for the process.** `piece::picture` serves the
  fixed-size pieces (the scenes, lava lamp, donut) from a process-wide cache
  keyed by piece and edge; it computes outside the lock, and two sessions
  racing on one edge both compute it (a frame of CPU, nothing else).
  Plasma is drawn to the area's size, so it is computed per call.
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
  over the ground on a steep curve (dim sky and trees fall to dark, fog
  and sun stand out), held to sixteen steps, and every ink is rounded to
  eight steps a channel, so a cell changes only when the scene
  moves it a visible step. Text art is centred, cropped evenly when larger
  than the area, in one theme ink per piece
  (`ui::ink`).
- **Cadence in the tick.** A drawn Zen ascii tile, or the screensaver,
  repaints on its cadence's edge (`App::ascii_edge`: the half edge for a
  lively piece, the 1Hz edge for the slow one) and a lively one asks
  `wake_hint` for `ANIM_HALF_TICK` (`App::lively_ascii_visible`); the slow
  piece asks for nothing, the idle floor carries its edge. The screensaver
  returns its tier ahead of every other check, since it covers everything
  and a pointer moving over it must not open the hot window.

## 4. Porting a piece

1. Port the TS file line by line into `<slug>.rs` with a `frame` that
   returns a `TextFrame` (or a `ShadedFrame` for a coloured scene: its
   `ground`, and per cell the level, the raw colour, and the ink the
   original gives its largest dot, `want = 1` in its colour step; keep the
   original's `dot` and palette under `#[cfg(test)]` for the golden frame).
   Keep
   JavaScript's semantics where they decide a glyph: `Math.round` is
   `piece::js_round` (halves up), `x | 0` is `piece::js_i32`, `Math.imul`
   and `>>>` are `u32` wrapping arithmetic, and a `Float32Array` is a
   `Vec<f32>` (store rounded, read back as `f64`).
2. Add the variant to `Scene` or `TextPiece` (late-core: key, label,
   `ALL`), its entries to `AsciiPiece::ALL` and `AsciiPiece::as_str`, and
   an arm in `piece::picture` (and `ui::ink` for a text piece); the
   compiler finds the rest (the cycle test's list). The pickers and the
   Tweaks row read `ALL` and `label`.
3. Record golden frames: run the original under node (23+ strips the
   types) for `frame(t)` at t = 0, 1, 2.5, write each to
   `fixtures/<slug>_t<t>.txt`, and assert the port's `to_text` equals them
   (`donut_test.rs` is the shape). A scene is recorded once, at t = 1,
   with its colours (`frame(1, { color })`, the nearest-colour cache
   removed, see Gotchas) and checked through its `dot`
   (`alpine_dawn_test.rs`).
4. Name the file in `LICENSE-ascii-rest`.

## 5. Gotchas

- ascii.rest's scenes cache their nearest-colour lookup by 5-bit colour
  bucket, filled by whichever colour asks first, so their colours depend on
  draw order (about a tenth of lit cells differ from an exact lookup). The
  ports look up exactly; the `fixtures/*_t1.colors` were recorded from the
  originals with that cache removed. The dots do not depend on it.
- Alpine dawn raymarches its range once per process (`OnceLock`), about
  half a second of one core. `piece::warm` builds it and the aurora's land
  on a blocking thread at startup (`main.rs`), so no session pays it under
  the app lock; a test binary pays it on the first scene test.
- ascii.rest's `paper` (a light page flipping the ramp) is not ported:
  every piece draws its dark-ground ramp.
- A lively piece as the screensaver (a full-screen two-colour picture at
  ~7.5fps per away session) has not had its bandwidth measured on the
  wire; `late_ssh_render*` and the output-budget metrics are where it
  would show. The default is the slow piece for that reason: its cost is
  pinned by the cell budget test, not estimated.
