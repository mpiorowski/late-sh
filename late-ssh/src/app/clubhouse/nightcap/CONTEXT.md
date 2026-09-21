# Nightcap Context (late-ssh/src/app/clubhouse/nightcap)

## Metadata
- Domain: a small bar out back of the Clubhouse, a sub-slice of the Clubhouse domain with its own `Screen::Nightcap` (contextual, not in the Tab cycle)
- Status: Active

## 1. Summary

Six stools, a fixed menu, the last few things said, and a wall: a muted TV
in the corner, the tab board, and whatever the last sitter carved into
each stool. Reached with `n` from the Clubhouse or Enter at the back door
past the counter (`clubhouse/input.rs`, `map::BACK_DOOR`), Esc returns
there (handled centrally in `app/input.rs::dispatch_escape`, which drops
the carving knife and peels the drink menu first if either is open). Deliberately much smaller than the Clubhouse: no
walking, no floor plan, no AI bartender, and the opposite temperament. The
tavern is loud and everyone is in it; the bar out back is quiet, and only
the seated speak. See `clubhouse/CONTEXT.md` for the parent slice.

## 2. Module map

| File | Owns |
|---|---|
| `lobby.rs` | `SharedSeats`, the process-global `Arc<Mutex<..>>` seat state: who sits where, since when, pours landed this sitting, the seated roster a round is for. |
| `state.rs` | Per-session view state: seat snapshot, roster-refresh cadence, the `Drink` menu, the one `Order` in flight, the outcome channel, the footer line. Pure; never touches chips. |
| `wall.rs` | `SharedWall`, the process-global wall snapshot: the TV's headline and newest Artboard piece, the tab board, the carvings by stool. Read-only for sessions. |
| `svc.rs` | Orchestration: `NightcapHouse` (the DB and the wall) runs the one wall refresh task per process and writes carvings; `spawn_order` places an order off-thread on the chip service, mirrors the buzz into the tavern's drunk map, says the drink out loud through `HouseVoice`, logs and counts; `spawn_credit_check` counts the drinks a patron is holding. All report an `Outcome` back. |
| `input.rs` | `1`-`6` sit/stand, `i`/Enter compose (seated only), `d` menu (`1`-`4` pour, `r` a round), `c` the knife; while the knife is out every key goes to the one-line field. `compose_room` is the seat gate the icon picker also asks. |
| `ui.rs` | Renderer: title, the TV line, stool rows (name, drunk word, pour marks, time on the stool, the carving), the tab board, the last lines, the notice row, then the key hints (or the menu, or the carving field), and the composer block while this session holds a stool. |

## 3. The room (chat contract)

- `chat_rooms.kind = 'nightcap'` (migration 189), slug `nightcap`, public,
  auto-joined and permanent like #lounge, seeded at startup by
  `ChatRoom::ensure_nightcap` next to `ensure_lounge` (`main.rs`). The
  ensure also inserts a membership for every existing account, because
  auto-join runs only at account creation: without it, accounts older than
  the bar would sit down with no room to speak into. Every session holds
  the room after its first room load; a visit never writes a membership
  row. `i` before that load says "the bar is still opening up."
- Hidden by kind, not by slug: `chat/state.rs::is_chat_list_room` returns
  false for `is_nightcap_room`, which keeps it off the rail, the picker,
  Home's selection fallbacks, and the visual order. Browse lists only
  `topic` rooms and IRC projects only lounge/language/topic, so those skip
  it by construction. The slug is reserved from user creation.
- Membership is no gate here, since every account holds the room, so the
  cross-room readers that would otherwise admit it exclude the kind
  explicitly through `chat_room::HIDDEN_ROOM_KINDS`: message search
  (`ChatMessage::search_for_user`), history paging
  (`ChatMessage::list_page_for_viewer`, which the search modal's context
  window and the history modal ride), and mention resolution
  (`Notification::resolve_mentioned_user_ids`, so an @name said at the bar
  mentions nobody and never creates a row the rail could not open). A new
  hidden kind goes into that one list. Pinned by
  `chat_message_test.rs::search_and_history_never_read_a_hidden_room` and
  `notification_test.rs::a_name_said_in_a_hidden_room_mentions_nobody`.
