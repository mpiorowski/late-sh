# Rooms Context

## Metadata
- Scope: `late-ssh/src/app/rooms`
- Last updated: 2026-06-17
- Purpose: local working context for the persistent game-room directory and trait-backed room game runtimes.

## Source Map
- `mod.rs` only declares modules. Keep it declaration-only; do not add `pub use` re-exports.
- `backend.rs` defines the room-game traits: `RoomGameManager` for static/table-manager behavior, `ActiveRoomBackend` for per-session active-room behavior, and `RoomGameEvent` for cross-game runtime events such as successful seat joins.
- `registry.rs` owns the process-local `RoomGameRegistry` and dispatches `GameKind` to Asterion/Blackjack/Chess/Poker/ssHattrick/Tic-Tac-Toe/Tron managers.
- `svc.rs` owns persistent room creation/listing/deletion over `game_rooms` plus associated `chat_rooms(kind='game')`. It stores opaque `settings: serde_json::Value` plus generic `runtime_state: serde_json::Value`; games parse their own settings and, when they opt in, their own runtime state. Slug prefixes and human-readable labels are resolved from `RoomGameRegistry` at the call site and passed into room creation; `svc.rs` does not match on `GameKind` for either.
- `state.rs` drains `RoomsService` snapshots/events into `App` fields, clamps list selection, refreshes/prunes active room copies, completes validated room-entry events, and emits game-turn notifications.
- `input.rs` routes the room directory, create form, search mode, active table, and embedded room-chat keys.
- `ui.rs` renders the directory, create modal, active room split, and delegates game drawing.
- `game_ui.rs` owns room-game frame/sidebar/info helpers. Room games must not import Arcade UI helpers.
- Minted room-game rewards are recorded through `game_payout_claims`; Chess uses a 60-minute cooldown, ssHattrick uses a 15-minute cooldown, Tron uses a 5-minute cooldown, and Asterion uses a UTC daily claim.
- `filter.rs` is pure filter state over `All` or a real `GameKind`.
- `asterion/manager.rs` maps `GameRoom.id` to process-local `AsterionService` instances and prunes stopped runtimes.
- `asterion/svc.rs` is the authoritative in-memory Asterion runtime around `asterion-core`, with public/private snapshots, per-room update/render loops, daily escape payout, and empty-room shutdown.
- `asterion/state.rs` is the per-session Asterion client wrapper that drains public/private snapshots and leaves the hero slot on drop.
- `asterion/ui.rs` renders the private maze view, objective/progress/prize sidebar, radar, and power-up hints.
- `blackjack/manager.rs` maps `GameRoom.id` to process-local `BlackjackService` instances.
- `blackjack/svc.rs` is the authoritative in-memory Blackjack table runtime.
- `blackjack/state.rs` is the per-session client wrapper plus pure Blackjack scoring/bet logic.
- `blackjack/ui.rs` renders the Blackjack table in fancy or compact layouts.
- `blackjack/settings.rs` serializes table pace/stake settings into `game_rooms.settings`.
- `blackjack/player.rs` loads username and chip balance data for seated players.
- `chess/manager.rs` maps `GameRoom.id` to process-local `ChessService` instances.
- `chess/settings.rs` stores one of three clock modes in room settings: `blitz`, `rapid`, or `daily`; missing/unknown persisted values fall back to the default rapid control.
- `chess/svc.rs` is the authoritative in-memory timed Chess runtime backed by `cozy-chess` legal move generation.
- `chess/state.rs` is the per-session Chess client wrapper with local cursor/selection state.
- `chess/ui.rs` renders the cursor-first board, clocks, seats, and status.
- `poker/manager.rs` maps `GameRoom.id` to process-local `PokerService` instances.
- `poker/svc.rs` is the authoritative in-memory Poker table runtime and owns the public/private snapshot split.
- `poker/state.rs` is the per-session Poker client wrapper that drains both public table state and private hole-card state.
- `poker/ui.rs` renders the Blackjack-shaped Poker table with dealer/board top and four seats below.
- `sshattrick/manager.rs` maps `GameRoom.id` to process-local `SshattrickService` instances and prunes stopped runtimes.
- `sshattrick/svc.rs` is the authoritative in-memory real-time ssHattrick runtime around `sshattrick-core`, with public/private snapshots, per-room update/render loops, 15-minute cooldown win payout, and empty-service shutdown.
- `sshattrick/state.rs` is the per-session ssHattrick client wrapper that drains public/private snapshots and leaves the session on drop.
- `sshattrick/ui.rs` renders the half-block pitch image, overlays, score, seats, and controls.
- `tictactoe/manager.rs` maps `GameRoom.id` to process-local `TicTacToeService` instances.
- `tictactoe/svc.rs` is the authoritative in-memory Tic-Tac-Toe board runtime.
- `tictactoe/state.rs` is the per-session Tic-Tac-Toe client wrapper.
- `tictactoe/ui.rs` renders the Tic-Tac-Toe board and seats.
- `tron/manager.rs` maps `GameRoom.id` to process-local `TronService` instances.
- `tron/settings.rs` stores the light-cycle speed preset (`chill`, `standard`, or `quick`) plus the rules mode (`classic`, `gaps`, or `glitch`) in room settings. Existing persisted rooms without a `mode` key load as `classic`; newly-created default Tron rooms use `glitch`.
- `tron/svc.rs` is the authoritative in-memory Tron grid runtime and owns the real-time tick loop.
- `tron/state.rs` is the per-session Tron client wrapper.
- `tron/ui.rs` renders the light-cycle grid, riders, and controls.
- Global user-action activity lives outside Rooms in `late-ssh/src/app/activity`. The room `touch_activity` methods below are inactivity timers only. Asterion, Blackjack, Chess, Poker, ssHattrick, Tic-Tac-Toe, and Tron win outcomes publish structured `ActivityEvent::game_won(...)` values through `ActivityPublisher`; add future room-game challenge signals there instead of overloading room touch state.

