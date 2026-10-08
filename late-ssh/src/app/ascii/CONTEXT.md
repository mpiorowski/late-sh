# ascii Context

## Metadata
- Scope: `late-ssh/src/app/ascii`
- Purpose: animated ascii pieces, ported from ascii.rest, for the Zen ascii tile and the away screensaver.
- Parent context: `../../../../CONTEXT.md`

---

## 1. What it is

Five pieces ported to Rust from ascii.rest (https://github.com/bas3line/ascii,
MIT, `LICENSE-ascii-rest` beside this file names the commit): two colour
scenes, **aurora fjord** (aurora over a fjord, a lit cabin) and **alpine
dawn** (first light on snow peaks over a misty lake), each drawn in one of
two styles (`SceneStyle`: **dots**, the original's halftone as braille, or
**pixels**, solid half-blocks in true colour), and three text pieces,
**plasma**, **lava lamp**, and **donut**. `late_core::models::user::AsciiPiece`
(`Scene(Scene, SceneStyle)` or `Text(TextPiece)`) is the closed list of
choices, seven today; `AsciiPiece::ALL` is the picker's order. It is stored
by key (`as_str`: `aurora_fjord`, `aurora_fjord_pixels`, `donut`, ...) in
the Zen layout and in `users.settings.screensaver`.

Two surfaces draw them, both through `ui::draw_piece`:

- **The Zen ascii tile** (`../zen/CONTEXT.md`): the aurora in dots until
  `[` `]` step it through the pieces or Enter picks one from the list
  (`picker/`, a popup over the page: `j` `k` or a click, Enter picks, Esc
  closes; the pick is saved with the layout); `Z` zooms it over the page.
- **The away screensaver**: while a session is away (`/brb`, or
  `common::away::AWAY_AFTER` without a person's input), `App::screensaver`
  names the piece from Settings, Tweaks, `Screensaver` (the aurora in dots
  by default; Off turns it off; Enter on the row opens the settings modal's
  shared picker over Off and every piece, Left/Right cycle them), and
  `render.rs` draws it over the whole frame
  ahead of every page and modal, with no click targets. The input that
  brings the session back is swallowed in `App::handle_input`, so the
  waking key never acts on the page under it (a door game included).
  A bare mouse move or a focus report is not a person
  (`common::away::is_presence_input`): it neither holds off the away clock
  (`App::last_active_at`) nor drops the screensaver. `/brb` syncs away at
  once, so the screensaver is up on the frame after the Enter.
  `late_ssh_screensavers_total{trigger=idle|brb,piece}` counts each one
  put up (`App::sync_away`).

## 2. File map

```text
late-ssh/src/app/ascii/
|-- mod.rs              # module declarations only
|-- piece.rs            # TextFrame / ShadedFrame / Picture, the frame clock, the shared frame cache, JS helpers
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
- **One clock.** `piece::frame_index_now` counts `FRAME_MS` (132ms, the
  half tier) edges since the process's first ask, shared by every
  session, so two people away at once watch the same frame. `seconds`
  turns an edge into play time.
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
  which no terminal glyph can do. `ui::draw_dots` is the dots style: the
  braille halftone, each scene row a 2x2 of dots lit by brightness (ordered
  dither at scene coordinates), one ink per cell (its lit rows' inks,
  weighted by their dots), on the ground. Text art is centred, cropped
  evenly when larger than the area, in one theme ink per piece
  (`ui::ink`).
- **Cadence.** A drawn Zen ascii tile, or the screensaver, repaints on the
  half edge and asks `wake_hint` for `ANIM_HALF_TICK`; the
  screensaver returns that tier ahead of every other check, since it covers
  everything and a pointer moving over it must not open the hot window.

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
- Alpine dawn raymarches its range on first use (`OnceLock`): about half a
  second of one core, once per process, on the first frame asked for.
- ascii.rest's `paper` (a light page flipping the ramp) is not ported:
  every piece draws its dark-ground ramp.
- The screensaver's bandwidth (a full-screen two-colour picture at ~7.5fps per away
  session) has not been measured; `late_ssh_render*` and the
  output-budget metrics are where it would show.
