# Hub Context

## Metadata
- Scope: `late-ssh/src/app/hub`
- Purpose: local working context for the Hub domain: the Shop modal, the quest service behind the Arcade strip, and everyone's aquarium.
- Companion docs: `shop/CONTEXT.md` (the marketplace), `aquarium/CONTEXT.md` (the reef and its care).
- Parent context: `../../../../CONTEXT.md`

## Scope

`late-ssh/src/app/hub` owns the Shop modal (opened with Ctrl+S, the `/shop` composer command, or a locked-feature nudge; active games, profile/job editors, and Artboard input retain their local Ctrl+S bindings) and `QuestService` feeding the quest strip at the top of The Arcade lobby. The player-facing economy explanation lives in the global `?` guide's Economy topic (`late-ssh/src/app/help_modal/hub_guide.rs`). Hub also owns the Aquarium simulation and its care; the reef is a tile of Home's stock `home` layout (the stock `street` holds its small tank), with a small stand-in of its population in the tank tile, and fed with the `/aquarium feed` composer command (alias `/aq feed`). The Leaderboards page, its `LeaderboardService`, the board rosters, and monthly profile awards live in their own slice: `late-ssh/src/app/leaderboard/` with `app/leaderboard/CONTEXT.md`.

Hub is a cross-product domain surface. Its services may summarize Arcade, Lobby, economy, and marketplace information, but it must not own those runtimes. Arcade game state stays under `late-ssh/src/app/arcade`; the Lobby's game runtimes stay under `late-ssh/src/app/lobby`; generic chip earn/spend primitives stay in `late-core/src/models/chips.rs`. Hub-owned marketplace state and entitlement projections live under `hub/shop`.

Keep `mod.rs` declaration-only. Do not add `pub use` re-export layers.

## Source Map

