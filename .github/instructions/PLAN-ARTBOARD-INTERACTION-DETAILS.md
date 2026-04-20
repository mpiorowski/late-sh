# PLAN — Artboard: the artist's feel for the tool

This document specifies the **user ↔ canvas interaction model** for late-sh's embedded dartboard ("Artboard"). It is the authoritative reference for ergonomics: how the cursor moves, how selections are formed, how swatches capture and replay, how floating selections stamp, when brushstrokes fire and when they don't, what every key and every mouse gesture does under every mode.

The source of truth for this behavior is standalone dartboard. The key files, with the specific ranges you'll want on screen while implementing:

- `~/p/my/dartboard/dartboard/src/app.rs` — all interaction. Notable regions:
  - `app.rs:2081-2252` — the top-level `handle_event_inner` dispatch (key vs mouse, picker vs help vs canvas).
  - `app.rs:2254-2376` — `handle_key`, including the "floating swallows / dismisses / falls through" logic.
  - `app.rs:2010-2069` — `handle_control_key` and `handle_alt_key`.
  - `app.rs:1727-1864` — picker keyboard and mouse.
  - `app.rs:1165-1227` — swatch push / pin / activation.
  - `app.rs:1299-1456` — paint stroke and brushstroke geometry.
  - `app.rs:1506-1620` — draw-border glyph choices per selection shape.
  - `app.rs:1866-1914` — bracketed paste, backspace, delete.
  - `app.rs:138-171` — ellipse `contains` math.
- `~/p/my/dartboard/dartboard-core/src/ops.rs` — the op enum (`PaintCell`, `ClearCell`, `PaintRegion`, `ShiftRow`, `ShiftCol`, `Replace`) and `apply`. This is the vocabulary the artist's actions ultimately become.
- `~/p/my/dartboard/dartboard-core/src/canvas.rs` — wide-glyph semantics, `glyph_origin`, `is_continuation`, `put_glyph_colored`, the row/col shift primitives.
- `~/p/my/dartboard/dartboard-core/src/wire.rs` — `ClientMsg` / `ServerMsg` / `Peer`.
- `~/p/my/dartboard/dartboard-tui/src/lib.rs` — the reusable ratatui widget and its `SelectionView` / `FloatingView`.

Any late-sh implementation detail that conflicts with the behavior described here is a divergence to be closed, not a preference to be preserved.

**In scope (must match dartboard):** modes, cursor, selection, swatches, floating, brushstrokes, cut/copy/paste nuance, row/column shifts, wide-glyph handling, keybindings, mouse gestures, the emoji picker's contract, and any other interaction-level detail — the list in §§1-16 is representative, not exhaustive. If you find something in `app.rs` that shapes how the tool *feels* and isn't documented here, document it — default to "this matters" rather than "this is implementation detail."

**Out of scope — acceptable divergences:** chrome/title/layout of surrounding UI, how the user connects (SSH session vs. standalone launch), how the user's color is chosen (server-assigned in both cases, but the policy can differ), arcade-shell keys like `Ctrl+Q` for leave, help-overlay styling.

---

## 1. Mental model

The artist sits in front of a fixed-size grid canvas with:

- A **cursor** (the "nib") — always on a single cell, the point of action.
- A **viewport** — the visible window onto a larger canvas; scrolls with the cursor or pans independently.
- A **mode** — either `Draw` (default: type to paint) or `Select` (an active rectangle/ellipse exists).
- Up to five **swatches** — named slots that hold captured clipboard regions. Think of them as a painter's palette of small stencils. Any swatch can be **pinned** so it never gets evicted by newer captures.
- An optional **floating selection** — a stencil "lifted off the canvas" that follows the cursor. It's the tool through which all repeat-stamping and drag-painting happens.
- An optional **emoji picker** — a modal catalog that, when confirmed, inserts one glyph at the cursor.

The cursor is always present. At any moment, **at most one of** `{ active selection, floating selection, emoji picker }` is in play. Entering one cancels the others in ways made explicit below.

Every glyph that lands on the canvas is colored with the artist's server-assigned color. There is no separate color picker — the server decides at connect time and that color is how this user's marks are identified to everyone else.

Every paint action produces a **canvas op**: `PaintCell`, `ClearCell`, `PaintRegion` (a batched list of per-cell writes used for atomic multi-cell moves), or `ShiftRow`/`ShiftCol` (whole-row/column slides). Ops are submitted to the server and echoed back as `OpBroadcast`s; there is no CRDT, so concurrent writes to the same cell resolve by server-assigned sequence number (last write wins).

---

## 2. Cursor and viewport

### Cursor movement

- `Left` / `Right` / `Up` / `Down` — move one cell in that direction. If the cursor was in `Select` mode, the plain arrow **exits Select and clears the selection** before moving.
- `Home` — jump to the leftmost visible column on the current row (not the canvas left edge — the **visible** left edge).
- `End` — jump to the rightmost visible column.
- `PageUp` — jump to the topmost visible row.
- `PageDown` — jump to the bottommost visible row.
- `Enter` — typewriter-style `move_down`. It advances y by 1 **and leaves x alone**; it does not wrap back to column 0. This is a deliberate divergence from terminal conventions: the artist can stamp a vertical column by pressing Enter repeatedly.
- Shift + any movement key — enter `Select` mode and extend the selection to the new cursor position (see §3).

The cursor never moves past the canvas edges, and it never silently wraps around. A movement that would go out of bounds is a no-op.

### Viewport

The viewport scrolls **just enough** to keep the cursor visible — one column or row at a time. There is no recentering; if you move one cell past the right edge, the viewport shifts right by exactly one. This is important to the artist's feel: the view doesn't jump. Independent panning gives you jumpier control:

- `Alt + Arrow` — pan the viewport one cell in that direction. The cursor **does not move**; if panning would leave the cursor off-screen, the cursor stays pinned at the nearest visible edge.
- `Ctrl + Shift + Arrow` — same as `Alt + Arrow`. This pair exists because some terminals steal `Alt + Arrow`.
- **Right-mouse drag** — grab and pan the viewport. See §8.

Viewport and cursor are always clamped so the viewport never extends past the canvas edge and the cursor is always inside the viewport.

---

## 3. Selection

A selection is a region bounded by an **anchor** (set when the selection begins) and the current **cursor** (the other corner). Selections have a **shape** — either `Rect` (default) or `Ellipse`.

### Entering Select mode

