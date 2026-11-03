# Home layouts (design)

Home and Zen are one page: Zen was a screen, then Home's stock layout
`zen`, and is now the tests' fixture; the second stock layout is `street`.
Home stays exactly as it is for anyone who never touches it; underneath,
it is a tile tree, and a chord
switches the keys into arrange mode on the page you are already on. The
code it talks about is `late-ssh/src/app/zen` (the tree, `state.rs`,
`layout.rs`), the Home arm of `late-ssh/src/app/render.rs`, the sidebar in
`app/common/sidebar.rs`, and the strip in `app/live`.

## Built: v1

What is in the code now, and where it stops short of the sections below
(the current state lives in `late-ssh/src/app/zen/CONTEXT.md`; this is the
delta against the design):

- Home renders through the tree (`App::home`), with the stock Home fixed:
  the rail, the reef over the chat, and the sidebar column (presence,
  visualizer, music, lobby, live, pet, bonsai). `Ctrl+E` is Home's one layouts chord
  (arrange mode, then a digit or `L`; `Ctrl+F` is unbound). There is no
  Zen screen and no `zen` layout: layout 2 is the stock `street`, and the
  status line's care click loads `home` (`/layout home`), Home first; a
  device that deleted it gets the
  `No layout by that name or number` banner. The tour's Zen stop is the
  `VisitLayouts` stop: Home with `street` up after Calendars, and `home`
  put back on the way to the lounge. Settings "Land on" is Clubhouse or
  Home (a stored `zen` landing lands in the Clubhouse and switches no layout), and
  the backtick cycle's base is always Home.
- Arrange mode also takes the tile keys the Zen screen had: `j` `k` and
  Enter on attention, `[` `]` and Enter on an
  ascii tile, `a` on the reef or tank. Outside it the furniture takes no
  keys; a click on the reef or tank feeds it in either mode, the pet's
  way.
- Static kinds `rooms`, `presence` and `tank` exist (the tank is the
  aquarium's small form as its own kind, the stock sidebar's; the
  `aquarium` kind is the reef alone); there is no `center` kind.
  Home's chat is a chat tile: the active one (the focused chat tile, else
  the first) is the page's chat, draws Home's card with the synthetic
  views, and the rail shows its room (a stored `center` reads as `chat`).
  The cut list (pulse, friends, activity, visualizer, blank) is not cut:
  they stay tile kinds, and the stock Home's sidebar puts `visualizer`
  over `music`, the band and the dock as two tiles.
- Guests are the two that end, now and attention (`+` on a tile, one a
  tile), and a guest takes its host over while it is up. The live panel
  is a tile kind only; the board's day is a banner across the top of the
  whole tree, over every tile, not a tile or a guest.
- Fixed splits (`Share::First` / `Share::Second`) carry the rails; the Fit
  shares are not built.
- Chat tiles on Home bind rooms: a tile that becomes a chat is
  pinned to the room of the moment, the active tile is rebound to every
  room or News row the rail moves to and draws and names that selection,
  and the rail follows the focus onto another tile. Leaving a layout pins
  its unbound chats to the room of the moment, so every layout comes
  back where it was.
  Bindings are `Current`, `Room(id)`,
  `Permanent(slug)` and `News`, the one synthetic view a tile holds
  (`[` `]` stop on it); the MyStream binding is not built.
- Named layouts as below (`zen/presets.rs`): nine per device on the key
  (`user_ssh_keys.settings.home_layouts`), stock `home` and `street`,
  `Ctrl+E` digit, the `L` list with save, delete and reset,
  `/layout`. `R` resets `home` alone; `street` is seeded once per device
  (`Layouts::seed_stock`, a marker in the stored set so a deleted
  `street` stays deleted) and has no stock to return to. The status line
  `layout` component is not built. A key with no `home_layouts` seeds the
  set from its `home_layout`; the old `zen_layout` is read nowhere.