- The screen is the only surface. `App::current_visible_chat_room_id` pins
  it while the screen is up (read cursor and tail request ride that, same
  as the Clubhouse pins #lounge). `render.rs` hands `ui.rs` the room's tail
  and a `ComposerBlockView` only while `State::compose_allowed`; slash
  commands are off (`ComposerCommands::Disabled`, as in the Lounge).
- **Only the seated speak.** `i`/Enter opens the composer through
  `input::compose_room`, which is `None` off a stool; the footer says "take a
  seat first." rather than swallowing the key. The composer block appearing
  under the bar is the "you may speak" signal. This is a client gate: the
  room is unreachable from IRC and browse, and no other client lists it, so
  the seat map (process-global, like the seats themselves) is the only path
  in. A server-side check in the send path is the next step if a second
  client ever reaches the room.
- **No AI out back.** The subtitle says so on the screen. `GhostService` takes the room id at construction
  (`main.rs`, from the startup ensure) and every bot listener (`@bot`,
  `@graybeard`, `@bartender`) drops a message from this room before any
  other check (`is_silent_room`): no reply, no ladder step, no DB read. A
  mention here is a patron saying a name into a quiet room. The bots are
  still members through auto-join, which is harmless: they have no
  session, so they never sit and never post. Slash commands are off, so
  `/summary` and the like cannot target the room from its own screen, and
  it is not selectable anywhere else.
- **The wall keeps the last lines only.** `ui.rs` draws at most `MAX_LINES`
  (10) messages of the tail, oldest at the top, each word-wrapped the way
  the composer wraps it (`build_composer_rows`, newlines kept) with
  continuation rows indented under the name; the newest messages take the
  rows there are. No timestamps: a line is lit by its age (`age_styles`),
  bright within the hour, dim within the day, faint and italic after that,
  so a slow night's old lines read as old. There is no scrollback on this
  screen and no other screen shows the room, so what is said here scrolls
  off it. Storage is ordinary chat.

## 4. The shared seats (multiplayer contract)

- `crate::state::State.nightcap_lobby` is the single process-global
  `SharedSeats`, threaded into each session through
  `SessionConfig.nightcap_lobby` (same pattern as `clubhouse_lobby`).
  Single-replica by design, same constraint as the Clubhouse.
- Nobody holds a seat by default: you are only in the room's shared state
  once you press a seat number. `App::tick_nightcap` (called from `tick.rs`
  alongside `tick_clubhouse`) only evicts disconnected occupants; it never
  auto-seats anyone. Eviction runs only while a session is on the screen,
  and entering forces a refresh, so a stale stool is gone before anyone
  sees it. `tick_nightcap` returns whether the bar moved on screen (a
  settled outcome, another patron's stool, a pour, a carve, the TV's next
  caption) and `tick.rs` folds that into the frame decision: the screen
  is on the idle cadence and draws nothing on its own, so without that
  report a change from another session sits unrendered until a keypress.
  `State::refresh_snapshot` compares stools by what the row prints (the
  sitting time at the minute), not by raw duration, so an idle bar stays
  clean.
- **A stool is held only while its owner is in the room.** Leaving the
  screen by any route (Esc, `0`, Tab, a page digit) runs
  `State::leave_screen` from `App::set_screen`, which vacates the seat and
  closes the menu. This is load-bearing: `SharedSeats::sync` evicts only
  users who dropped out of `active_users`, which means disconnected, so a
  seat kept across a screen change would outlive the visit and six of them
  would close the bar.
- Pressing your own occupied seat's number again stands you up (and closes
  the menu). Pressing a stool someone else holds is refused with
  `SeatChange::Taken`, which the footer prints.
- Occupant names are re-read from the roster on every `sync`, never cached
  at sit time, so a rename reaches the stool. Root `CONTEXT.md` §8.1 names
  seat labels as the case not to build a per-feature username cache for.