## Persistence Model
- `late_core::models::game_room::GameKind` is a Rust enum over text. It currently has `Asterion`, `Blackjack`, `Chess`, `Poker`, `Sshattrick`, `TicTacToe`, and `Tron`.
- A game room persists in `game_rooms`; its chat pane is backed by a unique `chat_room_id` pointing at `chat_rooms(kind='game', visibility='public', auto_join=false, game_kind, slug)`.
- `GameRoom::create_with_chat_room` creates the chat room and game room in one SQL CTE. `RoomsService::create_game_room` wraps that in a transaction, creates/enables the game room's `voice_channels(target_kind='game_room')` row, then joins the fixed dealer user to the game chat.
- `RoomsService` publishes `RoomsSnapshot { rooms: Vec<RoomListItem> }` through `watch` and transient `RoomsEvent` values through `broadcast`. Entering a table is validated through `RoomsService::enter_game_room_task` before the client creates a game backend, so stale/deleted snapshot rows cannot render a board. Entry events carry a per-session request id; clients ignore completions that do not match the current pending request.
- `late-ssh/src/main.rs` calls `rooms_service.reconcile_round_statuses_task()` and `rooms_service.refresh_task()` at startup before the hourly inactive-table cleanup loop is started.
- Room creation is capped at 10 non-closed tables per creator per game kind.
- Every room-game service requires `RoomsService`; game backends use it to persist room status transitions, runtime state, and room activity needed by cleanup.
- `RoomsService::cleanup_inactive_tables_task` runs hourly and hard-deletes `open` tables after 1h without a `game_rooms.updated` touch. The hard delete removes the associated `chat_rooms(kind='game')` row, so existing FK cascades remove game chat membership/messages/reactions/notifications and the `game_rooms` row.
- Active rounds/matches set `game_rooms.status = 'in_round'`; cleanup never deletes `in_round` rows. When the round/match ends, the game sets status back to `open`, updating `game_rooms.updated` and giving the room a fresh 1h idle window.
- Startup reconciliation resets stale `in_round` rows to `open` for non-durable room games because their runtime state is lost on process restart. Chess is the durable exception: `in_round` is preserved only when `game_rooms.runtime_state.phase == "Active"`; finished/non-active Chess rows reset to `open`.
- Before a lazily-created Chess service exists after process restart, `RoomGameRegistry` derives Chess seat occupancy and `is_user_seated` from `game_rooms.runtime_state`, so backtick can still find durable active Chess matches.
- Entering any real room calls `RoomsService::touch_room_task(room.id)`. Active-room keyboard, arrow, scroll, and in-game mouse input also touch the room through a 60s per-session throttle, so the 1h idle window reflects ongoing room use without writing on every keypress.
- Admin deletion is also a hard delete through `GameRoom::delete_by_id`.

