# House Tables Context

## Metadata
- Scope: `late-ssh/src/app/lobby/house` — the fixed multiplayer "house tables" behind the Lobby modal (`Ctrl+Q`), plus the five game runtimes (Poker, Blackjack, Asterion, Tron, Super Snake).
- Last updated: 2026-07-17 (Super Snake joined the roster: a 4-seat real-time DOS-snake port with 20 embedded text-asset arenas in `late-ssh/assets/ssnake_levels/`, warp-tunnel edges, and a 300-chip cooldown win payout via `ssnake_win`, migration 118)
- Parent context: `../CONTEXT.md` (the Lobby domain), then the root `CONTEXT.md`; read `../daily/CONTEXT.md` for the daily correspondence domain.
- Status: Active.

## 1. Summary

House tables replaced the demolished Rooms directory: one fixed table per game, no creation flow, no settings forms, no DB rows (the `game_rooms` table is dropped, migration 111). The Lobby modal (`Ctrl+Q`) lists them in a fixed bottom section with live occupancy; Enter opens `Screen::HouseTable` (outside the Tab cycle, `q`/Esc back to the modal). A second stake tier later is a new `HouseTable` enum variant, not config.

Locked shape (owner decisions):
- Roster: Poker (1k stack, 10/20 blinds), Blackjack (10-chip stake), Asterion (the maze), Tron (quick speed + glitch mode), Super Snake (classic speed, 4 seats, random arena — seated players cycle the arena pick between matches).
- Everything is a closed enum + exhaustive match; no trait objects, no `_ =>` arms on the roster.
- House tables never touch the `daily_matches` correspondence model and have no DB rows of their own.

## 2. Module Map

| File | Responsibility |
|---|---|
| `mod.rs` | Declarations only. |
| `tables.rs` | `HouseTable` roster enum: fixed `table_id` (stable per-variant UUID; there is no DB row), display name, tagline, chat slug (`poker`/`blackjack`/`maze`/`tron`), `game_kind` (chat-room kind string), seat capacity, fixed settings constructors. |
| `types.rs` | Runtime-shared types (`InputAction`, `RoomTitleDetails`, `GameDrawCtx`, `RoomGameEvent`), inherited from the rooms-era `rooms/backend.rs`. |
| `registry.rs` | `HouseTableRegistry`, process-global: one lazy singleton service per variant, `ensure_chat_rooms` startup seeding (chat room + enabled voice channel per table, idempotent like `ensure_lounge`), `start_seat_activity_task` (the house `SatDown` choke point), an eagerly-created blackjack event channel (`blackjack_event_tx`, forwarded onto the shared seat-activity stream), `occupancy`, `is_user_seated`, `enter` → `HouseTableClient`. |
| `state.rs` | Per-session `HouseState` (open table, return screen, kept client, chat-join flag) and `HouseTableClient` — the closed enum over the five per-game client states with exhaustive delegation (tick/keys/arrows/draw/height/chip sync). Asterion drops on leave (frees its hero slot); the others keep the client so re-entering restores chip selection and cursors. |
| `input.rs` | `Screen::HouseTable` routing: chat-first split copied from the daily board (`i`/`j`/`k`/Ctrl+D/Ctrl+U always chat; message-action keys while a table-chat message is selected; arrows game-first), backtick continues the lobby cycle, `q`/Esc → `close_table` (back to the modal). |
| `ui.rs` | Screen renderer: game area (`preferred_game_height`) + rule + embedded chat, same vertical split as the old active room. |
| `game_ui.rs`, `image_render.rs` | Shared frame/sidebar/info helpers and half-block image rendering. |
| `poker/`, `blackjack/`, `asterion/`, `tron/`, `ssnake/` | The five game runtimes (svc/state/input/ui/settings, blackjack also `player.rs`, ssnake also `levels.rs` — 20 embedded text arenas). Authoritative in-memory services, restart-lossy by design. |

## 3. Runtime model

