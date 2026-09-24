# Zen Context

## Metadata
- Scope: `late-ssh/src/app/zen`
- Last updated: 2026-09-17 (the `Ctrl+/` picker on the page lists real rooms only, `PickerScope::RoomsOnly`: synthetic rail entries have no tile to land in. See the chat tile notes.)
- Purpose: the full-bleed page that cuts the clubhouse down to the things you keep alive.
- Status: Experimental. Reached with `Ctrl+F` from any page, or `/zen` from any composer (same toggle); a surface over that page, absent from the Tab cycle. The chord returns to the page it was opened from (`App::zen_return_screen`), or to the Clubhouse when the session landed on Zen (Settings, Tweaks, "Land on"). Leaving by any route (a digit, a tour step) clears the return page in `App::set_screen`, so a later chord never hands back a stale page. The one route back that is not the chord is the backtick chain: going into the games from Zen makes Zen the chain's base (`App::workspace_base`), which carries the return page and refills it when the wrap or an Esc off a board or table lands back here. The first-visit tour's `VisitZen` stop is reached with the chord (or Enter, for terminals that swallow it) and left with `0`.
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
the feeding streak or red for the days unfed, see the hub CONTEXT), pet (the box from `pet/ui.rs`, the only place the pet is drawn besides the profile portrait; its top row reads `name · mood`, the mood inferred from the session by `pet/state.rs` (purring, proud, sulking, chatty, asleep, vibing, idle); when its tile shares an edge with a tank or a bonsai tile and the pet is calm (idle, vibing, or chatty) it strolls for twenty minutes then sits against that edge for five with wide eyes, on the wall clock: `PetPose::for_frame`, `STROLL_TICKS`/`WATCH_TICKS`/`LEG_TICKS`, the side from `layout::neighbour_side` and the target from `Neighbours`. A round is two legs, the tank on the first watch window and the bonsai on the second, so with both beside it they alternate and with one that one takes both windows: its five minutes in twenty-five never depend on what else the page holds. At the glass it gasps at a passing fish; at the tree it leans in for a slower sniff; a click on it pets it (the first pet of the UTC day pays 100 chips, see the hub CONTEXT) and does *not* focus its tile, since petting is a passing gesture and the keys belong to the chat you are typing in (`handle_pet_click` takes the click before `focus_zen_tile_at`, `input_flow_test.rs`); while the terminal cursor is inside the tile an awake, unsulking pet walks after it, eyes on the cursor), chat, music (the track, then the source and the station it is tuned to, always the tile's last two rows, with the full-height visualizer filling every row above; `v1`..`v5` retune it), clock (block digits, the date below them when the tile is seven rows
or more; a one-row tile shows the time alone), visualizer, lobby (the daily games, compact:
only the running games, starting at the name with no marker column
(`RowMarker::Bare`; the sidebar panel keeps its `►`), plus one footer row of
count and keys), activity
(the #lounge feed as a list, newest on top, each event's age flush right
and a friend's line in the friend color: the same
`ChatState::activity_ticker` queue the one-row ticker packs, capped at
40), friends (connected friends, newest login first: name, `/status`
badge, audio source, time on), pulse (the numbers worth a glance, one
`label  value` row each and every row always drawn, zero included: people
online, your chips, unread mentions, friends online, and today's care, which
names bonsai, tank, and pet, each green once tended today (watered, fed,
petted), amber while due, faint when not owned; a short tile keeps the top
rows. It replaced the presence tile, and a stored `presence` reads as
`pulse` through a serde alias, so old layouts keep their shape), inbox (unread DMs by count, then mentions
newest first; see §3 for its keys), headlines (News articles and the
viewer's RSS entries merged newest first, two rows each (the title with
its source and age, then the link), an entry shared to News listed once,
as the article), blank. The look (border style, gap, titles) is
part of the layout.

The default, which `R` also resets to (rounded borders, no gap, titles on):
bonsai over the current room's chat on the left (64%), and a rail of
clock, music, lobby, then the pet over the reef on the right, so the pet
has the tank below it and the bonsai's edge to its left, and alternates
between them (pinned by `layout_test.rs`). Without a Pet Companion the tile says
so and points at `/shop`. Pulse is not in it.

