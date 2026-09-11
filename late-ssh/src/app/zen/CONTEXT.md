# Zen Context

## Metadata
- Scope: `late-ssh/src/app/zen`
- Last updated: 2026-09-11 (the tank tile is the reef and nothing else: the sprout's caption and the `/aq cut` command are gone, the sprout is tended on its Shop row; an unowned tank is a centered `/shop` note like the missing pet. Same day: chat tiles are per room and there can be up to ten: a chat leaf carries an optional `room` (absent means the current room, so old layouts read unchanged), `[` `]` rebind the focused chat tile instead of moving Home's selection, and `i`, Enter, `j`, `k` act only while a chat tile is focused; the active chat (focused chat tile, else the first) has the composer, the selection, and the read marking, the others watch their rooms read-only; `render.rs` builds one view per chat tile over `App::zen_chat_rows_caches`; the first opening of the page in a session focuses the first chat tile, and a click focuses the tile under it. Same day: every tile names its own keys on the right of its title, always, hidden with the titles by `t`: `tile_keys` in `ui.rs`; the body rows lost their key text (bonsai `w tend`, the music controls row, the pet's `click to pet`) and the clock's date row no longer repeats the time. The footer is the layout keys only, fitted to the width by dropping whole hints from the right (`hint_line_fitting`, `ui_test.rs`); `?` opens the new Zen topic of the help modal with the full list. Previously, 2026-09-10: the pet is a mood indicator: no feed, no `f` key, no bowl; the tile's top row reads `name · mood`, a click on the pet pets it, and the pet walks after the terminal cursor while it is inside the tile; the tank and the pet now live on this page only, the Home tray and the composer strip are gone. Earlier the same day, the tank tile's bottom row points at `/aq cut` while a sprout stands; no page key for it, on purpose; see the hub CONTEXT for the sprout clock. Earlier the same day, default swapped: chat under the bonsai, the pet over the reef on the rail; a pet tile beside a tank tile watches the fish. The music tile names the tuned station: `station_text` gives "radio · chillsynth" or "icecast · chill", YouTube stays one word. Bonsai care keys left the page: `w` opens the global Bonsai Care modal, and the page yields every key while a `v` chord is armed. Earlier: the drawn Room was built, liked, and cut the same day: Rice covers it. Default reworked: reef live for everyone, lobby tile added. Prototype; the split tree has tests (`state_test.rs`), the drawing does not.)
- Purpose: the full-bleed page that cuts the clubhouse down to the things you keep alive.
- Status: Experimental. Reached with `Ctrl+F` from any page; a surface over that page, absent from the Tab cycle. Esc or the chord returns to it (`App::zen_return_screen`).
- Parent context: `../../../../CONTEXT.md`

---

## 1. What it is

One top-level page with no app frame, no HUD, no sidebar: **Rice**, a
binary split tree of tiles (`state.rs`), edited in place and persisted per
account in `users.settings.zen_layout` as the JSON `RiceLayout` serializes
to. A chat leaf also carries the room it is bound to (`room`, absent for
the current room), so a page can hold several rooms side by side, up to
`MAX_CHAT_TILES` (10): each chat tile is a room drawn and row-cached every
frame, and a page of a hundred rooms would swallow every unread count.
Tile kinds are a closed enum: bonsai (the true 81x26 canvas when the
tile has the room, the preview otherwise), aquarium (the real reef
simulation once the account owns it; unowned, the tile is a centered
note pointing at `/shop`, the same shape as the pet's; owned, its title carries the care bar, fourteen boxes green for
the feeding streak or red for the days unfed, see the hub CONTEXT), pet (the box from `pet/ui.rs`, the only place the pet is drawn besides the profile portrait; its top row reads `name · mood`, the mood inferred from the session by `pet/state.rs` (purring, proud, sulking, chatty, asleep, vibing, idle); when its tile shares an edge with a tank tile and the pet is calm (idle or vibing) it strolls for twenty minutes then sits against that edge for five with wide eyes, watching the fish, on the wall clock: `PetPose::for_frame` and `STROLL_TICKS`/`WATCH_TICKS`, the side from `layout::neighbour_side`; a click on it pets it, and while the terminal cursor is inside the tile an awake, unsulking pet walks after it, eyes on the cursor), chat, music (the track, then the source and the station it is tuned to, `v1`..`v5` retune it), clock (block digits, online count
on the date row), visualizer, presence, lobby (the daily games, compact:
only the running games plus one footer row of count and keys), blank. The look (border style, gap, titles) is
part of the layout.

The default, which `R` also resets to (rounded borders, no gap, titles on):
bonsai over the current room's chat on the left (64%), and a rail of
clock, music, lobby, then the pet over the reef on the right, so the pet
sits against the tank and watches. Without a Pet Companion the tile says
so and points at `/shop`. Presence is not in it.

The active chat is `App::zen_chat_room_id`: the focused chat tile's
room, else the first chat tile's, else (no chat tile) the current room,
which is Home's selected room when it is a real room, else #lounge
(`ZenState::active_chat_index`, `App::zen_chat_rooms`; a tile bound to a
room the account has left shows the current room). Picking a room in the
`Ctrl+/` picker while the page is up stays on the page. The page counts as
a chat-pane screen in `app/input.rs` (`screen_has_chat_pane`,
`embedded_chat_room_id`) and reports the active room as the visible one
(`current_visible_chat_room_id` in `state.rs`, which marks it read and
keeps its tail fresh), so the composer, message actions, and chat clicks
work the same as on Home. The other chat tiles are read-only views of
their rooms: messages stream in, nothing is marked read until the tile is
focused, and the composer strip is not drawn (`composer_shown`). A draft
never follows the focus: every submit goes to the room the composer was
opened in, so when the focus lands where the active chat's room differs
from the draft's, `zen::input::focus_moved` closes the draft rather than
drawing it under another room's label (`input_flow_test.rs`). Zoomed, the
one pane drawn takes the active chat's frame, not the first chat tile's
(`draw_rice`).

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

Where they are shown: each tile names its own keys on the right of its
title (`tile_keys`, drawn with `hint_line` so the key is amber and the
word dim, as in the footer), always, so `t` hides them with the titles; the footer
carries Esc and `?` first, then the layout keys by use (focus, kind,
split, close, zoom, resize, flip, reset), and drops whole hints from the
right when the terminal is narrow;
`?` opens `HelpTopic::Zen` with everything. Nothing else on the page
names a key: the lobby's compact footer lost its key pair to the title.

When not composing: Esc or `Ctrl+F` leave. With a chat tile focused: `[`
`]` rebind it to the previous or next joined room (a layout edit, saved),
`i` / Enter compose in its room, `j` `k` select in it, and the message
actions (`d` `r` `e` `p` `c` `t` `G`, Enter, the reaction leader) act on
its selection: the page routes them to `handle_message_action_in_room`
through the same two gates the house table uses (`chat_priority_key`,
`selected_chat_key`), so `r` still flips the tile while nothing is
selected. With any other tile focused the chat keys are swallowed, so a
page of several chats never scrolls one you are not looking at
(`input_flow_test.rs`). The
first opening of the page in a session focuses the first chat tile; a left
click focuses the tile under it (not through a modal, the same guard as
the pet click) and falls through to the pet, composer, and message clicks
of that tile. Aquarium `a` feed (free, once a day, +100 chips on the first feed).
The pet has no key: it is petted with a left click and reads the rest of
the session itself. The sprout on the tank floor (the fortnightly bud;
leave it a week and it roots as a plant) is cut on its Shop row
(`-`, Companions), never a page key: dedicated keys accumulate and
collide, and the tile draws no caption for it either (2026-09-11). The bonsai has no keys of its own here: `w` is the global
Bonsai Care key and opens the same modal it opens on Home, so watering,
cutting, and steering work exactly as on the chat page. Layout: arrows move focus, `space` cycles the
focused tile's kind, `S` splits it (row when wide, column when tall), `X`
closes it (the last tile stays), `<` `>` trade one column of width and
`{` `}` one row of height with the nearest split of that direction (i3's
rule; a banner says so when there is none), `r` flips the parent, `z` zooms, `b` `g` `t` cycle
border, gap, titles, `R` resets to the default layout, focus on its chat tile. `S` is refused at
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
  (`aquarium_tray_visible` in `tick.rs`), though the tile only draws it
  once the tank is owned; unowned it is the shop note.
- The care bar (`care_bar_spans`, `ui_test.rs`) is the tank's only care
  readout on the page: `CareBar` comes from `AquariumCare::bar` in
  `hub/aquarium/state.rs`, the tile only paints it. `draw_tile_chrome`
  takes the optional span tail for exactly this; no other tile has one.
- The page defers every key while `App::music_prefix_armed` is set, so the
  `v` chords (`v x` audio source, `v s` skip vote) stay global; before that
  guard the page ate `x` and `s` as bonsai keys and the chord died. Adding a
  page key that collides with a chord suffix is fine, the guard covers it.
- One chat view per chat tile is built per frame in `render.rs`, over
  `App::zen_chat_rows_caches` (resized to the tile count each frame), and
  `draw_rice` hands them out in layout order. Only the active tile's view
  carries the composer, the selection, the overlay, and the mouse slots;
  `space` refuses Chat once ten tiles are chats (`cycle_focused_kind`
  skips it) and a chat tile that changes kind forgets its room.
- The stored layout is parsed leniently: anything unreadable falls back to
  the default, so a bad row never locks the page. Splits store the first
  child's share in per-mille (`share`, 100..900) so one cell of any split
  is a distinct value; the resize walks the tree with the rects it lays out
  on the current terminal, so a press moves exactly one row or column and
  writes back the share that reproduces it. Layouts saved with the earlier
  percent `ratio` field no longer parse and reset to the default.
- The watching pet is a render-time fact: `draw_rice` finds the pet tile's
  neighbouring tank from the frame's rects and passes the side into
  `draw_pet_box`, which records it with the travel, the box's rect, and
  where the pet stood in `App::last_pet_frame` (`PetFrameInputs`) so the
  tick-side gate (`pet::ui::frame_changed`) evaluates the same pose and
  the walk after the cursor (`PetState::tick`, fed `App::last_mouse` from
  every mouse report) knows where the cursor is relative to the box. A
  zoomed tile has no neighbour. A sulking or sleeping pet ignores both the
  fish and the cursor.
- Tests cover the split tree (`state_test.rs`), neighbour detection
  (`layout_test.rs`), and the care bar (`ui_test.rs`); the rest of the tile
  drawing is untested.
