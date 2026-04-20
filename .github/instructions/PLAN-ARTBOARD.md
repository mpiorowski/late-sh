# PLAN.md — Arcade Economy & Multiplayer Roadmap

## Immediate next: Dartboard in late-sh

**Goal:** integrate `~/p/my/dartboard` into `late-sh` as a shared multiplayer drawing board reachable from the Games screen, with `Ctrl+Q` as the quit key so `Esc` stays available for local selection/floating cancel.

### Discovery summary

- The arcade already has a Multiplayer section, but the selector is only wired for indices `0..=6` in `late-ssh/src/app/games/input.rs`. Rows `7+` are rendered as placeholders only. Dartboard cannot just be "added to the list"; the selector/index map needs to grow and be renumbered.
- Existing late-sh games already use `Esc` to leave the game, but dartboard relies on `Esc` for local selection / floating cancel. Embedded dartboard in late-sh should keep `Ctrl+Q` as quit instead of forcing the house style here.
- `late-ssh` currently enables mouse press/release + wheel tracking only (`?1000h` + `?1006h` in `late-ssh/src/app/state.rs`). Dartboard needs drag events, and full hover/move support if we want parity with standalone floating-selection behavior. The terminal bootstrap will need a stronger mouse mode (`?1003h` most likely) and matching teardown.
- The current VTE/input layer is too narrow for dartboard as-is. `ParsedInput` has bytes, plain arrows, ctrl-arrows, delete, paste, scroll, and a single `MousePress`, but dartboard needs at least:
  - modified arrows (`Shift`, `Alt`, `Ctrl+Shift`)
  - `Home`
  - richer mouse phases (`Down`, `Up`, `Drag`, `Moved`)
  - mouse button + modifier data (`Alt+click`, `Ctrl+drag`, right-drag pan)
- The imported dartboard plan says late-sh can avoid WS deps by depending on `dartboard-server`, but the current `dartboard-server` crate still directly depends on `tokio-tungstenite`. Per the preferred direction for this repo, we should split the in-proc/local transport from the WS transport before wiring late-sh to it.
- The imported dartboard plan suggests direct late-sh user-id passthrough, but the types do not match today: late-sh uses `Uuid`, dartboard wire ids are `u64`. v1 should use dartboard's server-assigned peer ids and keep late-sh identity mapping local to the integration layer unless dartboard's wire model changes.

### Decisions locked in

- Dartboard lives in the Games screen under the Multiplayer section.
- Embedded late-sh dartboard exits with `Ctrl+Q`; bare `Esc` remains local to the canvas.
- v1 is one shared in-proc canvas for the lifetime of the `late-sh` process.
- v1 is in-memory only; persistence stays out of scope.
- v1 uses a crate split so late-sh depends only on the local/in-proc dartboard host/client pieces, not the WS transport crate.
- v1 keeps full mouse parity with standalone dartboard, including hover/move behavior used by floating previews.
- Undo/redo stays unbound in late-sh v1, matching the imported plan and avoiding multiplayer history conflicts.

### Phase A: split dartboard crates for local-only embedding

**Goal:** make the dependency story match the intended architecture before touching late-sh.

- Split `dartboard-server` so `ServerHandle` + `LocalClient` live in a local-only crate with no `tokio-tungstenite` dependency.
- Move WS listener / websocket client-server glue into a separate transport crate or feature-gated module.
- Keep `dartboard-core` transport-agnostic.
- Update the dartboard standalone app to use the new local crate for embedded mode and the WS crate only for `--listen` / `--connect`.
- Treat this split as a prerequisite for late-sh integration.

### Phase B: late-sh host wiring

**Goal:** host one shared dartboard server inside `late-sh` and hand each SSH session a local client.

- Add the new local dartboard crates as dependencies in `late-ssh/Cargo.toml`.
- Create one shared server handle during `late-ssh/src/main.rs` startup alongside the other shared services.
- Store that handle in `late-ssh/src/state.rs::State`.
- Thread it through `late-ssh/src/ssh.rs` into `SessionConfig`.
- Add `late-ssh/src/app/games/dartboard/` with `mod.rs`, `svc.rs`, `state.rs`, `input.rs`, and `ui.rs`.

