# Daily Games Context

## Metadata
- Scope: `late-ssh/src/app/daily` (correspondence-game service, sidebar panel, modal, full-screen board) plus its persistence in `late-core/src/models/daily_match.rs` and migration `102_create_daily_matches.sql`. Design doc: `devdocs/FRD-DAILY.md`.
- Domain: async-first correspondence matches between two fixed players. Chess only in v1: post a challenge, walk away, play one move whenever you're around, 24h per move.
- Primary audience: LLM agents changing daily-game rules, the lobby/challenge flow, the sidebar panel, the modal, the board screen, or deadline/forfeit behavior.
- Last updated: 2026-07-09 (board screen: key hints pinned to the bottom row, player bars stay hugging the board).
- Status: Active
- Parent context: `../../../../CONTEXT.md`
- Stability note: `[STABLE]` sections change rarely; `[VOLATILE]` sections change with UI copy, keybindings, or v1 scope decisions.

---

## 0. Context Maintenance Protocol [STABLE]

Read this after root `CONTEXT.md` whenever a task touches daily matches, challenges, the Daily Games sidebar panel/modal/board, or the `daily_matches` table.

- Update this file when the match lifecycle, deadline policy, cap, payout, or UI surfaces change.
- Update root `CONTEXT.md` when global keybindings, the screen list, or the data model change.
- Chess rules/rendering primitives are NOT owned here: they live in `app/games/chess_core` (see `app/games/CONTEXT.md`). Live chess tables stay in `app/rooms/chess` (see `app/rooms/CONTEXT.md`).
- `mod.rs` stays declaration-only.

---

## 1. Summary [STABLE]

A daily match is a relationship between two people, not a place. Daily matches deliberately do NOT live in `game_rooms`: no seats, no ready-up, no AFK timers, no embedded chat room, no live actor per match. What is shared with table chess is the chess itself, through `app/games/chess_core`.

Core shape:
- **Open lobby is the centerpiece.** Anyone posts an open challenge; anyone claims it; claiming starts a match. Open challenges persist until claimed or cancelled (no expiry in v1). Directed challenges are the same row with `target_user_id` set.
- **24h per move, fixed in v1.** Missing the deadline forfeits (sweeper, §3).
- **Cap: 5 active entries per user** (`DAILY_MAX_ACTIVE_ENTRIES`): open challenges you posted plus active matches you play in, combined.
- **Winner payout** through the existing reward-template path: `daily_chess_win_payout`, 500 chips, 3600s cooldown claim policy (seeded in migration 102). That payout is the entire economy/social footprint of v1: no @dealer, no #lounge announcements, no `ActivityEvent` publishing, so no quest integration. The sidebar panel is the only broadcast surface.
- Three UI surfaces, one system of record: the passive right-sidebar panel, the Daily Games modal (`g`, all interaction), and the full-screen board (`Screen::DailyMatch`, entered only from the modal).

Non-goals for v1 (deferred by decision, 2026-07-08): wagers/escrow, spectating, games other than chess, tournaments, draw offers (draws happen only via stalemate/repetition), #lounge announcements, quest wiring. The schema leaves room for all of them (§8).

---

## 2. Module Map [STABLE]

| File | Responsibility |
|---|---|
| `mod.rs` | Declarations only. |
| `svc.rs` | `DailyService`: process-global singleton like `RoomsService`. Snapshot `watch` + event `broadcast`, fire-and-forget mutating tasks, the deadline sweeper, chip payout on finish. Owns `DailyChessState` (the persisted `state` JSON shape) and the snapshot item types. |
| `state.rs` | Per-session `DailyState`: snapshot/event drains (`tick`), lobby glow, modal cursor/confirm/prompt state, the full-screen board state (`DailyBoardState` + optimistic move), your-turn notification edges, `format_deadline`. |
| `panel.rs` | Right-sidebar panel: passive, fixed `DAILY_PANEL_HEIGHT = 8`, stable chrome (dash slots when empty). Pure `DailyPanelProps` line builder for tests. |
| `modal_input.rs` / `modal_ui.rs` | The Daily Games modal: one scrollable list (your matches, then the lobby), claim confirm, directed-challenge username prompt, footer actions. |
| `board_input.rs` / `board_ui.rs` | Full-screen match view over `chess_core::board_ui` + `cursor`: players/colors frame, move list, deadline, result banner, mouse hit test via render-recorded geometry. |

