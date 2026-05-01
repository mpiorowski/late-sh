# Rooms Context

## Metadata
- Scope: `late-ssh/src/app/rooms`
- Last updated: 2026-05-01
- Purpose: local working context for the persistent game-room directory and trait-backed room game runtimes.

## Source Map
- `mod.rs` only declares modules. Keep it declaration-only; do not add `pub use` re-exports.
- `backend.rs` defines the room-game traits: `RoomGameManager` for static/table-manager behavior and `ActiveRoomBackend` for per-session active-room behavior.
- `registry.rs` owns the process-local `RoomGameRegistry` and dispatches `GameKind` to Blackjack/Tic-Tac-Toe managers.
- `svc.rs` owns persistent room creation/listing/deletion over `game_rooms` plus associated `chat_rooms(kind='game')`. It stores opaque `settings: serde_json::Value`; games parse their own settings. Slug prefixes are resolved from `RoomGameRegistry` at the call site and passed into room creation.
- `state.rs` drains `RoomsService` snapshots/events into `App` fields, clamps list selection, and refreshes the active room copy.
- `input.rs` routes the room directory, create form, search mode, active table, and embedded room-chat keys.
- `ui.rs` renders the directory, create modal, active room split, and delegates game drawing.
- `filter.rs` is pure filter state over `All` or a real `GameKind`.
- `blackjack/manager.rs` maps `GameRoom.id` to process-local `BlackjackService` instances.
- `blackjack/svc.rs` is the authoritative in-memory Blackjack table runtime.
- `blackjack/state.rs` is the per-session client wrapper plus pure Blackjack scoring/bet logic.
- `blackjack/ui.rs` renders the Blackjack table in fancy or compact layouts.
- `blackjack/settings.rs` serializes table pace/stake settings into `game_rooms.settings`.
- `blackjack/player.rs` loads username and chip balance data for seated players.
- `tictactoe/manager.rs` maps `GameRoom.id` to process-local `TicTacToeService` instances.
- `tictactoe/svc.rs` is the authoritative in-memory Tic-Tac-Toe board runtime.
- `tictactoe/state.rs` is the per-session Tic-Tac-Toe client wrapper.
- `tictactoe/ui.rs` renders the Tic-Tac-Toe board and seats.

## Persistence Model
- `late_core::models::game_room::GameKind` is a Rust enum over text. It currently has `Blackjack` and `TicTacToe`.
- A game room persists in `game_rooms`; its chat pane is backed by a unique `chat_room_id` pointing at `chat_rooms(kind='game', visibility='public', auto_join=false, game_kind, slug)`.
- `GameRoom::create_with_chat_room` creates the chat room and game room in one SQL CTE. `RoomsService::create_game_room` then joins the fixed dealer user to that game chat.
- `RoomsService` publishes `RoomsSnapshot { rooms: Vec<RoomListItem> }` through `watch` and transient `RoomsEvent` values through `broadcast`.
- `late-ssh/src/main.rs` calls `rooms_service.refresh_task()` at startup before the hourly inactive-table cleanup loop is started.
- Room creation is capped at 3 non-closed tables per creator per game kind.
- `RoomsService::cleanup_inactive_tables_task` runs hourly and marks tables `closed` after 12h without a `game_rooms.updated` touch.
- Entering any real room calls `RoomsService::touch_room_task(room.id)`.
- Deleting a room is a soft close through `GameRoom::close_by_id`; closed rows disappear because snapshots use `GameRoom::list_open`.

## Directory Behavior
- The Rooms screen is key `4`.
- The list contains real `game_rooms`; placeholder rows were removed when Tic-Tac-Toe shipped.
- Filters cycle through `All` and each real `GameKind`.
- Search is a case-insensitive substring match on `RoomListItem.display_name`.
- `rooms_selected_index` counts only visible real rooms.
- `state.rs::visible_real_rooms_count` and `input.rs::visible_real_count`/`visible_real_room_at` intentionally duplicate the same filter/search predicate. Change them together.
- Wide directory layout starts at `NARROW_WIDTH = 80` and renders a columned table. Narrow layout renders two-line cards.
- Directory handlers support `j/k` and up/down arrows to navigate, `h/l` and left/right arrows to filter, `/` to search, `n` to create, `d` to delete, and `Enter` to enter. The rendered footer is role-aware: `n`/`d` show only for admins, and `Esc` shows only for admins/mods.
- In the idle directory, `Tab`, `Shift+Tab`, and number keys remain global screen navigation, not Rooms filter shortcuts. The create modal consumes `Tab`/`BackTab` for field focus, and active-room input is intercepted before global screen switching.
- Directory `Esc` peels state in this order: create form -> active search -> search query -> non-All filter -> active room/list exit. Active rooms bypass that directory escape path: `Esc` first clears embedded chat selection when present, then routes to the game and may leave the room.
- Create/search input limits: room name max 48 chars, search query max 32 chars, default create names come from `RoomGameRegistry`, and pasted text is passed through paste-marker sanitization.

