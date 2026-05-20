# Arcade Context

## Metadata
- Scope: `late-ssh/src/app/arcade`
- Last updated: 2026-05-14
- Purpose: local working context for The Arcade screen, single-player terminal games, and shared card/chip helpers.
- Parent context: `../../../../CONTEXT.md`

## Scope

`late-ssh/src/app/arcade` owns the SSH Arcade domain: lobby navigation, single-player game state/input/rendering, persisted progress, daily puzzle completions, high scores, and chip rewards.

Hub/leaderboard surfaces are separate and live under `late-ssh/src/app/hub`. Arcade games submit score and daily-win data; Hub refreshes and renders cross-product leaderboard/economy views from that data.

Rooms/table games are separate and live under `late-ssh/src/app/rooms`, but they intentionally reuse shared Arcade support modules:
- `cards.rs` for card ranks/suits/rendering.
- `chips/svc.rs` for Late Chips balances, initial grants, debits, payouts, floors, and daily bonuses.
- `ui.rs` for shared framed game drawing helpers.

Keep `mod.rs` declaration-only. Do not add `pub use` re-export layers.

## Source Map

- `mod.rs` declares Arcade modules.
- `input.rs` routes The Arcade lobby and selected active game input.
- `ui.rs` renders the lobby and exposes shared frame/sidebar/status helpers.
- `cards.rs` defines shared card primitives used by Solitaire and room card games.
- `chips/svc.rs` owns the Late Chips economy service.
- `twenty_forty_eight/`, `tetris/`, and `snake/` are high-score games.
- `sudoku/`, `nonogram/`, `minesweeper/`, and `solitaire/` are daily/personal puzzle games.

Per-game directories generally follow:
- `state.rs`: local per-session game state and pure rules.
- `input.rs`: key routing for that game.
- `ui.rs`: ratatui drawing for that game.
- `svc.rs`: DB-backed persistence/high-score/daily-win tasks.

## Lifecycle

- `late-ssh/src/main.rs` creates the Arcade services: 2048, Tetris, Snake, Sudoku, Nonogram, Solitaire, Minesweeper, and Chips. Hub creates the shared leaderboard refresh service.
- `late-ssh/src/session_bootstrap.rs` and `late-ssh/src/ssh.rs` load saved per-user game rows/high scores before `App::new`.
- `App::new` in `late-ssh/src/app/state.rs` builds one per-session state object per Arcade game.
- `App::tick` advances active real-time games only while `screen == Screen::Arcade && is_playing_game`.
- `App::render` builds `arcade::ui::ArcadeHubView` and calls `draw_arcade_hub`.
- Global input routes `Screen::Arcade` to `arcade::input`; active games suppress many global single-byte shortcuts until they return to the lobby.

## Navigation

- The top-level screen is `Screen::Arcade`, key `2`, rendered as `The Arcade`.
- `Tab` / `Shift+Tab` cycle through Dashboard/Home -> Arcade -> Rooms -> Artboard.
- Lobby order is defined in `arcade/input.rs` as `LOBBY_GAME_ORDER`; keep it in sync with `arcade/ui.rs` render order.
- `j/k` and up/down arrows move through the lobby.
- `Enter` launches the selected available game and sets `is_playing_game = true`.
- Nonograms are only launchable when `nonogram_state.has_puzzles()` is true; otherwise the lobby card is present but treated as unavailable/coming soon.
- `Esc`, `q`, or `Q` leaves an active Arcade game and returns to the lobby. Snake persists progress before leaving.
- Backtick from an active Arcade game records `DashboardGameToggleTarget::Arcade` and returns to Dashboard; Dashboard can return to the last Arcade target.

## Game Categories

| Category | Games | Persistence | Leaderboard |
| --- | --- | --- | --- |
| High-score | 2048, Tetris, Snake | One current run plus best score plus final score events | Monthly and all-time high scores in Hub |
| Daily puzzles | Sudoku, Nonograms, Minesweeper, Solitaire | One daily and one personal slot per user/difficulty or pack | Daily completion status / Arcade Wins in Hub |
| Economy support | Chips | `user_chips` plus `chip_ledger` | Monthly chip earners in Hub |

Blackjack, Poker, and Tic-Tac-Toe are Rooms games, not Arcade games, even though they share chips/cards/activity concepts.

## Adding A New Arcade Game

Decide the category first. High-score games behave like `tetris/`, `twenty_forty_eight/`, and `snake/`: one saved run, one all-time high-score row, and final score events for monthly Hub boards. Daily/personal puzzle games behave like `sudoku/`, `nonogram/`, `minesweeper/`, and `solitaire/`: one daily puzzle plus optional personal runs, daily win records, chip bonus, and Activity event.

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
- High-score games must write final score events through a `late-core` model method so monthly Hub boards do not depend only on legacy high-score table `updated` timestamps.
- Add the monthly score board fetch in `late-core/src/models/leaderboard.rs`.
- Add the all-time high-score fetch if the aggregate `high_scores` list should include the game.
- Render the new board in `app/hub/leaderboard.rs` only if it belongs in the compact Hub view. Do not put Hub UI under `arcade/`.