- `Shift + Arrow` / `Shift + Home` / `Shift + End` / `Shift + PageUp` / `Shift + PageDown` — if no selection exists, anchor at the current cursor and enter `Select`; in either case, extend to the new cursor position. Shape stays `Rect`.
- **Left-mouse drag** — on mouse-down, position the cursor at the clicked cell and record a drag origin; on drag, anchor the selection at the origin and follow the cursor. Shape is `Rect` unless a modifier overrides (below).
- **Ctrl + left-mouse drag** — same as left-drag, but shape is `Ellipse`.
- **Alt + left-click on canvas with an existing selection** — extends the current selection to the clicked point without creating a new one.

### In Select mode

- Typing any character **fills the entire selection** with that glyph. The fill respects the selection shape: an ellipse fills only cells inside the ellipse. Wide glyphs are placed with their continuation cell if both halves are inside the selection; otherwise they're skipped.
- `Backspace` or `Delete` — fills the selection with spaces (i.e. clears every cell in it).
- `Ctrl + T` — **transpose the selection corners**. The anchor and cursor swap. Useful when you started a drag from the wrong corner and want to resize from the opposite one without redoing the whole selection.
- `Ctrl + Space` — **smart-fill**. Picks a glyph based on the selection's bounding box:
  - 1 × N (column) → `|`
  - N × 1 (row) → `-`
  - Everything else (including 1 × 1) → `*`
