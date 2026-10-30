# Home layouts (design)

Home and Zen become one page. Home stays exactly as it is for anyone who
never touches it; underneath, it is a tile tree like Zen's, and a chord
switches the keys into arrange mode on the page you are already on. The
code it talks about is `late-ssh/src/app/zen` (the tree, `state.rs`,
`layout.rs`), the Home arm of `late-ssh/src/app/render.rs`, the sidebar in
`app/common/sidebar.rs`, and the strip in `app/live`.

## Built: v1

What is in the code now, and where it stops short of the sections below
(the current state lives in `late-ssh/src/app/zen/CONTEXT.md`; this is the
delta against the design):

- Home renders through the tree (`RicePage::Home`, `App::home`), with the
  stock Home fixed (the sidebar's panel order, then the owned companions). Zen stays its own
  screen on `Ctrl+F` with its boxed look; arrange mode on Home is `Ctrl+E`
  (Esc is done), since Ctrl+F still means Zen.
- Static kinds `rooms`, `presence` and `tank` exist (the tank is the
  aquarium's small form as its own kind, the stock sidebar's; the
  `aquarium` kind is the reef alone); there is no `center` kind.
  Home's chat is a chat tile: the active one (the focused chat tile, else
  the first) is the page's chat, draws Home's card with the synthetic
  views, and the rail shows its room (a stored `center` reads as `chat`).
  The cut list (pulse, friends, activity, visualizer, blank) is not cut:
  Zen keeps them.
- Dynamic kinds are guests only (`+` on a tile), the four of the design:
  now, lobby, live, banner. There is no dynamic tile of its own kind.
- Fixed splits (`Share::First` / `Share::Second`) carry the rails; the Fit
  shares are not built.
- Chat tiles on Home bind rooms as on Zen: a tile that becomes a chat is
  pinned to the room of the moment, the rail steers the active tile and
  follows the focus onto another. The MyStream binding is not built.
- No named layouts, no layout switching: one Home layout and one Zen
  layout per device, on the SSH key (`user_ssh_keys.settings.home_layout`
  / `zen_layout`), the account's stored layouts the seed for a key with
  none.
- The borders stay as they are: Home draws rules, Zen draws its look. No
  per-page border toggle.
- No chat-only key and no narrow fold: a page is drawn as arranged at
  every width, and a phone arranges its own Home. The room-list setting is
  gone, row and storage; the sidebar row is the Arcade's. The stock Home
  is fixed (the sidebar's panel order plus the owned companions), not read
  from the Settings panel list, which the Arcade alone uses.

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
`focus_zen_tile_at`). Typing always lands in the active chat. There is no
"chat lock" mode because there is nothing to lock: it is how the page
works.

**Arrange mode is where the layout keys live.** `Ctrl+F` enters it; Esc or
`Ctrl+F` leaves it. In arrange mode the focused tile gets a ring, the
arrows walk every tile including furniture, and the Zen layout keys edit
the tree. Outside it the keys are Home's keys, unchanged. Tab stays the
page cycle everywhere, so the Tab conflict Zen carries today is gone;
focus between several chat tiles moves in arrange mode.

**The frame stays.** Tabs, both status bars and the sponsor line are the
frame, and the tiles live inside it. A frameless look is a toggle on the
layout for people who want the Zen feel. Zen's own status row
(`build_zen_status_row`) goes away with the screen.

## The look: Home's, everywhere

Boxed tiles eat two rows and two columns each; on a Zen page of eight
tiles that is a sidebar's worth of screen. Home draws no boxes: a vertical
rule between columns, a `── music ──` title rule above a panel, one column
of air after the rule. That is the one look, for every tile, in every
layout.

- A vertical split draws one rule on its shared edge, owned by the split,
  never doubled (`DESIGN_GUIDE.md` section 8: one pane owns the divider).
- A horizontal split draws the lower tile's title rule as its divider.
  A tile with no title (chat, the reef) gets a plain faint rule.
- Titles carry the tile's keys on the right, as the sidebar panels do
  (`a feed`, `w tend`, `s1-4 open`), and the care bar where the tank has
  one.
- The chat tile draws no rule of its own: the composer block and the
  `── now ──` rule are the chat's chrome, as today.

