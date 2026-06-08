# Hub Context

## Metadata
- Scope: `late-ssh/src/app/hub`
- Last updated: 2026-06-08
- Purpose: local working context for the Hub domain: global modal, leaderboard, quests, admin reward-template/shop-item editing, shop, Shop-unlocked aquarium, and future event surfaces.
- Parent context: `../../../../CONTEXT.md`

## Scope

`late-ssh/src/app/hub` owns the global Hub modal opened with reserved global `Ctrl+G` (except active Artboard editing) and the cross-product domains surfaced inside it: Shop, Leaderboard, Quests, Events, and the admin-only reward-template/shop-item editor. Former Guide content now lives in the global `?` guide's Economy topic under `late-ssh/src/app/help_modal/hub_guide.rs`. Hub also owns the Shop-unlocked Aquarium tray toggled globally with `Ctrl+Q` or `Alt+A`.

Hub is a cross-product domain surface. It may render Arcade, Rooms, economy, marketplace, and event information, but it must not own those runtimes. Arcade game state stays under `late-ssh/src/app/arcade`; Rooms/table runtime stays under `late-ssh/src/app/rooms`; generic chip earn/spend primitives stay in `late-core/src/models/chips.rs`. Hub-owned marketplace state and entitlement projections live under `hub/shop`.

Keep `mod.rs` declaration-only. Do not add `pub use` re-export layers.

## Source Map

- `state.rs`: selected Hub tab and tab cycling.
- `input.rs`: Hub-only key routing (`Tab`/arrows cycle, `1-4` jump for normal users, `1-5` for admins, `Esc/q` close).
- `ui.rs`: modal frame, tabs, footer, and tab dispatch.
- `leaderboard.rs`: compact leaderboard panels.
- `admin/`:
  - `state.rs`: admin reward-template and shop-item catalogs, editable draft state, cursor-aware inline edit buffer, async load/save result drain.
  - `input.rs`: Admin-tab row/category/field navigation, inline text edits with Left/Right/Home/End cursor movement, numeric/toggle edits, save/reload actions.
  - `ui.rs`: admin-only two-pane reward-template/shop-item editor.
- `dailies.rs`: module root for the Quests surface.
- `dailies/`:
  - `svc.rs`: `QuestService`, current assignment generation, Activity-driven progress matching, per-user watch snapshots including daily streak state, completion banners, and Postgres LISTEN/NOTIFY refresh listener.
  - `state.rs`: snapshot/event drains for the Quests tab.
  - `ui.rs`: two daily quests, daily streak status, plus one weekly quest progress rendering.
- `events.rs`: placeholder product surface.
- `aquarium/`: animated ambient aquarium tray adapted from Reefs.
  - `state.rs`: embedded aquarium runtime state, per-frame movement, resize binding, and initial entity spawn.
  - `ui.rs`: bottom tray and aquarium renderer.
  - `config.rs`, `creature.rs`, `world.rs`, `kdl_parse.rs`: embedded KDL config/art parsing and creature/world model.
- `shop/`: Hub-owned marketplace domain.
  - `catalog.rs`: Shop categories and SKU helpers.
  - `entitlements.rs`: lightweight owned-feature projection for render/input gates.
  - `svc.rs`: `ShopService`, per-user watch snapshots, purchase tasks, and Postgres LISTEN/NOTIFY refresh listener.
  - `state.rs`: selected category/item, snapshot/event drains, and purchase activation.
  - `input.rs`: Shop-only item/category/buy input. `h`/`l` switch Shop categories/subtabs; `[`/`]` remain aliases.
  - `ui.rs`: Shop tab rendering.
- `svc.rs`: `LeaderboardService`, a shared watch-backed leaderboard refresh task.

## Tabs

- `Leaderboard`: functional compact leaderboard view.
- `Quests`: functional daily/weekly quest surface.
- `Shop`: functional marketplace surface. Pet Companion is the durable companion unlock.
- `Events`: placeholder for seasonal/monthly event surfaces.
- `Admin`: admin-only editor for quest titles/descriptions/requirements/rewards/weights/active state, fixed reward payouts, and Shop item names/descriptions/prices/sort order/active state.
- Former `Guide`: moved to the global guide's Economy topic.

If another tab is added, update `HubTab::ALL`, `HubTab::PUBLIC` if visibility differs, `HubTab::label`, `input.rs`, `ui.rs` dispatch, footer jump copy, and this file.

## Aquarium