- `Ctrl + B` — **draw a border**. Only works when a selection exists. For `Rect` shapes, draws ASCII corners (`.` top, `` ` `` bottom-left, `'` bottom-right) with `-` top/bottom and `|` sides; 1 × N degenerates to `.` on the ends with `|` between, N × 1 to `.` ends with `-` between, 1 × 1 to a single `*`. For `Ellipse`, draws `*` on every selected cell that has an unselected neighbor (i.e. the boundary).
- `Ctrl + C` — **copy** the selection into swatch 0 (see §4). The canvas is unchanged.
- `Ctrl + X` — **cut**: copy the selection into swatch 0 **and** fill it with spaces.
- `Ctrl + V` — **paste swatch 0**: stamp `swatches[0]` at the cursor position as a one-shot paste. Does **not** enter floating mode. `None` cells in the swatch stamp as blanks (opaque paste).

### Leaving Select mode

- `Esc` — clears the selection and returns to `Draw`.
- **Any plain arrow / Home / End / PageUp / PageDown** — clears the selection and moves the cursor.
- **Left-mouse click on the canvas without a modifier, when a selection already exists** — clears the current selection, positions the cursor at the clicked cell, and records a new drag origin. The click itself does not enter `Select`; only a subsequent drag does.

### Shape fidelity

The `Ellipse` shape is load-bearing: it affects fill (only cells inside the ellipse get painted), border (only boundary cells), cut/copy (only selected cells are captured; cells outside the ellipse bounding box are captured as `None`), and smart-fill. A selection is not "basically a rectangle with a shape flag" — every op that reads the selection honors the shape.

The exact ellipse containment test (from `app.rs:143-171`):

```rust
fn contains(self, pos: Pos) -> bool {
    let bounds = self.bounds();
    if pos.x < bounds.min_x || pos.x > bounds.max_x
        || pos.y < bounds.min_y || pos.y > bounds.max_y {
        return false;
    }
    match self.shape {
        SelectionShape::Rect => true,
        SelectionShape::Ellipse => {
            if bounds.width() <= 1 || bounds.height() <= 1 {
                return true; // degenerate: 1xN or Nx1 ellipse == rect
            }
            let px = pos.x as f64 + 0.5;
            let py = pos.y as f64 + 0.5;
            let cx = (bounds.min_x + bounds.max_x + 1) as f64 / 2.0;
            let cy = (bounds.min_y + bounds.max_y + 1) as f64 / 2.0;
            let rx = bounds.width()  as f64 / 2.0;
            let ry = bounds.height() as f64 / 2.0;
            let dx = (px - cx) / rx;
            let dy = (py - cy) / ry;
            dx * dx + dy * dy <= 1.0
        }
    }
}
```

The `+0.5` is cell-center sampling — match it exactly. The degenerate cases (1 × N or N × 1) short-circuit to rect behavior so a thin ellipse is a line, not an empty set.

Cut/copy from an ellipse normalizes to the bounding rect but fills non-selected cells with `None` (`app.rs:1057-1076`):

```rust
fn capture_selection(&self, selection: Selection) -> Clipboard {
    let bounds = selection.bounds().normalized_for_canvas(&self.canvas);
    let mut cells = Vec::with_capacity(bounds.width() * bounds.height());
    for y in bounds.min_y..=bounds.max_y {
        for x in bounds.min_x..=bounds.max_x {
            let pos = Pos { x, y };
            let include = selection.contains(pos)
                || self.canvas.glyph_origin(pos)
                    .is_some_and(|origin| selection.contains(origin));
            cells.push(include.then(|| self.canvas.cell(pos)).flatten());
        }
    }
    Clipboard { width: bounds.width(), height: bounds.height(), cells }
}
```

Note the wide-glyph escape hatch: a cell whose glyph *origin* is inside the selection is included even if the cell itself is a continuation outside the selection. This prevents half-glyph captures.

---

## 4. Swatches

Swatches are a five-slot palette of captured clipboard regions. They are how every copy/cut/paste/stamp flow works; there is no hidden unified clipboard.

### Capture

- `Ctrl + C` captures the current selection into swatch 0 (see "Push semantics" below). With no selection, it captures the **single cell under the cursor** (a 1 × 1 swatch).
- `Ctrl + X` does the same capture and then fills the captured region with spaces.
- Both `Ctrl + C` and `Ctrl + X` are **no-ops while floating** — they don't interfere with the in-flight stamp.

### Push semantics

Pushing a new swatch does **not** simply insert-at-0-and-shift. Pinned slots are immune. The exact algorithm (from `app.rs:1165-1189`):

```rust
fn push_swatch(&mut self, clipboard: Clipboard) {
    let unpinned_slots: Vec<usize> = (0..SWATCH_CAPACITY)
        .filter(|&i| !matches!(&self.swatches[i], Some(s) if s.pinned))
        .collect();
    if unpinned_slots.is_empty() {
        return; // all pinned — push is silently dropped
    }

    let mut queue: Vec<Swatch> = unpinned_slots
        .iter()
        .filter_map(|&i| self.swatches[i].take())
        .collect();
    queue.insert(0, Swatch { clipboard, pinned: false });
    queue.truncate(unpinned_slots.len());

    for (slot_idx, swatch) in unpinned_slots.iter().zip(queue.into_iter()) {
        self.swatches[*slot_idx] = Some(swatch);
    }
}
```

Walkthrough with slots = `[A, B(pinned), C, D(pinned), E]` and pushing `X`:

1. Unpinned indices = `[0, 2, 4]`, their values = `[A, C, E]`.
2. Queue after prepend = `[X, A, C, E]`; truncated to 3 = `[X, A, C]`.
3. Write back into `[0, 2, 4]` → final slots = `[X, B(pinned), A, D(pinned), C]`.

`E` is evicted; `B` and `D` don't move; new arrival lands at slot 0. Pins are positional anchors, not priority flags.

### Activation (entering floating from a swatch)

- `Ctrl + A` / `Ctrl + S` / `Ctrl + D` / `Ctrl + F` / `Ctrl + G` — activate swatch 0 / 1 / 2 / 3 / 4 respectively.
- **Left-click on a swatch's body zone** — same as the home-row activation for that slot.
- **Left-click on a swatch's pin zone** — toggles the pin state of that slot. No-op on empty slots.

Activation semantics (from `app.rs:1205-1227`):

```rust
pub fn activate_swatch(&mut self, idx: usize) {
    if idx >= SWATCH_CAPACITY { return; }
    let Some(swatch) = self.swatches[idx].as_ref() else { return }; // empty slot: no-op
    match self.floating.as_mut() {
        Some(floating) if floating.source_index == Some(idx) => {
            // Re-activating the same swatch: toggle transparency.
            floating.transparent = !floating.transparent;
        }
        _ => {
            // Different swatch (or nothing floating): replace & reset to opaque.
            let clipboard = swatch.clipboard.clone();
            self.end_paint_stroke();
            self.floating = Some(FloatingSelection {
                clipboard,
                transparent: false,
                source_index: Some(idx),
            });
            self.clear_selection();
        }
    }
}
```

The `end_paint_stroke()` call is load-bearing: activating a swatch mid-drag closes the current stroke as an undo unit before the new stencil takes over.

### Pinning

- Pinned swatches never get evicted by new captures.
- Pinned swatches keep their slot index (they don't shuffle).
- Pin state persists across floating activations — entering and leaving floating doesn't change what's pinned.
- There is no keyboard shortcut to toggle pin state; it's mouse-only via the pin zone.

### Swatch contents

A swatch stores a width × height grid of `Option<CellValue>` (glyph-or-empty). Cells outside the captured selection shape (for ellipse or irregular captures) are `None`. The swatch remembers the **cell values**, not the colors — when stamped, the stamping user's active color is used, not the original author's.

---

## 5. One-shot paste vs. floating

This distinction is central to the artist's feel. They are two different operations:

### `Ctrl + V` — one-shot paste

When **no floating selection is active**, `Ctrl + V` stamps `swatches[0]` at the cursor **once** and returns to `Draw`. The stamp is **opaque**: `None` cells in the swatch stamp as blanks, erasing whatever was underneath.

### `Ctrl + V` — repeat stamp

When **a floating selection is active**, `Ctrl + V` stamps the floating content at the cursor and **keeps floating active**. The preview remains under the cursor; you can move and stamp again. This is how you tile a pattern or lay a stencil along a path without having to re-activate the swatch each time.

### Activation vs. paste

- Activating a swatch (`Ctrl + A..G` or clicking the swatch body) **enters floating mode** with that swatch's contents.
- `Ctrl + V` with no floating active **does not enter floating**. It's a bare paste-and-done.

If you want "lift this region and carry it around":

1. Make a selection.
2. `Ctrl + X` to cut (pushes to swatch 0, clears the source region).
3. `Ctrl + A` to activate swatch 0 (enters floating with the captured content).

This is the canonical lift-and-move flow.

---

## 6. Floating selection (the stamp mode)

A floating selection is the artist's most powerful tool. It's a width × height stencil pinned to the cursor, rendered as a preview, and committed to the canvas via explicit actions.

### Entering floating

There are only three ways to enter floating:

1. **Swatch activation** — `Ctrl + A..G` or clicking a swatch body. Content = the swatch. Opacity = `opaque`. Any active selection is cleared.
2. **(late-sh addition only)** — explicit "lift" via a dedicated key, if late-sh adds one. Standalone dartboard has no dedicated lift binding; the standard flow is cut-then-activate.
3. **Mouse click on a swatch** — same as keyboard activation.

Entering floating always:

- Ends any in-flight paint stroke (see §7).
- Clears any active selection.
- Resets the stamp mode to `opaque` (unless re-activating the same swatch, which toggles).

### Moving the floating selection

The floating selection follows the cursor. All cursor movements (arrows, Home/End, PageUp/PageDown, mouse `Moved`, mouse `Left Down` on canvas) reposition the preview without committing anything.

- `Left` / `Right` / `Up` / `Down` — move the cursor (and the preview with it). No modifiers.
- `Home` / `End` / `PageUp` / `PageDown` — jump-move, still no commit.
- `Alt + Arrow` — pan the viewport without moving the cursor. The preview moves along with the cursor because the cursor hasn't moved — but the viewport has, so the preview appears to slide across the screen.
- **Mouse `Moved`** — reposition the cursor (and preview) to the hovered cell.

### Opacity

- `opaque` mode — `None` cells in the stencil stamp as blanks, erasing what's underneath.
- `transparent` mode — `None` cells pass through: whatever is already on the canvas under them stays put.

Toggles:

- `Ctrl + T` — toggle transparency of the current floating selection.
- **Re-activating the same swatch** (`Ctrl + A` when swatch 0 is already floating) — also toggles transparency. This doubles as a convenient "flip between opaque and transparent" shortcut: same-swatch press two times flips; different-swatch press swaps and resets to opaque.

### Committing

- `Ctrl + V` — stamp at cursor, **keep floating active**. The preview remains and you can stamp again.
- `Enter` — stamp at cursor and **dismiss floating**. This is the "commit" commit. (In late-sh, a single `PaintRegion` op is emitted containing both the source-region clears and the destination paints, so the move is atomic against concurrent peer writes.)
- **Mouse `Left Down` on canvas** — start a paint stroke (see §7). Click-without-drag still counts as a one-cell stroke and stamps.

### Dismissing

- `Esc` — dismiss floating without stamping.
- **Mouse `Right Down`** — dismiss floating without stamping.
- **Typing any character that isn't a floating-specific binding** — dismisses floating and then processes the typed character normally. Practically: if you're holding a stencil and you press `x`, floating goes away and `x` gets typed at the cursor. This means you never need to explicitly dismiss before resuming normal typing.

The exact swallow-vs-dismiss decision lives in `app.rs:2254-2304`:

```rust
fn handle_key(&mut self, key: KeyEvent) {
    if self.floating.is_some() {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt  = key.modifiers.intersects(KeyModifiers::ALT | KeyModifiers::META);

        match key.code {
            KeyCode::Char('t') if ctrl => { self.toggle_float_transparency(); return; }
            KeyCode::Char(ch)  if ctrl && swatch_home_row_index(ch).is_some() => {
                self.activate_swatch(swatch_home_row_index(ch).unwrap()); return;
            }
            KeyCode::Char('c') | KeyCode::Char('x') if ctrl => { return; } // swallow, no-op
            KeyCode::Char('v') if ctrl => { self.stamp_floating(); return; } // repeat-stamp
            KeyCode::Esc => { self.dismiss_floating(); return; }

            // Plain arrows move cursor (and the floating preview with it).
            KeyCode::Up    if !ctrl && !alt => { self.move_up();    return; }
            KeyCode::Down  if !ctrl && !alt => { self.move_down();  return; }
            KeyCode::Left  if !ctrl && !alt => { self.move_left();  return; }
            KeyCode::Right if !ctrl && !alt => { self.move_right(); return; }

            // Alt-anything: keep floating, let the alt handler below process it
            // (e.g. Alt+Arrow pans, Alt+C exports to system clipboard).
            _ if alt => {}
            // Ctrl+Shift+Arrow: keep floating, let ctrl handler handle pan.
            KeyCode::Up | KeyCode::Down | KeyCode::Left | KeyCode::Right
                if ctrl && key.modifiers.contains(KeyModifiers::SHIFT) => {}

            _ => { self.dismiss_floating(); /* FALL THROUGH — key processed below */ }
        }
    }
    // ... non-floating key handling continues here ...
}
```

Important structural detail: the outer `_ => dismiss_floating()` arm does **not** `return`. It falls through to the normal key handler so the same keystroke that dismissed the float is also the keystroke that types a glyph / fills a selection / invokes `Ctrl+B` / whatever. This is why "press `x` while floating" types an `x` instead of eating the key silently.

Everything the floating branch explicitly returns from is "float-preserving" or "float-controlling." Everything it falls through from is "float-ending." The carve-outs are narrow: Alt-anything and Ctrl+Shift+Arrow keep floating alive so you can pan with a stencil in hand.

---

## 7. Brushstrokes (mouse painting while floating)

"Brushstrokes" in dartboard are a specific thing: they're the behavior you get when you **click-and-drag on the canvas while a floating selection is active**. Outside floating mode there is no click-drag paint; left-drag on the canvas creates a selection instead (see §8).

### Anatomy of a stroke

A stroke has four phases driven by mouse events:

1. **`Left Down`** on the canvas — `begin_paint_stroke()` captures the canvas snapshot (for undo — unused in late-sh v1), records a stroke anchor at the current cursor, and stamps the floating selection at that cursor.
2. **`Left Drag`** — for each drag event, stamp along the path from the last stamp to the current position using the rules below.
3. **`Left Up`** — `end_paint_stroke()` finalizes the stroke as one undo step.
4. **Entering floating in the middle of a drag is not possible** — floating must be active before `Left Down` for the stroke to paint. If you click on a swatch during a drag, the stroke ends and a new floating takes over; the next `Left Down` starts a new stroke.

### Stamp placement rules during drag

The stamp gate isn't "stamp at every pixel the mouse crosses." It's brush-width aware. Prose descriptions of this behavior misread easily, so the source is the spec. From `app.rs:1406-1456`:

```rust
fn paint_floating_drag(&mut self, raw_pos: Pos) {
    let Some(last) = self.paint_stroke_last else {
        self.cursor = raw_pos;
        self.paint_floating_at_cursor();
        return;
    };

    let anchor = self.paint_stroke_anchor.unwrap_or(last);
    let brush_width = self.floating_brush_width(); // max(1, floating.width)
    let is_pure_horizontal =
        brush_width > 1 && raw_pos.y == last.y && raw_pos.y == anchor.y && last.y == anchor.y;

    if is_pure_horizontal {
        // Anchor-aligned horizontal run: snap x to a brush-width grid off anchor.
        let snapped_x = Self::snap_horizontal_brush_x(anchor.x, raw_pos.x, brush_width);
        let target = Pos { x: snapped_x, y: raw_pos.y };
        if target == last { return; } // already stamped this cell
        self.cursor = target;
        self.paint_floating_at_cursor();
        return;
    }

    if brush_width > 1 && raw_pos.y == last.y {
        // Off-anchor horizontal drift: gate by brush width, no snap.
        if raw_pos.x.abs_diff(last.x) < brush_width { return; }
        self.cursor = raw_pos;
        self.paint_floating_at_cursor();
        return;
    }

    if brush_width > 1 && raw_pos.y != last.y {
        // Diagonal: walk Bresenham and stamp with the non-overlap gate.
        self.paint_floating_diagonal_segment(last, raw_pos, brush_width);
        return;
    }

    // Narrow brush (width 1): stamp every cell you cross that isn't `last`.
    if raw_pos == last { return; }
    self.cursor = raw_pos;
    self.paint_floating_at_cursor();
}
```

The horizontal-snap helper (`app.rs:1331-1341`):

```rust
fn snap_horizontal_brush_x(anchor_x: usize, raw_x: usize, brush_width: usize) -> usize {
    if brush_width <= 1 { return raw_x; }
    if raw_x >= anchor_x {
        anchor_x + ((raw_x - anchor_x) / brush_width) * brush_width
    } else {
        anchor_x - ((anchor_x - raw_x) / brush_width) * brush_width
    }
}
```

And the diagonal walker (`app.rs:1384-1404`):

```rust
fn paint_floating_diagonal_segment(&mut self, start: Pos, end: Pos, brush_width: usize) {
    let mut last_stamped = start;
    for point in Self::line_points(start, end).into_iter().skip(1) {
        let should_stamp =
            point.y != last_stamped.y || point.x.abs_diff(last_stamped.x) >= brush_width;
        if !should_stamp { continue; }
        self.cursor = point;
        self.paint_floating_at_cursor();
        last_stamped = point;
    }
    // Stamp the endpoint if the gate still allows it.
    let should_stamp_end =
        end.y != last_stamped.y || end.x.abs_diff(last_stamped.x) >= brush_width;
    if should_stamp_end {
        self.cursor = end;
        self.paint_floating_at_cursor();
    }
}
```

`line_points` is standard Bresenham (`app.rs:1348-1382`). What matters for feel:

- A 3-wide stencil dragged along an anchor row tiles on a 3-column grid regardless of mouse speed — the snap uses `anchor.x` as the modulus origin, not the canvas left edge.
- If the user strays off the anchor row, the stamp gate switches from "snap-to-anchor-grid" to "keep ≥ brush_width apart from the last stamp." The anchor grid is discarded for the rest of the stroke; it doesn't come back if you wander back onto the anchor row.
- Diagonals stamp every cell whose row changes, plus any cell that moves ≥ brush_width horizontally within the same row. A 1-wide stencil dragged along a diagonal fills every cell the Bresenham line touches.

### Opacity during stroke

Every stamp in a stroke honors the floating selection's current opacity — so a transparent stencil dragged across the canvas overlays without erasing, and an opaque one leaves a solid swath.

### Stroke ends floating? No.

A stroke ending on `Left Up` leaves floating active. You can keep stamping with `Ctrl + V`, move the cursor with keys, or start a new stroke with another `Left Down`.

---

## 8. Mouse gestures (outside floating)

Source: `app.rs:2137-2241`. Read it — the full branch structure is more legible as code than in prose.

### Left button

- **`Left Down` on canvas, no modifier** — position the cursor, clear any active selection, record a drag origin. No paint, no selection yet.
- **`Left Drag`, no modifier** — if the drag moved or we were already selecting, anchor the selection at the drag origin (from `Left Down`) and follow the cursor. Shape = `Rect`.
- **`Left Down` with `Ctrl`** — same as plain `Left Down` but the selection shape (once a drag starts) is `Ellipse`.
- **`Left Down` with `Alt`, when a selection already exists** — extend that selection to the clicked cell. Doesn't create a new anchor.
- **`Left Up`** — clear the drag origin. End of selection drag.
- **`Left Down` on a swatch body** — activate that swatch (enters floating).
- **`Left Down` on a swatch pin zone** — toggle pin.
- **`Left Down` on a help-tab hit area (when help overlay is open)** — switch help tab.

### Right button

- **`Right Down` on canvas** — begin viewport pan.
- **`Right Drag`** — pan the viewport by the drag delta from the pan origin. The cursor stays clamped to the viewport.
- **`Right Up`** — end pan.

### Movement

- **`Moved` (no button held)** — **standalone dartboard: no-op outside floating**. The cursor does not follow the mouse. (This is the model.)
- **`Moved` while floating** — cursor follows the mouse, floating preview moves with it.

### Wheel

- **`ScrollUp` / `ScrollDown`** — **no effect on the canvas**. There is no zoom. Scroll events matter only inside the emoji picker, where they move the selection index.

### Modal overlays

When the help overlay or emoji picker is open, canvas mouse gestures are ignored except for hit-testing on the overlay's own controls.

---

## 9. Keyboard: direct typing and erasure

Outside `Select`, outside floating, with the picker closed:

- **Any printable character** (including Unicode / emoji) — `insert_char`: paints the glyph at the cursor in the artist's color, then advances the cursor by the glyph's display width (1 for narrow, 2 for wide). Wide glyphs occupy their origin cell + a continuation cell.
- **`Backspace`** — move left by one column. If the new position is the continuation cell of a wide glyph, clear that cell (clears the whole glyph) and snap the cursor to the glyph's origin. Otherwise clear the cell at the new position. Net effect: `Backspace` erases the glyph to the left of the cursor, correctly handling wide glyphs.
- **`Delete`** — if the cursor is on the continuation of a wide glyph, snap it to the origin first, then clear. Net effect: `Delete` erases the glyph at the cursor regardless of which half of a wide glyph you were on.
- **`Enter`** — move down one row, x unchanged (typewriter behavior, see §2).

Inside `Select` with an anchor:

- **Any printable character** — `fill_selection_or_cell`: fill the selection (respecting shape and wide-glyph tiling) with that glyph.
- **`Backspace` / `Delete`** — fill the selection with spaces (clears it).

While floating:

- **Any printable character that isn't a floating binding** — dismiss floating, then process as above.

---

## 10. Row and column shifts

Keys to push or pull whole rows and columns. These are their own canvas ops (`ShiftRow` / `ShiftCol`) because replaying them as N cell writes would race concurrent peers.

- `Ctrl + H` or `Ctrl + Backspace` — push row left (everything right of cursor.x moves left by one, leftmost cell falls off).
- `Ctrl + J` — push column down.
- `Ctrl + K` — push column up.
- `Ctrl + L` — push row right.
- `Ctrl + Y` — pull from left (everything right of cursor shifts right; gap at cursor).
- `Ctrl + U` — pull from below (column shifts up, gap at cursor row).
- `Ctrl + I` or `Ctrl + Tab` — pull from above (column shifts down, gap at cursor row).
- `Ctrl + O` — pull from right (row shifts left toward cursor; gap at right edge).

Pushes "close a gap by shoving;" pulls "open a gap by stealing from the neighbor." These operate on the row/column through the cursor. The cursor's x (for row ops) or y (for column ops) is always taken from the cursor's glyph origin so operating on the continuation half of a wide glyph acts on the whole glyph.

---

## 11. Wide-glyph semantics

Wide glyphs (CJK, many emoji) occupy two cells: an **origin** cell holding the glyph value and a **continuation** cell marking the second half.

- Typing a wide glyph advances the cursor by 2.
- Fills and stamps never overwrite only half of a wide glyph: if a fill would land a wide glyph at `max_x` with no room for the continuation, it's skipped.
- Cut/copy normalize the selection bounds outward to include the origin of a wide glyph that was selected only by its continuation, and the continuation of one selected only by its origin. You never get a half-glyph in a swatch.
- `Backspace` and `Delete` snap to the origin before clearing (see §9).
- `Ctrl + hjkl / yuio` compute the row/column from the cursor's glyph origin so shifts operate on whole glyphs.

The artist should never have to think about wide-glyph mechanics — every tool respects them.

---

## 12. Cut / copy / paste — nuance summary

Multiple clipboard-adjacent operations, each with subtly different behavior. Get these right:

| Key | Source | Target | Canvas effect | Notes |
|---|---|---|---|---|
| `Ctrl + C` | selection, else 1 × 1 at cursor | swatch 0 | none | No-op while floating |
| `Ctrl + X` | selection, else 1 × 1 at cursor | swatch 0 | fills captured region with spaces | No-op while floating |
| `Ctrl + V` (no float) | swatch 0 | canvas at cursor | stamps opaquely | Does not enter floating |
| `Ctrl + V` (floating) | floating content | canvas at cursor | stamps, keeps floating | Repeat-stamp |
| `Alt + C` | selection, else **full canvas** | system clipboard (OSC 52) | none | Note: no-selection fallback is the whole canvas, not a single cell |
| `Ctrl + A..G` | swatch 0..4 | floating | none (until stamped) | Enters floating; same-swatch toggles transparency |

Things to internalize:

- `Ctrl + C` with no selection captures **one cell**. `Alt + C` with no selection captures the **whole canvas**. The asymmetry is deliberate: the swatch is for small stencils, OSC 52 is for sharing whole pictures.
- Copy and cut **do not affect floating state** — they're no-ops while floating so you don't accidentally overwrite your current swatch while carrying a stencil.
- There is no "clear swatch" action and no "empty a slot" key. Pinning and pushing new captures are the only ways slots change.

---

## 13. Emoji picker

A modal catalog for inserting glyphs the artist can't easily type.

### Open

- `Ctrl + ]` — open the picker.
- `Ctrl + 5` — open the picker.
- Literal GS (`\x1d`) byte — open the picker.

All three are equivalent. The multiple bindings exist because different terminals map the same key differently.

### Inside the picker

The picker is a tabbed, searchable list.

- **Typing** — appends to the search query.
- **`Backspace`** — remove a character from the search query (at the cursor position inside the search field).
- **`Left` / `Right`** — move the search-field cursor.
- **`Up` / `Down`** — move the selection up or down one item.
- **`PageUp` / `PageDown`** — move the selection by a visible-page height.
- **`Tab`** — next tab (category). Resets selection to 0.
- **`Shift + Tab`** — previous tab.
- **`Enter`** — insert the selected glyph at the cursor **and close** the picker. If floating was active, floating is dismissed first.
- **`Alt + Enter`** — insert the selected glyph and **keep the picker open**, so you can insert several glyphs in sequence.
- **`Esc`** — close the picker without inserting.

### Mouse inside the picker

- **`Left Down`** on a tab header — select that tab.
- **`Left Down`** on a list row — select that item. A **double-click** (same item within 400ms) inserts and closes, same as `Enter`.
- **`ScrollUp` / `ScrollDown`** — move the selection by 3 items.

### Insertion

An insertion from the picker is exactly equivalent to typing the glyph manually: one `PaintCell` op at the cursor, cursor advances by display width.

---

## 14. Undo and help

- `Ctrl + Z` / `Ctrl + R` — undo / redo in standalone. **Unbound in late-sh v1.** A local snapshot stack is incoherent under LWW multiplayer: replaying "undo my last op" as a series of writes can clobber a concurrent peer's unrelated work. Dartboard's standalone undo is explicitly gated to "no other writers." late-sh defers these until a CRDT-like story or per-author op-log replay exists.
- `Ctrl + P` / `F1` — toggle the help overlay. Chrome only; no canvas effect.
- `Tab` / `Shift + Tab` (outside the picker and outside help) — cycle local users in standalone's Embedded demo. **Irrelevant in late-sh** because each SSH session has exactly one user; Tab can be rebound or unbound here without affecting the interaction model.

---

## 15. Leaving the canvas

- **Standalone:** `Ctrl + Q` quits the process.
- **late-sh:** `Ctrl + Q` leaves the canvas and returns to the Games hub. This is the only intentional arcade-shell override; everything else passes through.

Bare `Esc` **must not** leave the canvas. Bare `Esc` is reserved for:

1. Dismissing a floating selection, if one is active.
2. Else clearing a selection.
3. Else clearing any transient per-session UI state (e.g., brush sampling — a late-sh concept).

The arcade convention of "Esc = back" is explicitly overridden here.

---

## 16. Bracketed paste

When the terminal sends a bracketed paste, `app.rs:1866-1896` applies:

```rust
fn paste_text_block(&mut self, text: &str) {
    if text.is_empty() { return; }
    let origin = self.cursor;                 // paste origin x is remembered
    let color  = self.active_user_color();
    self.apply_canvas_edit(|canvas| {
        let mut x = origin.x;
        let mut y = origin.y;
        for ch in text.chars() {
            match ch {
                '\r' => {}                    // CR dropped; CRLF folds via LF
                '\n' => {
                    x = origin.x;             // WRAP TO ORIGIN X, not column 0
                    y += 1;
                    if y >= canvas.height { break; }
                }
                _ => {
                    if x < canvas.width && y < canvas.height {
                        let _ = canvas.put_glyph_colored(Pos { x, y }, ch, color);
                    }
                    x += Canvas::display_width(ch);   // wide glyphs advance by 2
                }
            }
        }
    });
}
```

Key behaviors that shape artist feel:

- Newlines wrap to the **column where the paste started**, not column 0. Pasting a block of ASCII art preserves its shape relative to the cursor.
- Characters that would land past the right edge are silently discarded for that line; the next `\n` picks up on the next row from the origin column. Paste stops on the first row that lands past the bottom.
- Control characters other than `\r`/`\n` are dropped. No tabs, no form feeds.
- All pasted glyphs use the artist's color, not whatever was on the clipboard source.

Late-sh's `paste_bytes` (`late-ssh/src/app/games/dartboard/state.rs:160-219`) implements the same shape but with per-cell `PaintCell` ops and a `std::str::from_utf8` gate — non-UTF-8 payloads are dropped rather than inserted as mojibake.

Paste does **not** consume an active selection; it paints over it at the cursor. Any subsequent edit continues from wherever the paste left the cursor.

---

## 17. Finer points and quiet invariants

Miscellany that doesn't fit neatly under a single topic, each of which shapes the artist's feel and all of which can be missed by a casual read of `app.rs`.

### `Ctrl + T` is overloaded by context

- While **floating**: toggle transparency. Handled in the floating branch of `handle_key` (`app.rs:2262-2265`).
- While **selecting with an anchor**: transpose the selection's anchor and cursor corners. Returns `true` / `false` from `handle_control_key` at `app.rs:2046` — it's a no-op if no anchor exists.
- Otherwise: no-op.

Same keystroke, three meanings. Implementations must dispatch by state before acting.

### `Ctrl + V` is overloaded by context

- While **floating**: repeat-stamp (`stamp_floating`, preserves floating).
- Otherwise: one-shot paste of `swatches[0]` (`paste_clipboard`). Does **not** enter floating.

These two paths take different code routes on purpose — the one-shot is cheaper and doesn't clear the current selection.

### Alt+C differs from Ctrl+C

- `Ctrl + C`: copy selection (or 1 × 1 at cursor) to **swatch 0**.
- `Alt + C`: copy selection (or **full canvas**) to the **system clipboard** via OSC 52.

The no-selection fallback differs — swatch is for snippets, system clipboard is for the whole piece. See `system_clipboard_bounds` at `app.rs:932-936` and `copy_to_system_clipboard` at `app.rs:1141-1144`.

### Copy/cut are no-ops while floating

Both `copy_selection_or_cell` (`app.rs:1078-1092`) and `cut_selection_or_cell` (`app.rs:1146-1163`) start with `if self.floating.is_some() { return; }`. Also reinforced in the floating branch of `handle_key`: `KeyCode::Char('c') | KeyCode::Char('x') if ctrl => { return; }`. The swatch palette is explicitly protected while the artist is carrying a stencil.

### `Enter` has two meanings

- While **floating**: commit the stamp and dismiss. (Note: the current standalone binding does **not** wire Enter to commit — committing is `Ctrl + V` plus `Esc`, or mouse `Left Down`; this spec simplifies the flow by binding `Enter` as an explicit commit for late-sh. If you find this mismatch when reading `app.rs:2360`, prefer the spec — `Enter` ⇒ `commit_floating` ⇒ `move_down` for late-sh. Standalone's `Enter` in floating falls through and dismisses on character-type-through.)
- Else: typewriter `move_down`, x unchanged.

### Empty-slot activation is silent

`activate_swatch` returns immediately if the slot is `None`. Pressing `Ctrl + A` before any cut/copy has happened does nothing. No sound, no error, no feedback — the artist just doesn't enter floating.

### `Ctrl + Tab` == `Ctrl + I`

Both bind to `pull_from_up` (`app.rs:2039`). Provided because some terminals eat `Ctrl + I` and remap it to Tab. Emoji pickers also use `Tab` for tab-switching, so keybind-readers need to distinguish picker-open vs canvas contexts.

### Plain `Tab` (no Ctrl) outside the picker

Cycles the active local user in standalone's Embedded 5-user demo (`app.rs:2119-2127`). In late-sh, each SSH session has exactly one user — this binding has no purpose and can be repurposed or unbound without breaking the interaction model.

### `Esc` layering

`Esc` has a strict priority order:

1. If emoji picker is open → close picker.
2. Else if help overlay is open → close help.
3. Else if floating is active → dismiss floating.
4. Else if selection is active → clear selection (return to `Draw`).
5. In late-sh's shell: otherwise clear transient per-session state (e.g. brush sampling).

`Esc` **never** exits the canvas. That's `Ctrl + Q`.

### Swatch home-row vs Draw-mode typing

`Ctrl + A..G` activates swatches; the **same keys without Ctrl** type `a`..`g` at the cursor. There's no modal fight here — the Ctrl modifier is the only distinction. Implementations must not treat `a`..`g` specially at any layer above `handle_control_key`.

### Draw-mode character fallback vs Select-mode character fill

From `app.rs:2362-2374`:

```rust
_ if self.mode.is_selecting() && self.selection_anchor.is_some() => match key.code {
    KeyCode::Char(ch)                       => self.fill_selection_or_cell(ch),
    KeyCode::Backspace | KeyCode::Delete    => self.fill_selection_or_cell(' '),
    _ => {}
},
_ => match key.code {
    KeyCode::Char(ch)       => self.insert_char(ch),
    KeyCode::Backspace      => self.backspace(),
    KeyCode::Delete         => self.delete_at_cursor(),
    _ => {}
},
```

The `is_selecting() && anchor.is_some()` guard is load-bearing: `Select` mode without an anchor (which can happen transiently) still types normally. But the moment the anchor exists, every character types into the selection as a fill, not at the cursor.

### Wide-glyph in `backspace` and `delete`

```rust
fn backspace(&mut self) {                     // app.rs:1898-1906
    self.move_left();
    let origin = self.canvas.glyph_origin(self.cursor);
    let cursor = self.cursor;
    self.apply_canvas_edit(|canvas| canvas.clear(cursor));
    if let Some(origin) = origin {
        self.cursor = origin;
    }
}

fn delete_at_cursor(&mut self) {              // app.rs:1908-1914
    if let Some(origin) = self.canvas.glyph_origin(self.cursor) {
        self.cursor = origin;
    }
    let cursor = self.cursor;
    self.apply_canvas_edit(|canvas| canvas.clear(cursor));
}
```

Backspace: step back one column, clear at that position, then snap to origin if we landed on a continuation. Delete: snap to origin first, then clear. The asymmetry is deliberate — backspace moves and then normalizes, delete normalizes and then clears in place.

### Viewport auto-scroll is per-cell, not per-page

`scroll_viewport_to_cursor` (`app.rs:760-773`) shifts the viewport origin by the exact distance needed to bring the cursor back into view — one column or row at a time. It never jumps. This matters for feel: the view glides with the cursor rather than snapping in chunks.

### `Home`/`End`/`PageUp`/`PageDown` jump to **visible** bounds, not canvas bounds

```rust
KeyCode::Home     => self.cursor.x = self.visible_bounds().min_x,
KeyCode::End      => self.cursor.x = self.visible_bounds().max_x,
KeyCode::PageUp   => self.cursor.y = self.visible_bounds().min_y,
KeyCode::PageDown => self.cursor.y = self.visible_bounds().max_y,
```

(`app.rs:2338-2341` in Draw; `2356-2359` in shifted select; `handle_key` in general.) The cursor can only reach canvas edges that are actually scrolled into view. To reach a far-off edge you must pan first.

### Mouse `Moved` outside floating is a no-op in standalone

See `app.rs:2188-2241`: the non-floating mouse branch matches `Left/Right Down/Drag/Up` explicitly. There is no `MouseEventKind::Moved` arm. Moving the mouse across the canvas without a button held does nothing to the cursor. This is the default.

### Mouse `Moved` inside floating *does* move the cursor

`app.rs:2161-2166`:

```rust
MouseEventKind::Moved => {
    if let Some(pos) = canvas_pos {
        self.cursor = pos;
    }
}
```

So the floating preview tracks the mouse without needing a click. Combined with `Left Down` stamping at cursor, "hover then click" is how single-stamp placement works.

### Right-click dismisses floating without stamping

`app.rs:2182-2184`:

```rust
MouseEventKind::Down(MouseButton::Right) => {
    self.dismiss_floating();
}
```

Note: right-click on canvas **outside** floating begins a pan (`app.rs:2189-2193`). Same button, different meaning depending on whether you're carrying a stencil. Artists switch modes by picking up / putting down the stencil, not by holding a modifier.

### Canvas OOB on stamp skips rather than clipping

`stamp_floating` and `stamp_onto_canvas` (`app.rs:1235-1297`) both skip cells where `target_x >= canvas.width || target_y >= canvas.height`. Floating stencils that extend past the edge get truncated, not wrapped.

### `CanvasOp::Replace` exists

From `dartboard-core/src/ops.rs:27-33`:

```rust
/// Replace the entire canvas. Used for large structural edits (undo /
/// redo, paste of big regions) where itemizing per-cell writes would be
/// more expensive than just shipping a snapshot. Safe on SP; WS plan
/// will want to avoid this path for high-frequency edits.
Replace { canvas: Canvas },
```

late-sh v1 should avoid emitting `Replace` because it fights LWW fairness — one user's replace clobbers everyone's recent writes. Stick to `PaintCell` / `ClearCell` / `PaintRegion` / `ShiftRow|Col`.

### Lift-to-floating commits as one `PaintRegion`

Late-sh's `commit_floating` (`state.rs:324-368`) builds a single `PaintRegion` containing:

- `Clear` writes for every cell of the **source bounds** (the region lifted from).
- `Paint` / `Clear` writes for every cell of the **destination** (the floating stencil stamped at the new cursor).

One op, one sequence number, atomic from the server's perspective. A concurrent peer's ops are ordered strictly before or after this region — they can't interleave between the source-clear and destination-paint. This is the right default for any "lift and move" flow in a multi-writer canvas.

### Paint stroke is a client-side undo unit, not a wire unit

Each individual stamp during a stroke is its own `CanvasOp` submission to the server. `paint_canvas_before` snapshots the canvas at stroke start purely for local undo; the server sees N `PaintCell`/`PaintRegion` ops, not one batched one. Stroke begin/end only exist for the undo stack. (Since late-sh v1 has no undo, the stroke boundary is purely conceptual — it still matters for `end_paint_stroke` during swatch activation, to avoid mixing the in-flight stroke's geometry with the new stencil's.)

### Picker search cursor vs. list cursor

Inside the emoji picker, `Left` / `Right` move the search-field caret, while `Up` / `Down` move the list-selection cursor. They're different cursors in the same modal. Mouse click on a list row sets the list-selection; it doesn't touch the search caret. Double-click on a list row inserts-and-closes (window: 400ms by default).

### `Welcome` resets the canvas

A client that receives `Welcome { snapshot, .. }` mid-session replaces its local canvas with the snapshot. In practice this happens only on connect. The "Welcome race" — first user op being stomped by a late-arriving Welcome snapshot — is a real concern for Remote mode; `App::new_remote` drains until Welcome lands before returning. `LocalClient` doesn't race because `Welcome` is enqueued synchronously during `connect_local`, but late-sh preserves the drain-until-welcome invariant on startup anyway.

### `OpBroadcast` echoes to the sender too

The server sends `OpBroadcast { from, op, seq }` to every connected client **including the originator**. Clients that apply ops optimistically on the local canvas must not double-apply on echo. Late-sh's `svc.rs` runs the op through `canvas.apply` locally before submitting to the server, then ignores the echo's delta (the `Seq` watermark catches drift). Standalone uses a different pattern — it doesn't apply until the broadcast arrives in Remote mode. Either works; pick one and stick to it.

### Peer list has names and colors, not cursors

`Peer { user_id, name, color }` is the complete wire shape (`wire.rs:11-16`). There is no per-peer cursor, selection, or floating state on the wire in v1. Any "see where your friend is drawing" feature requires a protocol extension. Don't let UI wishes leak a cursor coord into `Hello` or `ClientMsg` without a spec change.

### `PeerLeft` fires on `Drop`

`impl Drop for LocalClient` calls `server.disconnect(self.user_id)` which retains-without and broadcasts `PeerLeft` (`dartboard-server/src/lib.rs:238-244`; vendored copy at `dartboard-local`). A dropped `DartboardService` therefore leaves the room cleanly; no explicit teardown call is required.

### Color-collision remap is server-side

A joining client's requested color is checked against in-use colors; if taken, the server picks the next free palette entry and returns the actual assigned color in `Welcome.your_color`. Clients must read `your_color` and display *that*, not the color they asked for. The 10-entry palette in `dartboard-local` doubles as the seat cap; the 11th connect is a `ConnectRejected` surfaced on the snapshot via `connect_rejected`.

### `apply_canvas_edit` is the single gate for canvas mutations in standalone

All direct canvas edits (typing, backspace, delete, paste, smart-fill, draw-border, stamp, push/pull) go through `apply_canvas_edit(|canvas| { ... })` at `app.rs:544-558`. It captures before/after, diffs to a `CanvasOp`, pushes the before onto undo, and submits the op via the active client. Late-sh does not use this pattern (it submits ops directly from `state.rs`), which is fine for a no-undo client — but if undo is ever added, the wrapper is the clean point to reinstate.

---

## 18. Known late-sh divergences to close

Items in current late-sh (`late-ssh/src/app/games/dartboard/`) that do **not** match this spec and should be brought into line:

1. **Left-drag paints a sampled brush.** late-sh samples the glyph under the `Left Down` point and stamps it along the drag path (the `drag_brush` concept). Dartboard has **no such behavior**: left-drag outside floating mode is for selection only. Painting happens only through floating. This is a fundamental artist-feel mismatch — remove the drag-brush and route left-drag to rectangular selection.
2. **`Ctrl + X` enters floating.** late-sh's `Ctrl + X` currently calls `lift_selection_to_floating` — it cuts the region AND enters floating immediately. In dartboard, `Ctrl + X` only cuts (pushes to swatch 0, blanks the region); entering floating is a separate step (`Ctrl + A..G` or swatch click). Align: `Ctrl + X` = cut only; floating requires activation.
3. **`Moved` always moves the cursor.** late-sh updates the cursor on every `Moved` event whether or not floating is active. Dartboard only honors `Moved` while floating. Align: outside floating, `Moved` is a no-op.
4. **Swatches not implemented.** The whole 5-slot LRU, pinning, home-row activation, and the panel UI need to ship.
5. **Floating is rectangular only, opaque only, single-stamp only.** No ellipse captures, no `Ctrl + T` transparency, no `Ctrl + V` repeat-stamp, no `Ctrl + A..G` re-activation toggle, no brushstroke mechanics.
6. **Row/column shifts unbound.** `Ctrl + hjkl / yuio` emit nothing; the `ShiftRow` / `ShiftCol` ops are never submitted.
7. **Smart-fill, draw-border, transpose unbound.** `Ctrl + Space`, `Ctrl + B`, `Ctrl + T` need to land.
8. **`Alt + Arrow` / `Ctrl + Shift + Arrow` jump the cursor.** They should pan the viewport without moving the cursor. Current behavior sends the cursor to the visible edge instead.
9. **Right-drag pan not implemented.**
10. **Selection shape is fixed to `Rect`.** Ctrl + left-drag should produce an ellipse selection; shape must propagate through fill, border, and capture.
11. **Ellipse-aware fill/border absent** (follows from 10).
12. **Emoji picker not implemented.** Reserve the open keys (`Ctrl + ]`, `Ctrl + 5`, GS) in `ParsedInput` so the eventual add doesn't rebind existing shortcuts.
13. **Typing inside a selection inserts at cursor instead of filling the selection.** In Select mode with an anchor, a character should fill the selection, not insert a single glyph.
14. **No peer list UI.** `DartboardSnapshot.peers` carries names and colors; the sidebar should show them.

Closing these gaps is the substance of the remaining integration work. None of them require new transport, new ops, or new types — every op and event needed already exists in `dartboard-core` and the `DartboardService` API. What's left is faithful reconstruction of the interaction model above.
