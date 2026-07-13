# FRD: Lobby Consolidation (Phases 2 & 3)

- Status: Phase 1 shipped 2026-07-13 (uncommitted in tree at time of writing); phases 2 and 3 approved but not started.
- Owner decisions are LOCKED unless the owner reopens them; do not re-litigate.
- Read first: root `CONTEXT.md`, `late-ssh/src/app/daily/CONTEXT.md`, `late-ssh/src/app/rooms/CONTEXT.md`. Memory file `project_lobby_consolidation.md` mirrors the short version of this doc.

## The product decision

The Rooms directory (user-created real-time tables) assumed simultaneous-online liquidity the community doesn't have; the async daily Lobby is what actually gets used. So: the Lobby (`Ctrl+Q`) becomes the single front door for all multiplayer play, and the Rooms domain is deleted. Kept games move into the Lobby as fixed "house tables"; everything else dies.

Locked decisions:
- Keep as house tables: **Poker** (1k stack, fixed blinds), **Blackjack** (one fixed stake), **Asterion** (the maze), **Tron** (quick speed + glitch mode — owner explicitly preserved it).
- Delete with no replacement: live table **Chess** (daily chess is the replacement), **Tic-Tac-Toe** (connect four owns the niche), **ssHattrick**.
- No user table creation, no settings forms, fixed stakes. One table per variant; a second stake tier later = a new enum variant, not config.
- Poker/blackjack must NOT be forced through the `daily_matches` correspondence model (N-player, house-banked, real-time; the 24h-deadline shape would corrupt the daily domain). Rejected explicitly.
- **Quest re-pointing (phase 3): STOP AND DISCUSS WITH THE OWNER FIRST.** He asked for this verbatim ("when you get to quests sides, talk with me"). `QuestService` assigns a "multiplayer room-game daily quest" fed by rooms activity events (`SatDown` via `RoomGameRegistry::start_dashboard_room_join_feed_task`, `game_won` publishers in each runtime); deleting rooms without re-pointing leaves players uncompletable daily quests.

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

## Phase 3: demolition (only after phase 2 feels right)

Checklist assembled from phase-1 archaeology; grep beyond it, this codebase hides seams:

- **Quests first, with owner sign-off** (see locked decisions). Reward templates with room-game params, `QuestService`'s multiplayer daily draw, activity consumers.
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

## House rules that bit during phase 1 (save yourself the rediscovery)

- Owner workflow: no unprompted commits, no `cargo test`/`clippy`/`nextest` — `cargo check --tests` is the agent-side gate. Lowercase `bail!` strings, sentence-case banners. UUID v7. No em dashes in UI copy. Stable chrome (fixed heights between states). Forward migrations only.
- `is_chat_composer_context` in `app/input.rs` is the master gate for "typed bytes go to the chat composer" — any new screen with embedded chat must join it AND the sibling gates (overlay handling + close, reaction-leader Esc, scroll routing, `chat_click_room_id`, composer/scroll click hit tests). Phase 1's `Screen::DailyMatch` edits are the worked example.
- `ChatRoomMember::join` takes `&Client`; transaction-scoped model methods need `&impl GenericClient` (pattern: `DailyMatch::claim`). Widening is backward-compatible.
- `voice_channels.target_id` has no FK — every deletion path must clean voice channels explicitly.
- `compose_room_switch_allowed` deliberately allows only Dashboard; embedded-chat screens must not allow Ctrl+N/P room switching.
