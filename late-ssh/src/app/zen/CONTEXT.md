# Zen Context

## Metadata
- Scope: `late-ssh/src/app/zen`
- Last updated: 2026-09-10 (the drawn Room was built, liked, and cut the same day: Rice covers it. Default reworked: reef live for everyone, lobby tile added. Prototype; the split tree has tests (`state_test.rs`), the drawing does not.)
- Purpose: the full-bleed page that cuts the clubhouse down to the things you keep alive.
- Status: Experimental. Reached with `Ctrl+F` from any page; a surface over that page, absent from the Tab cycle. Esc or the chord returns to it (`App::zen_return_screen`).
- Parent context: `../../../../CONTEXT.md`

---

## 1. What it is

One top-level page with no app frame, no HUD, no sidebar: **Rice**, a
binary split tree of tiles (`state.rs`), edited in place and persisted per
account in `users.settings.zen_layout` as the JSON `RiceLayout` serializes
to. Tile kinds are a closed enum: bonsai (the true 81x26 canvas when the
tile has the room, the preview otherwise), aquarium (the real reef
simulation, drawn for everyone: unowned it swims empty under a `/shop`
caption), pet (the strip), chat, music, clock (block digits, online count
on the date row), visualizer, presence, lobby (the daily games, compact:
only the running games plus one footer row of count and keys), blank. The look (border style, gap, titles) is
part of the layout.

The default, which `R` also resets to: bonsai over the reef on the left
(64%), and a rail of clock, music, lobby, and the current room's chat on
the right. Pet and presence are not in it; owners add the pet with `S` and
`space`.

The current room is `App::zen_chat_room_id`: Home's selected room when it
is a real room, else #lounge. Picking a room in the `Ctrl+/` picker while
the page is up stays on the page, and `[` `]` walk the joined rooms. The
page counts as a chat-pane screen in `app/input.rs` (`screen_has_chat_pane`,
`embedded_chat_room_id`) and reports that room as the visible one
(`current_visible_chat_room_id` in `state.rs`, which marks it read and
keeps its tail fresh), so the composer, message actions, and chat clicks
work the same as on Home.

## 2. File map

```text
late-ssh/src/app/zen/
|-- mod.rs        # module declarations only
|-- state.rs      # TileKind, Node (split tree), Look, RiceLayout (serde), ZenState + edits
|-- layout.rs     # pure rect math: rice_areas, tile_rects, tile_inner
|-- ui.rs         # ZenView, draw_rice, the tile widgets
|-- input.rs      # care keys, room walk, focus and layout keys
`-- bigclock.rs   # block digit font for the clock tile
```

Glue: `Screen::Zen` in `common/primitives.rs`; the frame skip, `ZenView`
assembly, and `zen_chat_view` in `render.rs`; the digit `7`, the top-bar
hit test, the picker staying put in `room_search_modal/input.rs`, and the
dedicated-input hook in `input.rs`; `App::zen`, `App::zen_chat_rows_cache`,
`sync_aquarium_bounds`, and `persist_zen_layout` in `state.rs`; the
aquarium stepping and anim edge in `tick.rs`; `extract_zen_layout` /
`User::set_zen_layout` in `late-core/src/models/user.rs`.

## 3. Keys

When not composing: Esc or `Ctrl+F` leave, `[` `]` walk rooms, `i` / Enter compose in the current
room; bonsai `w` water (replant when dead), `x` cut, `p` pinch, `s` split,
`n`/`N` branch, hjkl steer while a bonsai tile has focus; pet `f` feed;
aquarium `a` feed (both free, once a day, +100 chips on the first feed). Layout: arrows move focus, `space` cycles the
focused tile's kind, `S` splits it (row when wide, column when tall), `X`
closes it (the last tile stays), `<` `>` trade one column of width and
`{` `}` one row of height with the nearest split of that direction (i3's
rule; a banner says so when there is none), `r` flips the parent, `z` zooms, `b` `g` `t` cycle
border, gap, titles, `R` resets to the default layout. Every layout edit
persists.

## 4. Gotchas

- The aquarium simulation is bound to one rect at a time. `App::sync_aquarium_bounds`
  re-binds it on every `set_screen`, resize, and layout edit to the
  aquarium tile's inner rect, from the same pure functions the renderer
  uses (`layout.rs`), so the sim and the drawing never disagree on size.
  It steps on the quarter edge whenever the page is up, owned or not
  (`aquarium_tray_visible` in `tick.rs`); only the fish need the unlock.
- The chat view is built once per frame and taken by the first chat tile;
  a second chat tile shows a note instead.
- The stored layout is parsed leniently: anything unreadable falls back to
  the default, so a bad row never locks the page. Splits store the first
  child's share in per-mille (`share`, 100..900) so one cell of any split
  is a distinct value; the resize walks the tree with the rects it lays out
  on the current terminal, so a press moves exactly one row or column and
  writes back the share that reproduces it. Layouts saved with the earlier
  percent `ratio` field no longer parse and reset to the default.
- Tests cover the split tree only (`state_test.rs`); the tile drawing is untested.
