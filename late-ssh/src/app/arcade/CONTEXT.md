# Arcade Context

## Metadata
- Scope: `late-ssh/src/app/arcade`
- Purpose: local working context for The Arcade screen and single-player terminal games.
- Parent context: `../../../../CONTEXT.md`

## Scope

`late-ssh/src/app/arcade` owns the SSH Arcade domain: lobby navigation, single-player game state/input/rendering, persisted progress, daily puzzle completions, high scores, and chip rewards.

The Leaderboards page (`late-ssh/src/app/leaderboard`) and the Hub's quests and Shop (`late-ssh/src/app/hub`) are separate. Arcade games submit score and daily-win data; the leaderboard service refreshes and renders the boards from that data. The falling-block game is user-facing `Lateris`; lowercase `tetris` remains the internal compatibility key/table/module namespace for existing saved games, score events, quests, and award categories.

Shared game-domain primitives live under `late-ssh/src/app/games`:
- `games/cards.rs` for card ranks/suits/rendering used by Solitaire and room card games.
- `games/chips/svc.rs` for Late Chips balances, initial grants, debits, payouts, floors, and daily bonuses.

Multiplayer table games are separate and live under `late-ssh/src/app/lobby`. Do not make the Lobby depend on Arcade modules for shared game behavior.

Keep `mod.rs` declaration-only. Do not add `pub use` re-export layers.

## Source Map

