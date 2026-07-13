# FRD: Lobby Consolidation (Phases 2, 3 & 4)

- Status: ALL FOUR PHASES SHIPPED. Phases 1+2 shipped 2026-07-13; phases 3 (demolition) and 4 (lobby consolidation) executed 2026-07-14 (uncommitted in tree at time of writing). `app/rooms/` is deleted wholesale, `Screen::Rooms` and the tab number are gone (Artboard `4`, Directory `5`, World Cup `6`), the `Option<RoomsService>` seams collapsed, migration `111_drop_game_rooms.sql` drops `game_rooms` + rooms-era chat/voice rows, and @dealer rides the house blackjack singleton's eager event channel. Phase 4 landed with the owner's execution-time choices: nested `App::lobby: LobbyState` (modal cursor/claim-confirm/glow, synced each tick from the daily snapshot) and the `LobbyEntry` name. The domain now lives at `late-ssh/src/app/lobby/` (`mod.rs`, `state.rs`, `modal_input.rs`, `modal_ui.rs`, `workspace.rs`, `daily/`, `house/`) — see `late-ssh/src/app/lobby/CONTEXT.md`. Notable phase-3 extras beyond the checklist: `ActivityKind::GamePlayed` deleted (no publishers left), `GameKind` shrunk to the four-table roster, the clubhouse poker landmark opens the Lobby modal and its signpost reads `LOBBY`, the help-modal Tables topic became Lobby. House poker/blackjack still have no off-screen your-turn notify (dropped with the rooms scan; re-add on the singletons if missed).
- Owner decisions are LOCKED unless the owner reopens them; do not re-litigate.
- Read first: root `CONTEXT.md`, `late-ssh/src/app/daily/CONTEXT.md`, `late-ssh/src/app/rooms/CONTEXT.md`. Memory file `project_lobby_consolidation.md` mirrors the short version of this doc.

## The product decision

The Rooms directory (user-created real-time tables) assumed simultaneous-online liquidity the community doesn't have; the async daily Lobby is what actually gets used. So: the Lobby (`Ctrl+Q`) becomes the single front door for all multiplayer play, and the Rooms domain is deleted. Kept games move into the Lobby as fixed "house tables"; everything else dies.

Locked decisions:
- Keep as house tables: **Poker** (1k stack, fixed blinds), **Blackjack** (one fixed stake), **Asterion** (the maze), **Tron** (quick speed + glitch mode — owner explicitly preserved it).
- Delete with no replacement: live table **Chess** (daily chess is the replacement), **Tic-Tac-Toe** (connect four owns the niche), **ssHattrick**.
- No user table creation, no settings forms, fixed stakes. One table per variant; a second stake tier later = a new enum variant, not config.
- Poker/blackjack must NOT be forced through the `daily_matches` correspondence model (N-player, house-banked, real-time; the 24h-deadline shape would corrupt the daily domain). Rejected explicitly.
- **Quest re-pointing: RESOLVED with the owner 2026-07-13, implemented same day (ahead of the demolition).** Decisions: (a) assigned quests are arcade-only for now — daily slot 1 easy, daily slot 2 medium, weekly slot hard, all from the arcade page (score/level runs plus the daily puzzles; the code's `QuestSource::Arcade` covers both). Implemented in `late-core/src/models/quest.rs` (`slot_difficulty_preference` / `slot_source_preference`, unit-tested) plus migration `110_disable_room_quests.sql` (deactivates every `room_rounds_played`/`room_wins` template — poker, blackjack, tron, chess — and deletes current/future assignments pointing at them so the draw refills from the arcade pool; progress rows cascade). (b) House-table activity is sit-downs ONLY: someone sits at poker/blackjack/tron or enters the maze (`ActivityKind::SatDown` via the registries' choke points). The `game_won`/`game_played` publishers were deleted from the four house runtimes, and the now-dead `ActivityPublisher` seams were stripped from the services, both registries' constructors, the rooms managers, and `main.rs`. Chip payouts were never event-driven (direct `ChipService` calls) and are unchanged. Accepted consequences: #lounge loses the tron/asterion win lines (sit-down invitation lines stay), and `metrics::record_game_win` no longer counts house games. Still true: deleting rooms chess kills nothing (its quest templates are already inactive).

## What phase 1 shipped (context for the code you'll touch)

