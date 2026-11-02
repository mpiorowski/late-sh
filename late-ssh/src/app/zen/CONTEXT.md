# Zen Context (Home's tile tree)

## Metadata
- Scope: `late-ssh/src/app/zen`
- Purpose: the tile tree behind Home (`Screen::Dashboard`): its room rail, chats, sidebar panels and the things you keep alive are tiles of one tree, `App::home`, arranged in place and kept as up to nine named layouts per device. One of the two stock layouts is named `zen` (the bonsai and the reef over a chat, boxed): `/zen` and the status line's care click load it. The design and what v1 left out: `devdocs/HOME-LAYOUTS.md`.
- Status: Experimental.
- Parent context: `../../../../CONTEXT.md`

---

## 1. What it is

**Rice**: a binary split tree of tiles (`state.rs`, `RiceLayout`, the
JSON it serializes to), drawn as Home inside the app frame, edited in
place in arrange mode and persisted per device on the SSH key the session
authenticated with (`user_ssh_keys.settings.home_layouts`, §Home). A chat leaf also carries the room it is bound to (`room`, absent for
the current room), so a page can hold several rooms side by side, up to
`MAX_CHAT_TILES` (10): each chat tile is a room drawn and row-cached every
frame, and a page of a hundred rooms would swallow every unread count.
Tile kinds are a closed enum: bonsai (always the care modal's true
81x26 canvas, never the preview; a smaller tile cuts it, top rows first
and the sides evenly so the trunk stays centered, `../bonsai/render.rs`
`draw_tile`, `render_test.rs`; no status row, every row is tree, since day, vigor, and
stress live in the `w` care modal), aquarium (the real reef
simulation, in a tile wider than the rail and
taller than the small tank (`../hub/aquarium/ui.rs` `reef_form`), else a
note pointing at the tank tile), tank (the same
aquarium the sidebar's way: the fish in a six-row band across the tile,
centered in a taller one, `hub/aquarium/ui::draw_mini_tank`; the two are
the one tank two ways, and both draw the care bar on the water's surface,
fourteen dots green for the feeding streak or red for the days unfed,
see the hub CONTEXT), ascii (an
animated piece ported from ascii.rest, `../ascii/CONTEXT.md`: earthrise
in dots, the screensaver's default and a slow piece, until `[` `]` step it through the others or Enter picks one from
the list (`ascii/picker`, over the page), including random and randomer.
Random holds one scene/style per visit; randomer changes landscape every five minutes.
The title names the piece, `ascii · aurora fjord · pixels`, with the random mode
before it when selected. Each tile keeps independent session-only playback in
`ZenState`; slots follow surviving leaves through splits/closes. Leaving Home
clears playback; resizing and overlays preserve it and its timer. Only
the selection is stored on the leaf beside `room`, absent
for the default, rides along on a split, and is forgotten when the tile
changes kind, as a chat tile's room is), pet (the box from `pet/ui.rs`, also drawn by the sidebar Pet panel and as the profile portrait; its top row, `pet::ui::status_line`, reads `name · mood`, the mood inferred from the session by `pet/state.rs` (purring, proud, sulking, chatty, asleep, vibing, idle); when its tile shares an edge with a tank or a bonsai tile and the pet is calm (idle, vibing, or chatty) it strolls for twenty minutes then sits against that edge for five with wide eyes, on the wall clock: `PetPose::for_frame`, `STROLL_TICKS`/`WATCH_TICKS`/`LEG_TICKS`, the side from `layout::neighbour_side` and the target from `Neighbours`. A round is two legs, the tank on the first watch window and the bonsai on the second, so with both beside it they alternate and with one that one takes both windows: its five minutes in twenty-five never depend on what else the page holds. At the glass it gasps at a passing fish; at the tree it leans in for a slower sniff; a click on it pets it (the first pet of the UTC day pays 100 chips, see the hub CONTEXT) and does *not* focus its tile, since petting is a passing gesture and the keys belong to the chat you are typing in (`handle_pet_click` takes the click before `focus_home_tile_at`, `input_flow_test.rs`); while the terminal cursor is inside the tile an awake, unsulking pet walks after it, eyes on the cursor), chat, music (the sidebar's dock without its band, `sidebar::draw_music_dock`, once the tile is `MUSIC_DOCK_HEIGHT` rows and eighteen columns; shorter, the track over the station, centred; `v1`..`v5` retune it, `m` mutes), clock (block digits, the date below them when the tile is seven rows
or more; a one-row tile shows the time alone), visualizer, lobby (the latest tables on the
board, newest change first: open challenges, matches by their last move,
unseen results, each `○ chess srw 2m` with the state as the marker and the
age flush right, `panel::daily_feed_rows`, plus the `N open · entries/cap`
footer; what waits on you is the attention guest's, which the stock lobby
hosts), activity
(the #lounge feed as a list, newest on top, each event's age flush right
and a friend's line in the friend color: the same
`ChatState::activity_ticker` queue the one-row ticker packs, capped at
40), friends (connected friends, here before away, then newest login:
name, the away glyph, audio source, time on; both lists are chat's data,
drawn in `../chat/ui.rs`), pulse (the people online now over the
last day of them as a sparkline, the peak named, a time axis under it; the
samples are presence's, `../presence/svc.rs`, and so is the drawing,
`../presence/ui.rs`; a one-row tile is the sparkline alone), live (the Live panel's rows, `../live/PANEL.md`:
the same four rows the sidebar's panel draws, each with its `s1`-`s4`
key, wider so less is cut; `nobody playing` while nothing is on), presence
(the sidebar's pinned rows, `presence/ui::draw_core_block`: the online count
and the clock, then the connected friends), rooms (Home's room rail,
one per page: below), blank. The look
(border style, gap, titles) is part of the layout: a layout without a
border draws Home's rules, one with a border boxes every tile (below).

