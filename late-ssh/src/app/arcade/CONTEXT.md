# Arcade Context

## Metadata
- Scope: `late-ssh/src/app/arcade`
- Last updated: 2026-08-21 (Sliding Puzzle has persisted, unrewarded personal boards)
- Purpose: local working context for The Arcade screen and single-player terminal games.
- Parent context: `../../../../CONTEXT.md`

## Scope

`late-ssh/src/app/arcade` owns the SSH Arcade domain: lobby navigation, single-player game state/input/rendering, persisted progress, daily puzzle completions, high scores, and chip rewards.

Hub/leaderboard surfaces are separate and live under `late-ssh/src/app/hub`. Arcade games submit score and daily-win data; Hub refreshes and renders cross-product leaderboard/economy views from that data. The falling-block game is user-facing `Lateris`; lowercase `tetris` remains the internal compatibility key/table/module namespace for existing saved games, score events, quests, and award categories.

Shared game-domain primitives live under `late-ssh/src/app/games`:
- `games/cards.rs` for card ranks/suits/rendering used by Solitaire and room card games.
- `games/chips/svc.rs` for Late Chips balances, initial grants, debits, payouts, floors, and daily bonuses.

Multiplayer table games are separate and live under `late-ssh/src/app/lobby`. Do not make the Lobby depend on Arcade modules for shared game behavior.

Keep `mod.rs` declaration-only. Do not add `pub use` re-export layers.

## Source Map