## Directory Behavior
- The Rooms screen is key `3`.
- The list contains real `game_rooms`; placeholder rows were removed when Tic-Tac-Toe shipped.
- Filters cycle through `All` and each real `GameKind`.
- Search is a case-insensitive substring match on `RoomListItem.display_name`.
- `rooms_selected_index` counts only visible real rooms.
- `state.rs::visible_real_rooms_count` and `input.rs::visible_real_count`/`visible_real_room_at` intentionally duplicate the same filter/search predicate. Change them together.
- Wide directory layout starts at `WIDE_LIST_MIN_WIDTH = 96` and renders a columned table with dynamic breathing room plus a `Creator` column. Narrow layout renders two-line cards and includes the creator in the metadata line.
- The selected room must be obvious at a glance: wide and narrow directory rows use an amber-selected full-row highlight, not just a leading marker.
- Directory handlers support `j/k` and up/down arrows to navigate, `h/l` and left/right arrows to filter, `/` to search, `n` to create, `d` to delete, and `Enter` to enter. The rendered footer is role-aware: `n` always shows, `d` shows only for admins, and `Esc` shows only for admins/mods.
- In the idle directory, `Tab`, `Shift+Tab`, and number keys remain global screen navigation, not Rooms filter shortcuts. The create modal consumes `Tab`/`BackTab` for field focus, and active-room input is intercepted before global screen switching.
- Directory `Esc` peels state in this order: create form -> active search -> search query -> non-All filter -> active room/list exit. Active rooms bypass that directory escape path: `Esc` first clears embedded chat selection when present, then routes to the game and may leave the room.
- Create/search input limits: room name max 48 chars, search query max 32 chars, default create names come from `RoomGameRegistry`, and pasted text is passed through paste-marker sanitization.

## Access Policy
- Room creation is open to every user for Asterion, Blackjack, Chess, Poker, ssHattrick, Tic-Tac-Toe, and Tron. The 10-non-closed-tables-per-creator-per-game-kind cap is enforced server-side in `RoomsService::create_game_room`; over-cap attempts surface to the client via `RoomsEvent::Error` (banner).
- Room deletion is admin-only in `input.rs` (`can_delete_room`).
- Room entry is open to every user for Asterion, Blackjack, Chess, Poker, ssHattrick, Tic-Tac-Toe, and Tron.
- Create modal lets any user pick a real game kind. Blackjack-specific pace/stake fields render only when Blackjack is selected; Chess-specific clock preset fields render only when Chess is selected; Poker-specific pace/blind fields render only when Poker is selected; Tron-specific speed/mode fields render only when Tron is selected; Asterion, ssHattrick, and Tic-Tac-Toe use empty JSON settings.

## Active Room and Chat
- Entering a room first calls `RoomsService::enter_game_room_task` to re-read and validate the persistent row. `RoomsEvent::EnterReady` then completes activation:
  - `app.chat.join_game_room_chat(room.chat_room_id)`
  - `app.rooms_service.touch_room_task(room.id)`
  - `app.room_game_registry.enter(&room, app.user_id, app.chip_balance)` when the active backend is not already for the same `room.id`
- Game-chat joining is async. `ChatEvent::GameRoomJoined` triggers a chat `request_list()` refresh and another tail request after the membership write lands.
- The active room area is a vertical split: preferred game height, one spacer, then an embedded chat pane.
- The bottom pane is no longer just a placeholder; `render.rs` builds `EmbeddedRoomChatView` from the associated game chat room and `rooms/ui.rs` calls `chat::ui::draw_embedded_room_chat`.
- Room-game rendering receives usernames from the render snapshot derived from `State.username_directory`, with chat-known names only as fallback. Seated users should render by username even if they have never spoken in chat.
- Active room key routing lets embedded chat own composer/message actions first: `i`, `j/k`, scroll, selected-message `f` react, `r` reply, `e` edit, `d` delete, `p` profile, `c` copy, `Enter` reply jump/image/news open, and selection escape. Game keys such as Poker `f` fold / `r` raise / `c` call still apply when no chat message is selected.
- Arrow keys are routed to the active game backend first; only if the backend declines (returns `false`) do they fall through to embedded chat message selection. Backends that don't override `handle_arrow` (e.g. Blackjack) keep the prior chat-first behavior.
- The active `ActiveRoomBackend` receives remaining game keys. `q`/`Esc` leave where implemented by the active backend, including Asterion and ssHattrick, which return `drop_on_leave = true` and free their per-session slot/state.
- The outer Rooms title appends active-room status from backend `title_details`: room name, seated count, role/seat label, and optional chip balance.
- `App.active_room_game` is the single per-session active game backend. Do not add per-game `Option<State>` fields to `App`.

