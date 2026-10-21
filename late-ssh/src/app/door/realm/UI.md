# Realm: screens

What a session sees: the hub card, the board and its five views, the world's
chat, the live strip, and the history slider. State lives in
`state.rs::RealmState`; the rules behind every number are in `RULES.md`.

## Surfaces

- **The Games hub card** (`landing_ui.rs`, `HubGame::Realm`, in the house group after Lateania): the standing world as the session's snapshot has it — season, world, pace, the gates (mustering with the countdown, open with the share claimed, or closed), the refill hour in both clocks, who is alive, your own holdings — the last world's end while it is still listed, how the game runs, what the pot pays, and the keys before Enter (`j` while the gates are open to this viewer) and after it. Laid out like the other door landings, DCSS the reference: block logo, pitch, a strip of frontier in the board's own player colours with its legend, then headed sections. Enter opens the board (`input::open_realm`, through the same `enter_board` the strip uses), to play or to watch, the last world's between seasons, and resumes a board hopped away from; between a fall and the next rise (a sweeper tick at most) it says so in a banner. The card wears the green pip while this session's board is open (`HubGame::live_screen`), which also makes the realm a stop on the backtick cycle.
- **Joining happens on the board.** `j` on the map, for a visitor while the gates are open (`RealmState::can_join`), opens the colour picker (`join_ui.rs`, `RealmJoinDraft`): the free colours, who wears the taken ones, and the cursor never on one of those; Enter marches in, Esc puts it away. The service settles the colour again (`svc::pick_color`) because two people can pick at once. There is no create path anywhere in the UI: the house opens worlds.
- **The world's chat** (one room per world, `chat_rooms(kind='game', game_kind='realm', visibility='public')`, slug `realm-{game_id}`). Born in the opening transaction (`RealmGame::attach_chat_room` inside `open_world`), so every season starts on an empty room and the last world's talk stays with the last world's board while both are listed. Public, so anyone who opens the board talks, players and watchers alike: the board joins it once per open (`RealmState::take_chat_join`, from `App::tick`, the daily board's shape), shown or not. **Hidden by default**, so the board opens on the whole map; `c` puts it up as a right column on every view (a third of the width, never narrower than the rail; on the map it stands where the rail was, and the rail is gone while it is up), `c` again puts it away. `RealmState::chat_open` is per session and survives reopening the board. While it is up, `i` types into it and a message picked with the mouse takes the message keys and `j`/`k`/arrows until Esc lets it go; the chat never claims `j`/`k` on its own, because the board needs them. Hidden, the footer leads with "c chat (N new)", from the room's unread count; the room only counts as read while it is shown (`shown_chat_room_id` is what `embedded_chat_room_id` and the visible-room sync read). Every chat gate (clicks, wheel, composer priority, Esc peeling) comes from the two rosters in `app/input.rs`. A world from before rooms gets one from the sweeper (`late_ssh_realm_chat_room_attachments_total`, by outcome; `late_ssh_realm_chat_rooms_reaped_total` counts the 30-day reap); the attachment is in the snapshot diff, so a board loaded before it reloads on the sweep, and until then says so on `c`. A lone Esc on the board peels the colour picker, then the results card, then a picked message, before it closes the board (`dispatch_escape` in `app/input.rs`). No voice channel.
- **The live strip** (`live.rs`): the world's last moment, drawn beside a scrap of coastline, with who is alive and how much is claimed; `o` or a click opens the board the way the card does (`input::open_from_live`, which owns the "already on that board" and "left the listing" checks; `RealmSnapshot::listed` is the one answer to which worlds the strip can show). The strip never builds a world: the news carries the country's name.
- **Watching** (`state::ViewerRole`): the board can be opened by anyone from the card or the strip, and a visitor is watching until they take a spot. The role is derived per board: `Playing` (in it and alive), `Fallen` (on the roster, out of the war), `Watching` (never in it). Read-only is enforced twice over — `RealmState::act_on` refuses before the round trip, and `apply_action` refuses again with `ActionRejected::NotPlaying` — so the presentation only has to be honest, not load-bearing. What changes: the header trades points and the refill countdown for the state of the world, the rail drops the points card and the odds (they are computed from land the viewer does not have), the target table drops the hops and odds columns and becomes an atlas, the standings say what they are looking at instead of "you hold nothing", and the footer stops offering keys that do nothing.
- **Selected rows are filled, not just marked.** The target table and the
  holdings list pad each row to the panel width and hang `theme::row_style`
  off the line, so the selection covers every column rather than stopping at
  the last word — six columns of a row is too much to tie together with a
  `▸` alone. `row_style` is the shared treatment (a fill where the palette has
  a paintable canvas, `REVERSED` where it does not), which is why the check
  compares only the part it contributes: cells keep their own fg and bold.
