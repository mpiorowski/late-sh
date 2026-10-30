# Realm: screens

What a session sees: the hub card, the board and its six views, the order
prompt and the night's dispatch, the world's chat, the live strip, the
colours, and the history slider with its replay. State lives in
`state.rs::RealmState`, which is also the board's one place that asks which
engine runs the game (`Resolution`); every view draws what it is handed. The
rules behind every number are in `RULES.md`.

## Surfaces

- **The Games hub card** (`landing_ui.rs`, `HubGame::Realm`, in the house group after Lateania): the standing world as the session's snapshot has it — season, world, pace, the gates (mustering with the countdown, open with the share claimed, or closed), the refill hour in both clocks, who is alive, your own holdings — the last world's end while it is still listed (who won and how, from the news's `win_kind`), how the game runs, what the pot pays, and the keys before Enter (`j` while the gates are open to this viewer) and after it. Laid out like the other door landings, DCSS the reference: block logo, pitch (how this world is won and how many may play, from the standing world's ruleset, the house's when none stands: `how_it_is_won`, the tagline), a strip of frontier in the board's own player colours with its legend, then headed sections. Enter opens the board (`input::open_realm`, through the same `enter_board` the strip uses), to play or to watch, the last world's between seasons, and resumes a board hopped away from; between a fall and the next rise (a sweeper tick at most) it says so in a banner. The card wears the green pip while this session's board is open (`HubGame::live_screen`), which also makes the realm a stop on the backtick cycle.
- **Joining happens on the board.** `j` on the map, for a visitor while the gates are open (`RealmState::can_join`), opens the colour picker (`join_ui.rs`, `RealmJoinDraft`): the twenty colours in two columns of ten, read down, who wears the taken ones, and the cursor never on one of those; Enter marches in, Esc puts it away. The service settles the colour again (`svc::pick_color`) because two people can pick at once. There is no create path anywhere in the UI: the house opens worlds.
- **The world's chat** (one room per world, `chat_rooms(kind='game', game_kind='realm', visibility='public')`, slug `realm-{game_id}`). Born in the opening transaction (`RealmGame::attach_chat_room` inside `open_world`), so every season starts on an empty room and the last world's talk stays with the last world's board while both are listed. Public, so anyone who opens the board talks, players and watchers alike: the board joins it once per open (`RealmState::take_chat_join`, from `App::tick`, the daily board's shape), shown or not. **Hidden by default**, so the board opens on the whole map; `c` (or F4, the key that also shows or hides the watchable doors' watcher chat) puts it up as a right column on every view (a third of the width, never narrower than the rail; on the map it stands where the rail was, and the rail is gone while it is up), `c` again puts it away. `RealmState::chat_open` is per session and survives reopening the board. While it is up, `i` types into it and a message picked with the mouse takes the message keys and `j`/`k`/arrows until Esc lets it go; the chat never claims `j`/`k` on its own, because the board needs them. Hidden, the footer leads with "c chat (N new)", from the room's unread count; the room only counts as read while it is shown (`shown_chat_room_id` is what `embedded_chat_room_id` and the visible-room sync read). Every chat gate (clicks, wheel, composer priority, Esc peeling) comes from the two rosters in `app/input.rs`. A world from before rooms gets one from the sweeper (`late_ssh_realm_chat_room_attachments_total`, by outcome; `late_ssh_realm_chat_rooms_reaped_total` counts the 30-day reap); the attachment is in the snapshot diff, so a board loaded before it reloads on the sweep, and until then says so on `c`. A lone Esc on the board peels the colour picker, then the results card, then the order prompt, then a replaying night (`RealmState::peel_overlay`, the one order, read by `dispatch_escape` in `app/input.rs` and by the board's own `q`/Esc), then a picked message, before it closes the board. No voice channel.
- **The live strip** (`live.rs`): the world's last moment, drawn beside a scrap of coastline, with who is alive and how much is claimed; `o` or a click opens the board the way the card does (`input::open_from_live`, which owns the "already on that board" and "left the listing" checks; `RealmSnapshot::listed` is the one answer to which worlds the strip can show). The strip never builds a world: the news carries the country's name.
- **Watching** (`state::ViewerRole`): the board can be opened by anyone from the card or the strip, and a visitor is watching until they take a spot. The role is derived per board: `Playing` (in it and alive), `Fallen` (on the roster, out of the war), `Watching` (never in it). Read-only is enforced twice over — `RealmState::act_on`/`fortify_on` refuse before the round trip (`gate`), and the engine refuses again (`ActionRejected::NotPlaying`, `OrderRejected::NotPlaying`) — so the presentation only has to be honest, not load-bearing. What changes: the header trades points and the night's countdown for the state of the world (the season's day stays), the rail drops the points card and the odds (they are computed from land the viewer does not have), the target table drops the reach or front and odds columns and becomes an atlas, the orders view has no orders and no prompt and shows the last night whole (every fight, not only theirs), the standings say what they are looking at instead of "you hold nothing", and the footer stops offering keys that do nothing. The replay is theirs in full.
- **Selected rows are filled, not just marked.** The target table and the
  holdings list pad each row to the panel width and hang `theme::row_style`
  off the line, so the selection covers every column rather than stopping at
  the last word — six columns of a row is too much to tie together with a
  `▸` alone. `row_style` is the shared treatment (a fill where the palette has
  a paintable canvas, `REVERSED` where it does not), which is why the check
  compares only the part it contributes: cells keep their own fg and bold.
- **`Screen::Realm`** (outside the Tab cycle; entered from the hub card or the live strip; `q`/Esc back to the Games hub; a backtick hop keeps the board open, and `App::tick` closes a board left off screen for the shared door window, `door::game::IDLE_WINDOW` (30 min), exactly as `q` would: `RealmState::touch_board` stamps it every tick it is the open screen, `board_idle_expired` reads it): one screen, six Tab-cycled views (`RealmView`: map, targets, orders, overview, log, history). The header reads "day 12 of 28 · 4/7 pts left · night falls in 04:28:10 (18:00 UTC · 19:00 local)": the season's day on a ruleset with a clock, the points left to spend or order, and when the day turns over. The footer leads with "o orders (N pts left)" on every view for a player.
  - *Map* (`map_ui.rs`): half-block ▀ world render (2 px/terminal row). The viewport is **continuous**: `scale` is base-grid cells per drawn pixel (`MIN_SCALE` 0.25 magnifies to ~2.5km/px, the ceiling fits the whole world), `+`/`-` and the wheel multiply it by `ZOOM_STEP` (1.25), and `level_for` picks the finest mip that is not finer than the scale so small countries survive zooming out while the steps stay smooth. Pan with arrows or a left-button drag (a press that moves pans, one that does not selects — `drag_anchor`/`dragged`). **Territory outlines** are computed in screen space (does this pixel's territory differ from its neighbours?), so a border is one pixel wide at any zoom — and they are drawn only at `BORDER_MAX_SCALE` (6, ~60km/px) or finer: on a whole-world view most countries are a pixel or two wide, so outlining them is noise rather than information. A continent gone one owner's colour is still readable; the globe stays clean. `n`/`N` walks your own territories and frames each; `z` re-frames the selection; framing goes through `focus_request` because only the renderer knows the drawn area (it reads the territory's `bbox`, computed once at map build). Right rail: selected-territory card (owner, pop/area, its walls, the odds with how the take gets there in words, the route on the standard ruleset or the front on the campaign, or why a rule forbids it, and your order on it tonight, all from `RealmState::target_row`), points + the night's countdown + the zoom in km/px, the last few log lines (the day's while it has any, else the last night's), players. At `BORDER_MAX_SCALE` or finer, walls of 2 and up are drawn as their digit on the territory's centre cell (`draw_wall_digits`), inked in the owner's dark shade, or their pale one on a fill already dark: the shade says roughly how dug in a place is, the digit says exactly.
  - *Targets* (`targets_ui.rs`): the map as a table — **your own territories first** (so it doubles as the atlas of what you hold), then everything you can act on, with holder, walls, population and the real percentage. `RealmState` builds the rows by engine and the table draws them blind: on the standard ruleset nearest first then best odds, the `reach` column the route's cost (`~` across water), `war Nd` / `near Nd` while the truce blocks it; on the campaign the open targets first by their odds at one point, the `front` column the walls behind a take (`~` for a landing), and `no front`, `war Nd`, `shore Nd`, `new Nd` or `near Nd` when a rule stands in the way (`Route`, `Lock`). `j`/`k` walk it (the cursor mirrors into the map selection, so Tab keeps your place), Enter/`a` acts, `g` shows the row on the map framed; Enter on your own land frames it too, since there is nothing to act on. This is the view for playing without squinting at a 4096-wide grid.
  - *Orders* (`orders_ui.rs`): tonight's orders, each with its size, its odds as the board now stands (others' walls can go up during the day) and where it comes from, with what is left to order; `j`/`k` walk them (mirrored into the map selection), Enter puts one back in the prompt to resize, `x` withdraws it (anywhere else `x` still says a running realm cannot be left), `g` frames it. "nobody else can see these until the night". The list takes at most the top half of the view and scrolls under its heading to keep the cursor row on screen, so a day of one-point orders never pushes what is beneath it off the screen. Below it the **dispatch**: what the last night did, from `state.last_night`. A player gets their count ("you took 2, lost 1, 1 bounced", `state::dispatch_line`), every fight they were in as taker or holder, a roll of theirs with the odds their board showed when it was ordered beside the rolled odds and why they differ, read from the night's own log (walls went up, a neighbour of theirs was engaged, a landing), their own walls and fizzles, and every fall and win. `o` jumps here from any view. On a ruleset without nights it says none has passed.
  - *Overview* (`overview_ui.rs`): standings **by share of the world** (`night::share`, each territory held at `1 + weight` over the whole map), with land, people, area, share, walls, nights dominating (`N/3`, on a ruleset that wins by domination) and days played per player, plus points left with anything banked (your own on a campaign; a rival there reads `in play`, since their points left would say how much they ordered tonight), or days quiet and how far their grip has slipped. The domination bar and the season clock are spelled out beneath where the ruleset has them. Then your own clock spelled out — days played against the bar, today's allowance and bank, what another missed day would bank, and how long before your own empire starts to crumble — the world totals, the pot, and the truce countdown while it lasts. Your holdings below are a cursor list: `j`/`k` walks them, `g`/Enter frames one on the map.
  - *Results* (`results_ui.rs`): the card that goes up when a realm ends — who took it and how (dominated, ahead when the season ran out, the whole world, or the last one standing, from the `Won` line's `WinKind`, `state::win_kind`), then every player with their land, days played, chips and how they went out, then what the pot paid. It raises itself once per session when a finished game loads (and on the `GameFinished` event, with a banner), any key puts it away, `r` calls it back. The table comes from `svc::final_table`, which is **the same function the payout walks** — a results screen that recomputed the ranking would eventually disagree with the ledger, and the ledger would be right.
  - *Log* (`log_ui.rs`): the live day from state + archived days paged from `realm_days`, loading the next page as you scroll. A night's fight is one line per taker (`fought_line`, shared with the dispatch and the replay's caption): "spoon → Micronesia · 3 pts · 42% · taken from mat", or "bounced", "outfought" (another take was stronger), "held". `Claimed` and `Attacked` lines are the standard ruleset's; the campaign never writes them.
  - `a` and `f` mean what the engine says (`RealmState::act_on`/`fortify_on`, after one `gate`). On the standard ruleset `a` acts on the selection immediately (claim/attack auto-picked by ownership) and `f` digs in. On the campaign they open the **order prompt** (`board.prompt`, drawn by `orders_ui::draw_prompt` where the rail stands, over any view): the take or the walls, sized at your standing order there or at one and never past what `night::order_quote` says it can carry (the engine's own count, which also gives the "pts left after" line), the holder, the odds if the night fell now with the front behind it and where it comes from (`RealmState::prompt_projection`, through `night::projection`, recomputed per point), and what is left after it. `+`/`-`, the arrows or the wheel step the size, a digit sets it, Enter places it, Esc puts it away; the prompt owns the keys while it is up. Refusals come from `night::place_order` run on a copy of the board (`check_order`), on opening and again on Enter, so the round trip never carries an order the board could refuse, and a refusal on Enter leaves the prompt up to be fixed. `act_in_flight` blocks a second action or order until the answer lands, so a double-tap cannot spend twice. `c` shows or hides the world's chat on every view.
- **Session state** (`state.rs::RealmState` at `App::realm`): snapshot/event drains in `tick()` (result banners from `ActionResolved`, `OrderPlaced` and `OrderWithdrawn`, quiet reloads from `GameChanged`, the rise banner from `SeasonOpened`, and on `DayRolled` the summons, or with that board open the night's **dispatch banner**: "The night has passed: you took 2, lost 1, 1 bounced", raised by `take_dispatch` once a reload that started after the event lands, falling back to the summons on a game with no nights), `live()`/`can_join()` over the snapshot for the card, the strip and the join key, the colour picker (`join_draft`), board open/close with `return_screen`, oneshot row/log loads (daily board pattern), and `viewer_tz` mirrored from `App` for the hour labels.

## Colours

- **Twenty colours, three shades each** (`state::PLAYER_PALETTES`,
  `PALETTE_SLOTS` 20, tied together by a test). The hue says whose a
  territory is, the shade how dug in: walls 1 pale, 2 to 3 mid, 4 to 5 dark
  (`shade_index`). The middle shade stands for the player away from the map
  (standings, results, rail). The first ten are the middle three shades of
  season one's five-step ladders under their old names, so a game from then
  draws as before; the other ten were placed around them by a search in
  CIELAB for the widest gaps, every shade light enough to read on a dark
  terminal. `map_ui::no_two_map_fills_look_alike` holds all sixty fills at
  ΔE 15 or more across players, ladder steps at 8 or more, and every fill
  20 or more from water, free land and the selection ring.

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
  replayed onto a map, and nothing reconstructs one: its days stay readable in
  the log view and are simply not offered on the slider.
- **The night's replay** (campaign days). With a day on screen, `p` puts its
  night up on the first fight, still (`RealmState::replay_toggle`, offered
  only where `replay_available` says so: a ruleset with nights, a day whose
  opening board was kept, at least one fight); `p` again plays it, a fight
  every `REPLAY_STEP` (500 ms, advanced from `tick()`), or holds it; `.` and
  `,` step it by hand; Esc stops it, and walking to another day drops it. One
  `Fought` entry a step (`replay_steps`), drawn over the board the night
  **opened** on (`realm_days.board_open`, migration 272, written by the same
  `archive_day` upsert with the same COALESCE rule): the target lit
  (`REPLAY_TARGET`), the land each take came from lit in its taker's colour
  (their palest shade halfway to white, which no fill is), and a caption on
  the map's top row, "night · fight 2 of 7 · spoon → Micronesia · 3 pts · 42%
  · taken from mat", told by the take that decided it. The frame stays the
  whole world, as for every other day, and the rail reads "as the night of 4
  Oct began" from the opening board. `history_ui_test` steps a night through.