- The border is the layout's: a border of `none` draws Home's rules, any
  other boxes the page in its look (`b` cycles them in arrange mode; the
  stock `street` is boxed). There is no separate `f` frame key.
- No chat-only key and no narrow fold: a page is drawn as arranged at
  every width, and a phone arranges its own Home. The room-list setting is
  gone, row and storage; the sidebar row is the Arcade's. The stock Home
  is fixed (the sidebar's panel order with the pet in the tank's slot,
  owned or not), not read from the Settings panel list, which the Arcade
  alone uses.
- A tile keeps to one room (`O`, `only` on the leaf): anywhere else it goes
  down and its sibling takes its cells. The rail and the chats stay up. The
  rules are in `late-ssh/src/app/zen/CONTEXT.md`.

## Why

Attention, from `AUDIENCE.md` section 2:

| Surface | Hours | Active share | Per visit |
|---|---|---|---|
| Home | 8,260 | 39% | 20 min |
| Zen | 1,804 | 7% | 45 min |

Home is where people act; Zen is where a terminal is parked. The two
complaints from #suggestions say why Zen never replaces Home:

- "I only go into Zen to do the chores." Home has the rail, the synthetic
  centers (News, Mentions, Feeds, Discover, Voice, Showcase, Work), the
  live strip, the calendar banner, the full music stage and the frame.
  Zen has none of them, so it is a detour.
- "I click a widget, see a message, type a reply, and the keys go
  somewhere else." In `zen/input.rs` every tile is a focus target and the
  key router switches on the focused kind. Home has one focus, the chat;
  the sidebar panels are view only. That one difference is why Home feels
  safe.

So the merge is not "Zen replaces Home". It is "Home becomes a layout, the
rail and the sidebar become tiles, and the keys never leave the chat".

## The page

**Chat is the anchor, everything else is furniture.** Only chat tiles can
hold focus. Every other tile is furniture: drawn, clickable for its one
gesture (pet, feed, a live row, a lobby row), never focused. A click on
furniture performs the gesture and leaves the focus where it was, the way
the pet click already does (`handle_pet_click` runs before
`focus_home_tile_at`). Typing always lands in the active chat. There is no
"chat lock" mode because there is nothing to lock: it is how the page
works.

**Arrange mode is where the layout keys live.** `Ctrl+E` enters it; Esc or
`Ctrl+E` leaves it. In arrange mode the focused tile gets a ring, the
arrows and Tab walk every tile including furniture, and the layout keys
edit the tree. Outside it the keys are Home's keys, unchanged, and Tab is
the page cycle; focus between several chat tiles moves in arrange mode or
by a click.

**The frame stays.** Tabs, both status bars and the sponsor line are the
frame, and the tiles live inside it. A boxed look is a toggle on the
layout for people who want the Zen feel. The page has no status row of
its own.

## The look: Home's, everywhere

Boxed tiles eat two rows and two columns each; on a page of eight boxed
tiles that is a sidebar's worth of screen. Home draws no boxes: a vertical
rule between columns, a `── music ──` title rule above a panel, one column
of air after the rule. That is the one look, for every tile, in every
layout.

- A vertical split draws one rule on its shared edge, owned by the split,
  never doubled (`DESIGN_GUIDE.md` section 8: one pane owns the divider).
- A horizontal split draws the lower tile's title rule as its divider.
  A tile with no title (chat, the reef) gets a plain faint rule.
- Every tile wears a title, a bare rule or nothing, as `t` picks. A title
  is the tile's name, and in arrange mode only its key hint on the right
  (`w tend`, `m mute`) too: the tank's care bar sits on its water, not in
  the title. A key the tile takes itself (`a feed`, `[ ] Enter pick`,
  `[ ] cycle`, `j k Enter act`) shows only on the focused tile; a house
  key on every tile. One table, `TileKind::hints`, names the hint and
  answers the key.
- The chat tile wears its title like every other; the composer block
  stays the chat's own chrome.

