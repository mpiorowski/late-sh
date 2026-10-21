# Hub Context

## Metadata
- Scope: `late-ssh/src/app/hub`
- Purpose: local working context for the Hub domain: the Shop modal, the quest service behind the Arcade strip, and the Shop-unlocked aquarium.
- Companion docs: `shop/CONTEXT.md` (the marketplace), `aquarium/CONTEXT.md` (the reef and its care).
- Parent context: `../../../../CONTEXT.md`

## Scope

`late-ssh/src/app/hub` owns the Shop modal (opened with Ctrl+S, the `/shop` composer command, or a locked-feature nudge; active games, profile/job editors, and Artboard input retain their local Ctrl+S bindings) and `QuestService` feeding the quest strip at the top of The Arcade lobby. The player-facing economy explanation lives in the global `?` guide's Economy topic (`late-ssh/src/app/help_modal/hub_guide.rs`). Hub also owns the Shop-unlocked Aquarium simulation and its care; the reef is drawn on the Zen page (`Ctrl+F`), with a small stand-in of its population in the opt-in sidebar Tank panel, and fed with the `/aquarium feed` composer command (alias `/aq feed`). The Leaderboards page, its `LeaderboardService`, the board rosters, and monthly profile awards live in their own slice: `late-ssh/src/app/leaderboard/` with `app/leaderboard/CONTEXT.md`.

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
- `aquarium/`: the Shop-unlocked reef, its drawing and its care (`aquarium/CONTEXT.md`).
- `shop/`: Hub-owned marketplace domain: catalog, purchases, entitlements, rentals, flair (`shop/CONTEXT.md`).

## The modal

The modal is the Shop and nothing else: a functional marketplace surface where Pet Companion is the durable companion unlock. It has no tab strip and no tab state; `Tab`/arrows switch the Shop's own category tabs. It is opened by Ctrl+S, the `/shop` composer command (`ChatState` request drained in `chat/input.rs`, calling `input::open_shop_modal_globally`), and locked pet/aquarium nudges. Active games, profile/job editors, and Artboard input retain their local Ctrl+S bindings.

Leaderboards live on page `6` (`app/leaderboard`), quests on the strip at the top of the Arcade lobby (`dailies/ui.rs::draw_arcade_strip`), and the economy's player-facing explanation in the global guide's Economy topic. There is no in-app editor for rewards or catalog rows (Known Gaps).

## Aquarium

Aquarium is a Shop unlock (10,000 chips, Companions tab): an ambient reef drawn on the Zen page, the profile and the opt-in sidebar Tank panel, fed for free once a UTC day with `/aquarium feed` (`/aq feed`, or `a` on Zen), with fish that breed and starve and plants that sprout and root. The tank, its stock caps, its care clocks and their gotchas are in `aquarium/CONTEXT.md`.

## Leaderboard Data

The refresh service, board rosters, the page and monthly awards are in `late-ssh/src/app/leaderboard/CONTEXT.md`. Hub keeps only the Arcade Wins scoring note below, because its points come from the same `Difficulty` enum as the quest and daily payouts hub owns.

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
- Bonsai watering pays 200 chips once per UTC day (`app/bonsai/CONTEXT.md`).
- Quest completions pay their template-defined chip reward automatically once per active assignment.
- Asterion escapes pay 4000 chips once per UTC day through `game_payout_claims`.
- Lateania boss achievements pay through `game_payout_claims` behind two gates (migrations 158 and 207): 10,000 chips for each of the four crowns, once per `mud_characters.id` and at most once every 30 days per account, so a full run is 40,000 a month.
- Tron wins pay 50/75/100 chips for 2/3/4 round-start riders through `game_payout_claims` with a 5-minute per-player cooldown.
- Blackjack and Poker chips move through per-game reasons (`ChipMove::BlackjackBet` / `BlackjackPayout`, `PokerBet` / `PokerPayout`, plus `FloorRestore` after a losing settlement), every row carrying the round or hand id as `source_ref`, and no side counts toward Top Chips: wagered money is off the board so a colluding table cannot fold one seat to the top. Rows from before the per-game reasons keep their `LegacyTable*` reasons and are excluded too.
- Chat gilds are a player-to-player sink that lives outside the Shop entirely: `$` on someone else's message in a public room pays 500 / 5,000 / 50,000 chips, two thirds of which reaches the author (`ChipMove::GildReceived`, which counts on Top Chips; the buyer's `GildSent` is a debit and does not) while the last third is burned. Nothing about it is a `marketplace_item` or a `reward_template`; see `late-ssh/src/app/chat/gild/CONTEXT.md`.

`reward_templates` is the DB-backed source of truth for fixed minted rewards: daily puzzle base payouts, the Asterion daily escape, Tron win cooldown payouts, daily correspondence wins, door milestones, and quest rewards. Betting games still settle from wager/pot state. Keep `late-ssh/src/app/help_modal/hub_guide.rs`, `dailies.rs`, and the Arcade and Lobby contexts aligned when seeded reward rows change.

## Quests

Daily/weekly quests are DB-backed and Hub-owned, with durable models in `late_core::models::quest`.