Aquarium is a Shop unlock, not an admin/mod preview. The Aquarium feature costs 10,000 chips, lives in the Companions Shop category, and unlocks Aquarium ownership/use. The Aquarium Shop category is fish-only and browseable before unlock so users can preview fish, but fish purchases and active-count changes are blocked until the Aquarium feature is owned. `Ctrl+Q` or `Alt+A` toggles the owned user's full-width bottom tray across screens; locked users are sent to Hub Shop with a banner. Visible frame/shop hints intentionally keep the shorter `Ctrl+Q` text; the `Alt+A` fallback is user-documented in the global `?` guide.

The runtime is ambient-only for now:
- Fish ownership and active counts persist through `marketplace_items` / `user_purchases`.
- Fish SKUs cost 1,000 chips each and are repeatable purchases; buying the same fish N times gives owned quantity N and does not change active population.
- Active aquarium population is capped at 20 fish total for now; owned fish quantity is not capped by that active limit.
- `+` / `-` in the Aquarium Shop category adjusts the selected fish's active count, bounded by owned quantity and the 20-fish active cap.
- No non-Shop service calls, economy, or activity events.
- It ticks only while the tray is open and rebinds on terminal resize.

Assets live under `late-ssh/assets/aquarium`. The source was adapted from `github.com/mevanlc/reefs`; keep attribution/licensing notes with any future asset or behavior changes.

## Leaderboard Data

`hub::svc::LeaderboardService` refreshes `LeaderboardData` from DB every 30 seconds and publishes it through a `watch::Receiver<Arc<LeaderboardData>>`.

Current compact boards:
- `Top Chips`: monthly net chip delta from `chip_ledger`, excluding `floor_restore` and `shop_purchase`. Betting losses offset betting wins; Shop spending does not reduce this rank.
- `Arcade Wins`: monthly weighted daily-puzzle completions across Sudoku, Nonogram, Solitaire, and Minesweeper.
- `Lateris`, `2048`, `Snake`: each score-game panel shows monthly score events and all-time high scores.

Monthly windows use UTC calendar months. Score all-time boards persist.

Monthly profile awards:
- Migration `077_create_profile_awards.sql` adds `profile_awards`, one permanent row per user/category/month placement.
- `LeaderboardService::start_profile_award_snapshot_loop` runs once at startup and then daily as a catch-up mechanism. It creates missing previous-UTC-month `profile_awards` rows and leaves existing rows frozen.
- Awarded categories are `top_chips`, `arcade_wins`, `tetris`, `twenty_forty_eight`, and `snake`; ranks 1 through 5 are persisted. The `tetris` category renders publicly as `Lateris`.
- Profile modal overview shows a compact earned-awards preview before Showcases: up to six badges with period month, then `+N more`; there is no separate Badges tab.
- Chat author labels show at most one automatic current award badge from the last completed UTC month, selected by lowest rank and then category priority. Users do not manually equip these awards.

## Economy Rules

Current user-facing chip amounts:
- New chip rows start at 1,000 chips.
- Table losses can restore users to the 100-chip floor.
- Daily puzzle completions pay once per solved daily board:
  - easy: 100 chips
  - medium / solitaire draw-1: 250 chips
  - hard / solitaire draw-3: 500 chips
- Bonsai watering pays 200 chips once per day when the daily care row changes from unwatered to watered.
- Quest completions pay their template-defined chip reward automatically once per active assignment.
- Asterion escapes pay 4000 chips once per UTC day through `game_payout_claims`.
- Chess decisive wins pay 500 chips through `game_payout_claims` with a 60-minute per-player cooldown.
- Tron wins pay 50/75/100 chips for 2/3/4 round-start riders through `game_payout_claims` with a 5-minute per-player cooldown.
- Blackjack and Poker chips move through bets and pots.
- Tic-Tac-Toe currently publishes activity wins but does not pay chips.

`reward_templates` is the DB-backed source of truth for fixed minted rewards: daily puzzle base payouts, Asterion daily escape, Chess win cooldown payouts, Tron win cooldown payouts, and quest rewards. Betting games still settle from wager/pot state. Keep `late-ssh/src/app/help_modal/hub_guide.rs`, `dailies.rs`, root context, and Arcade/Rooms context aligned when seeded reward rows change.

## Quests

Daily/weekly quests are DB-backed and Hub-owned, with durable models in `late_core::models::quest`.

