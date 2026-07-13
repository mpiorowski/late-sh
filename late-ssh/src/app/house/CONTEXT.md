# House Tables Context

## Metadata
- Scope: `late-ssh/src/app/house` — the fixed multiplayer "house tables" behind the Lobby modal (`Ctrl+Q`), plus the four surviving game runtimes (Poker, Blackjack, Asterion, Tron).
- Last updated: 2026-07-13 (phase 2 of the Lobby consolidation: house tables shipped; see `devdocs/FRD-LOBBY-CONSOLIDATION.md`)
- Parent context: `../../../../CONTEXT.md`; read `late-ssh/src/app/daily/CONTEXT.md` for the Lobby modal that fronts these tables.
- Status: Active. `app/rooms/` is scheduled for wholesale deletion (phase 3); nothing in this folder may grow a dependency on it beyond the two seams noted in §4.

## 1. Summary

House tables replace user-created game rooms for the four games worth keeping: one fixed table per game, no creation flow, no settings forms, no `game_rooms` rows. The Lobby modal (`Ctrl+Q`) lists them in a fixed bottom section with live occupancy; Enter opens `Screen::HouseTable` (outside the Tab cycle, `q`/Esc back to the modal). A second stake tier later is a new `HouseTable` enum variant, not config.

Locked shape (owner decisions):
- Roster: Poker (1k stack, 10/20 blinds), Blackjack (10-chip stake), Asterion (the maze), Tron (quick speed + glitch mode).
- Everything is a closed enum + exhaustive match; no trait objects, no `_ =>` arms on the roster.
- House tables never touch the `daily_matches` correspondence model and never touch `game_rooms`.

## 2. Module Map

| File | Responsibility |
|---|---|
| `mod.rs` | Declarations only. |
| `tables.rs` | `HouseTable` roster enum: fixed `table_id` (stable per-variant UUID; there is no DB row), display name, tagline, chat slug (`poker`/`blackjack`/`maze`/`tron`), `game_kind` (chat-room kind string), seat capacity, fixed settings constructors. |
| `types.rs` | Runtime-shared types that moved out of `rooms/backend.rs` with the runtimes: `InputAction`, `RoomTitleDetails`, `GameDrawCtx`, `RoomGameEvent`. `rooms/backend.rs` re-exports them until the demolition. |
| `registry.rs` | `HouseTableRegistry`, process-global: one lazy singleton service per variant (`rooms_service: None`), `ensure_chat_rooms` startup seeding (chat room + enabled voice channel per table, idempotent like `ensure_lounge`), `start_seat_activity_task` (the house `SatDown` choke point), `occupancy`, `is_user_seated`, `enter` → `HouseTableClient`. |
| `state.rs` | Per-session `HouseState` (open table, return screen, kept client, chat-join flag) and `HouseTableClient` — the closed enum over the four per-game client states with exhaustive delegation (tick/keys/arrows/draw/height/chip sync). Asterion drops on leave (frees its hero slot); the others keep the client so re-entering restores chip selection and cursors. |
| `input.rs` | `Screen::HouseTable` routing: chat-first split copied from the daily board (`i`/`j`/`k`/Ctrl+D/Ctrl+U always chat; message-action keys while a table-chat message is selected; arrows game-first), backtick continues the lobby cycle, `q`/Esc → `close_table` (back to the modal). |
| `ui.rs` | Screen renderer: game area (`preferred_game_height`) + rule + embedded chat, same vertical split as the old active room. |
| `game_ui.rs`, `image_render.rs` | Shared frame/sidebar/info helpers and half-block image rendering, moved from `rooms/`; the doomed rooms games (chess, sshattrick, tictactoe) still import them from here. |
| `poker/`, `blackjack/`, `asterion/`, `tron/` | The four game runtimes (svc/state/input/ui/settings, blackjack also `player.rs`), moved verbatim from `app/rooms/<game>/`. Authoritative in-memory services, restart-lossy by design. Their `manager.rs`/`create_modal.rs` stayed behind in `rooms/` and die with the directory. |