`Look` keeps `frame: bool` and nothing else. `BorderKind`, `gap` and
`titles`, and their keys `b` `g` `t`, are cut. A stored Zen layout's look
is ignored on migration.

## Tiles

### Static kinds

Always drawn. The closed `TileKind` becomes:

| Kind | What it is | Today |
|---|---|---|
| `chat` | A room, with the composer when active | Zen chat tile |
| `center` | Home's chat center: the selected rail entry, synthetic views included, the strip and the banner inserted at the top | `draw_chat_center` / `draw_dashboard_chat_card` |
| `rooms` | The room rail with its sections, counts and nav hints | `draw_room_list_rail` |
| `aquarium` | The reef, or the shop note | Zen tile |
| `tank` | The sidebar's small tank, the same aquarium in a six-row band | sidebar Tank |
| `bonsai` | The 81x26 canvas, cut to fit | Zen tile, sidebar Bonsai |
| `pet` | The box | Zen tile, sidebar Pet |
| `music` | The full stage: equalizer, volume, stations, the v keys | sidebar Music |
| `clock` | Block digits, the date under seven rows or more | Zen tile |
| `ascii` | An animated piece | Zen tile |
| `inbox` | Unread DMs, then mentions | Zen tile |
| `headlines` | News and feeds merged | Zen tile |

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
| `lobby` | a game waits on your move | `lobby.glow()` |
| `live` (the live panel) | at least one row is joinable | `live_panel_rows` non-empty |
| `banner` (the calendar banner) | an event is today | Home's top row |

"Now strip" is the name from here on; "live" is the panel. The two were
confused for long enough.

A dynamic kind is placed one of two ways:

- **As a tile.** Always drawn, with its idle text (`nobody playing`, the
  lobby's `11 open`). Today's sidebar panels.
- **As a guest on a static tile.** Invisible while idle; while active it
  paints over the top of the host under its own title rule
  (`── now ──`, `── lobby ──`), so the reader knows what landed. The host
  keeps its rect: the reef keeps swimming underneath and the simulation
  is never rebound (`sync_aquarium_bounds` stays as it is).

A host holds guests in a list. Active guests stack from the top in list
order, each at its natural height (the strip's `LIVE_STRIP_HEIGHT`, the
panel's `LIVE_PANEL_HEIGHT`, the lobby's compact rows, one row for the
banner); a guest that does not fit is dropped from the bottom, the
sidebar's rule. The `center` and `chat` tiles are the one kind of host
that inserts instead of covering: a guest on them pushes the messages
down, which is the strip's and the banner's behavior on Home today,
"wait while a message is selected" rule included. On furniture nothing
waits, as in a Zen tile.

Each dynamic kind appears once on the page, as a tile or as a guest, so
the `s` prefix, `Ctrl+G` and the clicks have one target. Clicks on a
guest work as on its panel. Guests never take focus.

The stock Home places `now` and `banner` as guests on `center` and
`lobby` and `live` as tiles in the sidebar: today's page. The use this was
drawn for: the reef across the top of the chat, hosting `now`, so the
strip lands on the fish and the chat never loses a row.

Candidates for later, once the guest list exists: `inbox` while anything
is unread, `headlines` while a story is under an hour old. The model does
not change for them.

### Chat bindings

A chat tile binds to one of a closed set, replacing `room: Option<Uuid>`:

- `Current`: Home's rail selection, the stored default.
- `Room(id)`: a room by id, as today.
- `MyStream`: the account's stream room (`get_or_create_stream_room`,
  permanent per streamer). Symbolic, so a layout holding it works before
  the first `/golive` and across a rename.

A `center` tile is `Current` by definition and draws the synthetic views;
a `chat` tile draws real rooms only, as Zen's does. `MAX_CHAT_TILES` (10)
stays.

## The tree

Zen's binary split tree (`Node`, `Dir`, per-mille `share`) is kept. Two
changes:

- **Fit shares.** `share` becomes a closed enum: a per-mille value, or
  `Fit`, meaning the first child takes its natural height (the pet's
  three rows, the lobby panel's height, the music stage's constant
  height) and the second takes the rest. Nested fit splits reproduce the
  sidebar stack exactly, drop-from-the-bottom included, and the resize
  keys skip them. Without this the sidebar is five nested per-mille
  splits that are miserable to edit.