`Look` keeps `frame: bool` and nothing else. `BorderKind`, `gap` and
`titles`, and their keys `b` `g` `t`, are cut. A stored `zen_layout`'s look
is ignored on migration.

## Tiles

### Static kinds

Always drawn. The closed `TileKind` becomes:

| Kind | What it is | Drawn before as |
|---|---|---|
| `chat` | A room, with the composer when active | a chat tile |
| `center` | Home's chat center: the selected rail entry, synthetic views included, the strip and the banner inserted at the top | `draw_chat_center` / `draw_dashboard_chat_card` |
| `rooms` | The room rail with its sections, counts and nav hints | `draw_room_list_rail` |
| `aquarium` | The reef | a tile |
| `tank` | The sidebar's small tank, the same aquarium in a six-row band | sidebar Tank |
| `bonsai` | The 81x26 canvas, cut to fit; narrower than half of it, the sidebar's preview fitted to the tile | a tile, sidebar Bonsai |
| `pet` | The box | a tile, sidebar Pet |
| `music` | The dock alone: volume, stations, the v keys; a short tile shows the track over the station | sidebar Music |
| `visualizer` | The equalizer at the tile's full height | sidebar Music's band |
| `clock` | Block digits, the date under seven rows or more | a tile |
| `ascii` | An animated piece | a tile |
| `pulse` | People online now over a day of them, a sparkline with the peak and a time axis | a tile |

The sidebar's pinned rows (online count, clock, friends) are the top of
the stock sidebar column and not a kind of their own: they draw at the
top of whatever column holds the `music` tile first, as they do today.
If that proves awkward in the build, they become a `presence` kind; the
design does not depend on it.

Cut: `pulse`, `friends`, `activity`, `visualizer`, `blank`. Each is a thin
copy of something the frame, the ticker or another kind already shows;
`blank` is replaced by fit-sized splits (below). A stored layout naming
one of them reads as `blank` space during migration and is then dropped.

### Dynamic kinds

A dynamic widget has one yes/no question the session already answers:

| Kind | Active while | The answer today |
|---|---|---|
| `now` (the now strip) | the queue has something up | `LiveState::shown` |
| `attention` | anything waits on you: your move or an unseen result in a daily match, an unread DM or mention, the tank, tree or pet uncared for today | `App::attention_items` non-empty (`late-ssh/src/app/zen/CONTEXT.md`, the attention guest) |
| `live` (the live panel) | at least one row is joinable | `live_panel_rows` non-empty |

"Now strip" is the name from here on; "live" is the panel. The two were
confused for long enough.

A dynamic kind is placed one of two ways:

- **As a tile.** Always drawn, with its idle text (`nobody playing`, the
  lobby's `11 open`). Today's sidebar panels.
- **As a guest on a static tile.** Invisible while idle; while active it
  *is* the tile: its label in the title rule, its panel where the host's
  was, nothing of the host underneath. The host keeps its rect, so the
  reef's simulation is never rebound (`sync_aquarium_bounds` stays as it
  is) and shows whole the moment the guest goes.

