# Arcade games: per-game notes, controls, and adding a game

The Arcade's shape, cross-game invariants (daily rollover, board-date
stamping, telemetry), lifecycle and persistence are in `CONTEXT.md`. This file
holds what is particular to one game, the controls, and the checklist for a
new one.

## High-score games

- `twenty_forty_eight/`, `tetris/`, and `snake/` are high-score games. Snake's board glyph colors live in `ui.rs` (`glyph_color`), resolved per frame from `theme::` tokens and passed through `theme::legible_on_selection`: every cell is painted over `theme::BG_SELECTION()`, and twelve AMOLED palettes reuse an accent as that very fill (`success` on Greenery is byte-identical), so a raw token can render fg == bg. `snake/ui_test.rs` sweeps every palette asserting each glyph clears `theme::MIN_GLYPH_CONTRAST` against the fill. State carries only `ThingKind`; per-frame resolution also means a mid-level theme switch repaints glyphs together with the board.
- `traffic/` is a multi-track high-score game. Each track finish is graded to a normalized `0..=1000` score (`Track::grade_time`, from the track's theoretical fastest/slowest completion time, so every track yields a comparable range regardless of its distance/speed definition); crashing before the finish scores nothing. The user's Traffic high score is the **sum** of their per-track bests. Persistence keeps one best per `(user, track_key)` in `traffic_track_scores` plus a mirrored aggregate row in `traffic_high_scores` (`= SUM(track scores)`) so leaderboard queries stay uniform with the other high-score games. `track_key` is the `Track::name`.

## Daily puzzles

- `sudoku/`, `nonogram/`, `minesweeper/`, `solitaire/`, `le_word/`, `rubiks_cube/`, and `sliding_puzzle/` are daily puzzle games. Le Word has a single global daily word rather than personal runs. Rubik's Cube has one shared daily scramble and no personal mode. Sliding Puzzle pairs each deterministic UTC-daily difficulty with one saved random personal board. Its art is a gallery piece: `sliding_puzzle/art.rs` (pure) lays the day's piece over the board at the piece's own size (`art_grid`: tile = piece ÷ dimension, never under `MIN_ART_TILE_GEOMETRY` 6×3, the piece centred in blank cells, one span per cell, a wide glyph on a tile's last column blanked so no cut tears it) and `tile_fragment` cuts a tile out of it; `svc.rs::load_daily_art_task` claims the day's piece through `ArtboardPiece::feature_for_day` and decodes the canvas off the tick, owning the load's log line and `metrics::record_sliding_puzzle_art(SlidingPuzzleArtLoad)`; `state.rs` holds the `ArtSlot` (unrequested, loading, ready with a grid per difficulty, stale, refreshing, empty, failed with a 30s retry) that `poll_art` walks from `App::tick` while the board is the open screen, reset by the midnight rollover and re-asked by every open from the lobby (`open_daily`), so a mod's `/mod artboard feature` or removal shows without a reconnect; a piece already on the board stays up while the re-ask is in flight (stale, then refreshing), so an open never flashes numbered tiles, and a failed re-ask keeps it. The tip line shows the art status (loading, empty, failed, too wide) except while a two-press confirm is pending, which always wins. `ui.rs::add_art_tile_number` widens its overlay to a whole wide-glyph pair so the row keeps its width. No images, no downloads, no per-session caches worth releasing.
- `rubiks_cube/` is a daily deterministic puzzle game with a real cube state, face turns, a three-face angled render, and a compact net. It records one daily win per user/date, publishes Activity for the once-per-day base chip payout and Hub quest progress, and counts toward Arcade Wins. The in-progress cube persists per user in `rubiks_cube_games` (54-char sticker string + move count, saved fire-and-forget on every move/reset; rows from an older date are ignored on load since the daily scramble is deterministic). Reset (`0`) is a two-press confirm, and is refused outright once the cube is solved: the day's win is already banked, so re-scrambling would just put a finished puzzle back on the board.
- Sliding Puzzle draws its art at the piece's size, so **the tile geometry follows the piece, not the terminal**: `ui::board_layout` takes the art's tile size when the grid fits the board area and the numbered 7×3 grid otherwise, and the draw path and mouse hit-testing both go through it (`hit_test` takes `State::art_tile_geometry`) so a click lands on the tile the frame drew. A piece too big for the terminal says so on the tip line and plays numbered. The credit (`title by @painter`) sits on the row under the grid when there is one; the tip line stays the game's own messages. Faint amber tile numbers overlay the art while unsolved, since sparse ASCII tiles look alike. Tests build pieces from `dartboard_core::Canvas` directly, no network.
- **Solitaire's win cascade** (`solitaire/win_anim.rs`) is the classic finish: the moment the last card lands, the foundations throw themselves across the board a few cards a second, each one arcing off the floor and leaving its own art behind. It is pure (physics in board cells, card art stamped into a private character canvas) and area-independent: `ui.rs` records the board's drawn rect into `State::win_view` (a `Cell`, a render mirror and never a rule input) every frame, `App::tick` feeds that to `tick_win_animation` on the hot 15fps edge the Arcade already pays while a game is open, and `ui::draw_win_cascade` blits the canvas onto the frame buffer cell by cell so the board shows through the gaps. Foundations draw through `State::displayed_foundation_top`, which hides the cards already in the air. The cascade starts only on the first crossing into a win (a card pulled back off a full foundation and replayed, or the winning move undone and replayed, must not restart it), is cleared by `apply_snapshot` and `undo` so every new deal starts clean, and is not part of a snapshot: a won board reloaded from a save shows the result, not a replay. The `YOU WON!` overlay waits for the last card to land, and any key press or click runs the cascade out at once; other mouse traffic (a scroll, a drifting pointer, a button release) does not, so a mouse resting on the board cannot cut the finish short.