- Each stool shows how long it has been held (`SeatView.seated_for`,
  coarse minutes) and the occupant's drunk `(word)` from `App.drunk_levels`,
  the same map that labels chat authors.

## 5. Drinks are real (the tavern's rails, a different way of ordering)

- The menu is four fixed pours (`state::Drink`, 100 to 1000 chips, the
  same band the tavern's bartender quotes) plus `r`, a round for the other
  stools at `ROUND_PRICE_PER_PATRON`. No AI and no haggling: the pour shows
  as `●` marks on the stool row, a footer line for the patron who ordered,
  and one `system` line in the room for everyone else (§5.1). The house
  beer's row in the menu carries `(free xN)` while the patron is holding
  banked drinks; the count is read when the menu opens
  (`svc::spawn_credit_check`, `DrinkCredit::count_open`) and again from
  every comped pour's `remaining`, since only the house measure comes off a
  credit.
- `svc::spawn_order` runs every order on the same service calls
  `ai/ghost.rs` uses for `@bartender`: a banked round credit is cashed
  first (`ChipService::cash_round_drink`), but only for the house beer
  (`Drink::on_the_round`), since a credit pours the round's flat measure and
  a priced pick names a drink the credit would contradict; otherwise
  `buy_drink` debits
  (ledger reason `drink_purchase`, source_ref = drink name) atomically with
  the `user_drinks` buzz upsert; a round is `buy_round` with the seated
  roster minus the buyer as candidates. Each success mirrors the buzz into
  the tavern's `SharedLobby::record_drink`, so the wobble, the passed-out
  figure, and the chat `(word)` follow the patron back inside, and a credit
  bought at either bar can be cashed at either bar.
- **A round here is priced for a bar that will drink it.** `drink_rounds`
  carries the bar that sold it (`drink_round::Bar`, migration 191), and the
  buzz one drink off a round is worth follows it (`Bar::drink_points`): the
  tavern pours a flat `ROUND_DRINK_POINTS` (400), because it buys for
  everyone online and most of them never walk up, while a round bought here
  pours `ROUND_PRICE_PER_PATRON` (100), chips to points 1:1 like every
  other drink at this bar, because it buys for the patrons on the stools,
  who are sitting at the bar and will drink it. The credit itself is good
  at either bar; what it pours is set where it was bought, not where it is
  drunk.
- One order at a time per session: `State::pick` refuses while one is in
  flight ("the house is on it."), because the chips move off-thread and a
  second press would double-charge. The outcome returns over the session's
  unbounded channel and `drain_outcomes` runs every tick, on or off the
  screen, so a settled order is never lost. `OrderOutcome` is plain data;
  the failure was logged in `svc.rs`.
- Telemetry: `metrics::record_nightcap_order` (poured/comped/bounced/failed)
  for single pours; rounds count under the shared `record_round_bought` /
  `record_round_refused`, whichever bar sold them. The house's off-thread
  work has its own counter, `record_nightcap_house_failure`
  (credit_count/house_line): a stale free-drink count or a house line that
  never posted is invisible to the order counter, which keeps looking
  healthy. A round bought here does not post to the activity feed (the
  feed's sender is not threaded into the session); the tavern's bartender
  round does.

## 5.1 The house says what it pours

- Every drink that lands is announced in the room as an ordinary message
  from the `system` author (`svc::HouseVoice`,
  `ChatService::send_house_line_task`): `mat orders the whiskey neat
  (stiff).`, `mat orders the house beer (easy), on mossy's round.`, `mat
  buys the stools a round: 3 drinks, 300 chips.` The strength word is
  `Drink::strength`, which follows the price because the price is the buzz.
  A bounced or failed order says nothing: no chips moved. The footer only
  ever talks to the patron who ordered; this is how the other stools see
  it.
- No `· ` prefix. A prefixed line is an ambient #lounge feed line that
  `chat/state.rs::filter_messages` strips out of every room's messages and
  diverts into the activity ticker, so a prefixed line here would never
  reach the wall it was written for.
- The author is the same `system` bot the #lounge feed uses, ensured at
  startup by `activity::lounge::start_lounge_feed_task`; before that lands
  (or before this session's snapshot carries the room) the house says
  nothing and the drink still pours. It is not an AI, and the bot listeners
  never see it: they drop this room before any other check. The comped line
  names the buyer from `CompedDrink.buyer_user_id`, so a patron always
  knows whose round they are drinking, and reads "somebody" once that
  account is gone.

## 6. The wall (something to look at)

- **The muted TV.** One caption under the title, held about half a minute
  (`TV_DWELL_TICKS`), cycling through whatever the house has tonight: what
  the jukebox is playing (the session's `now_playing`), the last thing
  that happened on late.sh (the activity feed every session already drains
  in `tick.rs`; the drain hands `State::note_activity` the line), the
  newest article's title, and the newest piece hanging on the Artboard.
  Never posted, never spoken. `State::tv_pick` is the clock; `ui.rs`
  assembles the captions it has and asks for one.
- **The tab board.** This bar's three biggest round buyers, all time, by
  chips (`UserChips::top_round_buyers(Bar::Nightcap, ..)`). The ledger row
  does not say which bar took the order, so the round it is keyed on does:
  the query joins `drink_rounds` on `source_ref` and counts only what was
  sold here. A tavern round reaches everyone online, so at the same price a
  head it costs an order of magnitude more and would own the board forever.
- **Carved into the bar.** Each stool has one line in the wood
  (`nightcap_carvings`, migration 190, `late_core::models::nightcap_carving`),
  written by whoever sits there with `c`, one trimmed line of at most 60
  characters, kept until the next sitter carves over it. It trails the
  stool's row whether or not anyone is on it. The carver's id stays on the
  row for accountability. There is no filter and no wipe command: anyone
  who sits there can overwrite it, which is also how a moderator clears
  one.
- **One reader per process.** `NightcapHouse::spawn_wall_refresh_task`
  (`main.rs`, on the singleton shutdown) re-reads the wall every
  `WALL_REFRESH_INTERVAL` (5 min) into `SharedWall`; a carve updates the
  wall in place the moment it lands, so nobody waits for the refresh.
  Sessions copy the snapshot in `refresh_snapshot`, never read the DB.
  Same single-replica assumption as the seats.

## 7. Testing

- `lobby_test.rs`: seat toggle/move/collision, vacating, pour counting,
  the round roster, roster eviction and relabelling. Pure `SharedSeats`.
- `state_test.rs`: the seat is given back on `leave_screen`, a taken stool
  reports itself, seated-only compose and order, one order in flight until
  the channel answers, every `Outcome` footer line, the banked-drink count
  (never speaks, never frees a pour, follows a comped pour's `remaining`),
  the menu closing on stand/leave, the knife (needs a stool, trims, refuses
  an empty line, drops on stand/Esc, never frees a pour in flight), the TV
  clock, the roster cadence. `State::new` takes an
  `Option<SharedSeats>`, a `Uuid` and a name, so neither file needs an
  `App` fixture.
- `chat/state_internal_test.rs` pins that the room is never a list room;
  `late-core` `chat_room_test.rs` pins `ensure_nightcap` as idempotent,
  auto-joined, and seating accounts older than the room;
  `nightcap_carving_test.rs`, `chips_test.rs` (the tab board counts one
  bar's rounds and ranks them by chips), `drink_round_test.rs` (a credit
  pours what the bar that bought it pours, and the open count the menu
  prints) and `artboard_piece_test.rs` (the newest piece) cover the wall's
  reads and the round's rails.
- `clubhouse/map_test.rs` probes the back door (`BACK_DOOR`, a 6-wide
  door at the end of the counter with one cell of air on each side; Enter
  in front of it steps out here).
- No `ui_test.rs`/`input_test.rs`/`svc_test.rs`: the chip paths are covered
  by `games/chips/svc_test.rs`, and the rest is thin enough that the
  behavior it carries is asserted a layer down.