- Every claimed daily match gets a private two-player chat room + enabled voice channel, created in the claim transaction (`DailyService::claim_challenge`, `ChatRoom::create_daily_match_room`, migration `109_add_daily_match_chat.sql`, `daily_matches.chat_room_id` FK `ON DELETE SET NULL`). Reaped by the daily sweeper 30 days after finish/cancel (`DailyMatch::delete_stale_chat_rooms` — deletes the voice channel in the same CTE because `voice_channels.target_id` is polymorphic and has no FK).
- `kind='game'` chat rooms now have two flavors: public (attached to `game_rooms`) and private (daily match chat, `visibility='private'`, membership fixed at claim). `ChatService::join_game_room` rejects non-members for private ones. Post-demolition, `kind='game'` simply means "chat attached to a game surface" — that reinterpretation is why we reused the kind instead of adding one.
- The daily board (`Screen::DailyMatch`) renders an embedded chat pane + voice strip below the game (`board_ui::split_board_and_chat`; `EmbeddedRoomChatView` built in `render.rs` as `daily_chat_view`, cache `App::daily_chat_rows_cache`). Key routing mirrors the old active-room split (`board_input.rs`).
- Backtick is lobby-only: Home chat ↔ daily matches waiting on your move (`GameWorkspace` enum + pure `next_workspace` in `app/dashboard/input.rs`, unit-tested). Removed from rooms/arcade; `DashboardGameToggleTarget` deleted (it was write-only).
- `RoomGameRegistry::is_user_seated` is already orphaned (its only caller was the old backtick); left in place for the phase 3 demolition.

## Phase 2: house tables in the Lobby

Goal shape (all new code in `late-ssh/src/app/daily/` or a sibling `late-ssh/src/app/house/` module — owner prefers enums over traits, exhaustive matches, no `_ =>` on roster enums):

1. **`HouseTable` enum** — closed roster: `Poker`, `Blackjack`, `Asterion`, `Tron`. Every per-table fact behind exhaustive matches: display name, tagline, chat-room slug (`poker` / `blackjack` / `maze` / `tron`), fixed settings (poker 1k starting stack + fixed blinds; blackjack one stake preset; tron quick speed + glitch mode; asterion none). Mirror the `DailyGame` add-a-game checklist style in its doc comment.
2. **Singleton runtimes.** The existing `PokerService` / `BlackjackService` / `AsterionService` / `TronService` survive nearly intact — they're in-memory and already restart-lossy. Replace the per-`GameRoom.id` manager maps with one process-global instance per variant (lazy-create on first enter, same empty-shutdown behavior). They currently take `RoomsService` for status persistence (`in_round`/`open`, `touch_room_task`) — house tables need none of that; strip or stub those seams rather than keeping `game_rooms` rows alive for them.
3. **Chat + voice**: one permanent public `chat_rooms(kind='game')` row per variant plus enabled voice channel, seeded idempotently at startup (like `ChatRoom::ensure_lounge`) or by migration. Embedded chat/voice on the table screen reuses the `EmbeddedRoomChatView` pattern from the daily board (`render.rs::daily_chat_view` is the template).
4. **Lobby modal section**: a fixed "house tables" block at the bottom of the modal list — one row per variant, always present (stable chrome), showing live occupancy (`2 seated · in round` / `empty`). Enter opens the table. Occupancy comes from the singleton services' watch snapshots.
5. **Screen**: decided — a `Screen::HouseTable` sibling of `Screen::DailyMatch`, outside the Tab cycle, entered only from the modal, `q`/`Esc` back to the modal. Table UIs (`poker/ui.rs`, `blackjack/ui.rs`, etc.) largely port over; input routing copies the daily board's chat-first split.
6. **Backtick**: add a `GameWorkspace::HouseTable(HouseTable)` variant for tables where you're seated; slot after the your-turn daily boards in `next_workspace`. Keep the pure function pure.
7. **Payouts/activity**: keep each game's existing chip payout paths and `ActivityEvent::game_won`/`SatDown` publishing; the `SatDown` choke point moves from the rooms registry to wherever house-table seat joins converge.

Phase 2 leaves the Rooms screen alive as a fallback; it just becomes redundant. That's deliberate — the owner wants to feel the house-table UX before demolition.