Implemented:
- `reward_templates` stores the reward catalog. Rows with `is_quest = true` are eligible for daily/weekly assignment; non-quest rows describe always-available fixed payouts and their claim policy. Migration `056_create_quests.sql` seeds the initial catalog; edits go through migrations.
- `quest_assignments` stores globally drawn quests per UTC period. Daily assigns two slots; weekly assigns one slot. Assignment generation is deterministic and protected by a Postgres advisory transaction lock.
- Assigned quests are arcade-only (owner decision): every slot draws from Arcade-source templates (`daily_puzzle_win`, `arcade_score`, `arcade_level` — score/level runs plus the daily puzzles), split by difficulty: daily slot 1 easy, daily slot 2 medium, weekly slot hard (`quest.rs::slot_difficulty_preference`).
- `user_quest_progress` tracks per-user progress, completion, and reward payment. `quest_progress_events` deduplicates per assignment/event id.
- Rewards write `chip_ledger` with reason `quest_reward`, source kind `quest_assignment`, and the assignment id as `source_ref`.
- `user_daily_quest_streaks` tracks per-user daily streaks. Completing at least one daily quest for a UTC day advances the streak; weekly quests do not count. The first streak day records day 1 with no streak bonus. Consecutive streak days then pay +100 chips at streak level 1 on day 2, +200 at level 2 on day 3, up to +500 at level 5; later consecutive days keep paying +500. Streak bonus ledger rows use reason `daily_quest_streak_reward` and source kind `daily_quest_streak`.
- `QuestService` subscribes to the global Activity channel and matches structured `ActivityKind` values against active templates. It publishes per-user `QuestSnapshot` values through watch channels and completion banners through a broadcast channel.
- `QuestService::start_notify_worker` takes `quest_user_changed` and `quest_assignments_changed` from the process listener (`pg_listener.rs`) for cross-process refreshes, and re-reads every active board on a resync or a failed notify, so reward-template edits (migrations, manual SQL) refresh active quest snapshots without rerolling the assignment rows as long as the edit notifies `quest_assignments_changed`.

Supported template kinds:
- `daily_puzzle_win`: params `{ "game": "...", "difficulty": "..." }`.
- `arcade_puzzle_solved`: params `{ "game": "...", "difficulty": "..." }`.
- `arcade_score`: params `{ "game": "tetris" }`, target is the required final score.
- `arcade_level`: params `{ "game": "snake" }`, target is the required final level reached.
- `bonsai_watered`, `login_once`: no params.

Activity gateway notes:
- `ActivityEvent` carries an event id for quest-progress dedupe.
- What reaches #lounge is decided by `activity/filter.rs::lounge_includes` (`late-ssh/CONTEXT.md`, Activity).
- Hidden quest-progress events use `ActivityCategory::Quest` for score signals so they reach no public surface.
- Lateris and Snake publish final-score Activity events; Snake includes final level. House tables publish sit-downs only (`app/lobby/house/CONTEXT.md`).

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

Durable marketplace ownership: `marketplace_items` / `user_purchases`, purchases as one chip-debit transaction, per-user `ShopSnapshot` watch channels kept fresh by the process listener, entitlements (Pet Companion, the tank), room consumables, rentals (username effects, badge and flag rentals, titles including the AI-screened custom title), burn milestones, and the `NameFlairDirectory` that distributes name flair to every session. All of it, and the future Shop work, is in `shop/CONTEXT.md`.

## Events (no surface today)

Rules for when seasonal events return:
- Add event/season-specific award categories on top of the monthly leaderboard-award table.
- Do not delete source ledger/event rows; monthly boards naturally re-window.
- Monthly placement should remain a permanent profile/status badge, not a chip bonus.

## Testing Guidance

- Pure state/input/layout helpers can have inline unit tests.
- DB/service behavior belongs in adjacent `_test.rs` files beside the module it exercises, using `crate::test_helpers::new_test_db`.
- Agents run targeted tests through `make test-llm ARGS="-p late-ssh -E 'test(hub)'"`; `make check` stays with the human owner.

## Known Gaps

- There is no in-app editor for reward templates or marketplace items: every catalog change (presentation, economy fields, new quest templates or Shop SKUs, JSON params/payload/kind/cadence/slot/windows, rerolling current assignments) is direct DB/migration work.
- Shop has implemented categories for Chat, Badges, Flags, Companions, and Ultimates (that order); keep this context in sync when adding another category or changing unlock gates. The Ultimates tab sells two unrelated things (burn milestones, then the two spells), so `matches_item` admits both kinds and the action label tells them apart before offering to cast anything.
- Leaderboard refresh is polling-based, so Activity events can appear before the Leaderboards page catches up: a score set at minute 0 shows up on the board within 5 minutes, not at once. Sessions seed from the published snapshot at construction and a connect refreshes a stale one, so the boards are never *empty*, just up to one interval behind. Quest and Shop snapshots refresh on session init, local mutations, and Postgres notifications; the leaderboard has no equivalent notify path.
- The Leaderboards page has no scrolling inside a board's standings beyond the around-you tail; a board deeper than the pane clips.
- DCSS, NetHack, and Brogue each have their board triple via the log pipe (`app/door/ingest/CONTEXT.md` owns the contract).