## Access Policy
- Room creation and deletion are admin-only in `input.rs`.
- Room entry is currently open to every user: `can_enter_room` returns `true` for admin, mod, and ordinary users. Older root-context notes that only admins/mods can enter are stale.
- Create modal lets admins pick a real game kind. Blackjack-specific pace/stake fields render only when Blackjack is selected; Tic-Tac-Toe uses empty JSON settings.

## Active Room and Chat
- Entering a room calls:
  - `app.chat.join_game_room_chat(room.chat_room_id)`
  - `app.chat.request_room_tail(room.chat_room_id)`
  - `app.rooms_service.touch_room_task(room.id)`
  - `app.room_game_registry.enter(&room, app.user_id, app.chip_balance)` when the active backend is not already for the same `room.id`
- Game-chat joining is async. `ChatEvent::GameRoomJoined` triggers a chat `request_list()` refresh and another tail request after the membership write lands.
- The active room area is a vertical split: preferred game height, one spacer, then an embedded chat pane.
- The bottom pane is no longer just a placeholder; `render.rs` builds `EmbeddedRoomChatView` from the associated game chat room and `rooms/ui.rs` calls `chat::ui::draw_embedded_room_chat`.
- Active room key routing lets embedded chat own composer/message actions first for keys like `i`, `j/k`, arrows, scroll, reactions, copy, reply/edit/delete, and selection escape.
- The active `ActiveRoomBackend` receives remaining game keys. `q` is normalized to `Esc` inside active Blackjack rooms by its backend implementation.
- The outer Rooms title appends active-room status from backend `title_details`: room name, seated count, role/seat label, and optional chip balance.
- `App.active_room_game` is the single per-session active game backend. Do not add per-game `Option<State>` fields to `App`.

## Dashboard Integration
- `dashboard/ui.rs` renders a Blackjack room strip above dashboard chat when the full dashboard/header layout is active, room showcases are enabled, the viewport meets the dashboard's width/height gates, and there is enough space above the chat section.
- The strip takes the first three Blackjack rooms from `RoomsSnapshot`.
- Slot keying is a two-key prefix: `b1`, `b2`, `b3`. The input path only arms `b` when room showcases are enabled and at least one Blackjack room exists.
- `dashboard/input.rs::enter_blackjack_room_slot` delegates to `rooms::input::enter_room`, then switches to `Screen::Rooms`, so table touch, chat join/tail load, and Blackjack runtime setup are shared with the directory path.
- Backtick toggles Dashboard <-> the last active game target. Room-backed tables set the target to `DashboardGameToggleTarget::Room`; Arcade games under `late-ssh/src/app/games` set it to `DashboardGameToggleTarget::Arcade`. `rooms::input::enter_room` records `App.rooms_last_active_room_id`; Dashboard resolves room targets against the current `RoomsSnapshot`, while active-room backtick returns to Dashboard without clearing `rooms_active_room`.
- Direct global screen jump `4` opens the Rooms directory, not the active room. It clears `App.rooms_active_room` but keeps `rooms_last_active_room_id`, so backtick remains the way to return to the last game room.

## Blackjack Table Runtime
- `BlackjackTableManager` is process-local. It lazily maps each entered `GameRoom.id` to a `BlackjackService`.
- Restarting the SSH process drops all in-memory table state. Existing open `game_rooms` survive, but re-entering creates a fresh runtime table.
- `BlackjackService` owns the table truth: seats, shoe, dealer hand, phase, deadlines, stakes, pending bets, and settlements.
- `blackjack::State` is only a per-session client wrapper around service snapshots/events.
- `BlackjackPlayerDirectory` reads `late_core::models::blackjack::BlackjackPlayer` so seats can carry `BlackjackPlayerInfo { user_id, username, balance }`.
- Player info is loaded from DB on sit. Accepted bets and settlements update the seated player's balance in-place; if no player info was hydrated, the service may synthesize a fallback username of `player`. Rendering should read `BlackjackSeat.player`; do not add per-render DB/chip lookups.
- There are four seats. Entering a room starts as a viewer. `s` or `Enter` sits in the first open seat.
- `l` leaves a seat when safe. Locked/pending bets block leaving during active phases, but settled players may leave during `Phase::Settling`.
- Seated players build a shared visible stake through service-owned `SeatState.stake_chips`.
- Chip selection is client-local (`selected_chip_index`). Thrown stake chips are service-owned and appear in every subscriber's `BlackjackSeat.stake_chips`. Re-entering the same active Blackjack room from Dashboard reuses the existing client `blackjack::State` so selected chip, private notices, and subscription cursors do not reset; entering a different table still creates a fresh client wrapper.
- Betting keys: `[`/`a` selects previous chip, `]`/`d` selects next chip, Space throws the selected chip, Backspace pulls one chip, `c`/Ctrl+W clears, `Enter`/`s` submits.
- Player action keys: `h`/Space hits, `s` stands, and `d`/`D` doubles down when eligible.
- Table stake settings are `10`, `50`, `100`, or `500` chips. `min_bet` is the stake and `max_bet` is `stake * 10`.
- Table pace settings (`Quick`, `Standard`, `Chill`) control the player action timeout only: 2m, 5m, or 10m.
- The first confirmed bet starts a fixed 30s betting cap (`BETTING_LOCK_CAP_SECS`). It does not restart on later bets. If all seated players have locked bets, the round deals immediately.
- Pending async chip debits can delay auto-deal; the service waits until no pending bets remain.
- During `PlayerTurn`, all betting seats can hit/stand/double their own hands in parallel. Dealer resolution runs after every unresolved hand has stood, busted, or naturally settled.
- Double down is allowed only on an active two-card hand with a locked bet and enough chip balance for one extra wager equal to the original bet. The service marks the seat `SeatPhase::ActionPending` while the extra chip debit is pending, then doubles the recorded bet, draws exactly one card, and auto-stands or bust-settles the hand. Double-down settlement uses the doubled bet amount.
- Action timeout auto-stands remaining hands when the pace-specific deadline expires, then removes those non-acting seats after settlement.
- A seated player who misses 3 deals without a locked bet is removed from the table.
- A seated player who sends no active-room input for 5 minutes is removed from the table; active-room keys, arrows, and scrolls refresh this room timer while seated.
- Settlements use `ChipService`: zero-credit losses call `restore_floor`, payouts call `credit_payout`, and `BlackjackEvent::HandSettled` updates client balances.
- House rules: 6-deck shoe, reshuffle at 52-card penetration, dealer stands on soft 17, natural blackjack requires exactly two cards, and blackjack pays 3:2.
- `Phase::BetPending` exists in the shared enum and input/UI paths, but current pending debit state is expressed per seat as `SeatPhase::BetPending`; the service does not currently transition the whole table into `Phase::BetPending`.
- `BlackjackService::deal_task` exists as a manual deal API, but active room input does not currently route a key to it. Normal play deals by all seated players locking bets or by the 30s betting cap.