Persistence:
- `late-core/src/models/daily_match.rs`: `DailyMatch` via `model!` with bespoke methods (`create_challenge`, guarded `claim`, `cancel_challenge`, `update_state`, `finish`, `forfeit_expired`, `count_active_entries`, `list_open`, `list_active`). Status/result string constants live here.
- `late-core/migrations/102_create_daily_matches.sql`: the `daily_matches` table (single table; a challenge IS a match waiting for its second player), indexes, and the reward-template seed.

Cross-module touchpoints (outside this folder):
- `main.rs`: constructs `DailyService`, runs `refresh_task()` + `start_sweeper_task()` once per process.
- `app/state.rs`: `App::daily` (`DailyState`), `show_daily_modal`, `SessionConfig::daily_service`; `DailyState::new` receives a cloned `notify::Notifier`.
- `app/tick.rs`: `self.daily.tick()` returns targeted error/win banners.
- `app/input.rs`: `g` (not composing) opens the modal (marks lobby seen first); modal input routes to `modal_input.rs`; `Screen::DailyMatch` routes to `board_input.rs`.
- `app/render.rs`: modal + board dispatch, sidebar props population.
- `app/common/primitives.rs`: `Screen::DailyMatch` (outside the Tab cycle, like door games).
- `app/common/sidebar.rs`: `DAILY_HEIGHT`, render arm, `SidebarProps.daily`.
- `late-core/src/models/user.rs`: `RightSidebarComponent::Daily` (key `daily`, label `Daily Games`), default order `[Visualizer, Music, Daily, Activity, Bonsai]`, `normalize_right_sidebar_components` backfills missing panels for existing users.
- `app/chat/state.rs` + `app/chat/input.rs`: composer `/challenge [@user] chess` parses to a `DailyChallengeRequest` which chat input hands to `DailyState` post methods.
- `app/notify/mod.rs`: `Notification::daily_your_turn` (`Kind::GameEvents`).
- `app/help_modal/data.rs`: `g` + `/challenge` help entries.

---

## 3. Service And Persistence Model [STABLE]

- **No live actor per match.** Every mutation (`post_challenge`, `claim_challenge`, `cancel_challenge`, `play_move`, `resign`) loads the row, validates against `chess_core::rules` over `state.fen`, and persists. Nothing to reconcile after a restart.
- **Snapshot**: one global `watch::Receiver<Arc<DailySnapshot>>` (`open_challenges` + `active_matches` summaries with usernames resolved). Per-session UI filters for "mine". Republished on every mutation; the sweeper loop republishes every 60s as the slow-poll backstop.
- **Events**: `broadcast<DailyEvent>`: `ChallengePosted`, `ChallengeClaimed`, `MovePlayed`, `MatchFinished`, targeted `Error { user_id, message }` for banners (svc error strings lowercase; banners sentence case).
- **Row lifecycle**: `open -> active` (claim) `-> finished | cancelled`. Claim is a guarded UPDATE (`WHERE status='open' AND opponent_id IS NULL`) so two simultaneous claims can't both win; colors are random at claim time and stored in `state.colors`.
- **State JSON** (`DailyChessState`, version 1): `revision`, `fen`, `colors`, `move_history` (SAN labels + timestamps), `position_history` (FENs for threefold repetition). `DailyMatch::update_state` carries the same monotonic-revision guard idea as `GameRoom::update_runtime_state`: the update is guarded on the expected mover/turn so a superseded write updates 0 rows and the service surfaces "move was superseded". `DailyMatch::finish` is guarded on the exact loaded `revision` (`STORED_REVISION_EQ_SQL`) so a resign can't clobber an opponent's just-committed move; `resign` reloads and retries on a 0-row (superseded) finish.
- **Deadlines are durable by construction**: `turn_deadline_at` is a DB timestamp, never an in-process `sleep_until`. `play_move` rejects a move once `turn_deadline_at <= now()` (and so never resets a dead clock); the sweeper (60s loop) is the forfeit executor for `status='active' AND turn_deadline_at < now()` rows (winner = other player, result `timeout`) and therefore survives restarts for free.
- **Finish payout**: decisive winner credits `ChipService::credit_per_event_reward_template(daily_chess_win_payout, event_key = match_id)`. The `daily_chess_win_payout` template is `claim_policy = per_event`, so each distinct match win pays exactly once (idempotent per match id): concurrent wins within any window all pay, batched sweeper forfeits after downtime each pay, and a re-broadcast never double-pays. A duplicate claim for the same match is logged, not surfaced.
- Results: `checkmate`, `draw`, `resign`, `timeout`, `''` while running.