The stock `zen` layout (`RiceLayout::default`, what `R` resets a layout
named `zen` to; rounded borders, no gap, titles on): on the left (72%),
the bonsai beside the reef over the current room's chat; on the right a rail of clock, pet, lobby, live, then music, so the
pet has the tank against its left edge and watches it (the bonsai is out
of its reach; pinned by `layout_test.rs`). Without a Pet Companion the
tile says so and points at `/shop`. Pulse is not in it.

The active chat is `App::home_chat_room_id`: the focused chat tile's
room, else the first chat tile's, else (no chat tile) the current room,
which is Home's selected room when it is a real room, else #lounge
(`ZenState::active_chat_index`, `App::home_chat_rooms`; a tile bound to no
room shows the current room, and so does one bound to a room the account
has left). The rail and the active tile are one room, kept so from both
ends: the focus landing on a chat tile bound to a room selects that room
on the rail (`App::sync_rail_to_active_chat`, from
`zen::input::focus_moved` and on arriving at Home), and the rail
moving to another real room rebinds the active tile to it, a layout edit
(`App::steer_active_chat_from_rail`, inside `sync_visible_chat_room`,
which every rail move already calls). The synthetic entries are views the
active tile shows, never bindings. A tile that just became a chat
is pinned to the room of the moment, together with every tile bound to no
room (`App::pin_chat_tiles_on_pick`): with two chats on a page, a tile
that followed the rail would only ever show the other tile's room. The
`Ctrl+/` picker (or `/picker`) is Home's: a room pick moves the rail,
which steers the active tile; a `?query` message jump lands the same way
and selects the message. The other chat tiles are read-only views of
their rooms: messages stream in and nothing is marked read until the tile
is focused, but every tile draws its own composer strip,
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
drawing it under another room's label (`input_flow_test.rs`). A chat
tile's title names its room, and a stream room's also
carries the rail's watcher count, `chat · #mat-live [3]` (`[…]` while
pending, `chat::ui::stream_count_badge`): the tile draws no stream header,
so this is the page's only count. On a narrow tile the room label shortens,
never the count (`chat_tile_title`, `ui_test.rs`).

### Home