## 3. Runtime model

- **Singletons.** `HouseTableRegistry` holds `Arc<Mutex<Option<Service>>>` per variant; first enter creates the service with the variant's fixed settings and `HouseTable::table_id()` as its table id. Poker/blackjack/tron singletons live for the whole process; Asterion keeps its stop-when-empty lifecycle (stopped singleton discarded and respawned on next enter, mirroring the old manager).
- **No persistence seam.** The services' `rooms_service: Option<RoomsService>` is `None` for house tables, so `sync_room_status`/`touch_room` are no-ops. The rooms directory path passes `Some(...)` until phase 3 deletes it, after which the Option can collapse.
- **Chat + voice.** One permanent public `chat_rooms(kind='game')` row per variant (slug from the roster) plus an enabled `voice_channels(target_kind='chat_room')`, both ensured idempotently at startup (`ensure_chat_rooms`, called in `main.rs` before serving). Entering a table fires the idempotent `join_game_room_chat` from `App::tick` (membership is what makes the room and its voice channel appear in the user's chat snapshot). `kind='game'` keeps house chat off the Home rail, out of Mentions, and off IRC.
- **Activity.** Sit-downs ONLY (owner decision 2026-07-13, see the FRD): poker/asterion/tron publish `RoomGameEvent::SeatJoined` onto the registry's shared channel; blackjack's `BlackjackEvent::SeatJoined` is translated by a forwarder; `start_seat_activity_task` turns those into `ActivityEvent::sat_down`. The runtimes publish NO other activity — their `game_won`/`game_played` emitters were deleted (quests are arcade-only, migration 110) and the services no longer hold an `ActivityPublisher`. Chip payouts are unchanged and were never event-driven: direct `ChipService` calls (asterion daily escape, tron cooldown payouts, poker/blackjack settlements).
- **Chip balance sync.** `App::tick` reads `HouseTableClient::chip_balance()` into `App::chip_balance` and mirrors external balance changes in via `sync_external_chip_balance`, gated on `can_sync_external_chip_balance` — same contract the rooms backend had.

## 4. Dependencies on `app/rooms/` (die in phase 3)

1. `rooms::svc::RoomsService` inside each runtime's `Option<RoomsService>` seam (always `None` for house singletons).
2. That's it. Everything else was moved or re-exported the other way (`rooms/backend.rs` re-exports `house::types::*`; the rooms managers and dead-game UIs import runtimes/`game_ui` from `house::`).

## 5. UI surfaces

- **Lobby modal section** (`daily/modal_ui.rs`): a fixed `house tables` block at the bottom of the list — always four rows (stable chrome), name + tagline + occupancy (`empty` / `2 seated` / `2 seated · in round`) read from the singleton watch snapshots via `HouseTableRegistry::occupancy`. Enter opens the table; the entry rows are `DailyModalEntry::House` appended after live games.
- **Screen::HouseTable**: outside the Tab cycle, entered only from the modal (or backtick). Chat-surface behavior comes from the two rosters in `app/input.rs` (`screen_has_chat_pane` + `embedded_chat_room_id`); the central gates there keep composing and chat overlays away from `house::input::handle_event`, so this module never re-checks them. Esc peels: selected table-chat message → close to modal. `compose_room_switch_allowed` stays Dashboard-only.
- **Backtick**: `GameWorkspace::HouseTable(HouseTable)` — tables where you hold a seat slot after the your-turn daily boards, roster order (`dashboard/input.rs::next_workspace`, unit-tested pure).

## 6. Known gaps / deferred

- No your-turn desktop notification for house poker/blackjack while off-screen (the rooms scan `App::notify_game_turn` still covers rooms only; re-point or drop in phase 3).
- No occupancy-driven repaint: modal occupancy is read per frame, which is fine at the render cadence.
- Table state is not durable across restart (unchanged from rooms; accepted).
- The rooms directory still lists user-created tables for the same games; deliberate redundancy until phase 3 demolition.
