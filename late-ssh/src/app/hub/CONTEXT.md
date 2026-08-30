# Hub Context

## Metadata
- Scope: `late-ssh/src/app/hub`
- Last updated: 2026-08-30 (Sliding Puzzle's easy/medium/hard reward, quest, and Arcade Wins tiers are documented alongside the rest of the Hub-owned economy surface.) Previously 2026-08-26 (burn milestones: migration 157 seeds three permanent `milestone_badge` glyphs in the Ultimates tab and drops both ultimate spells to 1,000,000; a milestone rides the flair directory and renders on top of a rented badge and flag, never in place of one.) Previously 2026-08-25 (one title, tab order, Chat sections: migration 151 retires the 72 curated title rows (`active = false`) and renames the custom pair Your Own Title, the only title the Shop now sells; `ShopCategory::ALL` runs Chat, Badges, Flags, Companions, Aquarium, Ultimates; the Chat list is split by section rows into Name effects / Title / Consumables. Chat gilds (Economy Rules below) are a chip sink that does NOT live here: they are a chat message action, not a Shop item.)
- Purpose: local working context for the Hub domain: the Shop modal, the quest service behind the Arcade strip, and the Shop-unlocked aquarium.
- Parent context: `../../../../CONTEXT.md`

## Scope

`late-ssh/src/app/hub` owns the Shop modal (opened with the `/shop` composer command or a locked-feature nudge; there is no global chord) and `QuestService` feeding the quest strip at the top of The Arcade lobby. Former Guide content lives in the global `?` guide's Economy topic under `late-ssh/src/app/help_modal/hub_guide.rs`. Hub also owns the Shop-unlocked Aquarium tray toggled with the `/aquarium` composer command (alias `/aq`). The Leaderboards page, its `LeaderboardService`, the board rosters, and monthly profile awards live in their own slice: `late-ssh/src/app/leaderboard/` with `app/leaderboard/CONTEXT.md`.

Hub is a cross-product domain surface. Its services may summarize Arcade, Lobby, economy, and marketplace information, but it must not own those runtimes. Arcade game state stays under `late-ssh/src/app/arcade`; the Lobby's game runtimes stay under `late-ssh/src/app/lobby`; generic chip earn/spend primitives stay in `late-core/src/models/chips.rs`. Hub-owned marketplace state and entitlement projections live under `hub/shop`.

Keep `mod.rs` declaration-only. Do not add `pub use` re-export layers.

## Source Map

- `input.rs`: Hub-only key routing (`Tab`/arrows switch Shop categories, `Esc/q` close).
- `ui.rs`: modal frame (titled " Shop "), footer, and Shop dispatch.
- `dailies.rs`: module root for the quest surface.
- `dailies/`:
  - `svc.rs`: `QuestService`, current assignment generation, Activity-driven progress matching, per-user watch snapshots including daily streak state, completion banners, and Postgres LISTEN/NOTIFY refresh listener.
  - `state.rs`: snapshot/event drains for the quest strip and completion banners.
  - `ui.rs`: `draw_arcade_strip`, the quest strip at the top of The Arcade lobby: a heading with the 6-slot daily-streak meter (`▰▱`, day 6 caps the bonus) and next-bonus chips, then Daily and Weekly sections, each with a done count, the open-chip sum, and a reset note. Items show a status glyph, colored difficulty tag (easy/medium/hard), reward, and a progress bar for multi-step quests on terminals at least 70 cols wide. Height is dynamic via `arcade_strip_height`; the lobby drops the strip when fewer than 13 rows would remain below it.
- `aquarium/`: animated ambient aquarium tray adapted from Reefs.
  - `state.rs`: embedded aquarium runtime state, per-frame movement, resize binding, and initial entity spawn.
  - `ui.rs`: top tray and aquarium renderer.
  - `config.rs`, `creature.rs`, `world.rs`, `kdl_parse.rs`: embedded KDL config/art parsing and creature/world model.
- `shop/`: Hub-owned marketplace domain.
  - `catalog.rs`: Shop categories and SKU helpers.
  - Rental vocabulary shared with `late-core` lives in `late-core/src/models/rental.rs`, not here.
  - `entitlements.rs`: lightweight owned-feature projection for render/input gates.
  - `svc.rs`: `ShopService`, per-user watch snapshots, purchase tasks, and Postgres LISTEN/NOTIFY refresh listener.
  - `state.rs`: selected category/item, snapshot/event drains, and purchase activation.
  - `input.rs`: Shop-only item/category/buy input. `h`/`l` switch Shop categories/subtabs; `[`/`]` remain aliases. Mouse left-click on a category sub-tab or item row selects it; scroll wheel moves item selection. The custom-title prompt is checked first and swallows the whole event stream while open, because in a text field every printable key is content rather than a binding.
  - `ui.rs`: Shop tab rendering.

## The modal

The modal is the Shop and nothing else: a functional marketplace surface where Pet Companion is the durable companion unlock. It has no tab strip and no tab state; `Tab`/arrows switch the Shop's own category tabs. It is opened by the `/shop` composer command (`ChatState` request drained in `chat/input.rs`, calling `input::open_shop_modal_globally`) and by locked pet/aquarium nudges; there is no global chord.

Former tabs, for archaeology:
- `Leaderboard`: replaced by the top-level Leaderboards page (screen `6`, `late-ssh/src/app/leaderboard/`).
- `Quests`: replaced by the strip at the top of The Arcade lobby (`dailies/ui.rs::draw_arcade_strip`).
- `Admin` (reward-template/shop-item editor): deleted; those edits are direct DB/migration work (§ Known Gaps).
- `Events`: deleted; the events pillar is parked (DRAGON.md graveyard note).
- `Guide`: moved to the global guide's Economy topic.

## Aquarium

Aquarium is a Shop unlock, not an admin/mod preview. The Aquarium feature costs 10,000 chips, lives in the Companions Shop category, and unlocks Aquarium ownership/use. The Aquarium Shop category is fish-only and browseable before unlock so users can preview fish, but fish purchases and active-count changes are blocked until the Aquarium feature is owned. The `/aquarium` composer command (alias `/aq`) toggles the owned user's 11-row tray, rendered only in the Home Lounge view where it is carved from the top of the lounge chat column (sidebars and other screens are untouched); the open/closed state persists in user settings (`show_aquarium_tray`); locked users are sent to the Shop modal with a banner. `carve_top_tray` skips the tray entirely when the chat column cannot also hold `dashboard::ui::MIN_CHAT_HEIGHT_WITH_LOUNGE` rows below it — since `/aquarium` is typed into the composer, a tray that eats the composer would strand an owner on a short terminal with no way to hide it. `/aquarium feed` replaces the old `Ctrl+F` feed chord. `show_aquarium_tray` defaults to true, so buying the Aquarium reveals the tray with no purchase hook, exactly as `show_pet_strip` reveals the pet strip on unlock; rendering ANDs the setting with `has_aquarium()`, so the default stays inert until the feature is owned and no code force-closes the tray when the entitlement is absent.

The runtime is ambient-only for now:
- Fish ownership and active counts persist through `marketplace_items` / `user_purchases`.
- Fish SKUs cost 1,000 chips each and are repeatable purchases; buying the same fish N times gives owned quantity N and does not change active population.
- Active aquarium population is capped at 20 fish total for now; owned fish quantity is not capped by that active limit.
- `+` / `-` in the Aquarium Shop category adjusts the selected fish's active count, bounded by owned quantity and the 20-fish active cap.
- No non-Shop service calls, economy, or activity events.
- It ticks only while the tray is open and rebinds on terminal resize.
- Active fish are also projected into profile snapshots via `marketplace::active_aquarium_fish_for_user`; Profile modal renders an Aquarium tab/panel for viewed users using active fish counts.

Assets live under `late-ssh/assets/aquarium`. The source was adapted from `github.com/mevanlc/reefs`; keep attribution/licensing notes with any future asset or behavior changes.

## Leaderboard Data

Moved to `late-ssh/src/app/leaderboard/CONTEXT.md` (2026-08-07), together
with `LeaderboardService` itself (now `app/leaderboard/svc.rs`): the
refresh model, the board rosters and queries, the page, monthly profile
awards, and the `make seed-leaderboard` script all live there. Hub keeps
only the Arcade Wins *scoring* note below because its points come from the
same `Difficulty` enum as the quest/daily payouts hub does own.

## Economy Rules

Current user-facing chip amounts:
- New chip rows start at 1,000 chips.
- Table losses can restore users to the 100-chip floor.
- Daily puzzle completions pay once per solved daily board:
  - easy: 100 chips
  - medium / solitaire draw-1: 250 chips
  - hard / solitaire draw-3: 500 chips
  - Le Word daily: 100 chips
  - Rubik's Cube daily: 250 chips
  - Sliding Puzzle easy/medium/hard: 100/250/500 chips
- Bonsai watering pays 200 chips once per day when the daily care row changes from unwatered to watered.
- Quest completions pay their template-defined chip reward automatically once per active assignment.
- Asterion escapes pay 4000 chips once per UTC day through `game_payout_claims`.
- Lateania boss achievements pay through `game_payout_claims` behind two gates (SHOP.md Phase 6, migration 158): 10,000 chips for the Archdemon Mal'gareth and the King Who Was Promised Nothing, 20,000 for Yssgar and Kaethyr Ascendant, each once per `mud_characters.id` and at most once every 7 days per account.
- Chess decisive wins pay 500 chips through `game_payout_claims` with a 60-minute per-player cooldown.
- ssHattrick decisive wins pay 300 chips through `game_payout_claims` with a 15-minute per-player cooldown.
- Tron wins pay 50/75/100 chips for 2/3/4 round-start riders through `game_payout_claims` with a 5-minute per-player cooldown.
- Blackjack and Poker chips move through bets and pots.
- Tic-Tac-Toe currently publishes activity wins but does not pay chips.
- Chat gilds are a player-to-player sink that lives outside the Shop entirely: `$` on someone else's message in a public room pays 500 / 5,000 / 50,000 chips, two thirds of which reaches the author (`ChipMove::GildReceived`, excluded from Top Chips) while the last third is burned. Nothing about it is a `marketplace_item` or a `reward_template`; see `late-ssh/src/app/chat/CONTEXT.md` §9b Gilds.

`reward_templates` is the DB-backed source of truth for fixed minted rewards: daily puzzle base payouts, Asterion daily escape, Chess win cooldown payouts, ssHattrick win cooldown payouts, Tron win cooldown payouts, and quest rewards. Betting games still settle from wager/pot state. Keep `late-ssh/src/app/help_modal/hub_guide.rs`, `dailies.rs`, root context, and Arcade/Rooms context aligned when seeded reward rows change.

## Quests

Daily/weekly quests are DB-backed and Hub-owned, with durable models in `late_core::models::quest`.

Implemented:
- `reward_templates` stores the reward catalog. Rows with `is_quest = true` are eligible for daily/weekly assignment; non-quest rows describe always-available fixed payouts and their claim policy. Migration `056_create_quests.sql` seeds the initial catalog; edits since the Admin tab's deletion go through migrations.
- `quest_assignments` stores globally drawn quests per UTC period. Daily assigns two slots; weekly assigns one slot. Assignment generation is deterministic and protected by a Postgres advisory transaction lock.
- Assigned quests are arcade-only for now (owner decision 2026-07-13, see `devdocs/FRD-LOBBY-CONSOLIDATION.md`): every slot draws from Arcade-source templates (`daily_puzzle_win`, `arcade_score`, `arcade_level` — score/level runs plus the daily puzzles), split by difficulty: daily slot 1 easy, daily slot 2 medium, weekly slot hard (`quest.rs::slot_difficulty_preference`). The room-game templates (`room_rounds_played`/`room_wins`) were deactivated by migration 110; their match arms in `hub/dailies/svc.rs` die with the Rooms demolition (phase 3).
- `user_quest_progress` tracks per-user progress, completion, and reward payment. `quest_progress_events` deduplicates per assignment/event id.
- Rewards write `chip_ledger` with reason `quest_reward`, source kind `quest_assignment`, and the assignment id as `source_ref`.
- `user_daily_quest_streaks` tracks per-user daily streaks. Completing at least one daily quest for a UTC day advances the streak; weekly quests do not count. The first streak day records day 1 with no streak bonus. Consecutive streak days then pay +100 chips at streak level 1 on day 2, +200 at level 2 on day 3, up to +500 at level 5; later consecutive days keep paying +500. Streak bonus ledger rows use reason `daily_quest_streak_reward` and source kind `daily_quest_streak`.
- `QuestService` subscribes to the global Activity channel and matches structured `ActivityKind` values against active templates. It publishes per-user `QuestSnapshot` values through watch channels and completion banners through a broadcast channel.
- `QuestService::start_listener_task` listens on `quest_user_changed` and `quest_assignments_changed` for cross-process refreshes, so reward-template edits (migrations, manual SQL) refresh active quest snapshots without rerolling the assignment rows as long as the edit notifies `quest_assignments_changed`.

Supported template kinds:
- `daily_puzzle_win`: params `{ "game": "...", "difficulty": "..." }`.
- `arcade_puzzle_solved`: params `{ "game": "...", "difficulty": "..." }`.
- `arcade_score`: params `{ "game": "tetris" }`, target is the required final score.
- `arcade_level`: params `{ "game": "snake" }`, target is the required final level reached.
- `room_rounds_played`: params `{ "game": "blackjack" | "poker" | "chess" | "tron" }`; targets mean settled hands, qualifying completed Chess games, or Tron rounds as seeded by template.
- `room_wins`: params `{ "game": "blackjack" | "poker" | "chess" | "tron" }`; target is win events.
- `bonsai_watered`, `login_once`: no params.

Activity gateway notes:
- `ActivityEvent` now carries an event id for quest-progress dedupe.
- Visible public events remain filtered through `ActivityFilter::dashboard()`.
- Hidden quest-progress events use `ActivityCategory::Quest` for score and hand-count signals so they do not spam the dashboard/sidebar feed.
- Lateris and Snake publish final-score Activity events; Snake includes final level. Blackjack and Poker publish hidden played-hand events on settlement, plus existing visible win events. Chess and Tron publish qualifying room-round/win events for seeded quests.

Seeded Arcade quest templates include Sudoku easy/medium, Nonogram easy/medium, Minesweeper easy/medium, Solitaire draw-1/draw-3, Le Word daily, Rubik's Cube daily, Sliding Puzzle easy/medium/hard, and score quests for Lateris, 2048, and Snake. Le Word uses `daily_puzzle_win` with params `{ "game": "le_word", "difficulty": "daily" }` and pays the quick quest reward of 150 chips. Rubik's Cube uses `arcade_puzzle_solved` with params `{ "game": "rubiks_cube", "difficulty": "daily" }` and pays the medium quest reward of 375 chips. Sliding Puzzle uses `daily_puzzle_win` with its matching difficulty key; easy/medium are daily templates and hard is weekly. Each slot rolls from its full difficulty bucket: there is no cross-slot domain avoidance, so the medium slot can draw a medium puzzle even after slot 1 took an easy puzzle. Rubik's Cube and draw-3 Solitaire each carry two quest templates so they sit in both tiers: Rubik's is medium daily (`solve_rubiks_cube`) and hard weekly (`solve_rubiks_cube_weekly`, migration 121); draw-3 Solitaire is hard weekly (`win_draw_3_solitaire`) and medium daily (`win_draw_3_solitaire_daily`, migration 121). The twins share completion params, so one solve ticks both when both are drawn.

## Arcade Wins Scoring

The monthly Arcade Wins board is not a chip board. It awards points for daily puzzle completions:
- easy / draw-1: 1 point
- medium: 3 points
- hard / draw-3: 5 points
- Le Word daily: 1 point
- Rubik's Cube daily: 3 points
- Sliding Puzzle easy/medium/hard: 1/3/5 points

This scoring lives in `late-core/src/models/leaderboard.rs` SQL. Completing more hard dailies across more daily games is the intended path to win the board.

## Shop / Marketplace

Durable marketplace ownership lives here with the Hub domain context.

Implemented:
- `late-core` owns durable data models in `late_core::models::marketplace`.
- `marketplace_items` defines curated purchasable items; `user_purchases` records durable per-user ownership. Catalog edits go through migrations (§ Known Gaps).
- Purchases debit `user_chips`, write `chip_ledger` with reason `shop_purchase`, then insert `user_purchases` in one transaction.
- `ShopService` publishes per-user `ShopSnapshot` values through watch channels. UI/input reads the current snapshot and does not query the DB per keypress/render.
- `ShopService::start_listener_task` opens a dedicated long-lived Postgres connection (outside the pool) and `LISTEN`s on marketplace channels via `late_core::models::marketplace::listen_for_shop_changes` and the generic chip channel via `late_core::models::chips::listen_for_chip_changes`; all SQL stays in `late-core`. `shop_user_changed` and `chip_user_changed` carry a `user_id` payload and refresh that user's snapshot when active; `shop_catalog_changed` refreshes every active user.
- `purchase_durable_item_by_sku` notifies `shop_user_changed` inside the purchase transaction so it fires on COMMIT. The buyer's own snapshot is already updated by a direct `refresh_user` call, so that notification is the cross-process / external-mutation path and is redundant in a single process. Chip balance mutations notify `chip_user_changed` via the `user_chips` triggers (migration 128, fires on any balance change including gifts), which keeps Shop balances fresh after daily puzzle rewards, bonsai rewards, gifts, and room-game chip settlement. Chat room consumable purchases activate their `shop_consumable_effects` row in the same transaction as the chip debit and notify `shop_catalog_changed` on COMMIT so every SSH replica refreshes active room-effect projections.
- Pet Companion is the companion unlock. Current code uses `PET_COMPANION_SKU` (`pet_companion`) and `ShopEntitlements::has_pet_companion()`; migration 065 renames the legacy `cat_companion` seed item/table to pet terminology. It gates the pet strip above the chat composer (see `app/pet`). `show_pet_strip` defaults to true and `render.rs` ANDs it with `has_pet_companion()`, so buying the pet reveals the strip with no purchase hook; `show_aquarium_tray` works the same way (§ Aquarium). Neither surface is force-closed when its entitlement is absent, because the render gate already hides it, and a force-close would stamp the setting to false and defeat the default.
- Dynamic Bonsai is a `feature_unlock` in Companions with slot `bonsai_variant`; buying auto-equips it, and pressing Enter on the owned/equipped item clears the slot and returns the user to classic Bonsai.
- Chat and companion consumables are repeatable Shop purchases. Migration 071 seeds `chat_consumable` rows for Bot Username Color, Room Spark, Room Glow, Room Pulse, Hack Room, and Room Bump, plus `companion_consumable` rows for Cat/Dog Food and Aquarium Food. Migration 104 retires Bot Username Color (`chat_bot_username_color_day`, deactivated rather than deleted so `user_purchases` and `shop_consumable_effects.source_sku` keep their history), leaving Chat consumables room-targeted only. Migration 152 retires Hack Room the same way (`chat_pinned_vibe`, live `pinned_vibe` rows deactivated, the rail styling it drove removed from `chat/ui.rs`) and moves Room Bump to the top of the consumables (`sort_order` 4005), so the Chat tab's Consumables section reads Room Bump, Room Spark, Room Glow, Room Pulse. Catalog payloads carry `effect_kind`, optional `target = "room"`, optional `duration_secs`, and optional `daily_limit = true`. Room-targeted Chat consumables open a confirmation dialog before purchase/activation; the dialog names the current target room, effect, price, and daily limit, and accepts `Enter`/`y` to confirm or `Esc`/`n` to cancel. Bought Cat/Dog Food is inventory; `/pet feed` (or clicking the food bowl or the pet in the strip) consumes one food once per UTC day, updates `last_fed`, and starts a 30-minute session-local full-screen stroll. Feeding is the only pet-food sink, so the food bowl renders `?` and its `/pet feed` label turns amber while the inventory is empty, and a feed attempt with no food opens the Shop. Bought Aquarium Food is inventory; `/aquarium feed` while the tray is open consumes one food, updates persisted `user_aquarium_care.last_fed`, and shows falling food flakes. Migration 103 restates the four companion item descriptions (`pet_companion`, `pet_food`, `aquarium`, `aquarium_food`) in terms of the composer commands that run them; the seeded copy in 071 still describes the removed pet care modal and the old `Ctrl+Q`/`Ctrl+F` chords, so edit 103 (or add a later migration), never 071.
- Aquarium hunger is persisted through `user_aquarium_care.last_fed`. `ShopSnapshot::aquarium_hungry` becomes true immediately after Aquarium purchase until the first feed, then whenever the latest feed time is older than 24 hours. Hungry fish move less frequently and bias toward the bottom of the tank/reef.
- Shop categories (in tab order: Chat, Badges, Flags, Companions, Aquarium, Ultimates; the name-adjacent tabs lead) and item rows are left-click selectable. The Chat and Badges lists carry section rows (`item_list_rows`): Chat splits into Name effects / Title / Consumables, Badges into Premium / Basic. During rendering, `draw_categories` stores per-category `Rect`s and `draw_item_list` stores per-item `Rect`s on `ShopState` via interior mutability (`Cell`/`RefCell`). The input handler converts SGR 1-based coordinates to 0-based and hit-tests against the stored rects. Scroll wheel on the item list moves selection up/down. Buying/activation remains keyboard-only (`Enter`).
- `shop_consumable_effects` stores active user/room effects. Room-targeted Chat consumables activate against the currently selected Home chat room and are rejected before purchase when no room is selected. Active room effects are projected into Shop snapshots as `active_room_effects`; Home chat renders active `room_spark`/`room_glow`/`room_pulse` as one-minute page-level visuals over selected room content, and renders active `room_bump` effects on non-permanent public topic rooms as plain synthetic top-section `join #slug` rows with no effect suffixes. No effect adds real-room rail text or color (Hack Room, the one that did, is retired by migration 152). `room_spark`, `room_glow`, and `room_pulse` must not add top text, promote rooms, or restyle room-list rows. Pressing Enter on a synthetic bump row joins/moves through the existing public-room join path, while the real room stays in normal navigation when present. Every Chat consumable must be room-targeted, and `activate_chat_consumable_in_tx` now fails the purchase transaction for one that is not, rather than charging for a no-op. Bot Username Color got exactly this wrong (a per-viewer flag only the buyer ever saw); migration 104 retired it and dropped the user-scoped partial index.
- Rentals are the Shop's one recurring-spend shape, shared by four item kinds: `username_effect`, `badge_rental`, `title_rental`, and (as their common vocabulary) `late-core/src/models/rental.rs`. That module owns the two windows we sell (`RENTAL_DAY_SECS` 86400 / `RENTAL_MONTH_SECS` 2592000, the month priced at 40x the day since migration 153: a convenience premium, not a discount), the copy that quotes them (`duration_label` "24 hours"/"30 days", `duration_tag` "24h"/"30d"), and the payload readers (`BadgeRental::from_payload`, `title_from_payload`). Every rental lands as a user-scoped `shop_consumable_effects` row (`room_id IS NULL`, `ends_at = now + duration_secs`) keyed on `effect_kind`, so there is one live row per user per slot, a rebuy replaces it and resets the clock (across tiers too), and expiry is read-time only: the queries filter `ends_at > current_timestamp` and there is no background job. `marketplace::rental_duration_secs` is the single reader for how long an item runs, so shop copy, purchase banner, and #lounge tag can never disagree with the activation.
- Badge and flag rentals (`badge_rental` item kind, migration 148) replace the permanent badge shop. The migration derives a `<legacy_sku>_day` and `<legacy_sku>_month` item from every `item_kind = 'badge'` row (so new badges only need their legacy seed plus a re-run of that INSERT shape), prices them 100/3,000 basic and 250/7,500 premium with flags at the basic price, lists the month tier directly under its day twin (`sort_order * 10`, `+5`), and then retires every permanent SKU with `active = false`: never deleted, because `user_purchases` and `shop_consumable_effects.source_sku` keep the history. A rental carries `slot = NULL` in the column and `{"emoji","slot","tier","duration_secs"}` in the payload: it must never go through `equipped_slot`, which is the permanent-equip path. `activate_badge_rental_in_tx` writes the effect row under `effect_kind = 'chat_badge'` or `'chat_flag'` (the same two strings as the legacy slots), and bails rather than charging when the payload names no renderable emoji and slot. Distribution is the chat label query, not the flair directory: `User::list_chat_author_metadata` resolves the live rental row and nothing else. Migration 165 finished the job 148 started, clearing every permanent `chat_badge`/`chat_flag` equip and granting each owner 30 days of the same emoji through the month SKU, so the label is rented or bare and a rental lapsing now leaves nothing behind rather than uncovering a permanent badge. Viewers pick a change up on the author's next message or the next room-tail load (the same cadence a legacy equip always had); the buyer's own session sees it at once because `ShopSnapshot.chat_label_badge`/`chat_label_flag` come from that same query and `tick.rs` writes them into the chat context. The Badges/Flags tabs and the rentals' own `is_flag_badge()` read `badge_slot`, which only a rental payload fills now, and the row label carries a `24h`/`30d` tag because two tiers of one badge are otherwise identical rows. `bonsai_variant` (Dynamic Bonsai) is the only slot anything still equips, which is why the Shop has no clear-badge key and no equip flow outside it. Purchases announce through `ActivityPublisher::badge_rented_task` → `ActivityKind::BadgeRented`.
- Burn milestones (`milestone_badge` item kind, migration 157) are the only permanent name-adjacent item the Shop sells: Wick 50,000 🕯️, Fuse 150,000 🧨, Furnace 500,000 🌋, payload `{"emoji"}`, `slot = NULL`, on the Ultimates tab under a "Burn milestones" section row (`ultimates_section_label`) above the two spells. They buy nothing but the glyph and the whole price is burned (a plain `ShopPurchase` debit, no credit anywhere). Deliberately NOT `item_kind = 'badge'`: migration 148 ends with a blanket `active = false WHERE item_kind = 'badge'` and invites re-running its INSERT shape for new badges, which would switch a milestone off. Nothing about a milestone is equipped, so it never touches `equipped_slot` and there is no clear-it key: it is a fourth glyph that renders **on top of** a rented badge and flag (`HeaderTarget::StoreMilestone`, after `StoreBadge`/`StoreFlag` in the badge stack, clicking through to the Ultimates tab), which is the whole point: a 100-chip rental must not hide a 500,000-chip purchase. Own two rungs and the dearer one shows, picked in the query by `MilestoneBadge::highest_for_user` (`ORDER BY price_chips DESC`), which is what lets the feature ship with no equip flow and no slot column. Distribution is the flair directory, not the chat label query: `NameFlair.milestone` / `ResolvedName.milestone`, seeded by `load_flair_entries` (`MilestoneBadge::highest_for_all`, one row per owner) and re-read by `refresh_user_flair`, which must load it because that path replaces the whole entry. `settle_purchase` sets `flair_changed` on a milestone purchase so the buyer sees it at once, and other replicas catch up on the `shop_user_changed` notify like every other flair change. Purchases announce through `ActivityPublisher::burn_milestone_task` -> `ActivityKind::BurnMilestone { name, price }`, whose #lounge repeat key is the rung, so a whale climbing two rungs in one sitting posts twice. The same migration drops both ultimate spells from 10,000,000 to 1,000,000, which makes 1M the ceiling of the Shop and the top milestone half of it.
- Title rentals (`title_rental` item kind) sell a short text printed after the username (`mira, the night clerk`), on the Chat tab under the username effects, capped at `TITLE_MAX_LEN` (20) characters. `activate_title_rental_in_tx` writes `effect_kind = 'title'`: its own slot, so a title and a username color never clear each other. Distribution rides the flair directory below. Migration 149 seeded 36 curated noir titles (payload `{"text","duration_secs"}`, 200/24h and 6,000/30d); migration 151 retired all 72 rows with `active = false` (never deleted: `user_purchases` and `source_sku` keep pointing at them, and a live curated title runs out on its own clock). The model still honours a text-carrying payload, so nothing breaks if one is ever reseeded, but the catalog sells none.
- The one title on sale is Your Own Title (migration 150, renamed by 151: `title_custom_day` 1,000 / `title_custom_month` 40,000 after migrations 153 and 155), a title rental with the text left out: payload `{"custom":true,"duration_secs"}` and no `text` key, because the title does not exist until someone types it. The list row carries the `24h`/`30d` tag, since the two tiers are otherwise identical rows. The flow has three gates and none of them can charge:
  1. `PendingCustomTitle` (the picker shape, with a text field where the swatches are) caps typing at `TITLE_MAX_LEN` and refuses to send a blank prompt.
  2. `CustomTitle::parse` (in `late-core/src/models/rental.rs`) is the one validator: trims, collapses internal whitespace runs, and refuses over-length (never clamps: the buyer pays for the words they typed), control/zero-width/bidi characters, and any `@` (titles reach the #lounge feed, whose bodies never carry a mention). Each refusal is its own `CustomTitleError` with its own banner line.
  3. `ShopService::purchase_custom_title` refuses, still free and without a call, when the SKU is not a visible custom title, when `balance < price` (the purchase transaction's own rule, read through `MarketplaceItem::find_visible_by_sku` + `UserChips::ensure`), or when the buyer's last screen was inside `CUSTOM_TITLE_SCREEN_COOLDOWN` (10s, process-local like the chat gift cooldown). Every screen is a paid API call and a refused one costs the buyer nothing, so this is what keeps a held-down Enter from being an unmetered bill. `custom_title_precheck` is the pure gate, tested in `svc_test.rs`.
  4. `app/ai/screen.rs` screens the text with an ungrounded schema-enforced `generate_json` call (the bartender's shape), and fails closed: unreadable JSON, no reply, or AI switched off are all refusals, never an allow.
  Only then does `purchase_item_by_sku_with_custom_title` open the transaction, which re-checks the pairing (`is_custom_title` payload flag vs. supplied text) and bails on a mismatch, so a curated SKU can never wear buyer text and a custom SKU can never activate without it. `ShopSnapshot.custom_titles_available` mirrors `AiService::is_enabled()`, so with no AI configured the custom SKUs render as `closed` / `unavailable` rather than unscreened. `ShopService::purchase_custom_title_task` is the one match listing every outcome (rented, refused uncharged, call broke); refusals log at info with the reason, and the purchase banner quotes the activated row's text rather than the SKU name (which is just "Custom Title").
- Username effects (`username_effect` item kind, sold on the Chat tab in two tiers: migration 112's 24h Name Glow 200 / Name Gradient 500 / Name Shimmer 1000, and migration 146's 30-day Monthly twins, repriced to 40x by migration 153, listed directly under each day item by `sort_order`) are the user-scoped effect done right: globally visible, not snapshot-projected. Enter arms a swatch picker (`PendingUsernameEffect`, modeled on the room-effect confirm; arrows cycle 6 glow colors / 6 gradient pairs, swatches render in their real colors), Enter buys via `purchase_item_by_sku_with_username_effect`, and `activate_username_effect_in_tx` writes a user-scoped `shop_consumable_effects` row (`effect_kind = 'username_effect'`, `room_id IS NULL`, `ends_at = now + duration_secs`) in the purchase transaction, deactivating any prior live username-effect row: one active effect per user, rebuy replaces and resets the clock (across tiers too: a month buy replaces a live day effect), no daily limit. The tier lives entirely in the payload `duration_secs` and the price; nothing branches on day-vs-month, and `username_effect_duration_secs` is the one reader (shop copy, purchase banner, and #lounge tag all quote it, so the day tier reads "24 hours"/"24h" and the month tier "30 days"/"30d"). Migration 112 recreates the user-scoped partial index. Distribution bypasses shop snapshots for viewers: `ShopService` (built `.with_flair_directory(...)` and `.with_activity(...)` in main.rs) seeds the process-shared `NameFlairDirectory` (`app/common/username_effect.rs`) at startup, writes through on purchase, and refreshes one user's flair on each `shop_user_changed` notify (never on chip notifies: too frequent); sessions resolve it once a second in `tick.rs` into `App.name_flair`, and chat/clubhouse paint the fg per character. The directory entry (`NameFlair`) holds the color effect and the rented title as two independent halves with their own expiries, resolved together into a `ResolvedName { style, title }`; a purchase re-reads both halves rather than writing one through, so buying a title never drops a live color and vice versa. Only the buyer's own `ShopSnapshot` carries `active_username_effect`, `active_badge_rental`, `active_flag_rental`, and `active_title` (detail panes show what is running and the remaining time; `ShopState::tick` prunes each at `ends_at`). Purchases announce through `ActivityPublisher::username_effect_task` → `ActivityKind::UsernameEffectApplied`.

Future Shop work:
- Add more curated cosmetics carefully: force-music vote consumable, mention sound variant, emoji slot remap, and additional curated badge/flag/ultimate packs. (Username color, badge/flag rentals, the title slot, and the buyer-written title all shipped above.)
- Add deeper behavioral hooks for Chat consumables after the first visible pass, especially real ordering semantics for Room Bump.
- Keep uploads out of MVP. The one free-text field, the custom title, goes through `CustomTitle::parse` and the AI screen before it can be charged for; any new free text follows the same two gates.
- Cosmetic render hooks should read purchase/equip state, not duplicate marketplace state in chat/profile/game modules.

Future Events work (the tab is deleted; these hold for whenever events return):
- Add event/season-specific award categories on top of the monthly leaderboard-award table.
- Do not delete source ledger/event rows; monthly boards naturally re-window.
- Monthly placement should remain a permanent profile/status badge, not a chip bonus.

## Testing Guidance

- Pure state/input/layout helpers can have inline unit tests.
- DB/service behavior belongs in adjacent `_test.rs` files beside the module it exercises, using `crate::test_helpers::new_test_db`.
- Root test policy applies: agents do not run `cargo test`, `cargo nextest`, or `cargo clippy`.

## Known Gaps

- There is no in-app editor for reward templates or marketplace items (the Admin tab was deleted): every catalog change (presentation, economy fields, new quest templates or Shop SKUs, JSON params/payload/kind/cadence/slot/windows, rerolling current assignments) is direct DB/migration work.
- Shop has implemented categories for Chat, Badges, Flags, Companions, Aquarium, and Ultimates (that order); keep this context in sync when adding another category or changing unlock gates. The Ultimates tab sells two unrelated things (burn milestones, then the two spells), so `matches_item` admits both kinds and the action label tells them apart before offering to cast anything.
- Leaderboard refresh is polling-based, so Activity events can appear before the Leaderboards page catches up: a score set at minute 0 shows up on the board within 5 minutes, not at once. Sessions seed from the published snapshot at construction and a connect refreshes a stale one, so the boards are never *empty*, just up to one interval behind. Quest and Shop snapshots refresh on session init, local mutations, and Postgres notifications; the leaderboard has no equivalent notify path.
- The Leaderboards page has no scrolling inside a board's standings beyond the around-you tail; a board deeper than the pane clips.
- DCSS has its board triple via the log pipe (Phase 1 of `devdocs/PLAN-ROGUELIKE-BOARDS.md`); NetHack and Brogue still have none — their phases (xlogfile ingestion + scrape removal, victory-log patch) are next in that plan.
