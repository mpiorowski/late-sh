# Zen Context

## Metadata
- Scope: `late-ssh/src/app/zen`
- Last updated: 2026-09-10 (default swapped: chat under the bonsai, the pet over the reef on the rail; a pet tile beside a tank tile watches the fish. The music tile names the tuned station: `station_text` gives "radio · chillsynth" or "icecast · chill", YouTube stays one word. Bonsai care keys left the page: `w` opens the global Bonsai Care modal, and the page yields every key while a `v` chord is armed. Earlier: the drawn Room was built, liked, and cut the same day: Rice covers it. Default reworked: reef live for everyone, lobby tile added. Prototype; the split tree has tests (`state_test.rs`), the drawing does not.)
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
caption; owned, its title carries the care bar, fourteen boxes green for
the feeding streak or red for the days unfed, see the hub CONTEXT), pet (the strip; when its tile shares an edge with a tank tile and the pet is fed, it strolls for twenty minutes then sits against that edge for five with wide eyes, watching the fish, on the wall clock: `PetPose::for_frame` and `STROLL_TICKS`/`WATCH_TICKS` in `pet/ui.rs`, the side from `layout::neighbour_side`), chat, music (the track, then the source and the station it is tuned to, `v1`..`v5` retune it), clock (block digits, online count
on the date row), visualizer, presence, lobby (the daily games, compact:
only the running games plus one footer row of count and keys), blank. The look (border style, gap, titles) is
part of the layout.

The default, which `R` also resets to (rounded borders, no gap, titles on):
bonsai over the current room's chat on the left (64%), and a rail of
clock, music, lobby, then the pet over the reef on the right, so the pet
sits against the tank and watches. Without a Pet Companion the tile says
so and points at `/shop`. Presence is not in it.

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
|-- layout.rs     # pure rect math: rice_areas, tile_rects, tile_inner, neighbour_side
|-- ui.rs         # ZenView, draw_rice, the tile widgets
|-- input.rs      # feed keys, room walk, focus and layout keys
`-- bigclock.rs   # block digit font for the clock tile
```

Glue: `Screen::Zen` in `common/primitives.rs`; the frame skip, `ZenView`
assembly, and `zen_chat_view` in `render.rs`; the digit `7`, the top-bar
hit test, the picker staying put in `room_search_modal/input.rs`, and the
dedicated-input hook in `input.rs`; `App::zen`, `App::zen_chat_rows_cache`,
`sync_aquarium_bounds`, and `mark_zen_layout_dirty` / `flush_zen_layout` in `state.rs`; the
aquarium stepping and anim edge in `tick.rs`; `extract_zen_layout` /
`User::set_zen_layout` in `late-core/src/models/user.rs`.

## 3. Keys

When not composing: Esc or `Ctrl+F` leave, `[` `]` walk rooms, `i` / Enter compose in the current
room; pet `f` feed; aquarium `a` feed (both free, once a day, +100 chips on
the first feed). The bonsai has no keys of its own here: `w` is the global
Bonsai Care key and opens the same modal it opens on Home, so watering,
cutting, and steering work exactly as on the chat page. Layout: arrows move focus, `space` cycles the
focused tile's kind, `S` splits it (row when wide, column when tall), `X`
closes it (the last tile stays), `<` `>` trade one column of width and
`{` `}` one row of height with the nearest split of that direction (i3's
rule; a banner says so when there is none), `r` flips the parent, `z` zooms, `b` `g` `t` cycle
border, gap, titles, `R` resets to the default layout. `S` is refused at
`MAX_TILES` (32): each split nests the stored JSON one level deeper and
serde_json stops reading at 128, so an uncapped held key would write a
settings row the login path can never parse. Every layout edit marks the
layout dirty; the write is debounced to tick's one-hertz edge and to
leaving the page, so a held resize key costs one row update.

## 4. Gotchas

- The aquarium simulation is bound to one rect at a time. `App::sync_aquarium_bounds`
  re-binds it on every `set_screen`, resize, and layout edit to the
  aquarium tile's inner rect, from the same pure functions the renderer
  uses (`layout.rs`), so the sim and the drawing never disagree on size.
  It steps on the quarter edge whenever the page is up, owned or not
  (`aquarium_tray_visible` in `tick.rs`); only the fish need the unlock.
- The care bar (`care_bar_spans`, `ui_test.rs`) is the tank's only care
  readout on the page: `CareBar` comes from `AquariumCare::bar` in
  `hub/aquarium/state.rs`, the tile only paints it. `draw_tile_chrome`
  takes the optional span tail for exactly this; no other tile has one.
- The page defers every key while `App::music_prefix_armed` is set, so the
  `v` chords (`v x` audio source, `v s` skip vote) stay global; before that
  guard the page ate `x` and `s` as bonsai keys and the chord died. Adding a
  page key that collides with a chord suffix is fine, the guard covers it.
- The chat view is built once per frame and taken by the first chat tile;
  a second chat tile shows a note instead.
- The stored layout is parsed leniently: anything unreadable falls back to
  the default, so a bad row never locks the page. Splits store the first
  child's share in per-mille (`share`, 100..900) so one cell of any split
  is a distinct value; the resize walks the tree with the rects it lays out
  on the current terminal, so a press moves exactly one row or column and
  writes back the share that reproduces it. Layouts saved with the earlier
  percent `ratio` field no longer parse and reset to the default.
- The watching pet is a render-time fact: `draw_rice` finds the pet tile's
  neighbouring tank from the frame's rects and passes the side into
  `draw_pet_box`, which records it with the travel in `App::last_pet_travel`
  (`PetFrameInputs`) so the tick-side gate (`pet::ui::frame_changed`)
  evaluates the same pose. A zoomed tile has no neighbour. A hungry pet
  sulks whatever is next to it.
- Tests cover the split tree (`state_test.rs`), neighbour detection
  (`layout_test.rs`), and the care bar (`ui_test.rs`); the rest of the tile
  drawing is untested.