Phase 2 as shipped (2026-07-13): everything above, plus the runtime relocation. `HouseTable` roster + `HouseTableRegistry` singletons + `Screen::HouseTable` + modal section + `GameWorkspace::HouseTable` backtick stop are in `late-ssh/src/app/house/` (see its `CONTEXT.md`). The four game runtimes (svc/state/input/ui/settings) moved from `app/rooms/<game>/` to `app/house/<game>/`; `game_ui.rs`/`image_render.rs` moved too; `InputAction`/`GameDrawCtx`/`RoomGameEvent`/`RoomTitleDetails` moved to `house/types.rs` with `rooms/backend.rs` re-exporting. The rooms managers/create modals stayed in `rooms/` and now import from `house::`. Deferred to phase 3: your-turn desktop notify for house poker/blackjack (rooms scan untouched), quest re-pointing (owner discussion required first).

## Phase 3: demolition (only after phase 2 feels right)

Checklist assembled from phase-1 archaeology; grep beyond it, this codebase hides seams:

- **Quests: DONE 2026-07-13** (see locked decisions — arcade-only slots, migration 110, house events reduced to sit-downs). What remains for phase 3 here: delete the `room_rounds_played`/`room_wins` match arms and `QuestSource::Multiplayer` in `hub/dailies/svc.rs` + `quest.rs` once the rooms runtimes stop emitting anything (the dead-game runtimes still publish `game_won` until deleted).
- Rooms screen + directory + create/delete/search/filter (`app/rooms/{input,ui,state,filter}.rs`), screen number freed in `primitives.rs`, topbar hit-test, splash tips (`late-ssh/assets/splash_tips/*.json` — check for Rooms/Tables mentions), help modal Tables topic.
- `RoomGameManager` / `ActiveRoomBackend` traits, `RoomGameRegistry`, per-game `manager.rs` files, `App::active_room_game`, `rooms_active_room` and friends.
- Dead runtimes: `chess/` (rooms), `tictactoe/`, `sshattrick/` — plus `ChessTimeControl::Daily` legacy parsing, which finally becomes deletable with the whole rooms chess module.
- `RoomsService`: creation/deletion/enter/caps/hourly cleanup/startup reconciliation. Check what survives for house tables (probably nothing).
- Home integration: multiplayer box, `b1`-`b4` shortcuts, `dashboard_room_joins` seeding, `recent_dashboard_rooms`.
- Notify: `App::notify_game_turn` scans room games via the registry (`is_awaiting_user_action`) — re-point at house tables or drop (daily already has its own your-turn notify).
- Moderation: `/mod` room commands and room-voice paths that assume `game_rooms`; game-room kick/ban voice revocation wording in `voice/CONTEXT.md`.
- DB: forward migration dropping `game_rooms` + orphaned public game chat rooms + their voice channels (mirror the `delete_stale_chat_rooms` CTE for the voice cleanup). Never edit applied migrations.
- Docs: rewrite `rooms/CONTEXT.md` (or fold into the new house-table context), root `CONTEXT.md` screen list/keybindings/data-model/service rows, `chat/CONTEXT.md` game-room references, `games/CONTEXT.md` chess_core ownership note.
- Tests: `late-ssh/tests/` rooms/blackjack/poker/etc. suites need porting to the singleton services, not deleting wholesale — the game-rule coverage is the valuable part.

## Phase 4: lobby domain consolidation (PLAN ONLY — execute after phase 3)

The Lobby now fronts two game domains that grew up separately: `app/daily/` (async correspondence matches) and `app/house/` (live house tables). They already share the modal, the backtick cycle, the embedded-chat screen shape, and the "leave surface → return screen → reopen modal" flow — but the shared parts live in `daily/` by historical accident (the modal predates house tables), and `dashboard/input.rs` owns the workspace cycle that spans both. Phase 4 puts everything under one `app/lobby/` domain. This is a REORGANIZATION, not a redesign: zero behavior change, no DB change, `cargo check --tests` green with no logic diffs.

Sequencing: strictly after phase 3. Moving the modules first would drag the rooms seams (`Option<RoomsService>`, `rooms/backend.rs` re-export) into the new tree and double the churn. Phase 3's test porting also lands file paths phase 4 would otherwise move twice.

Proposed layout (mirror the phase-2 runtime relocation mechanics — `git mv`, then fix paths):