---

## 4. UI Surfaces [VOLATILE]

### Sidebar panel (`panel.rs`)
- Fixed 8 rows: title, four match slots (your-turn rows glow and sort first, then nearest deadline), lobby activity line, entries `n/5` line, key hints. Slots render dashes when empty; the panel never changes height between states (stable-chrome rule).
- The lobby line is the liquidity engine: other people's open challenges glow when new since the modal was last opened. Own challenges never glow. `seen_open_ids` is seeded at session start so pre-existing challenges don't glow on login.

### Daily Games modal (`modal_*`)
- Opened by global `g` (not composing; yields to chat message selection and active games) and from the panel hint. Opening calls `mark_lobby_seen`.
- One scrollable list, `j`/`k`: your matches (Enter opens the board), then every open challenge (Enter claims with a confirm second-press; `x` cancels your own). `c` posts an open challenge, `C` opens the directed-challenge username prompt, `Esc`/`q` closes (prompt and confirm consume the first Esc).
- Composer command `/challenge @user chess` / `/challenge chess` posts through the same task path via chat state's `DailyChallengeRequest` handoff.

### Board screen (`board_*`)
- `Screen::DailyMatch`, outside the Tab cycle, entered only from the modal; `q`/`Esc` restores the return screen and reopens the modal (one keypress per hop across matches).
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
- Claim stays a guarded UPDATE; never split it into read-then-write without the status/opponent guard.
- Deadlines stay DB timestamps. Do not introduce in-process timers for correspondence deadlines; the rooms-chess `sleep_until` clock approach explicitly does not survive restarts and this domain must.
- The 5-entry cap counts open challenges posted plus active matches played, enforced server-side on post AND claim.
- `state.revision` only increases; superseded writes must fail loudly ("move was superseded, reload the match"), not last-write-win.
- Panel height is constant (8 rows); empty slots render dashes. Never collapse or grow the panel between states.
- Chess time control `daily` no longer appears in `rooms/chess` `TIME_CONTROL_OPTIONS` for new tables; the `ChessTimeControl::Daily` variant and its `from_id` parsing must survive until the last legacy daily table row is gone.
- v1 publishes no `ActivityEvent` and posts nothing to chat.

---

## 7. Tests [STABLE]

Root policy applies: agents run `cargo check --tests` as verification; the human owner runs the suite.

- Integration (`late-ssh/tests/daily/svc.rs`, real DB via `test_db()`): claim race has exactly one winner, directed-challenge targeting, move turn/legality validation, checkmate finish + payout, resign, stale-revision rejection, sweeper forfeits, entry cap, self-challenge rejection.
- Inline pure unit tests: state (deadline formatting, turn-edge detection), panel line builder, svc state parsing, settings round-trips.

---

## 8. Future Hooks [VOLATILE]

All deliberately pure additions with no schema impact unless noted:
- #lounge announcements + quest wiring: subscribers on the existing `DailyEvent` stream.
- Wager escrow: add a `wager` column + hold/settle in `ChipService`; claim/finish are the only touch points.
- Seeded arcade score duels: same table (`game_kind='duel_snake'` etc.), `state` holds seed + submitted scores, sweeper settles at deadline.
- Spectating: render `state.fen` read-only from any row; no live service needed.
- 1d/3d deadline choice, rematch one-key, draw offers: all raised in the FRD as cheap follow-ups if requested.
