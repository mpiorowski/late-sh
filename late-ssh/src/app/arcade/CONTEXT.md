# Arcade Context

## Metadata
- Scope: `late-ssh/src/app/arcade`
- Last updated: 2026-05-11
- Purpose: local working context for The Arcade screen, single-player terminal games, shared card/chip helpers, and Arcade leaderboard surfaces.
- Parent context: `../../../../CONTEXT.md`

## Scope

`late-ssh/src/app/arcade` owns the SSH Arcade domain: lobby navigation, single-player game state/input/rendering, persisted progress, daily puzzle completions, high scores, chip rewards, and leaderboard refresh support.

Rooms/table games are separate and live under `late-ssh/src/app/rooms`, but they intentionally reuse shared Arcade support modules:
- `cards.rs` for card ranks/suits/rendering.
- `chips/svc.rs` for Late Chips balances, stipends, debits, payouts, floors, and daily bonuses.
- `ui.rs` for shared framed game drawing helpers.

Keep `mod.rs` declaration-only. Do not add `pub use` re-export layers.

## Source Map

- `mod.rs` declares Arcade modules.
- `input.rs` routes The Arcade lobby and selected active game input.
- `ui.rs` renders the lobby and exposes shared frame/sidebar/status helpers.
- `cards.rs` defines shared card primitives used by Solitaire and room card games.
- `chips/svc.rs` owns the Late Chips economy service.
- `leaderboard/svc.rs` owns the shared `watch<Arc<LeaderboardData>>` refresh service.
- `twenty_forty_eight/`, `tetris/`, and `snake/` are high-score games.
- `sudoku/`, `nonogram/`, `minesweeper/`, and `solitaire/` are daily/personal puzzle games.

Per-game directories generally follow:
- `state.rs`: local per-session game state and pure rules.
- `input.rs`: key routing for that game.
- `ui.rs`: ratatui drawing for that game.
- `svc.rs`: DB-backed persistence/high-score/daily-win tasks.

## Lifecycle

- `late-ssh/src/main.rs` creates the Arcade services: 2048, Tetris, Snake, Sudoku, Nonogram, Solitaire, Minesweeper, Chips, and Leaderboard.
- `late-ssh/src/session_bootstrap.rs` and `late-ssh/src/ssh.rs` load saved per-user game rows/high scores before `App::new`.
- `App::new` in `late-ssh/src/app/state.rs` builds one per-session state object per Arcade game.
- `App::tick` advances active real-time games only while `screen == Screen::Arcade && is_playing_game`.
- `App::render` builds `arcade::ui::ArcadeHubView` and calls `draw_arcade_hub`.
- Global input routes `Screen::Arcade` to `arcade::input`; active games suppress many global single-byte shortcuts until they return to the lobby.

## Navigation

- The top-level screen is `Screen::Arcade`, key `3`, rendered as `The Arcade`.
- `Tab` / `Shift+Tab` cycle through Dashboard -> Chat -> Arcade -> Rooms -> Artboard.
- Lobby order is defined in `arcade/input.rs` as `LOBBY_GAME_ORDER`; keep it in sync with `arcade/ui.rs` render order.
- `j/k` and up/down arrows move through the lobby.
- `Enter` launches the selected available game and sets `is_playing_game = true`.
- `Esc`, `q`, or `Q` leaves an active Arcade game and returns to the lobby. Snake persists progress before leaving.
- Backtick from an active Arcade game records `DashboardGameToggleTarget::Arcade` and returns to Dashboard; Dashboard can return to the last Arcade target.

## Game Categories

| Category | Games | Persistence | Leaderboard |
| --- | --- | --- | --- |
| High-score | 2048, Tetris, Snake | One current run plus best score | All-time high scores |
| Daily puzzles | Sudoku, Nonograms, Minesweeper, Solitaire | One daily and one personal slot per user/difficulty or pack | Today's champions and streaks |
| Economy support | Chips | `user_chips` | Chip leaders |