## Room Game Events
- `RoomGameManager::subscribe_room_events` is the cross-game event interface. Every concrete room-game manager must expose a `broadcast::Receiver<RoomGameEvent>`.
- `ActiveRoomBackend::awaiting_my_action` (default `false`) reports whether the active game backend is blocked on the current user. `RoomGameManager::is_awaiting_user_action` is the session-wide scan used by `App::notify_game_turn` in `rooms/state.rs`; Blackjack, Poker, Chess, and Tic-Tac-Toe answer from their live table snapshots. Chess falls back to durable `game_rooms.runtime_state` only when no live Chess service exists, so stale persisted FEN cannot override the live board. The app scans on a coarse 500ms cadence, edge-detects per room through `App::rooms_turn_notified_room_ids`, emits one "your turn" desktop notification (`app/notify`, kind `game_events`) per pending turn, and resets when the turn passes. It does not suppress alerts just because the user is currently viewing the same active room, since the terminal window may be unfocused. Games never touch the notify domain directly.
- Successful first-time seating emits `RoomGameEvent::SeatJoined { room_id, user_id }`. Repeated sit presses by an already seated user must not emit another join event.
- `main.rs` starts a process-wide recent-room-join feed from all room-game event streams, keeps a bounded in-memory history, and gives each `App` a receiver plus an initial history snapshot for the Home multiplayer box. The history is process-local and best-effort like the activity feed: broadcast lag is logged but not replayed. Seat joins are not posted to `#lounge`/lounge chat.
- Individual games must not know about chat or post directly.

## Home Integration
- `dashboard::ui::recent_dashboard_rooms(&RoomsSnapshot, &RoomGameRegistry, &dashboard_room_joins, 4)` selects up to four recently joined multiplayer rooms for the Home lounge multiplayer box.
- The lounge multiplayer box displays recent seat joins as one-line shortcuts with `b1`, `b2`, `b3`, and `b4`, deduped by room so a busy table moves to the top instead of filling the box.
- `dashboard_room_joins` is still primarily process-local/live-event history, but new sessions and room snapshot refreshes seed missing entries from persisted Chess seats in `game_rooms.runtime_state`. This prepopulates the Home multiplayer box after restart for durable Chess matches; true cross-restart recency is not preserved without a future persisted event table.
- The global `b` prefix in `app/input.rs` delegates to `rooms::input::enter_room`, then switches to `Screen::Rooms`, so table touch, chat join/tail load, and runtime setup are shared with the directory path.
- Backtick cycles Dashboard/Home and the open room-backed games where the user is currently seated. Arcade games under `late-ssh/src/app/arcade` still use backtick to return to Dashboard and can be reopened from Dashboard when there are no seated room games.
- Direct global screen jump `3` opens the Rooms directory, not the active room. Backtick cycles Dashboard and the open game rooms where the user is seated.