Implemented:
- `reward_templates` stores the admin-editable reward catalog. Rows with `is_quest = true` are eligible for daily/weekly assignment; non-quest rows describe always-available fixed payouts and their claim policy. The Hub Admin tab can edit title, description, target requirement, chip reward, draw weight, and active state. Migration `056_create_quests.sql` seeds the initial catalog.
- `quest_assignments` stores globally drawn quests per UTC period. Daily assigns two slots; weekly assigns one slot. Assignment generation is deterministic and protected by a Postgres advisory transaction lock.
- Daily slot 1 is drawn from Arcade-source quest templates (`daily_puzzle_win`, `arcade_score`, `arcade_level`). Daily slot 2 is drawn from multiplayer room-game quest templates (`room_rounds_played`, `room_wins`). Weekly uses the weekly pool.
- `user_quest_progress` tracks per-user progress, completion, and reward payment. `quest_progress_events` deduplicates per assignment/event id.
- Rewards write `chip_ledger` with reason `quest_reward`, source kind `quest_assignment`, and the assignment id as `source_ref`.
- `user_daily_quest_streaks` tracks per-user daily streaks. Completing at least one daily quest for a UTC day advances the streak; weekly quests do not count. The first streak day records day 1 with no streak bonus. Consecutive streak days then pay +100 chips at streak level 1 on day 2, +200 at level 2 on day 3, up to +500 at level 5; later consecutive days keep paying +500. Streak bonus ledger rows use reason `daily_quest_streak_reward` and source kind `daily_quest_streak`.
- `QuestService` subscribes to the global Activity channel and matches structured `ActivityKind` values against active templates. It publishes per-user `QuestSnapshot` values through watch channels and completion banners through a broadcast channel.
- `QuestService::start_listener_task` listens on `quest_user_changed` and `quest_assignments_changed` for cross-process refreshes.
- `QuestService` also exposes admin-gated reward-template list/update helpers used by the Hub Admin tab. Template edits notify `quest_assignments_changed`, so active quest snapshots refresh without rerolling the assignment rows.

Supported template kinds:
- `daily_puzzle_win`: params `{ "game": "...", "difficulty": "..." }`.
- `arcade_score`: params `{ "game": "tetris" }`, target is the required final score.
- `arcade_level`: params `{ "game": "snake" }`, target is the required final level reached.
- `room_rounds_played`: params `{ "game": "blackjack" | "poker" }`, target is completed settled hands.
- `room_wins`: params `{ "game": "blackjack" | "poker" }`, target is win events.
- `bonsai_watered`, `vote_cast`, `login_once`: no params.

Activity gateway notes:
- `ActivityEvent` now carries an event id for quest-progress dedupe.
- Visible public events remain filtered through `ActivityFilter::dashboard()`.
- Hidden quest-progress events use `ActivityCategory::Quest` for score and hand-count signals so they do not spam the dashboard/sidebar feed.
- Lateris and Snake publish final-score Activity events; Snake includes final level. Blackjack and Poker publish hidden played-hand events on settlement, plus existing visible win events.

## Arcade Wins Scoring

The monthly Arcade Wins board is not a chip board. It awards points for daily puzzle completions:
- easy / draw-1: 1 point
- medium: 3 points
- hard / draw-3: 5 points

This scoring lives in `late-core/src/models/leaderboard.rs` SQL. Completing more hard dailies across more daily games is the intended path to win the board.

## Shop / Marketplace

Durable marketplace ownership lives here with the Hub domain context.