Blackjack, Poker, and Tic-Tac-Toe are Rooms games, not Arcade games, even though they share chips/cards/activity concepts.

## Persistence And Services

- High-score services load and save a current run and submit best scores.
- Daily puzzle services store board progress by `(user_id, difficulty_key or size_key, mode)`.
- Daily win tables record one completion fact per user/date/difficulty or pack, separate from board state.
- `ChipService::ensure_chips(user_id)` grants the daily 500-chip stipend on login.
- `ChipService::grant_daily_bonus_task(user_id, difficulty_key)` awards 50/100/150 chips for daily puzzle completions.
- Daily services call `record_win_task()` on completion. That records the daily win, grants chips, and publishes a structured Activity event.
- `LeaderboardService` refreshes from DB every 30s. Immediate win callouts come from Activity; leaderboard surfaces lag until the next refresh.
- Streak SQL uses gaps-and-islands across daily win rows. A streak remains current when its last day is today or yesterday.

## Nonogram Runtime

Nonograms are runtime-only inside `late-ssh`; puzzle generation is offline.

- `late-core/src/bin/gen_nonograms.rs` generates JSON packs and validates candidates with `number-loom`.
- `late-core/src/nonogram.rs` owns the shared JSON schema, clue derivation, pack validation, and deterministic daily selection.
- Assets live in `late-ssh/assets/nonograms/` as `index.json` plus one pack file per size.
- `arcade/nonogram/state.rs` loads assets at server startup through `include_bytes!`.
- SSH sessions never generate nonograms on demand.
- Runtime stores one `daily` and one `personal` slot per user and `size_key`.

## Rendering

- `arcade/ui.rs` renders the lobby header/list and delegates active games to their `ui.rs`.
- The lobby hides the ASCII header when the terminal is short and auto-scrolls the selected entry near the top third of the viewport.
- `draw_game_frame`, `draw_game_frame_with_info_sidebar`, `status_line`, `keys_line`, `tip_line`, and `info_label_value` are shared helpers used by Arcade games and some non-Arcade surfaces.
- The Arcade sidebar is controlled by profile setting `show_arcade_sidebar`; the reader still accepts the legacy `show_games_sidebar` key.

## Keybindings

Root context keeps only global Arcade shortcuts. Keep detailed per-game control copy in each game's `ui.rs` info panel and in help modal copy.

Current per-game basics:
- 2048: `h/j/k/l` or arrows move, `r` restarts after game over.
- Tetris: left/right move, down soft-drops, up rotates, `Space` hard-drops, `p` pauses, `r` restarts.
- Snake: arrows or `h/j/k/l` steer, `p` pauses, `r` restarts.
- Sudoku: arrows or `h/j/k/l` move, `1-9` fill, `0`/Backspace clear, `d/p/n` daily/personal/new, `[`/`]` difficulty.
- Nonograms: arrows or `h/j/k/l` move, `Space`/`x` toggle, `0`/Backspace/`c` clear, `d/p/n` daily/personal/new, `[`/`]` size pack.
- Minesweeper: arrows or `h/j/k/l` move, reveal/flag/chord controls live in the game info panel.
- Solitaire: card/tableau/foundation controls live in the game info panel.

## Tests

- Pure state/input/render helper tests stay inline in `src/app/arcade/**`.
- DB/service tests live under `late-ssh/tests/arcade/` and must use shared testcontainers helpers.
- Root test policy still applies: agents do not run `cargo test`, `cargo nextest`, or `cargo clippy`.
- App flow tests outside `tests/arcade/` may assert global Arcade navigation and render copy.

## Known Gaps

- Leaderboard refresh is polling-based, so Activity and leaderboard surfaces can briefly disagree.
- Nonogram generation remains an offline maintainer task; runtime has no fallback generator.
- Some high-score game state is still per-user single-slot rather than multi-run history.
- Arcade and Rooms share chips/cards but have separate runtime ownership; keep those boundaries explicit when adding casino or multiplayer features.