## Asterion Runtime
- `AsterionRoomManager` is process-local and lazily maps each entered `GameRoom.id` to an `AsterionService`.
- Restarting the SSH process drops in-memory maze state. Existing open `game_rooms` survive, but re-entering creates a fresh Asterion runtime.
- Asterion embeds `asterion-core` directly instead of proxying to external game servers. The core game owns the maze/minotaur simulation; late.sh owns room lifecycle, private terminal rendering, activity, and chip payout.
- Up to 12 heroes auto-join on room entry. There is no separate viewer/sit phase: entering creates a hero and `Esc`/`q` leaves the active room, drops the per-session state, and frees that hero slot.
- The service uses one public `watch::Sender<AsterionPublicSnapshot>` plus per-user `watch::Sender<AsterionPrivateSnapshot>` channels keyed by `user_id`. Public snapshots expose room occupancy. Private snapshots expose only the current user's maze view, position/progression, radar, power-up stats, win/death state, and daily prize claim state.
- Playable maze levels are 0 through 6. The sidebar presents progress as 1/7 through 7/7; stepping through the exit from maze 6 sets core `HeroState::Victory`.
- Escaping publishes `ActivityGame::Asterion`. The first escape per UTC day atomically records `game_payout_claims` with `game=asterion`, `payout_kind=escape`, `period_kind=utc_day`, credits 4000 chips, writes `chip_ledger`, and notifies `chip_user_changed`. Later escapes that day still publish activity but do not credit chips.
- Runtime update/render tasks are per Asterion service. They stop after the service has been empty for 5 minutes, and the manager prunes stopped services from its table map. Asterion marks the room `in_round` while at least one hero is present and returns it to `open` after the final hero leaves; the persistent row is then eligible for the 1h hard-delete cleanup.
- Active-room input refreshes `game_rooms.updated` through the normal room touch path, throttled to at most once per minute. Service update/render ticks never count as persistent room activity.
- Movement uses arrows or `w`/`a`/`s`/`d`; `h`/`l` remain accepted as legacy west/east aliases. When an embedded-chat message is selected, `d` still routes to chat delete before game input. `j/k` remain embedded-chat navigation. `,` and `.` rotate the hero's facing direction.
- Power-ups are passive pink map cells. Walking onto one auto-applies a random available upgrade: Speed lowers movement delay, Vision widens the view, and Memory keeps previously seen tiles visible longer.

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
- Chip selection is client-local (`selected_chip_index`). Thrown stake chips are service-owned and appear in every subscriber's `BlackjackSeat.stake_chips`. Re-entering the same active Blackjack room from Dashboard/Rooms reuses the existing client `blackjack::State` so selected chip, private notices, and subscription cursors do not reset; entering a different table still creates a fresh client wrapper.
- Betting keys: `[`/`a` selects previous chip, `]`/`d` selects next chip, Space throws the selected chip, Backspace pulls one chip, `c`/Ctrl+W clears, `Enter`/`s` submits.
- Player action keys: `h`/Space hits, `s` stands, and `d`/`D` doubles down when eligible.
- Table stake settings are `10`, `50`, `100`, or `500` chips. `min_bet` is the stake and `max_bet` is `stake * 10`.
- Table pace settings (`Quick`, `Standard`, `Chill`) control the player action timeout only: 2m, 5m, or 10m.
- The first confirmed bet starts a fixed 30s betting cap (`BETTING_LOCK_CAP_SECS`). It does not restart on later bets. If all seated players have locked bets, the round deals immediately.
- Pending async chip debits can delay auto-deal; the service waits until no pending bets remain.
- During `PlayerTurn`, all betting seats can hit/stand/double their own hands in parallel. Dealer resolution runs after every unresolved hand has stood, busted, or naturally settled.
- Dealer resolution reveals/draws one card per step with a 900ms service-side delay (`DEALER_CARD_DELAY_MS`).
- After settlement, next-hand input is blocked for 1200ms (`SETTLEMENT_MIN_VIEW_MS`) in both the service and per-session client state so everyone can see the result.
- Double down is allowed only on an active two-card hand with a locked bet and enough chip balance for one extra wager equal to the original bet. The service marks the seat `SeatPhase::ActionPending` while the extra chip debit is pending, then doubles the recorded bet, draws exactly one card, and auto-stands or bust-settles the hand. Double-down settlement uses the doubled bet amount.
- Action timeout auto-stands remaining hands when the pace-specific deadline expires, then removes those non-acting seats after settlement.
- A seated player who misses 3 deals without a locked bet is removed from the table.
- A seated player who sends no active-room input for 5 minutes is removed from the table; active-room keys, arrows, and scrolls refresh this room timer while seated.
- Settlements use `ChipService`: zero-credit losses call `restore_floor`, payouts call `credit_payout`, and `BlackjackEvent::HandSettled` updates client balances. Initial-deal settlements, such as natural blackjack, can happen inside bet submission; the bet confirmation must use the settled balance when available so it cannot overwrite the payout with the earlier post-debit balance.
- Every Blackjack settlement publishes a hidden quest Activity played-hand event. Winning settlements (`PlayerWin` or `PlayerBlackjack`) also publish visible `ActivityGame::Blackjack` win events with the bet in `detail`.
- House rules: 6-deck shoe, reshuffle at 52-card penetration, dealer stands on soft 17, natural blackjack requires exactly two cards, and blackjack pays 3:2.
- `Phase::BetPending` exists in the shared enum and input/UI paths, but current pending debit state is expressed per seat as `SeatPhase::BetPending`; the service does not currently transition the whole table into `Phase::BetPending`.
- `BlackjackService::deal_task` exists as a manual deal API, but active room input does not currently route a key to it. Normal play deals by all seated players locking bets or by the 30s betting cap.

## Tic-Tac-Toe Runtime
- `TicTacToeTableManager` is process-local and lazily maps each entered `GameRoom.id` to a `TicTacToeService`.
- Restarting the SSH process drops in-memory board state. Existing open `game_rooms` survive, but re-entering creates a fresh board.
- There are two seats: X and O. Entering starts as a viewer; `s`, `Space`, or `Enter` sits when not seated.
- Seated players can press `1`-`9` to place directly, move a local cursor with `w/a/s/d` or any of the four arrow keys, and press `Space` or `Enter` to place on the cursor. While seated, `s` is "move down" rather than "sit"; sit is reachable via `Space`/`Enter` (or `s` from a viewer state). `j/k` remain embedded-chat navigation; `tictactoe::input::handle_arrow` claims all four arrows when seated, otherwise it returns `false` and chat gets up/down for message selection.
- `n` starts a new round for seated players. `l` leaves a seat and resets the board.
- The board UI scales cell size to the available area (`pick_cell_dims` picks from `(11,5)`, `(9,5)`, `(7,3)` and falls back to `(5,3)`); `pick_glyph` selects a 5×5 or 3×3 block-character X/O glyph that fits inside the chosen cell. The compact path renders when `inner.height < 11` or `inner.width < 28`.
- `preferred_game_height` returns `min(area.height * 9 / 20, 19)` — the game caps at 19 rows (enough for the 11×5 cell tier: 17 board rows + 2 border rows) so the embedded chat below it keeps the rest of the active-room area.
- Tic-Tac-Toe has no chip-balance hook; `ActiveRoomBackend::chip_balance` returns `None`.
- Mark wins publish `ActivityGame::TicTacToe` events with the winning mark (`X`/`O`) in `detail`; draws do not publish win activity.