Implemented:
- `late-core` owns durable data models in `late_core::models::marketplace`.
- `marketplace_items` defines curated purchasable items; `user_purchases` records durable per-user ownership.
- The Hub Admin tab can edit existing marketplace item names, descriptions, chip prices, sort order, and active state. It does not add SKUs or edit item kind/slot/payload/start/end windows.
- Purchases debit `user_chips`, write `chip_ledger` with reason `shop_purchase`, then insert `user_purchases` in one transaction.
- `ShopService` publishes per-user `ShopSnapshot` values through watch channels. UI/input reads the current snapshot and does not query the DB per keypress/render.
- `ShopService::start_listener_task` opens a dedicated long-lived Postgres connection (outside the pool) and `LISTEN`s on marketplace channels via `late_core::models::marketplace::listen_for_shop_changes` and the generic chip channel via `late_core::models::chips::listen_for_chip_changes`; all SQL stays in `late-core`. `shop_user_changed` and `chip_user_changed` carry a `user_id` payload and refresh that user's snapshot when active; `shop_catalog_changed` refreshes every active user.
- `purchase_durable_item_by_sku` notifies `shop_user_changed` inside the purchase transaction so it fires on COMMIT. The buyer's own snapshot is already updated by a direct `refresh_user` call, so that notification is the cross-process / external-mutation path and is redundant in a single process. Generic chip balance mutations notify `chip_user_changed`, which keeps Shop balances fresh after daily puzzle rewards, bonsai rewards, and room-game chip settlement. Chat room consumable purchases activate their `shop_consumable_effects` row in the same transaction as the chip debit and notify `shop_catalog_changed` on COMMIT so every SSH replica refreshes active room-effect projections.
- Pet Companion is the companion unlock. Current code uses `PET_COMPANION_SKU` (`pet_companion`) and `ShopEntitlements::has_pet_companion()`; migration 065 renames the legacy `cat_companion` seed item/table to pet terminology. It gates the sidebar pet and the `c` pet-care launcher.
- Chat and companion consumables are repeatable Shop purchases. Migration 071 seeds `chat_consumable` rows for Bot Username Color, Room Spark, Room Glow, Room Pulse, Hack Room, and Room Bump, plus `companion_consumable` rows for Cat/Dog Food and Aquarium Food. Catalog payloads carry `effect_kind`, optional `target = "room"`, optional `duration_secs`, and optional `daily_limit = true`. Room-targeted Chat consumables open a confirmation dialog before purchase/activation; the dialog names the current target room, effect, price, and daily limit, and accepts `Enter`/`y` to confirm or `Esc`/`n` to cancel. Bought Cat/Dog Food is inventory; pressing `t` in the pet modal consumes one food once per UTC day, updates `last_treated`, and starts a 30-minute session-local full-screen stroll. Bought Aquarium Food is inventory; pressing `Ctrl+F` while the Aquarium tray is open consumes one food, updates persisted `user_aquarium_care.last_fed`, and shows falling food flakes.
- Aquarium hunger is persisted through `user_aquarium_care.last_fed`. `ShopSnapshot::aquarium_hungry` becomes true immediately after Aquarium purchase until the first feed, then whenever the latest feed time is older than 24 hours. Hungry fish move less frequently and bias toward the bottom of the tank/reef.
- `shop_consumable_effects` stores active user/room effects. Room-targeted Chat consumables activate against the currently selected Home chat room and are rejected before purchase when no room is selected. Active room effects are projected into Shop snapshots as `active_room_effects`; Home chat renders active `room_spark`/`room_glow`/`room_pulse` as one-minute page-level visuals over selected room content, renders active `room_bump` effects on non-permanent public topic rooms as plain synthetic top-section `join #slug` rows with no effect suffixes, and adds real-room rail text/color only for Hack Room (`pinned_vibe`, one hour, `hacking`). `room_spark`, `room_glow`, and `room_pulse` must not add top text, promote rooms, or restyle room-list rows. Pressing Enter on a synthetic bump row joins/moves through the existing public-room join path, while the real room stays in normal navigation when present. Bot Username Color is projected as `bot_username_color_active` and brightens bot/graybeard/dealer author labels for the buyer while active.

Future Shop work:
- Add more curated cosmetics carefully: username flat color, title slot, starter badge, force-music vote consumable, mention sound variant, emoji slot remap.
- Add deeper behavioral hooks for Chat consumables after the first visible pass, especially real ordering semantics for Room Bump.
- Keep user-provided free text and uploads out of MVP; use curated pools to avoid moderation load.
- Cosmetic render hooks should read purchase/equip state, not duplicate marketplace state in chat/profile/game modules.

Future Events work:
- Add event/season-specific award categories on top of the monthly leaderboard-award table.
- Do not delete source ledger/event rows; monthly boards naturally re-window.
- Monthly placement should remain a permanent profile/status badge, not a chip bonus.

## Testing Guidance

- Pure state/input/layout helpers can have inline unit tests.
- DB/service behavior belongs in `late-ssh/tests/` and must use the shared testcontainers helpers.
- Root test policy applies: agents do not run `cargo test`, `cargo nextest`, or `cargo clippy`.

## Known Gaps

- `Events` is still a placeholder.
- Hub Admin edits existing reward-template and marketplace item presentation/economy fields only; adding new quest templates or Shop SKUs, changing JSON params/payload/kind/cadence/slot/windows, and rerolling current assignments still require direct DB/migration work.
- Shop has implemented categories for Companions, Chat, Aquarium, Badges, Flags, and Ultimates; keep this context in sync when adding another category or changing unlock gates.
- Leaderboard refresh is polling-based, so Activity events can appear before leaderboard panels catch up. Quest and Shop snapshots refresh on session init, local mutations, and Postgres notifications.
- There is no paginated detail view yet; compact panels only show top rows plus an around-you tail where implemented.
- Events-specific awards are not implemented.