Testing guidance:
- Pure rules and key-routing helpers get inline unit tests in `state.rs` or `input.rs`.
- DB/service coverage belongs under `late-ssh/tests/arcade/` and must use the shared testcontainers helpers.
- Do not run `cargo test`, `cargo nextest`, or `cargo clippy` as an agent; leave those gates for the human owner.

## Persistence And Services

- High-score services load and save a current run and submit best scores.
- High-score services keep SQL inside `late-core` models. `late-ssh` services call model methods such as `HighScore::update_score_if_higher` and `HighScore::record_score_event`; do not insert score-event SQL directly from Arcade services.
- Daily puzzle services store board progress by `(user_id, difficulty_key, mode)`.
- Daily win tables record one completion fact per user/date/difficulty, separate from board state.
- `ChipService::ensure_chips(user_id)` creates new chip rows with 1000 chips.
- `ChipService::grant_daily_bonus_task(user_id, difficulty_key)` awards 50/100/150 chips for daily puzzle completions.
- Daily services call `record_win_task()` on completion. That records the daily win, grants chips, and publishes a structured Activity event.
- `hub::svc::LeaderboardService` refreshes from DB every 30s. Immediate win callouts come from Activity; Hub leaderboard surfaces lag until the next refresh.

## Nonogram Runtime

Nonograms are runtime-only inside `late-ssh`; puzzle generation is offline.

- `late-core/src/bin/gen_nonograms.rs` generates JSON packs and validates candidates with `number-loom`.
- `late-core/src/nonogram.rs` owns the shared JSON schema, clue derivation, pack validation, and deterministic daily selection.
- Assets live in `late-ssh/assets/nonograms/` as `index.json` plus one pack file per size.
- `arcade/nonogram/state.rs` loads assets at server startup through `include_bytes!`.
- SSH sessions never generate nonograms on demand.
- Runtime stores one `daily` and one `personal` slot per user and difficulty key (`easy`, `medium`, `hard`). Embedded packs still use size keys for asset lookup.

## Rendering

- `arcade/ui.rs` renders the lobby header/list and delegates active games to their `ui.rs`.
- The lobby hides the ASCII header when the terminal is short and auto-scrolls the selected entry near the top third of the viewport.
- `draw_game_frame`, `draw_game_frame_with_info_sidebar`, `draw_game_overlay`, `centered_rect`, `status_line`, `keys_line`, `tip_line`, `key_hint`, `info_label_value`, and `info_tagline` are shared helpers used by Arcade games and some non-Arcade surfaces.
- The old profile-controlled Arcade sidebar preference has been removed. Arcade game bottom status/key bars render unconditionally; `draw_game_frame_with_info_sidebar` remains a shared helper for non-Arcade room-game surfaces such as Blackjack.

## Keybindings

Root context keeps only global Arcade shortcuts. Keep detailed per-game control copy in each game's `ui.rs` info panel and in help modal copy.

Current per-game basics:
- 2048: `h/j/k/l` or arrows move, `r` restarts after game over.
- Tetris: left/right move, down soft-drops, up rotates, `Space` hard-drops, `p` pauses, `r` restarts.
- Snake: arrows or `h/j/k/l` steer, `p` pauses, `r` restarts.
- Sudoku: arrows or `h/j/k/l` move, `1-9` fill, `0`/Backspace clear, `d/p/n` daily/personal/new, `[`/`]` difficulty.
- Nonograms: arrows or `h/j/k/l` move, `Space`/`x` toggle, `0`/Backspace/`c` clear, `d/p/n` daily/personal/new, `[`/`]` difficulty.
- Minesweeper: arrows or `h/j/k/l` move, reveal/flag/chord controls live in the game info panel.
- Solitaire: card/tableau/foundation controls live in the game info panel.

## Tests

- Pure state/input/render helper tests stay inline in `src/app/arcade/**`.
- DB/service tests live under `late-ssh/tests/arcade/` and must use shared testcontainers helpers.
- Root test policy still applies: agents do not run `cargo test`, `cargo nextest`, or `cargo clippy`.
- App flow tests outside `tests/arcade/` may assert global Arcade navigation and render copy.

## Known Gaps

- Hub leaderboard refresh is polling-based, so Activity and leaderboard surfaces can briefly disagree.
- Nonogram generation remains an offline maintainer task; runtime has no fallback generator.
- Some high-score game state is still per-user single-slot rather than multi-run history.
- Arcade and Rooms share chips/cards but have separate runtime ownership; keep those boundaries explicit when adding casino or multiplayer features.