## Share cards

- `share.rs` owns the share-card grammar: `ShareCard` (a header, at most eleven body rows, the fixed `ssh late.sh` footer), the closed `Glyph` set with its emoji and ASCII spellings, `render`, `title` (with or without a result), `arrow_row`, `half_block_picture`, `puzzle_number` (one count of days since `ARCADE_EPOCH` shared by every card, so all of a day's cards carry the same number; never move it: every pasted card would renumber), and the day card (one glyph per daily in lobby order, a row of one icon per daily beneath it so a reader can tell the boxes apart, plus the quest streak; `icon` is the exhaustive `DailyPuzzle` to emoji map). Each daily has a pure `share.rs` beside its `state.rs`: `is_ready` is the cheap gate the key line reads, `from_state` returns `None` until it holds (and always on personal boards), `card` takes plain data and is whole-state tested in `share_test.rs`. No card needs a legend: Le Word is the guess grid; Nonograms the finished picture in half-blocks (the solution, on purpose: the picture is the brag); Sudoku the 9x9 with clues white and the player's fills green, read straight from `fixed_mask`; Minesweeper the mine count plus the last ten clicks as safe/flag/boom, from the session-local `click_log` (empty on a board restored from a save); Solitaire the four foundation bars; Rubik's Cube and Sliding Puzzle are before-and-after, the day's scramble above an arrow row with the move count above the solved board, both derived from the puzzle date (`scrambled_stickers`, `generate_scramble` with the snapshot seed), with sliding tiles coloured by their home row. Copying, the banner, and the `share_cards_total` metric live in one place, `share_card` in `input.rs`; `s` is intercepted there only while a card exists, so a Le Word letter keeps working on an open board.

## Controls

Root context keeps only global Arcade shortcuts. Keep detailed per-game control copy in each game's `ui.rs` info panel and in help modal copy.

Current per-game basics:
- 2048: `h/j/k/l` or arrows move, `r` restarts after game over.
- Lateris: left/right move, down soft-drops, up rotates, `Space` hard-drops, `c` holds the current piece, `p` pauses, `r` restarts.
- Snake: arrows or `h/j/k/l` steer, `p` pauses, `r` restarts.
- Sudoku: arrows or `h/j/k/l` move, `1-9` fill, `m` pencil mode, `u`/`U` undo (50-move queue, covering candidate mark edits), `0`/Backspace clear, `d/p/n` daily/personal/new, `[`/`]` difficulty.
- Nonograms: arrows or `h/j/k/l` move, `Space`/`x` toggle, `0`/Backspace/`c` clear, `d/p` daily/personal, `n` new board, `r` reset, `[`/`]` difficulty.
- Minesweeper: arrows or `h/j/k/l` move, reveal/flag/chord controls live in the game info panel.
- Solitaire: card/tableau/foundation controls live in the game info panel; mouse support maps left-click to select/place/draw stock, right-click to auto-move the clicked card, and wheel events over the board to tableau scroll.
- Le Word: type `a-z`, `Enter` submits, Backspace deletes, and `!` opens rules. A scored letter reads by shape as well as color (`ui.rs::score_tile`: correct is a solid tile, present a solid tile in brackets, absent the bare letter), because many themes give `SUCCESS`, `AMBER`, and `TEXT_FAINT` the same value; the rules legend draws the same tiles instead of naming colors.
- Rubik's Cube: everyone gets the same UTC daily scramble; `u/d/l/r/f/b` turns faces clockwise, uppercase turns inverse, `0` resets today's scramble (`s` is the share card, like every daily), `v` rotates the view right, and arrows rotate the view in their own directions.
- Sliding Puzzle: arrows or `h/j/k/l` slide an adjacent tile in the indicated direction into the gap; left-clicking an adjacent tile moves it directly. `i` toggles the session-only numbered/art view without changing the board or reward contract; on a failed art load, `i` twice asks again at once. `d`/`p` switch daily/personal mode, `n` twice starts a new personal board, `[`/`]` changes difficulty, and `r`/`0` twice restores the active scramble. Personal boards persist but have no reward. Reset is refused on a solved daily (the win is banked), like Rubik's Cube. Sliding Puzzle has no keypress-time rollover: a board left open across UTC midnight finishes under its own date and `refresh_daily_games` rolls it once the player looks away.

Destructive daily/personal puzzle reset keys use a local confirmation flag. Sudoku (`n`/`r`), Nonograms (`n`/`r`), Minesweeper (`n`), Solitaire (`n`/`r`), Rubik's Cube (`0`), and Sliding Puzzle (`n`/`r`/`0`) set `reset_pending` on the first press and reset only on a repeated matching action key. Any ordinary movement, mode/difficulty switch, board edit, card action, face turn, or view change clears the pending flag. Renderers surface a short "press again" tip while pending.

## Adding a new Arcade game

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
- Update the root `CONTEXT.md`, `CONTEXT.md` here and this file if the game changes Arcade categories, service ownership, or leaderboard semantics.

Leaderboard/Hub checklist:
- High-score games must write final score events through a `late-core` model method so monthly boards do not depend only on legacy high-score table `updated` timestamps. Lateris and Snake also publish hidden quest Activity score events on final score submission; Snake includes the reached level for weekly/daily quest matching.
- A restored run that was already over must not emit its final score event again (it fired when the run ended). A second emission is a fresh `GameScored` activity, which completes score-based daily quests for a game nobody played; `snake/state.rs` carries `score_event_recorded` across `restore` for this.
- Add the game to the matching roster in `late-core/src/models/leaderboard.rs`: a `DailyPuzzle` variant enrolls it in the per-game win boards, Arcade Wins points, today's champions, and daily statuses at once; a `ScoreGame` variant enrolls its monthly/all-time score boards. The compiler walks you through the per-variant facts, and the Leaderboards page (`app/leaderboard/`) picks the board up from the roster with no page change.

Testing guidance:
- Pure rules and key-routing helpers get inline unit tests in `state.rs` or `input.rs`.
- DB/service coverage lives in the adjacent `svc_test.rs` beside each game's `svc.rs` (wired with `#[cfg(test)] mod svc_test;`), using `crate::test_helpers::new_test_db`.
- Agents run targeted tests through `make test-llm ARGS="-p late-ssh --lib -E 'test(arcade)'"`; `make check` stays with the human owner.