- **Singletons.** `HouseTableRegistry` holds `Arc<Mutex<Option<Service>>>` per variant; first enter creates the service with the variant's fixed settings and `HouseTable::table_id()` as its table id. Poker/blackjack/tron/ssnake singletons live for the whole process; Asterion keeps its stop-when-empty lifecycle (stopped singleton discarded and respawned on next enter, mirroring the old manager).
- **No persistence seam.** The rooms-era `Option<RoomsService>` fields and `sync_room_status`/`touch_room` calls were stripped in phase 3; the services publish watch snapshots and nothing else.
- **Chat + voice.** One permanent public `chat_rooms(kind='game')` row per variant (slug from the roster) plus an enabled `voice_channels(target_kind='chat_room')`, both ensured idempotently at startup (`ensure_chat_rooms`, called in `main.rs` before serving). Entering a table fires the idempotent `join_game_room_chat` from `App::tick` (membership is what makes the room and its voice channel appear in the user's chat snapshot). `kind='game'` keeps house chat off the Home rail, out of Mentions, and off IRC.
- **Activity.** Sit-downs ONLY (owner decision 2026-07-13, see the FRD): poker/asterion/tron/ssnake publish `RoomGameEvent::SeatJoined` onto the registry's shared channel; blackjack's `BlackjackEvent::SeatJoined` is translated by a forwarder; `start_seat_activity_task` turns those into `ActivityEvent::sat_down`. The runtimes publish NO other activity — their `game_won`/`game_played` emitters were deleted (quests are arcade-only, migration 110) and the services no longer hold an `ActivityPublisher`. Chip payouts are unchanged and were never event-driven: direct `ChipService` calls (asterion daily escape, tron/ssnake cooldown payouts, poker/blackjack settlements).
- **Chip balance sync.** `App::tick` reads `HouseTableClient::chip_balance()` into `App::chip_balance` and mirrors external balance changes in via `sync_external_chip_balance`, gated on `can_sync_external_chip_balance` — same contract the rooms backend had.
- **Kick / idle timeouts.** Two independent layers, owned per-game (not by the shared scaffolding):
  1. *Per-turn action timeout* (poker + blackjack only): `action_timeout_secs()` = 20s (`*/settings.rs`). Miss your turn and you're auto-stood/auto-folded (`schedule_action_timeout` → `auto_stand_remaining`). Stalls an active hand, not idle-sitting.
  2. *Seat idle timeout* (poker, blackjack, tron, ssnake): `SEAT_IDLE_TIMEOUT_SECS = 5 * 60`. A generation-guarded self-cancelling timer — every meaningful action calls `touch_activity` → `record_activity`, which stamps `last_activity` and bumps a per-seat `activity_generation`, then arms a kick task that sleeps 5m and re-checks. If the generation changed or `last_activity.elapsed()` is under 5m the kick is a no-op, so the latest action always wins and stale timers evaporate. A pure sit-and-never-play (e.g. blackjack: no `pending_bet`/`bet`, never in `PlayerTurn`) falls through `kick_inactive_user` to the plain `SeatState::empty()` path and the seat reopens; tron also handles the idle-mid-race case (marks the rider crashed), and ssnake's kick routes through `leave`, which forfeits a mid-match snake.
  - **Asterion has no idle kick** by design (`asterion/state.rs`): heroes die to the maze and an empty maze stops itself.
  - These timers are `tokio::spawn` + `sleep`, so like everything here they are in-memory and restart-lossy: a restart drops pending kick timers, and the next `touch_activity` re-arms them.

## 4. The blackjack event feed

The blackjack event channel is created eagerly in `HouseTableRegistry::new` (not lazily with the service), so its sender outlives every service instance. `forward_blackjack_seat_joins` subscribes lazily, inside `blackjack_service`'s `get_or_insert_with` on the first sit-down, and relays seat joins onto the shared seat-activity stream; the subscribe runs just before the `BlackjackService` that emits those events is constructed with the same sender, so no seat join can be missed.

## 5. UI surfaces

- **Lobby modal section** (`../modal_ui.rs`): a fixed `house tables` block at the bottom of the list — always five rows (stable chrome), name + tagline + occupancy (`empty` / `2 seated` / `2 seated · in round`) read from the singleton watch snapshots via `HouseTableRegistry::occupancy`. Enter opens the table; the entry rows are `LobbyEntry::House` appended after live games.
- **Screen::HouseTable**: outside the Tab cycle, entered only from the modal (or backtick). Chat-surface behavior comes from the two rosters in `app/input.rs` (`screen_has_chat_pane` + `embedded_chat_room_id`); the central gates there keep composing and chat overlays away from `house::input::handle_event`, so this module never re-checks them. Esc peels: selected table-chat message → close to modal. `compose_room_switch_allowed` stays Dashboard-only.
- **Backtick**: `GameWorkspace::HouseTable(HouseTable)` — tables where you hold a seat slot after the your-turn daily boards, roster order (`../workspace.rs::next_workspace`, unit-tested pure).

## 6. Known gaps / deferred

- Your-turn desktop notify (poker/blackjack) is edge-detected in `HouseState::notify_turn_edges` (every tick, off-screen included) against `HouseTableRegistry::awaiting_action(table, user_id)`, which reads the live singleton snapshots for ALL seated tables (not only the open client). Seeded silent on the first tick; asterion/tron/ssnake have no turn concept.
- No occupancy-driven repaint: modal occupancy is read per frame, which is fine at the render cadence.
- Table state is not durable across restart (accepted).