- `mod.rs` declares Arcade modules.
- `input.rs` routes The Arcade lobby and selected active game input.
- `ui.rs` renders the lobby and exposes Arcade-only bottom-bar/status helpers. The lobby carves `hub::dailies::ui::arcade_strip_height(quest_state)` rows off the top for the quest strip (streak-meter heading plus grouped Daily/Weekly sections, from `QuestState`) whenever the area leaves at least 13 rows for the game list below it; active games never show it.
- `twenty_forty_eight/`, `tetris/`, and `snake/` are high-score games.
- `traffic/` is a multi-track high-score game. Each track finish is graded to a normalized `0..=1000` score (`Track::grade_time`, from the track's theoretical fastest/slowest completion time, so every track yields a comparable range regardless of its distance/speed definition); crashing before the finish scores nothing. The user's Traffic high score is the **sum** of their per-track bests. Persistence keeps one best per `(user, track_key)` in `traffic_track_scores` plus a mirrored aggregate row in `traffic_high_scores` (`= SUM(track scores)`) so leaderboard queries stay uniform with the other high-score games. `track_key` is the `Track::name`.
- `rubiks_cube/` is a daily deterministic puzzle game with a real cube state, face turns, a three-face angled render, and a compact net. It records one daily win per user/date, publishes Activity for the once-per-day base chip payout and Hub quest progress, and counts toward Arcade Wins. The in-progress cube persists per user in `rubiks_cube_games` (54-char sticker string + move count, saved fire-and-forget on every move/reset; rows from an older date are ignored on load since the daily scramble is deterministic).
- `sudoku/`, `nonogram/`, `minesweeper/`, `solitaire/`, `le_word/`, `rubiks_cube/`, and `sliding_puzzle/` are daily puzzle games. Le Word has a single global daily word rather than personal runs. Rubik's Cube has no personal mode. Sliding Puzzle pairs each deterministic UTC-daily difficulty with one saved random personal board.
- `workspace.rs` owns the Arcade leg of the backtick workspace cycle: the `ArcadeStop` closed enum (the seven daily puzzle games in lobby order), `unfinished_daily_stops` (today's daily boards with at least one player move and no win — each game state exposes `first_unfinished_daily()` / `has_unfinished_daily()`), and `open_stop` (points the Arcade at the right daily board and sets `is_playing_game`). Real-time games and personal boards never join; `lobby/workspace.rs` consumes this module.

Per-game directories generally follow:
- `state.rs`: local per-session game state and pure rules.
- `input.rs`: key routing for that game.
- `ui.rs`: ratatui drawing for that game.
- `svc.rs`: DB-backed persistence/high-score/daily-win tasks.

## Lifecycle

- `late-ssh/src/main.rs` creates the Arcade services: 2048, Lateris, Snake, Sudoku, Nonogram, Solitaire, Minesweeper, Le Word, Rubik's Cube, and Sliding Puzzle. It also creates the shared `games::chips::svc::ChipService`. Hub creates the shared leaderboard refresh service.
- `late-ssh/src/session_bootstrap.rs` and `late-ssh/src/ssh.rs` load saved per-user game rows/high scores before `App::new`.
- `App::new` in `late-ssh/src/app/state.rs` builds one per-session state object per Arcade game.
- `App::tick` advances active real-time games only while `screen == Screen::Arcade && is_playing_game`.
- `App::render` builds `arcade::ui::ArcadeHubView` and calls `draw_arcade_hub`.
- Global input routes `Screen::Arcade` to `arcade::input`; active games suppress many global single-byte shortcuts until they return to the lobby.

## Navigation

- The top-level screen is `Screen::Arcade`, key `2`, rendered as `The Arcade`.
- `Tab` / `Shift+Tab` cycle Clubhouse -> Home -> Arcade -> Games -> Artboard -> Directory -> Leaderboards. The door games are reached from the Games hub, not the tab cycle.
- Lobby order is defined in `arcade/input.rs` as `LOBBY_GAME_ORDER`; keep it in sync with `arcade/ui.rs` render order.
- `j/k` and up/down arrows move through the lobby.
- `Enter` launches the selected available game and sets `is_playing_game = true`.
- Nonograms are only launchable when `nonogram_state.has_puzzles()` is true; otherwise the lobby card is present but treated as unavailable/coming soon.
- `Esc`, `q`, or `Q` leaves an active Arcade game and returns to the lobby. Snake persists progress before leaving.
- Backtick inside an active daily puzzle game hops the workspace cycle (`lobby/workspace.rs` via `arcade/workspace.rs`); real-time games keep the byte. Hopping out clears `is_playing_game`; daily boards save move-by-move, so nothing else is flushed.

## Game Categories

| Category | Games | Persistence | Leaderboard |
| --- | --- | --- | --- |
| High-score | 2048, Lateris, Snake | One current run plus best score plus final score events | Monthly and all-time high scores in Hub |
| High-score (multi-track) | Traffic | One best per track (`traffic_track_scores`) plus aggregate sum (`traffic_high_scores`) plus final score events | Monthly and all-time Traffic high scores in Hub |
| Daily puzzles | Sudoku, Nonograms, Minesweeper, Solitaire, Le Word, Rubik's Cube, Sliding Puzzle | One daily and one personal slot per user/difficulty or pack, except Le Word's global daily answer and Rubik's shared daily scramble | Daily completion status / Arcade Wins in Hub, plus Hub Quests via Activity |
| Economy support | Chips | `user_chips` plus `chip_ledger` | Monthly chip earners in Hub |

Asterion, Blackjack, Chess, Poker, ssHattrick, Tic-Tac-Toe, and Tron are Rooms games, not Arcade games. Cards are shared by Solitaire/Blackjack/Poker; chips are shared by Arcade rewards and room-game payouts/settlements. Keep room runtimes, traits, registry wiring, and UI under `rooms/`.

## Adding A New Arcade Game

Decide the category first. High-score games behave like `tetris/`, `twenty_forty_eight/`, and `snake/`: one saved run, one all-time high-score row, and final score events for monthly Hub boards. Daily/personal puzzle games behave like `sudoku/`, `nonogram/`, `minesweeper/`, and `solitaire/`: one daily puzzle plus optional personal runs, daily win records, chip bonus, and Activity event. Le Word and Rubik's Cube are the same daily-win/reward/activity pattern with shared UTC puzzles and no personal mode.

Expected source shape:
- `late-ssh/src/app/arcade/<game>/mod.rs` declares only local modules.
- `state.rs` owns per-session state and pure rules.
- `input.rs` owns key routing for that game.
- `ui.rs` renders the game and its local help/status panel.
- `svc.rs` owns async tasks and calls `late-core` model APIs. Keep SQL in `late-core`, not in `late-ssh`.

Core model/persistence work:
- Add `late-core/src/models/<game>.rs` for DB-backed state/high-score/win models.
- Add a migration under `late-core/migrations/`.
- Add the model module to `late-core/src/models/mod.rs`.
- For high-score games, expose `HighScore::update_score_if_higher` and `HighScore::record_score_event`.
- For daily games, follow the existing daily-win model pattern and keep one completion fact per user/date/difficulty or pack.

Arcade wiring checklist:
- Add `pub mod <game>;` to `arcade/mod.rs`.
- Create the service in `late-ssh/src/main.rs` and store it in `late-ssh/src/state.rs`.
- Load saved state/high score in `session_bootstrap.rs` and `ssh.rs` if the game has persisted per-user state.
- Add per-session state to `App` in `app/state.rs`.
- Advance realtime state in `app/tick.rs` only when needed.
- Add lobby ordering/launch handling in `arcade/input.rs`.
- Add lobby card/rendering and active-game dispatch in `arcade/ui.rs`.
- Add help-modal copy in `app/help_modal/data.rs` when the game has user-facing controls.
- Update `CONTEXT.md` and this file if the game changes Arcade categories, service ownership, or leaderboard semantics.

Leaderboard/Hub checklist:
- High-score games must write final score events through a `late-core` model method so monthly boards do not depend only on legacy high-score table `updated` timestamps. Lateris and Snake also publish hidden quest Activity score events on final score submission; Snake includes the reached level for weekly/daily quest matching.
- A restored run that was already over must not emit its final score event again (it fired when the run ended). A second emission is a fresh `GameScored` activity today, which completes score-based daily quests for a game nobody played; `snake/state.rs` carries `score_event_recorded` across `restore` for this.
- Add the game to the matching roster in `late-core/src/models/leaderboard.rs`: a `DailyPuzzle` variant enrolls it in the per-game win boards, Arcade Wins points, today's champions, and daily statuses at once; a `ScoreGame` variant enrolls its monthly/all-time score boards. The compiler walks you through the per-variant facts, and the Leaderboards page (`app/leaderboard/`) picks the board up from the roster with no page change.

Testing guidance:
- Pure rules and key-routing helpers get inline unit tests in `state.rs` or `input.rs`.
- DB/service coverage lives in the adjacent `svc_test.rs` beside each game's `svc.rs` (wired with `#[cfg(test)] mod svc_test;`), using `crate::test_helpers::new_test_db`.
- Do not run `cargo test`, `cargo nextest`, or `cargo clippy` as an agent; leave those gates for the human owner.

## Persistence And Services

- High-score services load and save a current run and submit best scores.
- High-score services keep SQL inside `late-core` models. `late-ssh` services call model methods such as `HighScore::update_score_if_higher` and `HighScore::record_score_event`; do not insert score-event SQL directly from Arcade services.
- Daily puzzle services store board progress by `(user_id, difficulty_key, mode)`.
- Daily win tables record one completion fact per user/date/difficulty, separate from board state.
- Le Word stores progress by `(user_id, puzzle_date)` and records daily wins by `(user_id, puzzle_date)` with `difficulty = "daily"` in Activity/reward params. Hub derives monthly and all-time solve counts plus each user's longest consecutive-date solve streak from those win rows.
- Rubik's Cube stores daily wins by `(user_id, puzzle_date)` with `difficulty = "daily"` in Activity/reward params. The in-progress cube persists in `rubiks_cube_games` (one row per user, upserted on every move/reset); a row whose `puzzle_date` isn't today is ignored on load and the deterministic daily scramble is applied instead.
- Sliding Puzzle stores daily and personal progress rows per `(user_id, difficulty_key, mode)` in `sliding_puzzle_games`, saving every legal blank move and reset. Easy/medium/hard map to 3x3/4x4/5x5 boards (including the blank), generated by legal blank moves; daily seeds derive from UTC date plus difficulty and personal seeds are random. A restored row must be a permutation the slides can actually reach, so stale or unreachable daily rows regenerate and unreachable personal rows are rescrambled on activation. A daily win only records when the completed row carries that date's derived seed, the canonical solved board, and at least one move. Daily wins are unique per `(user_id, puzzle_date, difficulty_key)` and publish the move count as the Activity score; personal solves only persist the solved board and never emit Activity, rewards, quest progress, or Arcade Wins.
- `ChipService::ensure_chips(user_id)` creates new chip rows with 1000 chips.
- Generic chip balance mutations in `late-core/src/models/chips.rs` notify `chip_user_changed` with the affected `user_id`; Hub Shop listens to that channel to refresh active balance snapshots.
- Daily puzzle services record the persisted win and publish `ActivityEvent::GameWon`; `ChipService`'s activity reward task awards the corresponding daily puzzle base chips from `reward_templates` and records the once-per-UTC-day claim in `game_payout_claims`.
- Daily services call `record_win_task()` on completion. That records the daily win, grants chips, and publishes a structured Activity event with the difficulty key in `detail` so Hub Dailies quests can match goals such as "win medium Sudoku".
- `leaderboard::svc::LeaderboardService` refreshes from DB every 5 minutes while subscribed. Immediate win callouts come from Activity; the Leaderboards page lags until the next refresh.

## Nonogram Runtime

Nonograms are runtime-only inside `late-ssh`; puzzle generation is offline.

- `late-core/src/bin/gen_nonograms.rs` generates JSON packs and validates candidates with `number-loom`.
- `late-core/src/nonogram.rs` owns the shared JSON schema, clue derivation, pack validation, and deterministic daily selection.
- Assets live in `late-ssh/assets/nonograms/` as `index.json` plus one pack file per size.
- `arcade/nonogram/state.rs` loads assets at server startup through `include_bytes!`.
- SSH sessions never generate nonograms on demand.
- Runtime stores one `daily` and one `personal` slot per user and difficulty key (`easy`, `medium`, `hard`). Embedded packs still use size keys for asset lookup.

## Rendering

- `arcade/ui.rs` renders the lobby game list and delegates active games to their `ui.rs`.
- The lobby has no ASCII banner (dropped 2026-08: with the quest strip on top, small terminals had no headroom left) and auto-scrolls the selected entry near the top third of the viewport.
- `draw_game_frame`, `draw_game_overlay`, `centered_rect`, `status_line`, `keys_line`, and `tip_line` are Arcade-only helpers used by Arcade games.
- Daily puzzle QoL feedback is local to each game UI: Sudoku user-entered values render red only when they duplicate the same number in their row, column, or 3x3 box; Nonogram clue labels render green when the current filled runs satisfy that row/column clue and red when current fills/X marks make that row/column impossible, with the active row/column emphasized through clue text only; Minesweeper flags render green/red after game over based on whether they mark real mines and hidden cells that would open from a currently valid chord are highlighted.
- The old profile-controlled Arcade sidebar preference has been removed. Arcade game bottom status/key bars render unconditionally. Room-game sidebar helpers live in `rooms/game_ui.rs`.

## Keybindings

Root context keeps only global Arcade shortcuts. Keep detailed per-game control copy in each game's `ui.rs` info panel and in help modal copy.

Current per-game basics:
- 2048: `h/j/k/l` or arrows move, `r` restarts after game over.
- Lateris: left/right move, down soft-drops, up rotates, `Space` hard-drops, `p` pauses, `r` restarts.
- Snake: arrows or `h/j/k/l` steer, `p` pauses, `r` restarts.
- Sudoku: arrows or `h/j/k/l` move, `1-9` fill, `0`/Backspace clear, `d/p/n` daily/personal/new, `[`/`]` difficulty.
- Nonograms: arrows or `h/j/k/l` move, `Space`/`x` toggle, `0`/Backspace/`c` clear, `d/p/n` daily/personal/new, `[`/`]` difficulty.
- Minesweeper: arrows or `h/j/k/l` move, reveal/flag/chord controls live in the game info panel.
- Solitaire: card/tableau/foundation controls live in the game info panel; mouse support maps left-click to select/place/draw stock, right-click to auto-move the clicked card, and wheel events over the board to tableau scroll.
- Le Word: type `a-z`, `Enter` submits, Backspace deletes, and `!` opens rules.
- Rubik's Cube: everyone gets the same UTC daily scramble; `u/d/l/r/f/b` turns faces clockwise, uppercase turns inverse, `s`/`0` resets today's scramble, `v` rotates the view right, and arrows rotate the view in their own directions.
- Sliding Puzzle: arrows or `h/j/k/l` slide an adjacent tile in the indicated direction into the gap; left-clicking an adjacent tile moves it directly. `d`/`p` switch daily/personal mode, `n` twice starts a new personal board, `[`/`]` changes difficulty, and `r`/`0` twice restores the active scramble. Personal boards persist but have no reward.

Destructive daily/personal puzzle reset keys use a local confirmation flag. Sudoku (`n`/`r`), Minesweeper (`n`), Solitaire (`n`/`r`), Rubik's Cube (`s`/`0`), and Sliding Puzzle (`n`/`r`/`0`) set `reset_pending` on the first press and reset only on a repeated matching action key. Any ordinary movement, mode/difficulty switch, board edit, card action, face turn, or view change clears the pending flag. Renderers surface a short "press again" tip while pending.

## Tests

- Pure state/input/render helper tests stay inline in `src/app/arcade/**`.
- DB/service tests live in adjacent `svc_test.rs` files beside each game's `svc.rs`, using `crate::test_helpers::new_test_db` (chip payout tests sit at `src/app/games/chips/svc_test.rs`).
- Root test policy still applies: agents do not run `cargo test`, `cargo nextest`, or `cargo clippy`.
- App flow tests in `src/app/*_test.rs` may assert global Arcade navigation and render copy.

## Known Gaps

- Leaderboard refresh is polling-based, so Activity and the Leaderboards page can briefly disagree.
- Nonogram generation remains an offline maintainer task; runtime has no fallback generator.
- Some high-score game state is still per-user single-slot rather than multi-run history.
- Arcade and Rooms share chips/cards through `app/games`, but have separate runtime and UI ownership; keep those boundaries explicit when adding casino or multiplayer features.