Home (`Screen::Dashboard`, `App::on_home`) draws `App::home` with
`draw_rice` inside the app frame (the frame carries the status bars; in
arrange mode the bottom row is the key footer,
`statusline::bar::build_home_arrange_row`). The geometry is
`App::home_tiles_area` and `App::home_tile_rects`, from the same pure
functions the renderer uses. The stock
Home (`RiceLayout::home`) is fixed and the same for every account: the
room rail (`rooms`, `HOME_RAIL_COLS`), the reef across the top of the
chat (`aquarium`, `HOME_AQUARIUM_ROWS`) hosting the now strip, the chat
under it (a `chat` bound to no room, so it shows the rail's; a stored
`center` reads as one), and the sidebar column (`HOME_SIDEBAR_COLS`):
`presence`, then `visualizer` (the band's three rows), `music` (the dock's
twelve), `lobby` hosting attention (three rows: two tables over the status
line), `live` at its panel rows, `pet` (its three-row box, no mood row) and
`bonsai` taking the rest. The rail, the reef and `presence` take their
title off outside arrange mode (`Node::untitled`, `Node::title_at`): the
reef keeps the row for water, so nothing moves when the mode comes up, and
`presence` gets both its rows. An unowned
pet draws the shop's note in its tile. The Settings panel list is the
Arcade's alone. `R` resets to the stock. Nothing about the terminal's size
changes a layout: a page is drawn as arranged at every width, so a phone
arranges its own Home once and keeps it on its key.

Layouts (`presets.rs`): a device holds up to nine named layouts
(`Layouts`, `MAX_LAYOUTS`), each a `RiceLayout`, and which one Home shows
(`App::home_layouts`; `App::home` is the current one, written back into
its slot by `Layouts::store_current` on every flush and before every
switch, so nothing arranged is lost). Stored whole on the SSH key
(`user_ssh_keys.settings.home_layouts`); a key with none seeds the set
from the single layouts it stored before they had names (`home_layout`
as `home`, `zen_layout` as `zen`, the account's as their seeds), and a
blob that does not parse, is empty, or points past its items reads as
that seed. Every device has the two stock layouts to start: `home` (the
stock Home) and `zen` (`RiceLayout::default`, boxed in its look); `R`
puts a layout back to the stock its name says (`Layouts::stock_of`:
`zen` to the stock `zen`, anything else to stock Home). `/zen` and the
status line's care click load the layout named `zen`
(`zen::input::load_zen_layout`, `/layout zen` by a shorter name: Home
first, and the `No layout by that name or number` banner on a device
that renamed or deleted it). A stored `zen_layout` is only ever a seed.
Names are trimmed, one to `MAX_LAYOUT_NAME_CHARS` (16) characters,
unique case aside. Nothing switches a layout but the keys, `/layout`,
`/zen` and the care click: not `/golive`, not a game waiting, not the
screensaver, not the landing page.

The look is Home's rules (`layout::Chrome::Rules`) while the layout's
border is `none`, Home's stock and every layout arranged from it: no
boxes, a `── title ──` rule over a panel tile (`sidebar::draw_panel_rule`,
lit while the lobby glows or, in arrange mode, the focus is on it), a
column of air on each side of every tile but the page's chat and the
rail, and the gutter of each row split painted as one `│`
(`layout::row_gutters`, `Gaps { row: 1, col: 0 }`). Every tile wears a
title rule, and `t` picks one of three for it (`TileTitle`, `Chrome::wears`):
named, the rule alone with no text, or no row at all. In arrange mode every
tile is named whatever it says, so the focus has a ring to land on.
Outside arrange mode the rule is the tile's name and nothing else. In
arrange mode its key hint comes up on the right (`TileKind::hints`, one
table with the handler, `TileHint`): a house key the tile only names
(`HouseKey`: `w tend`, `m mute`, `ctrl+/ room`, `^G open`, `s1-4 open`,
`click pet`) on every tile, and a key the tile takes itself (`TileKey`:
`[ ] Enter pick` on an ascii tile, `a feed` on the reef or tank, `[ ]
cycle` on a chat, and `j k Enter act` for the attention guest,
`Guest::hints`) only on the focused one. The rail names none: it takes
clicks on its rows. A layout with
a border (the stock `zen`, or `b` in
arrange mode) is boxed in its look with its gap (`ZenState::chrome`,
`gaps`).

Furniture and focus: outside arrange mode only chat tiles take the focus
(`TileKind::takes_focus`), by click or the arrows of arrange mode; a
click on a panel is its gesture (the pet is petted, a live row opens) and
the keys stay with the chat (`focus_home_tile_at`). A page opens with
the focus on its first chat tile (`ZenState::new`, `ZenState::load`). Home's chat is its
active chat tile (`ZenState::active_chat_ordinal`: the focused chat tile,
else the first): it draws Home's card (the rail's selection, synthetic
entries included, `HomeView`) with the card's own chrome, its keys are
Home's (`dashboard::input`), and `App::home_chat_room_id` is `None` while
a synthetic entry is up. Every other chat tile watches its room under a
`chat · #room` rule, a click away from being the page's chat, so a stream
night is two chats: the stream room and #lounge.