### Phase C: arcade wiring

**Goal:** make dartboard reachable from the existing Games hub.

- Add `pub mod dartboard` in `late-ssh/src/app/games/mod.rs`.
- Add `dartboard_state` to `late-ssh/src/app/state.rs::App` and initialize it in `App::new`.
- Add render dispatch in `late-ssh/src/app/games/ui.rs` and `late-ssh/src/app/render.rs`.
- Add tick draining in `late-ssh/src/app/tick.rs`.
- Expand the selector count in `late-ssh/src/app/games/input.rs` and renumber Multiplayer rows so Dartboard gets a real selectable slot.
- Recommended slotting:
  - `6` = Blackjack
  - `7` = Dartboard
  - shift current placeholder multiplayer rows up by one
- Update arcade copy in the Games hub and help modal to mention Dartboard.

### Phase D: input/runtime upgrades in late-sh

**Goal:** let late-sh pass enough input detail to support dartboard cleanly.

- Extend `ParsedInput` in `late-ssh/src/app/input.rs` beyond byte-oriented game routing.
- Add modifier-aware key events for the combinations dartboard actually uses:
  - `Shift+Arrow`
  - `Alt+Arrow`
  - `Ctrl+Shift+Arrow`
  - `Home`
  - keep existing `BackTab`, `PageUp`, `PageDown`, `Delete`, paste
- Replace `MousePress` with a richer mouse event model carrying:
  - phase (`Down`, `Up`, `Drag`, `Moved`, `Scroll`)
  - button (`Left`, `Right`)
  - modifiers
  - coordinates
- Parse SGR mouse modifier bits instead of dropping them.
- Parse modified arrow CSI forms instead of only plain / ctrl arrows.
- Preserve the existing pending-`Esc` logic so bare `Esc` remains available for local dartboard cancel behavior without breaking Alt-prefixed sequences.
- Route `Ctrl+Q` to "leave dartboard" in the games shell without conflicting with existing global quit behavior.
- Update `App::enter_alt_screen()` / `leave_alt_screen()` so late-sh requests and later disables the mouse tracking level dartboard needs.
- Add parser tests for the new escape-sequence forms before game integration lands.

### Phase E: embedded dartboard module

**Goal:** adapt dartboard's client/session model to late-sh's `svc.rs` / `state.rs` / `input.rs` / `ui.rs` pattern.

- `svc.rs`
  - owns one local dartboard client per SSH session
  - drains server messages on a spawned task
  - publishes latest canvas snapshot via `watch`
  - publishes peer join/leave, ack, and reject events via `broadcast`
  - fire-and-forget op submission methods only
- `state.rs`
  - owns late-sh-local UI state: viewport, cursor, mode, swatches, selection, floating selection, emoji picker, peer list
  - drains `watch` + `broadcast` in `tick()`
  - keeps no authoritative canvas state outside the latest snapshot
- `input.rs`
  - consumes the richer parsed input events rather than plain bytes only
  - keeps bare `Esc` for standalone-style local cancel behavior
  - uses `Ctrl+Q` for "leave dartboard"
- `ui.rs`
  - pure ratatui draw
  - preferred path is a reusable canvas widget extracted from dartboard once the API settles
  - temporary fork is acceptable only if extraction blocks the integration

### Phase F: verification

- Add multiplayer service tests in `late-ssh/tests/games/`:
  - two simulated sessions subscribe to the same host
  - session A paints
  - session B ticks and sees the change
  - reject path coverage
  - peer join/leave coverage
- Add input-parser tests for modified arrows and richer mouse sequences.
- Run:
  - `cargo check -p late-ssh`
  - `cargo check -p late-ssh --tests`
  - targeted dartboard integration tests

### Open questions to resolve during implementation

- Widget extraction timing: land the shared canvas widget first, or temporarily fork `ui.rs` into late-sh and extract after the gameplay path is stable?

---

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