## Tic-Tac-Toe Runtime
- `TicTacToeTableManager` is process-local and lazily maps each entered `GameRoom.id` to a `TicTacToeService`.
- Restarting the SSH process drops in-memory board state. Existing open `game_rooms` survive, but re-entering creates a fresh board.
- There are two seats: X and O. Entering starts as a viewer; `s`, `Space`, or `Enter` sits when not seated.
- Seated players can press `1`-`9` to place directly, move a local cursor with `w/a/d/x` or left/right arrows, and press `Space` or `Enter` to place on the cursor. `j/k` and up/down remain embedded-chat navigation in active rooms.
- `n` starts a new round for seated players. `l` leaves a seat and resets the board.
- Tic-Tac-Toe has no chip-balance hook; `ActiveRoomBackend::chip_balance` returns `None`.

## Blackjack UI Invariants
- `blackjack/ui.rs` chooses render tier from area dimensions:
  - Fancy path when `area.height >= FANCY_MIN_HEIGHT` and `area.width >= FANCY_MIN_WIDTH`.
  - Ultra-fancy inside the fancy path when the area also satisfies `ULTRA_FANCY_MIN_*` and can fit all outline seat panels.
  - Compact path otherwise.
- Current constants are `FANCY_MIN_WIDTH = 60`, `FANCY_MIN_HEIGHT = 19`, `ULTRA_FANCY_MIN_WIDTH = 96`, `ULTRA_FANCY_MIN_HEIGHT = 23`, `SEAT_PANEL_WIDTH_OUTLINE = 22`, `SEAT_PANEL_HEIGHT_OUTLINE = 11`, `SEAT_PANEL_WIDTH = 12`, `SEAT_PANEL_HEIGHT = 7`, and `DEALER_BLOCK_HEIGHT = 9`.
- If panel dimensions change, update min thresholds first. The fancy layout indexes fixed vertical chunks and can panic if thresholds allow too-small areas.
- Player-specific info belongs on seats: username, balance, stake chips, cards, total, locked/pending bet, phase, and outcome.
- The bottom info bar should stay minimal: selected chip, phase, countdown/status. Do not duplicate balance/stake/locked bet there.
- Compact mode still uses the generic game frame/sidebar path; fancy modes use the custom table layout.

## Chat Interactions
- `chat_rooms.kind = 'game'` stays in chat state so embedded room chat works.
- Main Chat room-list rendering skips game rooms, so game-backed rooms do not appear as normal chat rooms or favorites.
- Room entry requests a chat tail; live broadcasts then keep the embedded chat updated like other room-explicit chat flows.

## Known Gaps
- Blackjack table state is not durable across process restart.
- There is no AFK/disconnect cleanup path tied to SSH session lifecycle.
- Blackjack and Tic-Tac-Toe are real. Dashboard showcases remain Blackjack-only.

## Test Guidance
- Pure rules in `filter.rs`, `settings.rs`, `blackjack/state.rs`, and key-routing helpers can use inline unit tests.
- Anything that touches `RoomsService`, `GameRoom`, `ChatRoom`, chip balances, or service tasks belongs in `late-ssh/tests/` and must use testcontainers through the existing helpers.
- Do not run `cargo test`, `cargo nextest`, or `cargo clippy` as an agent in this repo. Leave those gates for the human owner.