Guests (`Guest`: now, attention) are the two widgets that come and go:
the now strip while the queue has something up (`LiveState::shown`, down
on its clock), attention while anything waits on you (`app/attention`,
`../attention/CONTEXT.md`: your move or an unseen result in a daily
match, an unread DM or mention, a companion uncared for today; down when
the last row clears). Both end, which is what makes them guests; a widget
that stays up for an evening (the Live roster, the bonsai) is a tile
kind. A tile hosts any of the guests (`guests` on the leaf, in
`Guest::ALL` order, `Node::toggle_guest`; a guest is on the page once;
the stock Home's reef hosts now and its lobby attention), and a guest is the tile while it is
up: its label in the rule (both labels when both are up), its panel in
place of the host's (`draw_guests`: one guest has the whole tile, the
strip in its full form where the tile is at least `MIN_FULL_WIDTH` by
`LIVE_STRIP_HEIGHT`, else its one row, centred down the tile, attention's
rows from the top; with both up the strip takes its rows at the top and
attention the rest), nothing of the host underneath, so the host shows
whole the moment the last guest goes. Arranging, a tile's rule names the
guests it hosts (`reef +now +attention`), so a quiet guest is never a
surprise. Attention up on the focused
tile takes `j` `k` and Enter ahead of the host's keys (`handle_attention`,
`ZenState::attention_selected`; Enter does the row's act) in arrange
mode, where tile keys run; outside it Home's keys are its chat's and the
list reads only. A layout saved with a `guests`
list reads as its first guest still known (`live` and `banner` were
guests once and read as none; `lobby`, the guest attention grew out of,
reads as attention). The active chat
tile hosting the strip is the exception: there it inserts above the
messages as the #lounge card always did (the card draws it itself, gated
to #lounge, `active_chat_hosts`); the board's day is never a guest: on a
day the board has an all-day post it is a one-row banner across the top of
the whole tree, over every tile (`draw_board_day`, amber, `7 for the
board` flush right), and `layout::rice_areas` takes that row off the
tiles for the renderer, the clicks and the reef's bounds alike
(`App::home_board_day`). On any other host the strip shows whatever room is
selected (`App::home_strip_hosted`, which also gates the strip's animation
edge and `o`). The forms by size: a tank tile draws the reef (and binds the
simulation, `App::reef_tile_inner`) only in `hub/aquarium/ui::reef_form` (wider than
30, taller than the mini tank), else the sidebar's mini tank; a bonsai
tile narrower than half the canvas draws the sidebar's preview; a music
tile tall enough draws the sidebar's stage, else the tile's own rows.

There is no zoom, no chat-only key and no narrow fold: every tile on the
page is drawn, at every width, and a device that wants the chat alone
arranges it so.

## 2. File map