The active chat is `App::zen_chat_room_id`: the focused chat tile's
room, else the first chat tile's, else (no chat tile) the current room,
which is Home's selected room when it is a real room, else #lounge
(`ZenState::active_chat_index`, `App::zen_chat_rooms`; a tile bound to a
room the account has left shows the current room). The `Ctrl+/` picker
(or `/picker`) while the page is up lists real rooms only
(`PickerScope::RoomsOnly`): Mentions, News, feeds and the other synthetic
rail entries have no tile to land in, and the Inbox and headlines tiles
already carry them. A pick stays on the page: with a chat tile
focused, a room pick rebinds that tile the way `[` `]` do
(`zen::input::bind_focused_chat_to_room`, called from
`room_search_modal/input.rs`); a `?query` message jump lands in the same
path, so it rebinds the tile to the hit's room (saved) and selects the
message there; a pick with no chat tile focused moves Home's selection. The page counts as
a chat-pane screen in `app/input.rs` (`screen_has_chat_pane`,
`embedded_chat_room_id`) and reports the active room as the visible one
(`current_visible_chat_room_id` in `state.rs`, which marks it read and
keeps its tail fresh), so the composer, message actions, and chat clicks
work the same as on Home. The other chat tiles are read-only views of
their rooms: messages stream in and nothing is marked read until the tile
is focused, but every tile draws its own composer strip (2026-09-12),
because an input box that appears and disappears as the focus walks moves
every row under the reader. Only the active tile's composer is live; the
others are inert (`composer_inert` on the chat view, `inert` on
`ComposerBlockView`): titled "watching", a hint to focus the tile instead
of the chat keys, and handed an empty `TextArea`
(`idle_composer` in `render.rs`), so they never grow with someone else's
draft. A draft
never follows the focus: every submit goes to the room the composer was
opened in, so when the focus lands where the active chat's room differs
from the draft's, `zen::input::focus_moved` closes the draft rather than
drawing it under another room's label (`input_flow_test.rs`). Zoomed, the
one pane drawn takes the active chat's frame, not the first chat tile's
(`draw_rice`). A chat tile's title names its room, and a stream room's also
carries the rail's watcher count, `chat · #mat-live [3]` (`[…]` while
pending, `chat::ui::stream_count_badge`): the tile draws no stream header,
so this is the page's only count. On a narrow tile the room label shortens,
never the count (`chat_tile_title`, `ui_test.rs`).

## 2. File map