## ssHattrick Runtime
- `SshattrickRoomManager` is process-local and lazily maps each entered `GameRoom.id` to a `SshattrickService`, pruning stopped services. Restarting the SSH process drops in-memory match state.
- ssHattrick is a two-seat real-time game backed by `sshattrick-core`. It uses public snapshots for seats, score, timer, phase, winner, palette, and goal/disconnect state, plus per-user private snapshots for seat side and rendered pitch image.
- Entering starts as a spectator. `Space` sits, both occupied seats create a match, `w/a/s/d` or arrows move, `Space` shoots, `n`/`Space` rematches after `Ending`, and `Esc`/`q` leaves. The backend returns `drop_on_leave = true`.
- Update/render loops run per service; empty services stop after 5 minutes. `Starting`/`Running`/`AfterGoal` set the room `in_round`; `Waiting`/`Ending` return it to `open`.
- Decisive wins credit `sshattrick_win_payout` through `reward_templates`: 300 chips with a 15-minute cooldown, and publish `ActivityGame::Sshattrick`. A mid-match disconnect awards the survivor and marks `by_disconnect`.

## Chess Runtime
- `ChessTableManager` is process-local and lazily maps each entered `GameRoom.id` to a `ChessService`.
- Restarting the SSH process drops in-memory boards/clocks. Existing open `game_rooms` survive, but re-entering creates a fresh board.
- There are two seats: White and Black. Entering starts as a viewer; `s`, `Space`, or `Enter` sits in the first open color. `n` starts a game when both seats are occupied and the board is waiting or finished.
- Chess uses `cozy-chess` for legal move generation and game status. The service stores only public state; no private snapshot channel is needed.
- Chess UI seat labels must distinguish `None` seats from occupied seats whose username is absent from the shared username directory: empty seats render as `open seat`, while occupied-but-unresolved seats render as `player`.
- Chess move records store Standard Algebraic Notation labels (`Nc3`, `exd5`, `O-O`) for the right-sidebar move list and status-line last move, not raw coordinate notation.
- Chess sit, leave, ready/start, resign, accepted move, and timeout actions persist a chess-owned JSON payload into `game_rooms.runtime_state`; that write also refreshes `game_rooms.updated`. The payload stores seats, ready flags, phase/result, FEN, clocks/deadlines, SAN move history, position history for repetition checks, and a monotonic revision so older async saves cannot overwrite newer ones. Chess is currently the only room game using `runtime_state`; other games still treat it as opaque. Active Chess games set `game_rooms.status = 'in_round'` and are protected from idle deletion until checkmate, timeout, resignation, or draw returns status to `open`.
- Time controls are preset-only and intentionally generous: blitz is `5+3`, rapid is `15+10`, and daily is `1d/move`. Room settings store only `blitz`, `rapid`, or `daily`; old seven-preset IDs fall back to rapid. Countdown clocks debit elapsed time idempotently as clock state is settled and add increment after a legal move. Daily clocks use a per-move deadline instead of a banked player clock.
- When a new game starts after a finished round, the service swaps the two seated players so colours alternate. A decisive Chess win (checkmate, timeout, or resignation) credits the winner 500 chips when the user is outside the 60-minute DB-backed Chess payout cooldown; drawn games do not award chips.
- Input is cursor-first. Seated players move the local cursor with `w/a/s/d` or arrows, click a board square, or press `Space`/`Enter` to select a piece and then a destination; promotion defaults to queen. `r` resigns an active game; `l` leaves only before/after a game.
- Checkmate, timeout, and resignation publish `ActivityGame::Chess` win events with detail `checkmate`, `timeout`, or `resignation`. Draws do not publish win activity.