Only widgets that end are guests: `now` comes down on the queue's clock
(five minutes at most), `attention` when its last row clears (a read, a
move, a feed; the care rows return each UTC day). The live panel stays
up as long as anything is on, an evening at a time, so it is a tile kind
only. A tile hosts any of the guests (the stock reef hosts now and the
stock lobby attention; one tile may host both, the
strip's rows on top of attention's when both are up; arrange mode names
them in the rule, `reef +now +attention`), and a guest is on the page
once, so the
`s` prefix and the clicks have one target. Clicks on the strip work as
on its panel; attention has rows instead, `j` `k` and Enter where tile
keys run (Home's arrange mode). Guests never take focus.

The `chat` tile is the one host that inserts instead of taking over: the
strip on the page's chat pushes the messages down, which is the strip's
behavior on the #lounge card, "wait while a message is selected" rule
included. The board's all-day line is the banner over the tree
(`layout::rice_areas`), not a guest.

The stock Home puts the strip on the reef across the top of the chat, kept
to #lounge, so it lands on the fish and the chat never loses a row, and
every other room gives the chat the reef's rows; attention goes on the
lobby: what waits on you takes the tables' tile and the feed comes back
when the list clears. The stock lobby is four rows and the pet three under
their titles, the bonsai takes the rest of the column, and the rail, the
reef and the presence rows wear no title outside arrange mode.

Candidates for later: a fresh News story while it is under an hour old.
The model does not change for it; unread mail is attention's already.

### Chat bindings

A chat tile binds to one of a closed set, replacing `room: Option<Uuid>`:

- `Current`: Home's rail selection, the stored default.
- `Room(id)`: a room by id, as today.
- `MyStream`: the account's stream room (`get_or_create_stream_room`,
  permanent per streamer). Symbolic, so a layout holding it works before
  the first `/golive` and across a rename.

A `center` tile is `Current` by definition and draws the synthetic views;
a `chat` tile draws real rooms only. `MAX_CHAT_TILES` (10)
stays.

## The tree

The binary split tree (`Node`, `Dir`, per-mille `share`) is kept. Two
changes:

- **Fit shares.** `share` becomes a closed enum: a per-mille value, or
  `Fit`, meaning the first child takes its natural height (the pet's
  three rows, the lobby panel's height, the music stage's constant
  height) and the second takes the rest. Nested fit splits reproduce the
  sidebar stack exactly, drop-from-the-bottom included, and the resize
  keys skip them. Without this the sidebar is five nested per-mille
  splits that are miserable to edit.
