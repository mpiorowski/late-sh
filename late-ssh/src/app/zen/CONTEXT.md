# Zen Context

## Metadata
- Scope: `late-ssh/src/app/zen`
- Last updated: 2026-09-10 (first cut: prototype, pure UI, no tests yet)
- Purpose: the two full-bleed pages that cut the clubhouse down to the things you keep alive.
- Status: Experimental. Reached with `7`; sits in the Tab cycle after Leaderboards. Rice is the page; `o` swaps it for the Room and back.
- Parent context: `../../../../CONTEXT.md`

---

## 1. What it is

One top-level page with no app frame, no HUD, no sidebar, and two faces
(`ZenMode`, session-local, Rice by default):

- **The Room (`o` from Rice).** One hand-drawn scene at a fixed 140x34 (`room.rs`, whole in a 140x38 terminal),
  the tavern's recipe: bigger terminals center it, smaller ones look
  through a centered window onto it, and the art never reflows. A window
  with the night outside (stars, a moon, rain; a sun by day, from the
  profile timezone), a wall clock in block digits with the record player (spinning only
  while audio is paired and unmuted) on a shelf under it, the tank on a
  cabinet with the real reef simulation inside, the bonsai standing on
  the floor at the right in a 30x28 block (`render_fitted_lines`: 1:1
  until the crown outgrows the width, one integer factor after), and a
  desk that is mostly monitor (61 wide, ten rows of the current room) with a mug that steams, and the pet
  walking the floor (parked by the desk
  when sad). Under the scene: one status row and the composer. Nothing
  configurable; an unowned tank or pet leaves an empty stand or rug.
- **Rice (the page).** A binary split tree of tiles (`state.rs`), edited in place
  and persisted per account in `users.settings.zen_layout` as the JSON
  `RiceLayout` serializes to. Tile kinds are a closed enum: bonsai,
  aquarium, pet, chat, music, clock, visualizer, presence, blank. The look
  (border style, gap, titles) is part of the layout.

The current room is `App::zen_chat_room_id`: Home's selected room when it
is a real room, else #lounge. Picking a room in the `Ctrl+/` picker while
the page is up stays on the page, and `[` `]` walk the joined rooms. The
page counts as a chat-pane screen in
`app/input.rs` (`screen_has_chat_pane`, `embedded_chat_room_id`) and
report that room as the visible one (`current_visible_chat_room_id` in
`state.rs`, which marks it read and keeps its tail fresh), so the
composer, message actions, and chat clicks work the same as on Home.

## 2. File map

```text
late-ssh/src/app/zen/
|-- mod.rs        # module declarations only
|-- state.rs      # TileKind, Node (split tree), Look, RiceLayout (serde), ZenState + edits
|-- layout.rs     # pure rect math for Rice: rice_areas, tile_rects, tile_inner
|-- room.rs       # the drawn Room: scene constants, effects, the camera blit
|-- ui.rs         # ZenView, draw_rice, the tile widgets
|-- input.rs      # care keys (both pages), Room steering, Rice layout keys
`-- bigclock.rs   # block digit font for the clock tile
```

Glue: `Screen::Zen` in `common/primitives.rs`; the
frame skip, `ZenView` assembly, and `zen_chat_view` in `render.rs`; the digit
`7`, the top-bar hit test, the picker staying put in `room_search_modal/input.rs`, and the dedicated-input hook in `input.rs`;
`App::zen`, `App::zen_chat_rows_cache`, `sync_aquarium_bounds`, and
`persist_zen_layout` in `state.rs`; the aquarium stepping and anim edge in
`tick.rs`; `extract_zen_layout` / `User::set_zen_layout` in
`late-core/src/models/user.rs`.

## 3. Keys

Both faces, when not composing: `o` swaps faces, `[` `]` walk rooms, `i` /
Enter compose in the current room;
bonsai `w` water (replant when dead), `x` cut, `p` pinch, `s` split, `n`/`N`
branch; pet `f` feed, `d` water; aquarium `a` feed. The Room steers the
selected branch with arrows or hjkl. Rice: arrows move focus,
`space` cycles the focused tile's kind, `S` splits it (row when wide,
column when tall), `X` closes it (the last tile stays), `<` `>` trade width and `{` `}`
trade height with the nearest split of that direction (i3's rule; a
banner says so when there is none), `r` flips the parent, `z` zooms, `b` `g` `t` cycle
border, gap, titles, `R` resets to the default layout; hjkl steer the
bonsai only while a bonsai tile has focus. Every layout edit persists.

## 4. Gotchas

- The aquarium simulation is bound to one rect at a time. `App::sync_aquarium_bounds`
  re-binds it on every `set_screen`, resize, and Rice edit: the Room's
  `TANK_WATER` constant, or the aquarium tile's inner rect from the same
  pure functions the renderer uses (`layout.rs`), so the sim and the drawing
  never disagree on size. It steps on the quarter edge whenever a Zen page
  is up and the account owns an aquarium (`aquarium_tray_visible` in `tick.rs`).
- The chat view is built once per frame and taken by the first chat tile;
  a second chat tile shows a note instead. The Room does not use it: its
  monitor draws the message tail itself and the composer footer rides
  `ComposerBlockView`, the clubhouse's shape.
- The stored layout is parsed leniently: anything unreadable falls back to
  the default, so a bad row never locks the page.
- No tests yet: this is a prototype.