## Tron Runtime
- `TronTableManager` is process-local and lazily maps each entered `GameRoom.id` to a `TronService`.
- Restarting the SSH process drops in-memory grids. Existing open `game_rooms` survive, but re-entering creates a fresh grid.
- Tron is a 2-4 seat real-time light-cycle game on a 56×28 grid. Entering starts as a viewer; `s`, `Space`, or `Enter` sits in the first open seat when no round is running.
- Table speed settings are `chill`, `standard`, or `quick`, mapped to 700ms, 450ms, or 275ms service-side ticks.
- Table mode settings are `classic`, `gaps`, or `glitch`. `classic` keeps permanent trails. `gaps` skips every seventh successful trail cell per rider. `glitch` uses the same deterministic gap cadence and also seeds passive pickups.
- Seated players press `n` to start when at least two riders are seated, steer with `w/a/s/d` or arrows, press `l` to leave a seat, and use `q`/`Esc` to leave the active room.
- Direction changes are buffered and applied on the next service tick. Direct reverse turns are ignored.
- Trails are permanent walls for the round except in `gaps`/`glitch` mode gap cells. Wall hits, trail hits, and same-cell head-on collisions crash riders. The last alive rider wins; if no riders survive, the round is a draw.
- Glitch pickups are passive and apply from later ticks instead of requiring frame-perfect activation: `Shield` absorbs one wall/trail hit and leaves the rider stationary for that tick, `Phase` passes through one trail cell without overwriting it, and `Gap` makes the rider's next three successful moves leave no trail. Charges are visible in the rider sidebar as `S`, `P`, and `G` counters.
- Tron uses one public `watch::Sender<TronSnapshot>`; no private state or chip-balance hook is needed.
- Tron win outcomes credit chips by round-start rider count when the user is outside the 5-minute DB-backed Tron payout cooldown: 50 chips for 2 riders, 75 for 3 riders, and 100 for 4 riders. They publish `ActivityGame::Tron` events with the winning color in `detail`; draws do not publish win activity.

## Poker Runtime
- `PokerTableManager` is process-local and lazily maps each entered `GameRoom.id` to a `PokerService`.
- Restarting the SSH process drops in-memory poker state. Existing open `game_rooms` survive, but re-entering creates a fresh table.
- Poker is a configurable-stack/configurable-blind Texas Hold'em-style table: four seats, one 52-card deck, private two-card hole hands, shared flop/turn/river, room-configured starting stacks and blinds, call/check, bet/raise, fold, all-in, side pots, showdown hand ranking, and chip settlement through `ChipService`.
- The service uses one public `watch::Sender<PokerPublicSnapshot>` plus per-user `watch::Sender<PokerPrivateSnapshot>` channels keyed by `user_id`. Public snapshots include seat occupancy, visible stacks, committed chips, pot/current bet, card counts, folded/all-in/pending state, dealer button, board, phase, active seat, starting stack, and winners. Private snapshots include the current user's hole cards, table stack, global chip balance, call amount, minimum raise, and auto check/fold flag.
- The deck and all hole cards live only in `SharedState`; clients never receive other users' hole cards. `publish` lazily prunes orphaned private senders where `receiver_count() == 0`.
- Entering starts as a viewer. `s` or `Enter` sits in the first open seat if the user has at least the room's starting stack; every new seat starts with exactly that table stack, even if the global chip balance is larger. Mid-hand joiners keep empty hole cards and wait for the next deal. Seated players press `n` to deal when the table is waiting or at showdown, `c`/Space/Enter to check or call when active, `b`/`r` to bet or raise by the selected amount, `[`/`]` or `-`/`+` to adjust that amount, `a` to shove all-in, `x` to toggle auto check/fold, `f` to fold, and `l` to leave a seat.
- The dealer button advances to the next funded occupied seat each new hand. Heads-up uses the button as the small blind; larger tables post blinds left of the button. Pre-flop action starts left of the big blind, later streets start left of the button, and when no further betting is possible because all remaining players are all-in, the service runs out the board and settles showdown.
- Short all-ins smaller than the current call are legal. Short all-in raises update the amount to call but do not reopen raising for players whose action was already closed; those players can only call or fold unless a full raise has reopened action.
- Side pots are built from each distinct committed-chip level. Each pot is awarded only among eligible non-folded contenders for that level; tied winners split each pot, with odd chips assigned deterministically by seat order.
- Showdown currently auto-reveals every non-folded contender's hole cards. Real poker can allow players to muck at showdown instead of showing if they do not want to contest the pot; this app does not model a `show`/`muck` reveal phase yet.
- A seated player who sends no active-room input for 5 minutes is removed from the table when idle outside an active hand. During an active hand, inactivity folds the player and reconciles the hand.
- Poker wires `ActiveRoomBackend::chip_balance` to global chip balance and renders per-seat table stacks separately. External chip balance sync never tops up a seated table stack. Committing chips debits global chips and subtracts from the table stack; winning pot shares credit global chips and add to the table stack. Zero-credit losers still restore the global chip floor only.
- Every committed Poker settlement publishes a hidden quest Activity played-hand event. Positive Poker settlement credits also publish visible `ActivityGame::Poker` win events with the credited pot share in `detail`. Split-pot hands can publish one win event per credited winner.
- `poker/ui.rs` mirrors the Blackjack table thresholds and broad layout: dealer/board block on top, felt divider, four seat panels, status line, and key bar. The current user's panel renders private hole cards face-up from the private snapshot; other players render card backs.

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
- Home room-rail rendering skips game rooms, so game-backed rooms do not appear as normal chat rooms or favorites.
- Room entry requests a chat tail; live broadcasts then keep the embedded chat updated like other room-explicit chat flows.

