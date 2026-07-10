# Daily Games Context

## Metadata
- Scope: `late-ssh/src/app/daily` (correspondence-game service, game roster, sidebar panel, modal, full-screen boards) plus its persistence in `late-core/src/models/daily_match.rs` and migrations `102_create_daily_matches.sql` / `105_seed_daily_battleship_reward.sql` / `106_seed_daily_connect4_reward.sql`. Design doc: `devdocs/FRD-DAILY.md`.
- Domain: async-first correspondence matches between two fixed players. Three games — chess, battleship, connect four — behind one roster enum: post a challenge, walk away, play one move whenever you're around, 24h per move.
- Primary audience: LLM agents changing daily-game rules, adding a game to the roster, the lobby/challenge flow, the sidebar panel, the modal, the board screens, or deadline/forfeit behavior.
- Last updated: 2026-07-10 (connect four joins as the third game: drop-history-only state in `connect4.rs`, one drop per day, turn always passes, 400-chip win, can draw on a full board; `board_select_or_move` now dispatches through `DailyGameDetail::kind()` — exhaustive match, no bool dispatch).
- Status: Active
- Parent context: `../../../../CONTEXT.md`
- Stability note: `[STABLE]` sections change rarely; `[VOLATILE]` sections change with UI copy, keybindings, or v1 scope decisions.

---

## 0. Context Maintenance Protocol [STABLE]

Read this after root `CONTEXT.md` whenever a task touches daily matches, challenges, the game roster, the Daily Games sidebar panel/modal/boards, or the `daily_matches` table.

- Update this file when the match lifecycle, deadline policy, cap, payouts, the game roster, or UI surfaces change.
- Update root `CONTEXT.md` when global keybindings, the screen list, or the data model change.
- Chess rules/rendering primitives are NOT owned here: they live in `app/games/chess_core` (see `app/games/CONTEXT.md`). Live chess tables stay in `app/rooms/chess` (see `app/rooms/CONTEXT.md`). Battleship and connect four rules ARE owned here (`battleship.rs`, `connect4.rs`): no other surface plays them.
- `mod.rs` stays declaration-only.

### Adding a game to the roster

`games.rs` is deliberately one enum, no trait objects. Add a variant to `DailyGame` and the compiler walks you through every exhaustive match: DB kind string (`DailyMatch::GAME_KIND_*`), display label, win payout, reward key, ledger reason, tagline. Beyond the enum you owe: a state module (serde state + pure rules, like `battleship.rs` / `connect4.rs`), a `claim_challenge` initial-state arm and a `play_move` dispatch arm in `svc.rs`, a `DailyGameDetail` variant + board input ops in `state.rs`, a board renderer (like `battleship_ui.rs` / `connect4_ui.rs`), a `publish()` move-count arm, and a migration seeding `daily_<game>_win_payout` in `reward_templates` (plus the key const in `late-core/src/models/reward.rs`). The displayed prize (`DailyGame::win_payout`) and the seeded template's `reward_chips` must stay in sync — the template is what actually pays.

The compiler cannot demand everything. Grep-hunt these seams too: the result string's arm in `board_ui.rs::result_banner` (falls back to "Finished" silently), the `/challenge` copy in `app/help_modal/data.rs` and `app/chat/commands.rs` (static strings enumerate the roster), the `board_move_cursor` wildcard arm, the mouse hit-test arm in `board_input.rs` (each game interprets `target_geometry` its own way), and this file.

---

## 1. Summary [STABLE]

A daily match is a relationship between two people, not a place. Daily matches deliberately do NOT live in `game_rooms`: no seats, no ready-up, no AFK timers, no embedded chat room, no live actor per match. What is shared with table chess is the chess itself, through `app/games/chess_core`; battleship and connect four rules live entirely in this domain.