- `mod.rs` declares Arcade modules.
- `input.rs` routes The Arcade lobby and selected active game input.
- `ui.rs` renders the lobby and exposes Arcade-only bottom-bar/status helpers. The lobby carves `hub::dailies::ui::arcade_strip_height(quest_state)` rows off the top for the quest strip (streak-meter heading plus grouped Daily/Weekly sections, from `QuestState`) whenever the area leaves at least 13 rows for the game list below it; active games never show it.
- `twenty_forty_eight/`, `tetris/`, `snake/` and the multi-track `traffic/` are high-score games; `sudoku/`, `nonogram/`, `minesweeper/`, `solitaire/`, `le_word/`, `rubiks_cube/` and `sliding_puzzle/` are daily puzzles. What is particular to each (Snake's per-palette glyph contrast, Traffic's graded per-track sum, the cube's persisted state, Sliding Puzzle's gallery art, Solitaire's win cascade) is in [`GAMES.md`](GAMES.md).
- `share.rs` owns the share-card grammar (`ShareCard`, the closed `Glyph` set, `puzzle_number` counted from `ARCADE_EPOCH`: never move it, every pasted card would renumber); each daily has a pure `share.rs` beside its `state.rs`. The cards per game are in `GAMES.md`.
- **Daily rollover inside a live session.** Each daily game builds its boards once in `State::new`, which only reruns on connect, so without a re-check a client left running past UTC midnight would keep yesterday's puzzles while the quest strip above them had already rolled. Every daily state exposes `ensure_current_daily() -> bool`, comparing a stored `daily_date`/`puzzle_date` against `svc.today()` and rebuilding its dailies when they differ; `workspace::refresh_daily_games` calls all seven and `tick.rs` runs it on the shared 1Hz edge. It is skipped only while the player is looking at a board (`Screen::Arcade` + `is_playing_game`) so a puzzle is never swapped out mid-move; looking away rolls it on the next tick. The `is_playing_game` flag alone is not enough: it stays set when a board is left open behind a page switch. Sudoku regenerates through its existing off-thread `daily_generation_rx` channel (the screen shows its loading state until the boards land) and Le Word refetches its global word through a `oneshot` drained by `poll_word_reload`; the other five generate from a date-derived seed synchronously. Le Word's `puzzle_date` only advances once the word actually lands, so a failed fetch retries after a 30s backoff instead of leaving the board dead until reconnect.
- **Daily saves and wins are stamped with the board's own date, never the wall clock.** Minesweeper, Sudoku, Nonogram, and Solitaire pass `daily_date`, Rubik's Cube and Sliding Puzzle their `puzzle_date`, into `save_game_task`/`record_win_task` (Sliding Puzzle also skips the keypress-time `ensure_current_daily` Rubik's does in `arcade/input.rs`, so its open board is never swapped mid-solve), so a player who crosses UTC midnight mid-board banks the finish under the day the board belongs to; it never counts as today's daily, and its save lands under its own (then ignored) date. `svc.today()` at completion time would be wrong exactly in the window the rollover skip guard protects.
- **Solitaire's win cascade** (`solitaire/win_anim.rs`) is the classic finish: the moment the last card lands, the foundations throw themselves across the board a few cards a second, each one arcing off the floor and leaving its own art behind. It is pure — physics in board cells, card art stamped into a private character canvas — and area-independent: `ui.rs` records the board's drawn rect into `State::win_view` (a `Cell`, a render mirror and never a rule input) every frame, `App::tick` feeds that to `tick_win_animation` on the hot 15fps edge the Arcade already pays while a game is open, and `ui::draw_win_cascade` blits the canvas onto the frame buffer cell by cell so the board shows through the gaps. Foundations draw through `State::displayed_foundation_top`, which hides the cards already in the air. The cascade starts only on the first crossing into a win (a card pulled back off a full foundation and replayed, or the winning move undone and replayed, must not restart it), is cleared by `apply_snapshot` and `undo` so every new deal starts clean, and is not part of a snapshot: a won board reloaded from a save shows the result, not a replay. The `YOU WON!` overlay waits for the last card to land, and any key press or click runs the cascade out at once; other mouse traffic (a scroll, a drifting pointer, a button release) does not, so a mouse resting on the board cannot cut the finish short.
- **Telemetry.** Every daily puzzle's `svc.rs` has `record_finish(mode, difficulty, finish)`, called by its `state.rs` where a board ends (solved, or lost: Le Word out of guesses, Minesweeper out of lives), daily and personal alike, feeding `late_ssh_arcade_finishes_total{game, mode, difficulty, finish}`. Solitaire counts only the first crossing into a win, like its cascade. The `GameWon` Activity event (and `late_ssh_game_wins_total`, recorded in its constructor) stays daily-only and fires only for a fresh `DailyWin` insert: Solitaire's model returns `WinRecord { fresh }` like Sliding Puzzle's, because an undo and a replayed last card would otherwise publish the win again. `ui.rs::game_for_selection` names the open board as the `place` of `late_ssh_attention_seconds_total` and `late_ssh_place_visits_total`, which `tick.rs` feeds on the 1Hz edge.
- `daily.rs` also owns `SessionDailyWins`: the dailies this session banked today, marked in `tick.rs` from the session's own `GameWon` Activity events and OR'd into the lobby card's ✓/✗ tier marks, so a win turns green at once instead of on the next five-minute leaderboard pass. Wins stamped on any other UTC date are ignored, and the overlay empties itself at rollover.
- The Arcade leg of the backtick workspace cycle (the `ArcadeStop` closed enum, `unfinished_daily_stops`, `open_stop`) lives in the workspace domain, `app/workspace/arcade.rs`: it is cycle vocabulary, not an Arcade concept. It reads each game state's `first_unfinished_daily()` / `has_unfinished_daily()` / `is_daily_active()`; real-time games and personal boards never join. See `app/workspace/CONTEXT.md`.

Per-game directories generally follow:
- `state.rs`: local per-session game state and pure rules.
- `input.rs`: key routing for that game.
- `ui.rs`: ratatui drawing for that game.
- `svc.rs`: DB-backed persistence/high-score/daily-win tasks.

## Lifecycle

- `late-ssh/src/main.rs` creates the Arcade services: 2048, Lateris, Snake, Traffic, Sudoku, Nonogram, Solitaire, Minesweeper, Le Word, Rubik's Cube, and Sliding Puzzle. It also creates the shared `games::chips::svc::ChipService` and the `LeaderboardService`.
- `late-ssh/src/session_bootstrap.rs` and `late-ssh/src/ssh.rs` load saved per-user game rows/high scores before `App::new`.
- `App::new` in `late-ssh/src/app/state.rs` builds one per-session state object per Arcade game.
- `App::tick` advances active real-time games only while `screen == Screen::Arcade && is_playing_game`.
- `App::render` builds `arcade::ui::ArcadeHubView` and calls `draw_arcade_hub`.
- Global input routes `Screen::Arcade` to `arcade::input`; active games suppress many global single-byte shortcuts until they return to the lobby. They also own Ctrl+S, so the Shop shortcut only applies on the Arcade menu.

## Navigation

- The top-level screen is `Screen::Arcade`, key `2`, rendered as `The Arcade`.
- `Tab` / `Shift+Tab` cycle Clubhouse -> Home -> Arcade -> Games -> Artboard -> People -> Leaderboards -> Calendars. The door games are reached from the Games hub, not the tab cycle.
- Lobby order is defined in `arcade/input.rs` as `LOBBY_GAME_ORDER`; keep it in sync with `arcade/ui.rs` render order. Daily games come first (Le Word is the default selection), the score games below them.
- `j/k` and up/down arrows move through the lobby.
- `Enter` launches the selected available game and sets `is_playing_game = true`.
- Nonograms are only launchable when `nonogram_state.has_puzzles()` is true; otherwise the lobby card is present but treated as unavailable/coming soon.
- `Esc`, `q`, or `Q` leaves an active Arcade game and returns to the lobby. Snake persists progress before leaving.
- Backtick inside an active daily puzzle game hops the workspace cycle (`app/workspace/cycle.rs` via `app/workspace/arcade.rs`); real-time games keep the byte. Hopping out clears `is_playing_game`; daily boards save move-by-move, so nothing else is flushed.

## Game Categories

| Category | Games | Persistence | Leaderboard |
| --- | --- | --- | --- |
| High-score | 2048, Lateris, Snake | One current run plus best score plus final score events | Monthly and all-time high scores on the Leaderboards page |
| High-score (multi-track) | Traffic | One best per track (`traffic_track_scores`) plus aggregate sum (`traffic_high_scores`) plus final score events | Monthly and all-time Traffic high scores on the Leaderboards page |
| Daily puzzles | Sudoku, Nonograms, Minesweeper, Solitaire, Le Word, Rubik's Cube, Sliding Puzzle | One daily and one personal slot per user/difficulty or pack, except Le Word's global daily answer and Rubik's shared daily scramble | Daily completion status / Arcade Wins on the Leaderboards page, plus Hub quests via Activity |
| Economy support | Chips | `user_chips` plus `chip_ledger` | Top Chips on the Leaderboards page |

Poker, Blackjack, Asterion, Tron and Super Snake are house tables (`app/lobby/house`) and chess and the other correspondence games are dailies (`app/lobby/daily`), not Arcade games. Cards are shared by Solitaire/Blackjack/Poker; chips are shared by Arcade rewards and house-table settlements.

## Adding A New Arcade Game

Decide the category first (high-score or daily puzzle), then follow the source shape, the model and wiring checklists, and the leaderboard roster step in [`GAMES.md`](GAMES.md), "Adding a new Arcade game". Agents run targeted tests through `make test-llm ARGS="-p late-ssh --lib -E 'test(arcade)'"`; `make check` stays with the human owner.

## Persistence And Services

- High-score services load and save a current run and submit best scores.
- High-score services keep SQL inside `late-core` models. `late-ssh` services call model methods such as `HighScore::update_score_if_higher` and `HighScore::record_score_event`; do not insert score-event SQL directly from Arcade services.
- Daily puzzle services store board progress by `(user_id, difficulty_key, mode)`.
- Daily win tables record one completion fact per user/date/difficulty, separate from board state.
- Le Word stores progress by `(user_id, puzzle_date)` and records daily wins by `(user_id, puzzle_date)` with `difficulty = "daily"` in Activity/reward params. The Leaderboards page derives monthly and all-time solve counts from those win rows.
- Rubik's Cube stores daily wins by `(user_id, puzzle_date)` with `difficulty = "daily"` in Activity/reward params. The in-progress cube persists in `rubiks_cube_games` (one row per user, upserted on every move/reset); a row whose `puzzle_date` isn't today is ignored on load and the deterministic daily scramble is applied instead.
- Sliding Puzzle stores daily and personal progress rows per `(user_id, difficulty_key, mode)` in `sliding_puzzle_games`, saving every legal blank move and reset through one process-wide FIFO worker; `load_games` waits at most `LOAD_FLUSH_TIMEOUT` (2s) for that queue before reading the database, so one login never sits behind other players' moves. Easy/medium/hard map to 3x3/4x4/5x5 boards (including the blank), generated by legal blank moves; daily seeds derive from UTC date plus difficulty and personal seeds are random. A restored row must be a permutation the slides can actually reach, so stale or unreachable daily rows regenerate and unreachable personal rows are rescrambled on activation. The service trusts the state's finishing move; the table's own gates (`moves > 0`, one win per user/date/difficulty) are the checks that remain. Daily wins are unique per `(user_id, puzzle_date, difficulty_key)` and publish the move count as the Activity score; personal solves only persist the solved board and never emit Activity, rewards, quest progress, or Arcade Wins.
- `ChipService::ensure_chips(user_id)` creates new chip rows with 1000 chips.
- Generic chip balance mutations in `late-core/src/models/chips.rs` notify `chip_user_changed` with the affected `user_id`; Hub Shop listens to that channel to refresh active balance snapshots.
- Daily puzzle services record the persisted win and publish `ActivityEvent::GameWon`; `ChipService`'s activity reward task awards the corresponding daily puzzle base chips from `reward_templates` and records the once-per-UTC-day claim in `game_payout_claims`.
- Daily services call `record_win_task()` on completion. That records the daily win, grants chips, and publishes a structured Activity event with the difficulty key in `detail` so Hub Dailies quests can match goals such as "win medium Sudoku".
- `leaderboard::svc::LeaderboardService` refreshes from DB every 5 minutes while subscribed. Immediate win callouts come from Activity; the Leaderboards page lags until the next refresh.

## Nonogram Runtime

Nonograms are runtime-only inside `late-ssh`; puzzle generation is offline.

- `late-core/src/bin/gen_nonograms.rs` generates JSON packs (`cargo run -p late-core --release --bin gen_nonograms` rewrites `late-ssh/assets/nonograms/`). A candidate ships only when line logic alone finishes it (`nonogram::solve_by_lines`): no shipped puzzle needs a guess, and every one has exactly one answer. `arcade/nonogram/state_test.rs::every_shipped_puzzle_solves_by_line_logic` pins the shipped packs.
- Puzzle ids are `<size>-<digest of the solution>`. Saved games find their puzzle by id, so a regenerated pack never hands an old id to a different picture: a save whose puzzle left the pack does not load, and that slot starts a fresh puzzle. A daily win is keyed by date and difficulty, not puzzle, so it is never paid twice.
- `late-core/src/nonogram.rs` owns the shared JSON schema, clue derivation, the line-logic solver, pack validation, and deterministic daily selection. The runtime's red "impossible" clue labels come from the same solver (`line_fits`), with the player's marks mapped to its cells in `arcade/nonogram/state.rs`.
- Assets live in `late-ssh/assets/nonograms/` as `index.json` plus one pack file per size.
- `arcade/nonogram/state.rs` loads assets at server startup through `include_bytes!`.
- SSH sessions never generate nonograms on demand.
- Runtime stores one `daily` and one `personal` slot per user and difficulty key (`easy`, `medium`, `hard`). Embedded packs still use size keys for asset lookup.

## Rendering

- `arcade/ui.rs` renders the lobby game list and delegates active games to their `ui.rs`.
- The lobby has no ASCII banner (with the quest strip on top, small terminals have no headroom for one) and auto-scrolls the selected entry near the top third of the viewport.
- `draw_game_frame`, `draw_game_overlay`, `centered_rect`, `status_line`, `keys_line`, and `tip_line` are Arcade-only helpers used by Arcade games.
- Daily puzzle QoL feedback is local to each game UI: Sudoku user-entered values render red only when they duplicate the same number in their row, column, or 3x3 box; Nonogram clue labels render green when the current filled runs satisfy that row/column clue and red when current fills/X marks make that row/column impossible, with the active row/column emphasized through clue text only; Minesweeper flags render green/red after game over based on whether they mark real mines and hidden cells that would open from a currently valid chord are highlighted.
- Arcade game bottom status/key bars render unconditionally.

## Keybindings

Root context keeps only global Arcade shortcuts; each game's info panel and the help modal carry its controls, and `GAMES.md` lists the basics per game. Destructive daily/personal puzzle reset keys use a local confirmation flag: the first press sets `reset_pending`, only a repeated matching key resets, and any other action clears it.

## Tests

- Pure state/input/render helper tests stay inline in `src/app/arcade/**`.
- DB/service tests live in adjacent `svc_test.rs` files beside each game's `svc.rs`, using `crate::test_helpers::new_test_db` (chip payout tests sit at `src/app/games/chips/svc_test.rs`).
- App flow tests in `src/app/*_test.rs` may assert global Arcade navigation and render copy.

## Known Gaps

- Leaderboard refresh is polling-based, so Activity and the Leaderboards page can briefly disagree.
- Nonogram generation remains an offline maintainer task; runtime has no fallback generator.
- Some high-score game state is still per-user single-slot rather than multi-run history.
- The Arcade and the Lobby (house tables, dailies) share chips and cards through `app/games`, but have separate runtime and UI ownership; keep those boundaries explicit when adding casino or multiplayer features.