- `input.rs`: Hub-only key routing (`Tab`/arrows switch Shop categories, `Esc/q` close).
- `ui.rs`: modal frame (titled " Shop "), footer, and Shop dispatch.
- `dailies.rs`: module root for the quest surface.
- `dailies/`:
  - `svc.rs`: `QuestService`, current assignment generation, Activity-driven progress matching, the daily streak kept off the same Activity events, per-user watch snapshots including the streak, completion banners, and Postgres LISTEN/NOTIFY refresh listener.
  - `state.rs`: snapshot/event drains for the quest strip and completion banners.
  - `ui.rs`: `draw_arcade_strip`, the quest strip at the top of The Arcade lobby: a heading with the daily streak (`streak day 3 · any daily board today keeps it`, `tomorrow` once today's is won, `no streak · win any daily board to start one` at zero), then Daily and Weekly sections, each with a done count, the open-chip sum, and a reset note. Items show a status glyph, colored difficulty tag (easy/medium/hard), reward, and a progress bar for multi-step quests on terminals at least 70 cols wide. Height is dynamic via `arcade_strip_height`; the lobby drops the strip when fewer than 13 rows would remain below it.
- `aquarium/`: the reef everyone has, its drawing and its care (`aquarium/CONTEXT.md`).
- `shop/`: Hub-owned marketplace domain: catalog, purchases, entitlements, rentals, flair (`shop/CONTEXT.md`).

## The modal

The modal is the Shop and nothing else: a functional marketplace surface where Pet Companion is the durable companion unlock. It has no tab strip and no tab state; `Tab`/arrows switch the Shop's own category tabs. It is opened by Ctrl+S, the `/shop` composer command (`ChatState` request drained in `chat/input.rs`, calling `input::open_shop_modal_globally`), and the locked pet nudge. Active games, profile/job editors, and Artboard input retain their local Ctrl+S bindings.

Leaderboards live on page `6` (`app/leaderboard`), quests on the strip at the top of the Arcade lobby (`dailies/ui.rs::draw_arcade_strip`), and the economy's player-facing explanation in the global guide's Economy topic. There is no in-app editor for rewards or catalog rows (Known Gaps).

## Aquarium

Aquarium is everyone's from their first connect, nothing to buy (the Shop's Companions tab sells its fish, plants and shield): an ambient reef drawn as a tile of Home and on the profile, fed for free once a UTC day with `/aquarium feed` (`/aq feed`, a click on its tile, or `a` on it while arranging Home), with fish that breed and starve and plants that sprout and root. The tank, its stock caps, its care clocks and their gotchas are in `aquarium/CONTEXT.md`.

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
  - Netwalk easy/medium/hard: 100/250/500 chips
  - Chess Puzzle easy/medium/hard: 100/250/500 chips
  - Word Wheel Bronze/Silver/Gold (easy/medium/hard): 100/250/500 chips
  - Quartets daily: 250 chips
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
- Assigned quests are arcade-only (owner decision): every slot draws from Arcade-source templates (`daily_puzzle_win`, `arcade_score`, `arcade_level`: score/level runs plus the daily puzzles), split by difficulty: daily slot 1 easy, daily slot 2 medium, weekly slot hard (`quest.rs::slot_difficulty_preference`).
- `user_quest_progress` tracks per-user progress, completion, and reward payment. `quest_progress_events` deduplicates per assignment/event id.
- Rewards write `chip_ledger` with reason `quest_reward`, source kind `quest_assignment`, and the assignment id as `source_ref`.
- The daily streak (`late_core::models::daily_streak`, table `user_daily_quest_streaks`, which keeps its old name) is kept by any daily board win, not by quests: `QuestService::apply_activity_event` calls `record_daily_win` for every `GameWon` whose game is a daily puzzle, on the event's UTC date, so it rolls over with the boards. Any win keeps the day, however quick: a streak's job is tomorrow, not depth, and a bad night at 23:50 has to be able to keep the day. A day counts once, yesterday continues the run, anything older restarts it at 1, and a win dated before the last day kept (a late event) changes nothing. It pays no chips; the count is the reward: the day card and the profile modal carry it as a number, and beside the name in chat it is a moon that fills (`StreakMark`: nothing under three days, `☽` at 3, `◑` at 7, `●` at 30, `◉` at 100, gone the day after a run breaks; the chat reads it per author through `list_daily_streak_snapshots`, `chat/RENDERING.md`). The snapshot reads zero once a whole day has gone by without a win. Lagged Activity receivers can drop a win, and with it a day, the same risk the chip payout for the win has always carried. `late_ssh_daily_streak_days_total{day="first"|"kept"}` counts the days. The chip bonus the streak once paid is gone: `ChipMove::DailyQuestStreakReward` (reason `daily_quest_streak_reward`) stays so the old ledger rows read, and the `bonus_level` column waits in `late-core/migrations/deferred/` for the release after this one.
- The rung (`late_core::models::daily_rung`, table `daily_rung_stars`, migration 280) is one daily board a day, the same for everyone: `rung_for(day)` walks a fixed eleven-game rotation from a Monday epoch, so no game comes round twice inside a week, each game with a choice of boards at its medium one or harder (never a game's easy board; within that the pick is by the clock), a one-board game at that board, and on Saturday the day's game at its hardest board. Easy Netwalk, the tour's lent board, is never a rung, on purpose: the send-off's "tomorrow is your first star" is tomorrow named with a prize instead of a time. The rotation is keyed to the calendar, never the person, so the rung's roster is the day's crowd and the chat has one board to argue about; the knob is time, not the difficulty label. The same Activity hook calls `record_rung_win` with the win's puzzle and `detail` (its difficulty key) on the board's UTC date, the date the event carries (a board finished after midnight still counts for its own day): the rung's own board on its own day lands one star, `ON CONFLICT DO NOTHING`, and any other board is nothing. The star pays no chips. A new star publishes `QuestEvent::RungStar` with the count, which `state.rs` turns into the `✦ Today's rung, done` banner, and counts `late_ssh_rung_stars_total`. The snapshot carries the user's star count (`rung_stars`, `count_stars`) for the day card and the day of their newest star (`latest_star`, `latest_star_day`), which `QuestState::rung_standing` turns into today's `RungStanding` for the week's badge (`RungStanding`, `dailies/state.rs`); the leaderboard snapshot carries everyone with today's star (`starred_on`, `DailyStatuses::starred_on`) for the week's count; the profile modal shows the count with the streak day, and beside the name in chat the day's star is `✦` in the name's color, today only (`list_starred_on`, `chat/RENDERING.md`). Every surface reads the stars table; none derives a star from the daily marks. The Arcade lobby draws the week of rungs beside the cards and stars the rung's card (`arcade/CONTEXT.md`, the week); the tour's send-off names tomorrow's as the first star and its week stop shows its badge beside today's (`clubhouse/CONTEXT.md` §5).
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

Seeded Arcade quest templates include Sudoku easy/medium, Nonogram easy/medium, Minesweeper easy/medium, Solitaire draw-1/draw-3, Le Word daily, Rubik's Cube daily, Quartets daily, Sliding Puzzle, Netwalk, Chess Puzzle and Word Wheel easy/medium/hard, and score quests for Lateris, 2048, and Snake. Le Word uses `daily_puzzle_win` with params `{ "game": "le_word", "difficulty": "daily" }` and pays the quick quest reward of 150 chips. Rubik's Cube uses `arcade_puzzle_solved` with params `{ "game": "rubiks_cube", "difficulty": "daily" }` and pays the medium quest reward of 375 chips. Sliding Puzzle, Netwalk, Chess Puzzle and Word Wheel (migration 271) use `daily_puzzle_win` with their matching difficulty key; easy (quick, 150) and medium (skill, 375) are daily templates and hard (750) is weekly. Word Wheel's are titled by rank (Reach Bronze, Silver, Gold) and match the rank's tier key. Each slot rolls from its full difficulty bucket: there is no cross-slot domain avoidance, so the medium slot can draw a medium puzzle even after slot 1 took an easy puzzle. Rubik's Cube, Quartets and draw-3 Solitaire each carry two quest templates so they sit in both tiers: Rubik's is medium daily (`solve_rubiks_cube`) and hard weekly (`solve_rubiks_cube_weekly`, migration 121); Quartets is the same, `solve_quartets` and `solve_quartets_weekly` (migration 271, `daily_puzzle_win` with `{ "game": "quartets", "difficulty": "daily" }`); draw-3 Solitaire is hard weekly (`win_draw_3_solitaire`) and medium daily (`win_draw_3_solitaire_daily`, migration 121). The twins share completion params, so one solve ticks both when both are drawn.

## Arcade Wins Scoring

The monthly Arcade Wins board is not a chip board. It awards points for daily puzzle completions:
- easy: 1 point
- medium / draw-1: 3 points
- hard / draw-3: 5 points
- Le Word daily: 1 point
- Rubik's Cube daily: 3 points
- Sliding Puzzle easy/medium/hard: 1/3/5 points
- Netwalk easy/medium/hard: 1/3/5 points
- Chess Puzzle easy/medium/hard: 1/3/5 points
- Word Wheel Bronze/Silver/Gold (easy/medium/hard): 1/3/5 points
- Quartets daily: 3 points

Each board's tier is `DailyPuzzle::tiers` in `late-core/src/models/leaderboard.rs`, the same tier its chip payout comes from, and the points SQL is generated from it. Completing more hard dailies across more daily games is the intended path to win the board.

## Shop / Marketplace

Durable marketplace ownership: `marketplace_items` / `user_purchases`, purchases as one chip-debit transaction, per-user `ShopSnapshot` watch channels kept fresh by the process listener, entitlements (Pet Companion), room consumables, rentals (username effects, badge and flag rentals, titles including the AI-screened custom title), burn milestones, and the `NameFlairDirectory` that distributes name flair to every session. All of it, and the future Shop work, is in `shop/CONTEXT.md`.

## Events (no surface today)

Rules for when seasonal events return:
- Add event/season-specific award categories on top of the monthly leaderboard-award table.
- Do not delete source ledger/event rows; monthly boards naturally re-window.
- Monthly placement should remain a permanent profile/status badge, not a chip bonus.

## Testing Guidance

- Pure state/input/layout helpers can have inline unit tests.
- DB/service behavior belongs in adjacent `_test.rs` files beside the module it exercises, using `crate::test_helpers::new_test_db`.
- Agents run targeted tests through `make test-llm ARGS="-p late-ssh --lib -E 'test(hub)'"`; `make check` stays with the human owner.

## Known Gaps

- There is no in-app editor for reward templates or marketplace items: every catalog change (presentation, economy fields, new quest templates or Shop SKUs, JSON params/payload/kind/cadence/slot/windows, rerolling current assignments) is direct DB/migration work.
- Shop has implemented categories for Chat, Badges, Flags, Companions, and Ultimates (that order); keep this context in sync when adding another category or changing unlock gates. The Ultimates tab sells two unrelated things (burn milestones, then the two spells), so `matches_item` admits both kinds and the action label tells them apart before offering to cast anything.
- DCSS, NetHack, and Brogue each have their board triple via the log pipe (`app/door/ingest/CONTEXT.md` owns the contract).