- **A `hidden` flag is not added.** The rail modes (`\`, `auto`, the
  `AUTO_*_MIN_COLS` thresholds) are gone with nothing in their place: the
  half states (rail hidden, sidebar hidden) are gone, and a layout
  without a rail is a layout, saved as one. There is no zoom either;
  layout presets are the planned way to switch between a full page and
  a chat alone.

`MAX_TILES` (32), `MIN_TILE_CELLS` and the lenient parse stay. The now
strip's old home, the topic row of the #lounge card, is the `center`
tile's own business; the board's day left that row for the banner.

## Layouts

A device (an SSH key, the rails' precedent in `late-ssh/CONTEXT.md`
section 8) holds up to nine named layouts and which one is current. Each
is a tree plus its look. Stored on `user_ssh_keys.settings`, the account's
`home_layout` only a seed for a key with none, then ignored.

Stock, on every device:

1. `home`: today's Home, the default, the one `R` resets.
2. `street`: the tiles Home leaves out. An ascii piece with the tank
   beside it over two chats (News, and `#announcements` bound by slug),
   and a rail of clock, pulse, friends and activity, boxed. Seeded once,
   then the device's own.

Switching:

- `Ctrl+E` then a digit `1`..`9`: enter arrange mode, load that layout,
  leave arrange mode. Two keystrokes, like a tmux prefix.
- `Ctrl+E L`: the layouts list. `j` `k` move, Enter loads, `n` saves the
  page as a new layout, `d` deletes (not the last one), `R`
  resets one to stock Home.
- `/layout <name>` and `/layout save <name>` from any composer.
- A `layout` component in the status line customizer, off by default,
  shows the current name; a click cycles.

**No automatic switching.** `/golive` does not change the layout, nor
does a game waiting, nor the screensaver. A page that rearranges itself
on a house event is the keystroke-stealing problem in a new coat; the
guests cover "show me when it matters" without moving anything the reader
is holding. A streamer who wants #lounge beside their stream room saves a
`stream` layout and presses `Ctrl+E 3`.

The Settings landing page keeps Clubhouse and Home; a stored "Zen" lands
in the Clubhouse, and login never switches a layout.

## Keys

Outside arrange mode: Home's keys, exactly. `Ctrl+E` or `/edit` enters arrange mode.

Arrange mode:

| Key | Does |
|---|---|
| arrows, Tab, Shift+Tab | move the focus, every tile |
| `S` `X` `F` | split, close, flip |
| `<` `>` `{` `}` | resize; refused on a fit split |
| `space` | the kind picker over the focused tile |
| `+` | the guest list over the focused tile: the four dynamic kinds, a check beside the placed ones, toggled in place |
| `t` | the focused tile's title: named, the rule alone, or no row, its own say from then on (a new tile starts named) |
| `O` | keep the focused tile to the room the rail is on, or free it |
| `[` `]` | a chat tile walks the rail's rooms, an ascii tile its pieces |
| `b` | the border: none is Home's rules, the rest box the page in its look |
| `1`..`9` | load that layout and leave arrange mode |
| `L` | the layouts list |
| `R` | reset the current layout to stock Home |
| `?` | the guide, on the layouts topic |
| Esc, `Ctrl+E` | leave |

Mouse, in either mode: a divider drags to resize; in arrange mode a
title drags onto another tile to swap the two leaves. The hit geometry is
`layout::tile_rects`.

## Migration and safety

Home is 60% of all hours; a default that looks different costs more than
Zen ever earned.

- The stock `home` tree reproduces today's Home cell for cell. The gate
  is a snapshot test that renders a fixed session through the old Home
  arm and through the tree and asserts equal buffers, at three widths
  (the two `AUTO_*_MIN_COLS` thresholds and a wide one). The old arm is
  deleted only once that passes.
- A stored `zen_layout` is not read; layout 2 is the stock `street`,
  seeded on first login after the change. A chat tile's `room` reads as
  `Room(id)` for a uuid, `News` for `news`, and `Permanent(slug)` for any
  other string.
- A stored `right_sidebar_components` list becomes the stock `home`
  tree's sidebar column in that order; a panel the account turned off is
  left out. The list and the rail modes are then dead keys, dropped on
  read the way `activity` and `pot` are today.
- `Screen::Zen`, `zen_return_screen`, the workspace base dance in
  `input.rs::zen_return_screen` and the tour's `VisitZen` stop are gone
  (built). The tour stop became `VisitLayouts`, a `Ctrl+E` stop on Home
  with `street` up; the homecoming move puts `home` back, so a newcomer's
  layout never changes under them.

## What this costs

- The Home render arm is rewritten as a tree, the largest single change.
  Everything it draws already exists as a function taking a `Rect`; the
  work is plumbing, the snapshot test is the net.
- Fit shares touch every tree walker in `layout.rs` and `state.rs`.
- Guests are a new render pass in `draw_rice` and a new hit source.
- Nine layouts per device is a new settings shape on the key row.
- The help topic, the splash tips, `help_modal/data.rs`, the Zen and
  statusline contexts and the root `CONTEXT.md` pages table all change.

## Later, parked

An ASCII picture as the layout's share format, like CSS grid areas:

```
rrccccccccccss
rrccccccccccss
rrccccccccccmm
```

Each letter a kind, each letter's cells a solid rectangle. A picture whose
rectangles form a guillotine partition converts to the tree; a pinwheel is
refused with a message. It would make layouts shareable in chat
(`/layout share`). Not part of the merge.

## Build order

1. Chat lock and arrange mode inside the Zen screen. Shippable alone,
   and it tells whether the two complaints were the real reason.
2. Fit shares and the one look.
3. The `center`, `rooms`, `music` kinds and the stock `home` tree behind
   the snapshot test; the Home arm switches over.
4. Guests, `now` on the reef and `lobby` wherever its owner puts it.
5. Layouts per device, the stock `street`, `Ctrl+E` digit, the list,
   `/layout`.
6. Migration, the contexts, the help topic, the tips. Built: the Zen
   screen is gone (see Built: v1).
