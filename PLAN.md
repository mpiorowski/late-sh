# PLAN.md — Arcade Economy & Multiplayer Roadmap

## Phase 1: Blackjack MVP

**Goal:** ship a playable blackjack loop that validates the chip economy, service/state/input/ui boundaries, and the path toward shared multiplayer table games.

### Why Blackjack first

- Natural chip sink — betting is the whole point
- Simple rules, fast hands (~30s each)
- PvE so no matchmaking / coordination
- Establishes `state.rs` / `svc.rs` / `input.rs` / `ui.rs` pattern for Poker later
- Works in single-player mode today, upgrades to multi-seat cleanly when we wire the chat room later

### MVP scope — decisions locked in

| Decision | Choice |
|---|---|
| Seats per table | 1 active player in the current implementation |
| Shoe | 6-deck casino shoe, reshuffles at penetration |
| Bet range | 10–100 chips (from `state.rs` `MIN_BET`/`MAX_BET`) |
| Dealer rule | Stands on soft 17 |
| Blackjack payout | 3:2 (rounded toward zero on odd bets) |
| Splits | Not in MVP |
| Doubles | Not in MVP |
| Insurance / even money | Not in MVP |
| Settlement | Optimistic — local balance updates the moment `settle()` returns; `credit_payout` runs fire-and-forget; `HandSettled` event is confirmation only |
| Arcade placement | Existing games picker, but admin-only until Phase 2 lands |
| Chat room wiring | Deferred — migration 024 stays parked for Phase 2 |
| `refund_bet_task` | Dropped for MVP — no abandonment path exists |

### Already shipped

- `blackjack/state.rs` — pure math helpers plus a thin client-side blackjack view state. The app-local state now mainly owns UI input, current user balance, pending request tracking, and subscribed receivers.
- `blackjack/svc.rs` — `BlackjackService` now owns the authoritative shared table state in-memory, publishes `BlackjackSnapshot` via `watch`, and emits per-user action/result events via `broadcast` following the `vote/svc.rs` pattern.
- `BlackjackSnapshot` is now the read model for the UI. The game screen renders from snapshots instead of reading mutable blackjack internals directly.
- Arcade wiring is in place, but Blackjack is currently gated behind `is_admin` and shown grayed out for non-admin users.
- Migration `024_add_game_rooms.sql` — present but still unused in code (supports `kind='game'` + `game_kind` column + partial unique index on `(game_kind, slug)`).

### MVP shipped

- `ChipService` has `debit_bet` and `credit_payout`.
- `BlackjackService` owns the shared table and handles bet/deal/hit/stand/settle transitions.
- `watch` snapshots publish the latest table view; `broadcast` events publish per-user results/errors.
- App-local blackjack state is now a thin client wrapper with local input buffer, local balance, pending request tracking, and subscribed receivers.
- Input/UI/app-shell wiring is complete.
- Blackjack is admin-gated in the arcade while the shared table remains incomplete.

### Verification shipped

- `cargo check -p late-ssh`
- `cargo check -p late-ssh --tests`
- `cargo test -p late-ssh blackjack --lib`

---

## Current status

The code has already moved past strict per-session MVP architecture in one important way:

- Blackjack is no longer owned as authoritative state by each SSH session.
- The service is now the authority and publishes snapshots/events.
- Clients subscribe to the shared table snapshot and keep only thin local UI state.

That means the app now has **shared-state multiplayer plumbing**, but **not full multi-seat blackjack yet**.

### What exists right now

- One shared in-memory blackjack table owned by `BlackjackService`
- `watch` snapshots for latest table state
- `broadcast` events for per-user async results/errors
- One active player at a time
- Other connected clients can observe the same shared table state
- Admin-only gate in the arcade while this remains unfinished

### What is still missing before this counts as true multiplayer blackjack

- Seat map (`seat -> user`)
- Sit/leave flow
- Multiple simultaneous bets before a hand starts
- Per-seat hands and settlement
- Turn order across seated players
- AFK/disconnect handling
- Multiple tables / table IDs
- Game-room/chat binding via migration 024

## Phase 2: Multi-Seat Blackjack

Turn the current shared single-table implementation into a true multi-seat table game. This is where chat room wiring, migration 024, seat management, timers, and disconnect handling all land.

**Scope of Phase 2:**
- Bind a table to a `ChatRoom` of `kind='game'`, `game_kind='blackjack'`, single permanent row seeded at startup (slug `bj-001`, not `game-blackjack`, so the 1→N path is free)
- Expand the current single shared table into `Arc<Mutex<HashMap<RoomId, BlackjackTable>>>` owned by `BlackjackService`
- Seat management: 5 seats, sit/leave independent from chat membership
- Turn timers: 15s per action, 20s for betting, 3-strike AFK unseat
- Hard-disconnect hook via `SessionRegistry` drop → auto-stand + free seat at end of round
- Per-table chat (reuses existing chat infra once the room is wired)
- Split-pane UI: game table on top, scoped chat on bottom
- Activity feed broadcasts for big wins (`🃏 @mat won 80 chips at Blackjack`)
- Extract shared host concerns (seat state, turn timer, disconnect handling) into `app/games/table_host.rs` — wait for Poker to confirm the abstraction

### First concrete steps from the current code

- Replace `active_player_id` with a seat model
- Change snapshot shape from single-player hand/bet fields to per-seat table fields
- Add join/sit/leave actions and events
- Add round phases for multi-player betting and player turn rotation
- Keep the current `watch` snapshot + `broadcast` event split

**Still deferred to Phase 3+:**
- Splits, doubles, insurance
- Multiple tables (second room)
- Private tables (`visibility='private'`)
- Hand history / stats table
- Per-table chip leaderboard

---

## Phase 3+: Future (not planned yet)

### Monthly chip leaderboard resets
- Archive monthly chip leaders (top 3 get a permanent badge?)
- Reset balances to baseline at month end
- "Hall of Fame" display somewhere

### Strategy multiplayer (Chess, Battleship)
- No chips needed — W/L record + rating
- Async: make a move, come back later
- Game completion counts toward daily streaks
- `/challenge @user chess` in chat for matchmaking

### More casino games (Poker)
- Texas Hold'em: PvP, uses chip betting
- Needs turn management, pot logic, hand evaluation
- Validates the `table_host.rs` extraction
- Higher complexity — build after Blackjack Phase 2 validates the multi-seat host

### Chat-based matchmaking
- Activity feed broadcast when someone sits at an empty table
- `/play <game>` and `/challenge @user <game>` commands
- Accept/decline prompts

---

## Game category model (unified view)

| Category | Games | Win condition | Leaderboard section | Streaks | Chips |
|----------|-------|--------------|-------------------|---------|-------|
| Daily puzzles | Sudoku, Nonograms, Minesweeper, Solitaire | Solve the daily | Today's Champions | Yes | +50 bonus per completion |
| High-score | Tetris, 2048 | Personal best | All-Time High Scores | No | No |
| Casino | Blackjack, Poker (future) | Grow your chip balance | Chip Leaders | Optional | Bet and win/lose |
| Strategy | Chess, Battleship (future) | Beat opponent | W/L + Rating | Yes (game completed) | No |