```text
late-ssh/src/app/zen/
|-- mod.rs        # module declarations only
|-- state.rs      # TileKind, Node (split tree), Look, RiceLayout (serde), ZenState + edits
|-- layout.rs     # pure rect math: rice_areas, tile_rects, tile_inner, neighbour_side, pet_neighbours
|-- rows.rs       # pure row builders for the Inbox and Headlines tiles
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

`Ctrl+F` leaves. Esc only peels the tile picker, the composer, or a selected
message, and otherwise does nothing: it never leaves. Backtick runs the
workspace cycle as on Home: it hops into the games waiting on you, and the
chain comes home here. With a chat tile focused: `[`
`]` rebind it to the previous or next joined room (a layout edit, saved),
and a room picked in `Ctrl+/` binds it the same way,
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
the pet click) and falls through to the composer and message clicks of
that tile. A click on the pet is the exception: it pets it and leaves the
focus where it was. Aquarium `a` feed (free, once a day, +100 chips on the first feed).
With the Inbox focused, `j` `k` walk its rows and Enter opens the selected
row in the first chat tile and moves the focus there, so `i` answers at
once: a DM binds the tile to its room; a mention binds it to the
mention's room and selects the message the way a `Ctrl+/` message jump
does. A mention in a room the account never joined opens the history
modal instead, and a page with no chat tile says so in a banner.
With Headlines focused, `j` `k` walk its items the same way and Enter
copies the selected link to the clipboard (`pending_clipboard`, the way a
copied search hit goes).
The pet has no key: it is petted with a left click and reads the rest of
the session itself. The sprout on the tank floor (the fortnightly bud;
leave it a week and it roots as a plant) is cut on its Shop row
(`-`, Companions), never a page key: dedicated keys accumulate and
collide, and the tile draws no caption for it either (2026-09-11). The bonsai has no keys of its own here: `w` is the global
Bonsai Care key and opens the same modal it opens on Home, so watering,
cutting, and steering work exactly as on the chat page. Layout: arrows and Tab / Shift+Tab move focus (the page owns Tab; it is not the page switch here), `space` opens the tile
picker over the focused tile (`ZenState::kind_picker`: one row per
`TileKind`, alphabetical (`TileKind::ALL`), scrolled with the selection on a short page, `j` `k` and the arrows move, Enter or `space` picks, Esc
closes; the picker owns every key while it is up, and a refused row
stays up with a banner), `S` splits it (row when wide, column when tall), `X`
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
  (`aquarium_visible` in `tick.rs`), though the tile only draws it
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
  the tile picker refuses Chat once ten tiles are chats (`kind_allowed`,
  the row drawn faint with `full`) and a chat tile that changes kind
  forgets its room.
- The stored layout is parsed leniently: anything unreadable falls back to
  the default, so a bad row never locks the page. Splits store the first
  child's share in per-mille (`share`, 1..999) so one cell of any split
  is a distinct value; the resize walks the tree with the rects it lays out
  on the current terminal, so a press moves exactly one row or column and
  writes back the share that reproduces it. A resize stops with
  `MIN_TILE_CELLS` (3) left on either side, a border and one row inside,
  so a clock fits a one-row tile. Layouts saved with the earlier
  percent `ratio` field no longer parse and reset to the default.
- The watching pet is a render-time fact: `draw_rice` finds the pet tile's
  neighbouring tank and bonsai from the frame's rects (`layout::pet_neighbours`) and passes both sides
  into `draw_pet_box`, which records them with the travel, the box's rect,
  and where the pet stood in `App::last_pet_frame` (`PetFrameInputs`) so the
  tick-side gate (`pet::ui::frame_changed`) evaluates the same pose and
  the walk after the cursor (`PetState::tick`, fed `App::last_mouse` from
  every mouse report) knows where the cursor is relative to the box.
  Adjacency is exact edge contact, not proximity: one cell of overlap on
  the perpendicular axis and edges equal plus the gap, so a diagonal or
  corner-only tile is not a neighbour, and a zoomed tile has none at all.
  The check reads the tile kind only, so an unowned tank (the shop note) is
  watched like a live reef. A sulking or sleeping pet ignores the fish, the
  tree, and the cursor.
- Pulse draws only owned values the session already keeps for other
  surfaces (`online_count`, `chip_balance`, the mention count,
  `active_friends`, and the care: `BonsaiState::last_watered`,
  `AquariumCare::fed_on_day`, `PetState::petted_on`), so it adds no query
  and no lock. Every row stays visible at zero, and so should any row added
  later. The block is capped at `PULSE_MAX_WIDTH` (32) columns and centered,
  so a wide tile never strands a label far from its value.
- The list tiles (lobby, activity, friends, pulse) draw one column in from
  each border (`pad_sides` in `draw_rice`). The selectable ones (inbox,
  headlines) pad only the right (`pad_right`): their one-column row marker
  is the left padding, so every tile's text starts one column in; the chat,
  the canvases, the clock, and the equalizers keep the full inner area. A
  chat tile also asks the embedded chat for no inset of its own
  (`EmbeddedRoomChatView::messages_inset: 0`): the tile border already is
  the column of air, so the text sits where Home's does. House tables and
  daily boards have no border around their chat and keep an inset of 1. The pot and the status were tried there and cut as
  low value. A
  history chart was tried and dropped: presence is per replica, so a
  process-local day of headcounts would disagree between pods.
- The mentions list loads only on ask (`notifications.list()`), so while an
  Inbox tile is on the page tick requests it again whenever the unread count
  differs from the last request (`App::zen_inbox_listed_unread`). The Inbox
  and Headlines rows are built per frame only while their tile is on the
  page (`ZenState::shows`), and Enter rebuilds the same Inbox rows, so the
  marked row is the one that opens.
- Tests cover the split tree (`state_test.rs`), neighbour detection
  (`layout_test.rs`), the care bar and the music tile's rows (`ui_test.rs`),
  the resize floor (`state_test.rs`), the Inbox and Headlines rows
  (`rows_test.rs`), and Inbox Enter (`input_flow_test.rs`); the rest of the
  tile drawing is untested.