```
app/lobby/
  mod.rs           declarations only
  modal_input.rs   moved from daily/ (it already renders/routes both domains)
  modal_ui.rs      moved from daily/
  workspace.rs     GameWorkspace + next_workspace (+ their unit tests) moved out of dashboard/input.rs
  state.rs         LobbyState, split out of DailyState: modal cursor/scroll, claim-confirm,
                   mark_lobby_seen / seen_open_ids, the rule-label glow inputs
  daily/           app/daily/ moved wholesale, minus the modal files
  house/           app/house/ moved wholesale
```

Steps, in order:
1. Mechanical moves: `app/daily/*` → `app/lobby/daily/*`, `app/house/*` → `app/lobby/house/*`; sweep `crate::app::daily::` / `crate::app::house::` paths (includes the chat-surface rosters in `app/input.rs` — `embedded_chat_room_id` and the Esc-peel branches reference `app.daily` / `app.house` accessors; update paths only, do not restructure the gates).
2. Hoist the modal to `lobby/`: move `modal_input.rs` / `modal_ui.rs`; rename `DailyModalEntry` → `LobbyEntry` (its `House` variant being a "daily" type is the smell this phase exists to fix).
3. Split `LobbyState` out of `DailyState` (the risky step — the glow/seen logic is edge-triggered, port its unit tests with it). The `ChallengeDraft` stays in `daily/`: it posts daily challenges. `App::show_daily_modal` → `App::show_lobby_modal`.
4. Move the backtick cycle: `GameWorkspace` + `next_workspace` → `lobby/workspace.rs`; `dashboard/input.rs` keeps only the key binding + call.
5. Docs: new `lobby/CONTEXT.md` (entry points: modal, panel, backtick, both screens) with `lobby/daily/CONTEXT.md` and `lobby/house/CONTEXT.md` staying per-sub-domain; update root `CONTEXT.md` module map and this FRD's status line.

Locked-shape guardrails (owner style, do not drift):
- `DailyService` and `HouseTableRegistry` stay SEPARATE services. No unifying trait over them, no `GameSurface` abstraction — the modal already consumes both through plain exhaustive code; keep enums + exhaustive matches, no `_ =>` on roster enums.
- Keep `Screen::DailyMatch` and `Screen::HouseTable` as-is (no user-visible change, no churn in `primitives.rs`/input gates). Renaming screens is out of scope.
- `daily_matches`, reward keys, migrations: untouched. The sidebar panel stays in `lobby/daily/` (its content is matches; the `lobby` rule label is owned by `common/sidebar.rs` regardless).
- Don't merge the board/table input files just because they rhyme; they already share `chat::input::chat_priority_key` / `selected_chat_key` and the central composer/overlay gates. Extract further shared helpers only if a diff-shrinking, behavior-identical extraction falls out naturally.

Open items to confirm with the owner at execution time: (a) does `App` grow a nested `App::lobby` owning modal/glow state (proposed) or do `App::daily` / `App::house` simply move under a namespace; (b) the `LobbyEntry` name.

## House rules that bit during phase 1 (save yourself the rediscovery)

- Owner workflow: no unprompted commits, no `cargo test`/`clippy`/`nextest` — `cargo check --tests` is the agent-side gate. Lowercase `bail!` strings, sentence-case banners. UUID v7. No em dashes in UI copy. Stable chrome (fixed heights between states). Forward migrations only.
- Chat-surface gating in `app/input.rs` was consolidated 2026-07-13 (after `Screen::DailyMatch` and `Screen::HouseTable` both shipped with the game handler eating composer keys): a new screen with embedded chat now joins exactly TWO rosters and everything else follows. Add it to `screen_has_chat_pane` (drives composer/scroll click hit tests, reaction-leader Esc; `screen_composes_chat` layers Clubhouse on top and drives the composer-priority gate + chat overlays) and to `embedded_chat_room_id` (screen → visible chat room; drives click targets and wheel/page scroll). Composer-beats-game and overlay-beats-game are enforced centrally — one gate at the top of `handle_dedicated_screen_input`, one before it — so screen handlers must NOT re-check `chat_composing`/`has_overlay` themselves. The chat-vs-game key split on split screens is shared too: `chat::input::chat_priority_key` / `selected_chat_key`.
- `ChatRoomMember::join` takes `&Client`; transaction-scoped model methods need `&impl GenericClient` (pattern: `DailyMatch::claim`). Widening is backward-compatible.
- `voice_channels.target_id` has no FK — every deletion path must clean voice channels explicitly.
- `compose_room_switch_allowed` deliberately allows only Dashboard; embedded-chat screens must not allow Ctrl+N/P room switching.
