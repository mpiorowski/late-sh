# ascii Context

## Metadata
- Scope: `late-ssh/src/app/ascii`
- Purpose: animated ascii pieces, ported from ascii.rest, for the Zen ascii tile and the away screensaver.
- Parent context: `../../../../CONTEXT.md`

---

## 1. What it is

Four pieces ported to Rust from ascii.rest (https://github.com/bas3line/ascii,
MIT, `LICENSE-ascii-rest` beside this file names the commit): **aurora
fjord** (a colour scene: aurora over a fjord, a lit cabin), **plasma**,
**lava lamp**, and **donut**. `late_core::models::user::AsciiPiece` is the
closed list; it is stored by key in the Zen layout and in
`users.settings.screensaver`.

Two surfaces draw them, both through `ui::draw_piece`:

- **The Zen ascii tile** (`../zen/CONTEXT.md`): the aurora until `[` `]`
  step it through the pieces; `Z` zooms it over the page.
- **The away screensaver**: while a session is away (`/brb`, or
  `common::away::AWAY_AFTER` without a person's input), `App::screensaver`
  names the piece from Settings, Tweaks, `Screensaver` (the aurora by
  default; Off turns it off), and `render.rs` draws it over the whole frame
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
|-- aurora_fjord.rs     # the scene: land built once (OnceLock), sky + water per frame, the halftone `dot`
|-- plasma.rs           # the field, drawn at whatever size it is given
|-- lava_lamp.rs        # 30x27 text art, the glass built once
|-- donut.rs            # 40x22 text art
|-- ui.rs               # draw_piece: halftone sampling for the scene, centring for text art
|-- fixtures/           # golden frames recorded from the TS originals
|-- LICENSE-ascii-rest  # the MIT notice the port carries
```

## 3. Contracts

- **A piece is a pure function of play time.** `frame(t)` (plasma:
  `frame(t, cols, rows)`) holds no state between frames, so a frame is the
  same for every session and every replica: nothing to sync, nothing to
  persist.
- **One clock.** `piece::frame_index_now` counts `FRAME_MS` (264ms, the
  quarter tier) edges since the process's first ask, shared by every
  session, so two people away at once watch the same frame. `seconds`
  turns an edge into play time.
- **One frame per edge for the process.** `piece::picture` serves the
  fixed-size pieces (aurora, lava lamp, donut) from a process-wide cache
  keyed by piece and edge; it computes outside the lock, and two sessions
  racing on one edge both compute it (a frame of CPU, nothing else).
  Plasma is drawn to the area's size, so it is computed per call.
- **Drawing.** The aurora is shaded per square cell (`ShadedFrame`: a
  level and an unquantised colour per cell). `ui::draw_shaded` scales it to
  cover the area (the overflow cropped evenly), averages the two scene rows
  a terminal cell stands on, and runs the halftone (`aurora_fjord::dot`:
  dot size from brightness, ordered dither, nearest palette colour) at the
  terminal's own coordinates, so the dots stay crisp at any size, on the
  scene's ground colour. Text art is centred, cropped evenly when larger
  than the area, in one theme ink per piece (`ui::ink`).
- **Cadence.** A drawn Zen ascii tile, or the screensaver, repaints on the
  quarter edge and asks `wake_hint` for `ANIM_QUARTER_TICK`; the
  screensaver returns that tier ahead of every other check, since it covers
  everything and a pointer moving over it must not open the hot window.

## 4. Porting a piece

1. Port the TS file line by line into `<slug>.rs` with a `frame` that
   returns a `TextFrame` (or a `ShadedFrame` for a coloured scene). Keep
   JavaScript's semantics where they decide a glyph: `Math.round` is
   `piece::js_round` (halves up), `x | 0` is `piece::js_i32`, `Math.imul`
   and `>>>` are `u32` wrapping arithmetic, and a `Float32Array` is a
   `Vec<f32>` (store rounded, read back as `f64`).
2. Add the variant to `AsciiPiece` (late-core: key, label, `ALL`) and an
   arm in `piece::picture` and `ui::ink`; the compiler finds the rest
   (the Tweaks row's label).
3. Record golden frames: run the original under node (23+ strips the
   types) for `frame(t)` at t = 0, 1, 2.5, write each to
   `fixtures/<slug>_t<t>.txt`, and assert the port's `to_text` equals them
   (`donut_test.rs` is the shape).
4. Name the file in `LICENSE-ascii-rest`.

## 5. Gotchas

- ascii.rest's aurora caches its nearest-colour lookup by 5-bit colour
  bucket, filled by whichever colour asks first, so its colours depend on
  draw order (about a tenth of lit cells differ from an exact lookup). The
  port looks up exactly; `fixtures/aurora_fjord_t1.colors` was recorded
  from the original with that cache removed. The dots do not depend on it.
- ascii.rest's `paper` (a light page flipping the ramp) is not ported:
  every piece draws its dark-ground ramp.
- The screensaver's bandwidth (a full-screen halftone at ~3.8fps per away
  session) has not been measured; `late_ssh_render*` and the
  output-budget metrics are where it would show.