- **A `hidden` flag is not added.** The rail modes (`\`, `auto`, the
  `AUTO_*_MIN_COLS` thresholds) are replaced by one rule: `\` zooms the
  active chat tile over the whole page and back, the same as `z` on it in
  arrange mode, and a page narrower than `ZOOM_BELOW_COLS` zooms it by
  itself. The half states (rail hidden, sidebar hidden) are gone; a
  layout without a rail is a layout, saved as one.

`MAX_TILES` (32), `MIN_TILE_CELLS` and the lenient parse stay. The now
strip's old home, the topic row of the #lounge card, is the `center`
tile's own business.

## Layouts

A device (an SSH key, the rails' precedent in `late-ssh/CONTEXT.md`
section 8) holds up to nine named layouts and which one is current. Each
is a tree plus its look. Stored on `user_ssh_keys.settings`, the account's
`zen_layout` only a seed for a key with none, then ignored.

Stock, on every device:

1. `home`: today's Home, the default.
2. `zen`: today's Zen default (the reef and the bonsai over one chat, the
   clock, pet, lobby, live and music rail), frameless.

Switching:

- `Ctrl+F` then a digit `1`..`9`: enter arrange mode, load that layout,
  leave arrange mode. Two keystrokes, like a tmux prefix.
- `Ctrl+F L`: the layouts list. `j` `k` move, Enter loads, `n` saves the
  page as a new layout, `r` renames, `d` deletes (not the last one), `R`
  resets one to stock Home.
- `/layout <name>` and `/layout save <name>` from any composer.
- A `layout` component in the status line customizer, off by default,
  shows the current name; a click cycles.

**No automatic switching.** `/golive` does not change the layout, nor
does a game waiting, nor the screensaver. A page that rearranges itself
on a house event is the keystroke-stealing problem in a new coat; the
guests cover "show me when it matters" without moving anything the reader
is holding. A streamer who wants #lounge beside their stream room saves a
`stream` layout and presses `Ctrl+F 3`.

The Settings landing page keeps Clubhouse and Home; "Zen" there becomes
"Home, layout N".

## Keys

Outside arrange mode: Home's keys, exactly. `Ctrl+F` enters arrange mode
instead of leaving for Zen. `\` zooms the chat.

Arrange mode:

| Key | Does |
|---|---|
| arrows, Tab, Shift+Tab | move the focus, every tile |
| `S` `X` `F` | split, close, flip, as Zen |
| `<` `>` `{` `}` | resize, as Zen; refused on a fit split |
| `space` | the kind picker over the focused tile |
| `+` | the guest list over the focused tile: the four dynamic kinds, a check beside the placed ones, toggled in place |
| `[` `]` | a chat tile walks the rail's rooms, an ascii tile its pieces, as Zen |
| `z` | zoom the focused tile; `z` again unzooms |
| `f` | frame on, frame off |
| `1`..`9` | load that layout and leave arrange mode |
| `L` | the layouts list |
| `R` | reset the current layout to stock Home |
| `?` | the guide, on the layouts topic |
| Esc, `Ctrl+F` | leave |

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
- A stored `zen_layout` becomes layout 2 on first login after the change,
  its look dropped, its retired kinds dropped, its `room` fields read as
  `Room(id)`.
- A stored `right_sidebar_components` list becomes the stock `home`
  tree's sidebar column in that order; a panel the account turned off is
  left out. The list and the rail modes are then dead keys, dropped on
  read the way `activity` and `pot` are today.
- `Screen::Zen`, `zen_return_screen`, the workspace base dance in
  `input.rs::zen_return_screen` and the tour's `VisitZen` stop go away.
  The tour stop becomes a `Ctrl+F` stop on Home.

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

1. Chat lock and arrange mode inside Zen as it is today. Shippable alone,
   and it tells whether the two complaints were the real reason.
2. Fit shares and the one look.
3. The `center`, `rooms`, `music` kinds and the stock `home` tree behind
   the snapshot test; the Home arm switches over.
4. Guests, with `now` and `banner` on `center` first so the stock page
   does not change, then the reef as a host.
5. Layouts per device, the stock `zen`, `Ctrl+F` digit, the list,
   `/layout`.
6. Migration, the contexts, the help topic, the tips.