## Asymmetric-Info Game Pattern
- Blackjack, Chess, Tic-Tac-Toe, and Tron publish one public `watch::Sender<Snapshot>` and every session sees the same snapshot. Asterion, Poker, and ssHattrick use the split-channel pattern for games where each user sees a different view.
- Pattern: split the snapshot into a public part and a per-user private part. Service holds one `watch::Sender<PublicSnapshot>` plus a `HashMap<Uuid, watch::Sender<PrivateSnapshot>>` keyed by user_id. Per-session `State` caches both and drains both in `tick()`.
- `RoomGameManager::enter` already receives `user_id`, so the manager can register a private channel for the entering user and bind the receiver into the returned `Box<dyn ActiveRoomBackend>`. Rooms layer never sees the split.
- Cleanup of orphaned private channels (session disconnect drops the receiver but not the sender): prefer lazy GC inside the service's `publish` path — prune entries where `tx.receiver_count() == 0`.
- Keep hidden game state inside `SharedState` only, never put it on public snapshots. Hole cards or private maze views get sliced into the per-user private snapshot at publish time. Clients never receive secret state they aren't entitled to.

## Room Timeouts
- Asterion has no per-player AFK kick. The per-room service stops after 5 minutes with zero heroes; active-room input only refreshes the persistent 1h open-room idle window while a hero is present.
- Blackjack has three runtime timers. The first confirmed bet starts a fixed 30s betting/deal cap; the cap does not restart for later bets and deals immediately if all seated players lock. Player action starts a pace-specific action timer (`Quick` 2m, `Standard` 5m, `Chill` 10m) that auto-stands unresolved hands on expiry and removes those missed-action seats after settlement. Seated player inactivity is a separate 5m active-room idle timer; active-room input refreshes it, idle players leave immediately when safe or after settlement when a live bet blocks immediate removal.
- Poker has two timer types plus a missed-action policy. The per-turn action timer starts whenever `active_seat` is assigned in an action phase and restarts when action moves; the service publishes the deadline and clients render the visible countdown locally instead of receiving per-second service snapshots. On expiry it auto-checks when nothing is owed, otherwise auto-folds. A player who misses 3 turn timers is marked to leave at the nearest safe hand boundary, so one seated player cannot repeatedly consume the full turn clock. The existing 5m seat idle timer remains broader AFK cleanup: idle players leave outside active hands, and during active hands they fold and leave after the hand.
- Tic-Tac-Toe has no service-side turn clock or AFK seat timer. A seated player can keep a seat until they leave, the opponent leaves/resets, or the process restarts; future timeout work should be added explicitly instead of assuming Blackjack/Poker timers apply.
- Chess has two clock modes. Blitz/rapid use countdown clocks that update from monotonic elapsed time on moves; daily uses the active side's per-move deadline. There is no broad Chess seated idle cleanup while a game is active; active Chess is protected by `in_round`, and after the game ends the room returns to `open` for the normal 1h idle hard-delete window.
- ssHattrick has a service-side update/render loop and no separate seated AFK kick; active-room input touches the room at most once per minute, and the service stops after 5 minutes with no sessions.
- Tron has a service-side round tick loop and a 5m seated idle timer. Idle seated riders are removed outside a round and crash during a running round, which can immediately settle the round.

## Known Gaps
- Asterion maze state is not durable across process restart.
- Blackjack table state is not durable across process restart.
- Chess board/clock state is durable across process restart through `game_rooms.runtime_state`.
- Poker table state is not durable across process restart.
- ssHattrick match state is not durable across process restart.
- Tron grid state is not durable across process restart.
- Poker showdown reveal is simplified: all non-folded contenders auto-show, with no optional muck flow.
- There is no AFK/disconnect cleanup path tied to SSH session lifecycle.

## Test Guidance
- Pure rules in `filter.rs`, `settings.rs`, `blackjack/state.rs`, and key-routing helpers can use inline unit tests.
- Anything that touches `RoomsService`, `GameRoom`, `ChatRoom`, chip balances, or service tasks belongs in `late-ssh/tests/` and must use testcontainers through the existing helpers.
- Do not run `cargo test`, `cargo nextest`, or `cargo clippy` as an agent in this repo. Leave those gates for the human owner.