- **`Screen::Realm`** (outside the Tab cycle; entered from the hub card or the live strip; `q`/Esc back to the Games hub; a backtick hop keeps the board open, and `App::tick` closes a board left off screen for the shared door window, `door::game::IDLE_WINDOW` (30 min), exactly as `q` would: `RealmState::touch_board` stamps it every tick it is the open screen, `board_idle_expired` reads it): one screen, five Tab-cycled views (`RealmView`):
  - *Map* (`map_ui.rs`): half-block ▀ world render (2 px/terminal row). The viewport is **continuous**: `scale` is base-grid cells per drawn pixel (`MIN_SCALE` 0.25 magnifies to ~2.5km/px, the ceiling fits the whole world), `+`/`-` and the wheel multiply it by `ZOOM_STEP` (1.25), and `level_for` picks the finest mip that is not finer than the scale so small countries survive zooming out while the steps stay smooth. Pan with arrows or a left-button drag (a press that moves pans, one that does not selects — `drag_anchor`/`dragged`). **Territory outlines** are computed in screen space (does this pixel's territory differ from its neighbours?), so a border is one pixel wide at any zoom — and they are drawn only at `BORDER_MAX_SCALE` (6, ~60km/px) or finer: on a whole-world view most countries are a pixel or two wide, so outlining them is noise rather than information. A continent gone one owner's colour is still readable; the globe stays clean. `n`/`N` walks your own territories and frames each; `z` re-frames the selection; framing goes through `focus_request` because only the renderer knows the drawn area (it reads the territory's `bbox`, computed once at map build). Right rail: selected-territory card (owner, pop/area, live odds, hops out, or why the truce forbids it), action points + refill countdown + the zoom in km/px, the last few log lines, players.
  - *Targets* (`targets_ui.rs`): the map as a table — **your own territories first** (so it doubles as the atlas of what you hold), then everything you can act on, nearest first then best odds, with holder, hops, population and the real percentage (or `war Nd` / `near Nd` while the truce blocks it). `j`/`k` walk it (the cursor mirrors into the map selection, so Tab keeps your place), Enter/`a` acts, `g` shows the row on the map framed; Enter on your own land frames it too, since there is nothing to act on. This is the view for playing without squinting at a 4096-wide grid.
  - *Overview* (`overview_ui.rs`): standings with land, people, area, share of the world's population, **effective** strength (decay included) and days played per player, plus points left with anything banked, or days quiet and how far their grip has slipped. Then your own clock spelled out — days played against the bar, today's allowance and bank, what another missed day would bank, and how long before your own empire starts to crumble — the world totals, the pot, and the truce countdown while it lasts. Your holdings below are a cursor list: `j`/`k` walks them, `g`/Enter frames one on the map.
  - *Results* (`results_ui.rs`): the card that goes up when a realm ends — who took it and how, then every player with their land, days played, chips and how they went out, then what the pot paid. It raises itself once per session when a finished game loads (and on the `GameFinished` event, with a banner), any key puts it away, `r` calls it back. The table comes from `svc::final_table`, which is **the same function the payout walks** — a results screen that recomputed the ranking would eventually disagree with the ledger, and the ledger would be right.
  - *Log* (`log_ui.rs`): the live day from state + archived days paged from `realm_days`, loading the next page as you scroll.
  - `a` acts on the selection immediately (claim/attack auto-picked by ownership); `act_in_flight` blocks a second action until the result lands, so a double-tap cannot spend two points. `x` says a running realm cannot be left. `c` shows or hides the world's chat on every view.
- **Session state** (`state.rs::RealmState` at `App::realm`): snapshot/event drains in `tick()` (result banners from `ActionResolved`, quiet reloads from `GameChanged`, a refill banner from `DayRolled`, the rise banner from `SeasonOpened`), `live()`/`can_join()` over the snapshot for the card, the strip and the join key, the colour picker (`join_draft`), board open/close with `return_screen`, oneshot row/log loads (daily board pattern), and `viewer_tz` mirrored from `App` for the hour labels.

## History (the finished realm, walked day by day)

- **Every day keeps the board it closed on.** `realm_days.board` (migration
  198) holds a `DayBoard` — the roster, `(territory, slot)` pairs and the
  walls — written once at the rollover, which is the moment worth keeping: the
  map as it stood just before everyone's points came back. Written again at
  the finish, onto the day the game ended on, since there is no rollover left
  to carry it.
- **The snapshot carries its own roster.** A player who withdraws is *removed*
  from `RealmGameState::players`, so an index into the live roster means
  somebody else afterwards. `DayBoard::players` (`DayPlayer { user_id,
  username, color }`) makes a day self-describing: it replays in the colours
  it was played in, and it can still name somebody the game has forgotten.
  Territories point at slots in *that* list, because a uuid is 36 bytes and
  there are up to 242 a day; free land is absent rather than recorded.
- **`archive_day` takes `Option<&board>`** and the upsert does
  `board = COALESCE(EXCLUDED.board, realm_days.board)`. The day row is
  rewritten on every action, so a live mid-day write must never replace the
  closing snapshot with a half-played one.
- **Three log entries make the replay possible**: `Spawned` (an arrival, so
  a game's opening positions are somebody's doing), `Left` with the ground
  it freed, and `freed` on `Kicked` (the ground, not only the count). Without these the map could not be replayed from the log however
  complete the actions looked. `RealmGameState::log` is `pub(super)` for
  exactly these two service-side entries.
- **The view** is `history_ui.rs`: `RealmView::History` in the Tab cycle.
  Its whole interface is the slider — `[`/`]` or `←`/`→` walk the days,
  `h`/`l` too, `g`/`G` jump to the ends — and there is deliberately **no pan
  or zoom**: the map always shows the whole world at the same frame, because
  walking the days is a comparison and two frames of a different picture do
  not compare. The track is drawn between literal `[` and `]` so the strip
  says how to move it without spending a line on saying so. The strip sits
  **above** the map and leads with the word — "history · 12 Sep · day 3 of
  45" — because this view and the live board draw the same world in the same
  colours, and what tells them apart must not be the last line on screen.
  Days show as dates; the stored number is days since the epoch, which is
  right to compute with and no use to read. It **opens on day one**: the last
  day's board is all but the live map, which is the one thing this view must
  not be taken for, while day one is a handful of spawns on an empty world.
  For "day 1 of N" to be true the whole archive has to be in — the pages are
  fourteen days each and the *first* page is the newest fourteen, so an
  unfinished run's oldest day is somewhere in the middle. `pull_history_archive`
  keeps asking for the next page from the frame loop while this view is open
  (driving it off "a page landed" missed the usual case, where the first page
  arrives while the board is still on the map), and until the run is in the
  strip says "reading the run in…" rather than calling its oldest known day
  the first. The log view pages on demand. History draws through a `HistoryFrame`
  gathered before drawing, which is what lets it be tested without a session.
  Days archived before boards existed have logs and no positions; they are
  skipped rather than drawn as an empty world, which would read as everybody
  losing everything that day.
- **No backfill.** A finished game whose log carries no positions cannot be