Core shape:
- **A roster of games, one enum.** `games.rs::DailyGame` (chess, battleship, connect four) owns every per-game fact: `daily_matches.game_kind` string, label, win payout, reward key, ledger reason. Challenge posting picks a game (modal draft picker or `/challenge [@user] [chess|battleship|connect4]`); everything downstream dispatches on it. See §0 for the add-a-game checklist.
- **Open lobby is the centerpiece.** Anyone posts an open challenge; anyone claims it; claiming starts a match. Open challenges persist until claimed or cancelled (no expiry in v1). Directed challenges are the same row with `target_user_id` set.
- **24h per move, fixed in v1.** Missing the deadline forfeits (sweeper, §3).
- **Cap: 4 active entries per user** (`DAILY_MAX_ACTIVE_ENTRIES`): open challenges you posted plus active matches you play in, combined. 4 matches the panel's match slots exactly, so every entry is always visible in the sidebar (lowered from 5 on 2026-07-09 for exactly that reason).
- **Winner payout** through the existing reward-template path, one `per_event` template per game: `daily_chess_win_payout` 500 chips (migration 102), `daily_battleship_win_payout` 300 chips (migration 105), `daily_connect4_win_payout` 400 chips (migration 106). Prizes are shown on lobby rows and in the challenge draft; `DailyGame::win_payout` is the displayed number and must match the seeded `reward_chips`. That payout is the entire economy/social footprint: no @dealer, no #lounge announcements, no `ActivityEvent` publishing, so no quest integration. The sidebar panel is the only broadcast surface.
- **Battleship rules (v1):** both fleets (5/4/3/3/2) are placed randomly at claim time — a placement phase would cost a whole correspondence day — the coin flip picks the first shooter, one shot per turn on a 10x10 grid, a hit fires again (deadline still resets), sinking all five ships finishes with result `fleet_sunk`.
- **Connect four rules:** 7x6 grid, the claim-time coin flip picks who's red (red drops first), one drop per turn, the turn always passes. Four in a row finishes with result `four_in_a_row`; 42 drops with no line is a `draw`. The state stores only the drop history — grid, turn, and move count are derived, so the state can't self-contradict.
- Three UI surfaces, one system of record: the passive right-sidebar panel, the Lobby modal (`Ctrl+Q`, all interaction; "Lobby" is the user-facing name for the whole daily surface), and the full-screen board (`Screen::DailyMatch`, entered only from the modal; renders the match's game).

Non-goals for v1 (deferred by decision, 2026-07-08): wagers/escrow, spectating, tournaments, draw offers (chess draws happen only via stalemate/repetition), #lounge announcements, quest wiring. The schema leaves room for all of them (§8).

---

## 2. Module Map [STABLE]

| File | Responsibility |
|---|---|
| `mod.rs` | Declarations only. |
| `games.rs` | The roster: `DailyGame` enum (chess, battleship, connect four) with kind/label/win payout/reward key/ledger reason/tagline behind exhaustive matches, `from_kind`/`from_label` parsing, `usage_labels` for banners. |
| `battleship.rs` | Battleship rules, pure logic: `DailyBattleshipState` (the persisted `state` JSON for battleship matches), random legal fleet placement, `apply_shot` (bounds/repeat validation, sunk + fleet-sunk outcomes), `cell_label` (`A1`..`J10`). |
| `connect4.rs` | Connect four rules, pure logic: `DailyConnect4State` (persisted `state` JSON: red/yellow ids + drop history; grid/turn/count all derived), `apply_drop` (bounds/full-column validation, win + draw outcomes), `winning_line` for the end-of-match highlight, `column_label` (`a`..`g`). |
| `svc.rs` | `DailyService`: process-global singleton like `RoomsService`. Snapshot `watch` + event `broadcast`, fire-and-forget mutating tasks, the deadline sweeper, per-game chip payout on finish. Owns `DailyChessState` (the persisted `state` JSON for chess) and the snapshot item types (both carry `game: DailyGame`). `play_move` dispatches per game (`play_chess_move` / `play_battleship_shot` / `play_connect4_drop`); `resign` is game-agnostic (winner from the row, revision bumped on raw JSON). |
| `state.rs` | Per-session `DailyState`: snapshot/event drains (`tick`), lobby glow, modal cursor/confirm state, the `ChallengeDraft` (game picker + directed-username buffer), the full-screen board state (`DailyBoardState`; `DailyMatchDetail` wraps a `DailyGameDetail` enum of `ChessDetail` / `BattleshipDetail` / `Connect4Detail`, with `kind()` back to the roster for exhaustive dispatch), optimistic moves per game (`shot_in_flight` / `drop_in_flight`), your-turn notification edges, `format_deadline`. |
| `panel.rs` | Right-sidebar panel: passive, fixed `DAILY_PANEL_HEIGHT = 6`, stable chrome (dash slots when empty), no title row of its own. Pure `DailyPanelProps` line builder for tests. |
| `modal_input.rs` / `modal_ui.rs` | The Lobby modal: one scrollable list (your matches, then the lobby; rows show game + chip prize), claim confirm, the challenge draft (`c` open / `C` directed; Tab or ←/→ cycles the game, prize shown inline), footer actions. |
| `board_input.rs` / `board_ui.rs` | Full-screen match view: shared chrome (loading, result banner map incl. `fleet_sunk` / `four_in_a_row`, overlay, key hints) + the chess renderer over `chess_core::board_ui` + `cursor`; dispatches battleship to `battleship_ui`, connect4 to `connect4_ui`. Mouse hit tests via render-recorded geometry (`board_geometry` for chess; `target_geometry` per game — a battleship click maps to a cell, a connect4 click to a column). |
| `battleship_ui.rs` | Battleship board renderer: "their waters" target grid (cursor, shots, end-of-match ship reveal) beside "your fleet", per-side afloat counts, salvo-history rail, same pinned-hints/centred-stack shape as chess. |
| `connect4_ui.rs` | Connect four board renderer: one gravity grid, `▼` drop indicator + ghost landing disc in the cursor column, last-drop highlight, winning line as solid tiles on finish, drop-history rail, same pinned-hints/centred-stack shape. |

Persistence:
- `late-core/src/models/daily_match.rs`: `DailyMatch` via `model!` with bespoke methods (`create_challenge`, guarded `claim`, `cancel_challenge`, `update_state`, `finish`, `forfeit_expired`, `count_active_entries`, `list_open`, `list_active`). Status/result string constants live here.
- `late-core/migrations/102_create_daily_matches.sql`: the `daily_matches` table (single table; a challenge IS a match waiting for its second player), indexes, and the reward-template seed.

Cross-module touchpoints (outside this folder):
- `main.rs`: constructs `DailyService`, runs `refresh_task()` + `start_sweeper_task()` once per process.
- `app/state.rs`: `App::daily` (`DailyState`), `show_daily_modal`, `SessionConfig::daily_service`; `DailyState::new` receives a cloned `notify::Notifier`.
- `app/tick.rs`: `self.daily.tick()` returns targeted error/win banners.
- `app/input.rs`: reserved global `Ctrl+Q` toggles the modal from anywhere (open marks the lobby seen first); the old bare `g` binding is removed and `g` is free again; modal input routes to `modal_input.rs`; `Screen::DailyMatch` routes to `board_input.rs`.
- `app/render.rs`: modal + board dispatch, sidebar props population.
- `app/common/primitives.rs`: `Screen::DailyMatch` (outside the Tab cycle, like door games).
- `app/common/sidebar.rs`: `DAILY_HEIGHT`, render arm, `SidebarProps.daily`.
- `late-core/src/models/user.rs`: `RightSidebarComponent::Daily` (key `daily`, label `Lobby`), default order `[Visualizer, Music, Daily, Activity, Bonsai]`, `normalize_right_sidebar_components` backfills missing panels for existing users.
- `app/chat/state.rs` + `app/chat/input.rs`: composer `/challenge [@user] [chess|battleship|connect4]` parses to a `DailyChallengeRequest` (carrying the `DailyGame`) which chat input hands to `DailyState` post methods.
- `app/notify/mod.rs`: `Notification::daily_your_turn(game_label, opponent)` (`Kind::GameEvents`).
- `app/help_modal/data.rs`: `Ctrl+Q` + `/challenge` help entries.
- `app/render.rs`: `app_frame_help_hint_title` advertises `Lobby Ctrl+Q` in the outer frame footer.

---

## 3. Service And Persistence Model [STABLE]

- **No live actor per match.** Every mutation (`post_challenge`, `claim_challenge`, `cancel_challenge`, `play_move`, `resign`) loads the row, validates (chess: `chess_core::rules` over `state.fen`; battleship: `battleship.rs` over the stored fleets/shots; connect4: `connect4.rs` over the drop history), and persists. Nothing to reconcile after a restart. `play_move`'s shared prelude (active status, turn, deadline) runs before the per-game dispatch; a battleship "move" carries the target cell in `to`, a connect4 "move" the column.
- **Snapshot**: one global `watch::Receiver<Arc<DailySnapshot>>` (`open_challenges` + `active_matches` summaries with usernames resolved). Per-session UI filters for "mine". Republished on every mutation; the sweeper loop republishes every 60s as the slow-poll backstop.
- **Events**: `broadcast<DailyEvent>`: `ChallengePosted`, `ChallengeClaimed`, `MovePlayed`, `MatchFinished`, targeted `Error { user_id, message }` for banners (svc error strings lowercase; banners sentence case).
- **Row lifecycle**: `open -> active` (claim) `-> finished | cancelled`. Claim is a guarded UPDATE (`WHERE status='open' AND opponent_id IS NULL`) so two simultaneous claims can't both win; colors are random at claim time and stored in `state.colors`.
- **State JSON**, one shape per game, all with a top-level `revision`: `DailyChessState` (version 1: `fen`, `colors`, `move_history` with SAN labels + timestamps, `position_history` for threefold repetition), `DailyBattleshipState` (version 1: two `sides` of `{user_id, ships, shots}`; ships are contiguous cell lists, shots carry hit + timestamp), and `DailyConnect4State` (version 1: `red`/`yellow` user ids + `drops` column history; everything else derived). `DailyMatch::update_state` carries the same monotonic-revision guard idea as `GameRoom::update_runtime_state`: the update is guarded on the expected mover/turn so a superseded write updates 0 rows and the service surfaces "move was superseded". `DailyMatch::finish` is guarded on the exact loaded `revision` (`STORED_REVISION_EQ_SQL`) so a resign can't clobber an opponent's just-committed move; `resign` reloads and retries on a 0-row (superseded) finish, and stays game-agnostic by deriving the winner from the row and bumping `revision` on the raw JSON.
- **Deadlines are durable by construction**: `turn_deadline_at` is a DB timestamp, never an in-process `sleep_until`. `play_move` rejects a move once `turn_deadline_at <= now()` (and so never resets a dead clock); the sweeper (60s loop) is the forfeit executor for `status='active' AND turn_deadline_at < now()` rows (winner = other player, result `timeout`) and therefore survives restarts for free.
- **Finish payout**: decisive winner credits `ChipService::credit_per_event_reward_template(game.reward_key(), event_key = match_id)` — `daily_chess_win_payout` (500), `daily_battleship_win_payout` (300), or `daily_connect4_win_payout` (400). All templates are `claim_policy = per_event`, so each distinct match win pays exactly once (idempotent per match id): concurrent wins within any window all pay, batched sweeper forfeits after downtime each pay, and a re-broadcast never double-pays. A duplicate claim for the same match is logged, not surfaced.
- Results: `checkmate`, `draw`, `fleet_sunk`, `four_in_a_row`, `resign`, `timeout`, `''` while running. Battleship cannot draw; connect four draws on a full board (winner `None`, no payout, like chess draws).

---

## 4. UI Surfaces [VOLATILE]

### Sidebar panel (`panel.rs`)
- Fixed 6 rows: four match slots (your-turn rows glow and sort first, then nearest deadline), one status line (`N open · entries/cap`), key hints (`ctrl+q · /challenge`). Slots render dashes when empty; the panel never changes height between states (stable-chrome rule).
- The panel has no title row: the sidebar's labeled separator rule (`── lobby ────`, built in `app/common/sidebar.rs::draw_panel_rule`) is the title. Every sidebar panel's rule is labeled this way; the lobby's label is the only one with an active state.
- Attention is split across two signals: the rule label glows ONLY while it's your turn in any match (the sidebar computes this from `DailyState::my_matches`/`my_turn`); the status line's open count glows while there are open challenges unseen since the modal was last opened (the liquidity signal). Own challenges never glow. `seen_open_ids` is seeded at session start so pre-existing challenges don't glow on login.

### Lobby modal (`modal_*`)
- Opened by reserved global `Ctrl+Q` only (works anywhere, including while composing; pressed again it closes the modal). The old bare `g` binding is removed. Opening calls `mark_lobby_seen`.
- Near-fullscreen: sized from the terminal minus a margin (8 cols / 4 rows), capped at 100x40 so lines stay readable on large terminals. The daily surface is a primary destination, not a peek.
- One scrollable list, `j`/`k`: your matches (Enter opens the board), then every open challenge (Enter claims with a confirm second-press; `x` cancels your own). Rows show the game and its chip prize.
- `c` / `C` open the challenge picker: a small centered overlay on the modal with one row per roster game and its prize (`j`/`k` + Enter), so the roster scales without fighting the status line for width. Directed drafts (`C`) add a username step after the game is picked; `Esc` steps back (username → picker → closed), and confirm consumes its own first Esc.
- Composer command `/challenge [@user] [chess|battleship|connect4]` posts through the same task path via chat state's `DailyChallengeRequest` handoff (game defaults to chess when omitted).

### Board screen (`board_*`, `battleship_ui.rs`, `connect4_ui.rs`)
- `Screen::DailyMatch`, outside the Tab cycle, entered only from the modal; `q`/`Esc` restores the return screen and reopens the modal (one keypress per hop across matches). Renders whichever game the match is; the screen title is "Daily Match".
- Connect four layout: one centred grid (header letters, a `▼` indicator over the cursor column, ghost `◌` at the landing cell, drop count under the board), player bars showing disc colors, and a drop-history rail when wide enough. `arrows/wasd` slide the column cursor (the cursor IS a column index), `Space`/`Enter` drops (optimistic, `drop_in_flight` blocks a double drop until the reload), mouse drops via `target_geometry` (click maps to a column). The winning four render as solid tiles under the result overlay.
- Battleship layout: "their waters" target grid (left, cursor + fired shots; unfound enemy ships revealed as `░░` when the match ends) beside "your fleet" (right, ships + incoming fire), per-grid sunk/afloat summaries, player bars with afloat counts, and a salvo-history rail when wide enough. `arrows/wasd` aim, `Space`/`Enter` fires (optimistic, `shot_in_flight` blocks a double salvo until the reload), `r` resign, mouse fires via the render-recorded `target_geometry`.
- Vertical layout: the status line and the two player bars ride with the board as one centred group, so the colour/name labels always hug the board edges. The key-hint row is the exception and pins to the last row of the content column, with a `Min(0)` slack row absorbing the gap. Board sizing still reserves all four chrome rows (`CHROME_ROWS`), so pinning the hints does not change the tier the board picks.
- Loads the full row on open and on every `MovePlayed`/`ChallengeClaimed`/`MatchFinished` for the open match id (reload coalescing via `reload_pending`). Usernames are captured at open so names survive the match leaving the active snapshot on finish.
- Move flow mirrors table chess: cursor + Space/Enter or mouse click, promotion defaults to queen, `r` resign (press twice), `p` toggles piece graphics. The optimistic move applies locally and reconciles on the next reload; legal moves are cleared until then so the cursor can't pick up opponent pieces.
- Piece-graphics image ids seed from `match_id` (the `placement_seed` param of `chess_core::board_ui`).

---

## 5. Notifications [STABLE]

- **Your-turn desktop notify**: `DailyState` holds a cloned `notify::Notifier` and pushes `Notification::daily_your_turn(opponent)` (`Kind::GameEvents`) on the became-my-turn edge while connected. Edge detection: `turn_notified_match_ids` is seeded from the login snapshot (connecting never notifies; the panel's glow + your-turn rows are the on-login nudge) and pruned when the turn passes away, so a turn coming back is a fresh edge.
- **Explicitly NOT in v1**: #lounge announcements via @dealer and `ActivityEvent` publishing. When announcements land later they go through a `DailyEvent` subscriber posting via `ChatService::send_lounge_message_task`, preserving the "games never post to chat directly" boundary.

---

## 6. Critical Invariants [STABLE]

- Daily matches never touch `game_rooms` or the rooms runtime; rooms never reach into `daily_matches`. The only shared code is `chess_core` (and `ChipService`).
- The game roster is `games.rs::DailyGame` and nothing else: no game-kind string comparisons outside `DailyGame::from_kind`/`kind()`. Rows whose `game_kind` this build doesn't know are hidden from the snapshot (and skipped for payout with an error log), never guessed at.
- `DailyGame::win_payout` is display-only; the seeded reward template pays. Keep them equal.
- Claim stays a guarded UPDATE; never split it into read-then-write without the status/opponent guard.
- Deadlines stay DB timestamps. Do not introduce in-process timers for correspondence deadlines; the rooms-chess `sleep_until` clock approach explicitly does not survive restarts and this domain must.
- The entry cap (`DAILY_MAX_ACTIVE_ENTRIES`, 4) counts open challenges posted plus active matches played, enforced server-side on post AND claim. It must not exceed the panel's `MATCH_SLOTS` (4), or active matches become invisible in the sidebar.
- `state.revision` only increases; superseded writes must fail loudly ("move was superseded, reload the match"), not last-write-win.
- Panel height is constant (6 rows); empty slots render dashes. Never collapse or grow the panel between states.
- Chess time control `daily` no longer appears in `rooms/chess` `TIME_CONTROL_OPTIONS` for new tables; the `ChessTimeControl::Daily` variant and its `from_id` parsing must survive until the last legacy daily table row is gone.
- v1 publishes no `ActivityEvent` and posts nothing to chat.

---

## 7. Tests [STABLE]

Root policy applies: agents run `cargo check --tests` as verification; the human owner runs the suite.

- Integration (`late-ssh/tests/daily/svc.rs`, real DB via `test_db()`): claim race has exactly one winner, directed-challenge targeting, move turn/legality validation, checkmate finish + payout, resign, stale-revision rejection, sweeper forfeits, entry cap, self-challenge rejection; battleship claim/fleet shape, hit-fires-again, repeat/off-grid/out-of-turn rejection, miss passes turn, fleet-sunk finish + 300-chip payout, battleship resign; connect4 claim (red on the clock), out-of-turn/off-board/full-column rejection, turn always passes, four-in-a-row finish + 400-chip payout, full-board draw pays nobody.
- Inline pure unit tests: state (deadline formatting, turn-edge detection), panel line builder, svc state parsing, settings round-trips, roster round-trips (`games.rs`), battleship rules (`battleship.rs`: legal random fleets, hit/miss/repeat/sink, coordinate labels, JSON round-trip), connect four rules (`connect4.rs`: turn alternation, all three win directions + winning line, full-column/off-board rejection, checkerboard draw, JSON round-trip).

---

## 8. Future Hooks [VOLATILE]

All deliberately pure additions with no schema impact unless noted:
- #lounge announcements + quest wiring: subscribers on the existing `DailyEvent` stream.
- Wager escrow: add a `wager` column + hold/settle in `ChipService`; claim/finish are the only touch points.
- Seeded arcade score duels: same table (`game_kind='duel_snake'` etc.), `state` holds seed + submitted scores, sweeper settles at deadline.
- Spectating: render `state.fen` read-only from any row; no live service needed.
- 1d/3d deadline choice, rematch one-key, draw offers: all raised in the FRD as cheap follow-ups if requested.