```text
late-ssh/src/app/zen/
|-- mod.rs        # module declarations only
|-- state.rs      # TileKind and its key table (TileHint: TileKey, HouseKey), Guest, Node (split tree), Look, RiceLayout (serde), ZenState + edits
|-- layout.rs     # pure rect math: Gaps, Chrome (boxes, Home's rules), rice_fits (the 40x12 floor under which the page is its too-small notice alone), rice_areas (tiles + the arrange footer row while the mode is on), tile_rects, row_gutters, tile_inner, neighbour_side, pet_neighbours
|-- presets.rs    # Home's named layouts: Layouts (serde, the nine slots and the current), LayoutList (the L list and its typed name), LayoutCommand (/layout)
|-- rows.rs       # pure row builder: the unread DMs and mentions the attention guest lists
|-- ui.rs         # ZenView (one view per domain), draw_rice (the chrome, and the one match from a kind to its domain's drawer), the guests, the pickers, the layouts list, the clock and blank tiles
|-- input.rs      # handle_home_event (arrange mode's layout keys, the tiles' keys: act, one arm per TileKey), room walk, focus, the layouts (load, save, the list, /layout, /zen)
`-- bigclock.rs   # block digit font for the clock tile
```

Glue: `render.rs` builds `ZenView` with a `HomeView` (the chat and card
views the page's chat and the rail draw from), one view per domain for
the rest (`hub/aquarium/ui::TankView`, `presence/ui::PresenceView`,
`sidebar::MusicStageProps`, `pet/ui::PetView`, `live/panel::LivePanelProps`),
one chat view per chat tile over `App::home_chat_rows_caches`, the
arrange footer, and the guests' inputs (`StripGuest`, `attention`, the
rows `App::attention_items` gathers). Each tile's drawing lives in its
domain (`bonsai/render::draw_tile`, `hub/aquarium/ui::draw_reef_tile` /
`draw_tank_tile`, `pet/ui::draw_tile`, `sidebar::draw_music_tile`,
`audio/viz::draw_visualizer_tile`, `lobby/daily/panel::draw_daily_compact`,
`presence/ui::draw_pulse_tile`, `chat/ui::draw_activity_tile` /
`draw_friends_tile`, `live/panel::draw_live_inline`, `presence/ui::draw_core_block`);
`ui.rs` only dispatches on the kind. Adding a kind: the variant, its
row in `TileKind::ALL` (the one list the compiler does not check), and
the arms the build then asks for (`label`, `one_per_page`, `takes_focus`,
`hints`, the stock `home()`, the `draw_rice` match), a `draw_tile` on
the domain's surface, and the kind table in `devdocs/HOME-LAYOUTS.md`. `input.rs`: `handle_home_event` ahead of Home's own keys,
`Ctrl+E` (`layout_chord_globally`, `go_home_for_layouts`) among the
reserved chords, the Esc arm, `focus_home_tile_at`, the
`StatusClick::ZenLayout` click and the guide's `HelpTopic::Layouts`.
`/layout` and `/zen` are parsed in `chat/state.rs` and handed over in
`chat/input.rs`. `state.rs`: `App::home`, `App::home_layouts`,
`App::layout_list`, `on_home`, `home_tiles_area` / `home_tile_rects`,
`home_chat_room_id` / `home_chat_rooms`, `sync_aquarium_bounds`, and
`mark_home_layout_dirty` / `flush_home_layout`. `tick.rs`: the aquarium
stepping, the ascii tile's cadence edge (`ascii_edge`), the anim edge
and the debounced flush. `late-core`: `extract_key_home_layouts` /
`UserSshKey::set_home_layouts` and the seeds `extract_key_home_layout` /
`extract_key_zen_layout` in `user_ssh_key.rs`, the account seeds
`extract_home_layout` / `extract_zen_layout` in `user.rs`.

## 3. Keys

Where they are shown: in arrange mode only, each tile names its own keys on
the right of its title (`TileKind::hints` and `Guest::hints`, drawn with
`hint_line` so the key is amber and the word dim), the ones the tile takes
itself (`TileKey`: an ascii tile's pieces, the tank's feed, a chat's `[`
`]`, the attention guest's rows) only with the focus on the tile. The
house keys a title names (`HouseKey`) are answered in `app/input.rs`
after Home, so arrange mode lets their letters (`w`, `m`, `s`) through
where it swallows every other stray letter (`house_letter`,
`input_flow_test.rs`); the chords (`Ctrl+/`, `Ctrl+G`) are reserved
ahead of every page. In arrange mode `?` opens `HelpTopic::Layouts`,
which lists every key, and the bottom row is a footer naming the mode's keys
(`HOME_ARRANGE_KEYS`, trimmed from the right on a narrow page). A few panels
still name keys inside the tile in normal mode: the music dock's `v+r tune`
and the sidebar bonsai's `w care`. The lobby's compact footer has none.

Outside arrange mode Home's keys are Home's (`dashboard::input`): the
active chat tile is Home's chat, the rail its room, `i` / Enter compose
in it, `j` `k` select in it and the message actions act on its
selection; the furniture takes no keys. With a Live tile on the page,
`s` then a row's number opens that row (`live::input::open_from_prefix`,
Home's own prefix, as the sidebar's Live panel opens; any other key after
`s` is swallowed), and a click on a row opens it without focusing the
tile. There is no `o` for the tile: it is the panel, not the strip.

`Ctrl+E` enters arrange mode
(`zen::input::toggle_home_arrange`; `Ctrl+E` from any other page walks
Home into it first, closing the modals; not while Home's composer holds
a draft). Leaving or entering closes the pickers, the ascii piece picker
included. In it:

- Focus: Tab / Shift+Tab and the arrows walk every tile (the page owns
  Tab; it is not the page switch here); a left click focuses the tile
  under it (not through a modal, the same guard as the pet click). A
  click on the pet pets it and leaves the focus where it was.
- `space` opens the tile picker over the focused tile
  (`ZenState::kind_picker`: one row per `TileKind`, alphabetical
  (`TileKind::ALL`), scrolled with the selection on a short page, `j` `k`
  and the arrows move, Enter or `space` picks, Esc closes; the picker owns
  every key and click while it is up, and a refused row stays up with a
  banner: Chat past `MAX_CHAT_TILES`, a second rail, both `one per page`).
- `+` opens the guest picker over the focused tile
  (`ZenState::guest_picker`: one row per `Guest`, marked on the tile's
  own, Enter or `space` picks it, replacing the tile's guest, or takes
  the tile's own off, and the picker stays up).
- `S` splits the focused tile (row when wide, column when tall), `X`
  closes it (the last tile stays), `<` `>` trade one column of width and
  `{` `}` one row of height with the nearest split of that direction
  (i3's rule; a banner says so when there is none), `F` flips the
  parent, `R` resets the layout to the stock its name says, focus on its
  chat tile. `S` is refused at `MAX_TILES` (32): each split nests the
  stored JSON one level deeper and serde_json stops reading at 128, so an
  uncapped held key would write a settings row the login path can never
  parse. The layout keys that change the tree are uppercase (`S` `X` `F`
  `R`), keeping the lowercase letters free for the tiles' own keys.
- `b` cycles the border (a border boxes the layout in its look, none
  draws Home's rules), `g` the gap between boxed tiles, and `t` walks the
  focused tile's title through named, rule, hidden (`TileTitle`,
  `Node::title_at`, stored on the leaf and absent while named; a stored
  true or false from before reads as named or hidden). A rule alone costs
  the row a name does, and a lobby with a game waiting lights it. Arrange
  mode names every tile and marks the others `· rule` or `· hidden`. Under
  a border only the name draws: the border is already a rule.
- A digit `1`..`9` loads that layout and leaves the mode
  (`zen::input::load_layout`: the page goes into its own slot first; so
  `Ctrl+E 2` is the stock `zen` in two keystrokes, like a tmux prefix),
  and `L` opens the layouts list (`App::layout_list`,
  `presets::LayoutList`, drawn over the page: one row per layout with its
  digit and name, the current marked; `j` `k` and the arrows move, Enter
  loads the row and leaves the mode, `n` types a name and saves the page
  as a new layout, current from then on, `r` retypes the row's name, `d`
  deletes the row, never the last, the page showing its neighbor when it
  was the current one, `R` puts the row back to its stock; a refused name
  says why in a banner and keeps the field up).
- The focused tile's own keys (`handle_tile_keys`: the keys of the
  guests it hosts, then its own, from the same table as the title's
  hints, each a `TileKey` and an arm of `act`): `[` `]` on a chat
  tile rebind it to the previous or next room in the rail's order, top
  to bottom, wrapping (real rooms only, a collapsed section's skipped, as
  the rail skips them; a layout edit, saved), and on an ascii tile step
  it through the pieces (`TileKey::PickPiece`, saved with the layout);
  Enter on an ascii tile opens the piece picker (`../ascii/picker`,
  `App::piece_picker`, routed ahead of the page in `app/input.rs` and
  drawn over it in `render.rs`). Attention up on the focused tile takes
  `j` `k` and Enter (above); a DM or mention row opens in the first chat
  tile and moves the focus there (`open_message_target`): a DM binds the
  tile to its room; a mention binds it to the mention's room and selects
  the message the way a `Ctrl+/` message jump does. A mention in a room
  the account never joined opens the history modal instead, and a page
  with no chat tile says so in a banner. `a` on the reef or the tank feeds
  it (free, once a day, +100 chips on the first feed).
- Every other key is swallowed so a stray letter never reaches the chat;
  the `v` music chords still pass (§4). Esc peels a typed name, the list,
  a picker, then leaves the mode, puts the focus back on a chat and
  writes the layouts at once.

From any composer, `/layout` opens the list (walking Home into arrange
mode), `/layout <name or digit>` loads that layout, `/layout save <name>`
saves the page as a new one (`presets::LayoutCommand`,
`zen::input::run_layout_command`; a banner names the digit it got), and
`/zen` loads the layout named `zen`. Every form walks Home first
(`go_home_for_layouts`).

The pet has no key: it is petted with a left click and reads the rest of
the session itself. The sprout on the tank floor (the fortnightly bud;
leave it a week and it roots as a plant) is cut on its Shop row (`-`,
Companions), never a page key: dedicated keys accumulate and collide, and
the tile draws no caption for it either. The bonsai has no keys of its
own: `w` is the global Bonsai Care key. Every layout edit marks the
layout dirty; the write is debounced to tick's one-hertz edge, to leaving
arrange mode and to leaving the page, so a held resize key costs one row
update.

## 4. Gotchas

- The aquarium simulation is bound to one rect at a time. `App::sync_aquarium_bounds`
  re-binds it on every `set_screen`, resize, and layout edit to the
  aquarium tile's inner rect, from the same pure functions the renderer
  uses (`layout.rs`), so the sim and the drawing never disagree on size.
  Entering or leaving arrange mode moves the footer row, so it is a
  layout change like any other.
  It steps on the quarter edge while Home draws a reef tile
  (`aquarium_visible`, `App::reef_tile_inner` in `tick.rs`).
- The care bar is the tank's only care readout on the page: `CareBar`
  comes from `AquariumCare::bar` in `hub/aquarium/state.rs`, and the
  aquarium and tank tiles paint it (`hub/aquarium/ui.rs`,
  `care_bar_spans`, `ui_test.rs`) on the top row of the water, right
  end. A title row holds the title on the left and the tile's keys on
  the right, nothing else.
- Arrange mode passes `v` on and defers every key while
  `App::music_prefix_armed` is set, so the `v` chords (`v x` audio source, `v s` skip vote) stay global
  although the mode swallows stray letters and `X` closes a tile. Adding
  a layout key that collides with a chord suffix is fine, the guard
  covers it.
- One chat view per chat tile is built per frame in `render.rs`, over
  `App::home_chat_rows_caches` (resized to the tile count each frame), and
  `draw_rice` hands them out in layout order. Only the active tile's view
  carries the composer, the selection, the overlay, and the mouse slots;
  the tile picker refuses Chat once ten tiles are chats (`kind_allowed`,
  the row drawn faint with `full`) and a chat tile that changes kind
  forgets its room.
- The stored layout is parsed leniently: anything unreadable falls back to
  its stock, so a bad row never locks the page. Splits store the first
  child's share in per-mille (`share`, 1..999) so one cell of any split
  is a distinct value; the resize walks the tree with the rects it lays out
  on the current terminal, so a press moves exactly one row or column and
  writes back the share that reproduces it. Home's rails are fixed splits
  instead (`Share::First { first }` for the rail, `Share::Second { second }`
  for the sidebar, untagged beside the bare number), so the rail keeps its
  columns at every width and a resize moves the fixed count. A resize stops with
  `MIN_TILE_CELLS` (3) left on either side, a border and one row inside,
  so a clock fits a one-row tile. A saved layout that does not parse
  (one with a percent `ratio` field, say) resets to its stock.
- The watching pet is a render-time fact: `draw_rice` finds the pet tile's
  neighbouring tank and bonsai from the frame's rects (`layout::pet_neighbours`) and passes both sides
  into `draw_pet_box`, which records them with the travel, the box's rect,
  and where the pet stood in `App::last_pet_frame` (`PetFrameInputs`) so the
  tick-side gate (`pet::ui::frame_changed`) evaluates the same pose and
  the walk after the cursor (`PetState::tick`, fed `App::last_mouse` from
  every mouse report) knows where the cursor is relative to the box.
  Adjacency is exact edge contact, not proximity: one cell of overlap on
  the perpendicular axis and edges equal plus the gap, so a diagonal or
  corner-only tile is not a neighbour.
  A sulking or sleeping pet ignores the fish, the
  tree, and the cursor.
- Pulse draws only owned values: `online_count` and
  `PresenceSession::history`, the day of headcount buckets the tick copies
  from the presence service's watch (`../presence/svc.rs`), so it adds no
  query and no lock. The column and glyph maths is `presence/pulse.rs`;
  `presence/ui::draw_pulse_tile` lays out the header, the bars and the axis. The
  last column is live (the current count), so the chart never lags the
  header, and a bucket with no sample is a gap, never a zero.
- The list tiles (lobby, activity, friends, pulse) draw one column in from
  each border (`pad_sides` in `draw_rice`); the presence block pads only
  the right (`pad_right`); the chat,
  the canvases, the clock, and the equalizers keep the full inner area. A
  chat tile also asks the embedded chat for no inset of its own
  (`EmbeddedRoomChatView::messages_inset: 0`): the tile border already is
  the column of air, so the text sits where Home's does. House tables and
  daily boards have no border around their chat and keep an inset of 1. The pot and the status were tried there and cut as
  low value. A
  history chart was tried and dropped: presence is per replica, so a
  process-local day of headcounts would disagree between pods.
- The mentions list loads only on ask (`notifications.list()`), so while
  the page hosts the attention guest (`ZenState::hosts`) tick requests it
  again whenever the unread count differs from the last request
  (`App::home_attention_listed_unread`). A stored layout naming a dropped
  kind (`inbox`, `headlines`) reads it as `blank`; a small chat tile bound
  to News is the headlines strip now (`../chat/news/CONTEXT.md`, the
  compact list).
- Tests cover the split tree, the fixed splits and the guests' moves, the
  stock Home (`state_test.rs`), neighbour detection, the gutters and the
  rules chrome's insets (`layout_test.rs`), the named layouts and their
  seeds (`presets_test.rs`), the title modes, the hints and the chrome
  (`ui_test.rs`), the resize floor (`state_test.rs`),
  the unread rows (`rows_test.rs`), and the drawn-kinds gate
  (`state_test.rs`). `input_flow_test.rs` drives `Ctrl+E` and the keys it
  owns, `Ctrl+E` and a digit, `/zen`, Tab walking the tiles, the tile
  picker owning the keys and the clicks under it, the chat keys staying
  off a tile that is not a chat, the Live tile's `s` prefix,
  `[` `]` in rail order, a second chat tile keeping its room with the
  rail following the active one, a draft staying in its room, the pet
  click leaving Home's focus alone, and `?` opening the Layouts topic.
  The Live tile's rows are the panel's (`live/panel_test.rs`), the care
  bar and the tank tiles the aquarium's (`hub/aquarium/ui_test.rs`), the
  bonsai cut the bonsai's (`bonsai/render_test.rs`), the music tile's
  rows the sidebar's (`common/sidebar_test.rs`), the pulse rows
  presence's (`presence/ui_test.rs`), the note's wrap
  `common/primitives_test.rs`; the rest of the tile drawing is untested.
