# PLAN.md — Arcade Economy & Multiplayer Roadmap

## Phase 1: Blackjack MVP (Single-Player)

**Goal:** single-player-per-session blackjack. Sit at your own table, bet 10–100 chips, hit/stand against a dealer, chips debited/credited via the existing economy. First step toward multiplayer table games — proves the game loop, the chip economy under active play, and the state/svc/input/ui module shape for future table games.

### Why Blackjack first

- Natural chip sink — betting is the whole point
- Simple rules, fast hands (~30s each)
- PvE so no matchmaking / coordination
- Establishes `state.rs` / `svc.rs` / `input.rs` / `ui.rs` pattern for Poker later
- Works in single-player mode today, upgrades to multi-seat cleanly when we wire the chat room later

### MVP scope — decisions locked in

| Decision | Choice |
|---|---|
| Seats per table | 1 (single-player-per-session, each SSH session has its own table) |
| Shoe | 6-deck casino shoe, reshuffles at penetration |
| Bet range | 10–100 chips (from `state.rs` `MIN_BET`/`MAX_BET`) |
| Dealer rule | Stands on soft 17 |
| Blackjack payout | 3:2 (rounded toward zero on odd bets) |
| Splits | Not in MVP |
| Doubles | Not in MVP |
| Insurance / even money | Not in MVP |
| Settlement | Optimistic — local balance updates the moment `settle()` returns; `credit_payout` runs fire-and-forget; `HandSettled` event is confirmation only |
| Arcade placement | Existing games picker, Enter on "Blackjack" enters the game |
| Chat room wiring | Deferred — migration 024 stays parked, unused in MVP |
| `refund_bet_task` | Dropped for MVP — no abandonment path exists |

### Already shipped

- `blackjack/state.rs` — pure math: `Bet`, `Outcome`, `score`, `settle`, `payout_credit`, `dealer_must_hit`, 16 inline unit tests
- `blackjack/svc.rs` — `BlackjackService` + `BlackjackEvent` broadcast; `place_bet_task` fully wired with `BetFailure` / tracing split
- `BlackjackEvent` is `Debug + Clone`, ready to subscribe from state
- Migration `024_add_game_rooms.sql` — present but unused in MVP (supports `kind='game'` + `game_kind` column + partial unique index on `(game_kind, slug)`)

### Work ahead, in dependency order

**1. `ChipService` additions** (~16 lines in `games/chips/svc.rs`)
```rust
pub async fn debit_bet(&self, user_id, amount)     -> Result<Option<i64>>;
pub async fn credit_payout(&self, user_id, amount) -> Result<i64>;
```
Thin wrappers around `UserChips::deduct` / `UserChips::add_bonus`. Prereq for `svc.rs`.

**2. Finish `blackjack/svc.rs`** (~40 lines)
- `settle_hand_task`: compute credit via `payout_credit(bet, outcome)`, call `credit_payout`, broadcast `HandSettled`. Follow the `place_bet` shape — private `settle_hand` helper returning `Result<i64, SettleFailure>`, tracing at task layer.
- `refund_bet_task`: **delete** — no caller in MVP.

**3. `blackjack/state.rs` — mutable runtime** (biggest chunk, ~200 lines)
- `Shoe` — 6-deck shuffled `Vec<PlayingCard>`, draws from top, reshuffles at penetration threshold
- `Phase` — `Betting | BetPending | PlayerTurn | DealerTurn | Settling`
- `BlackjackState` — holds shoe, dealer hand, player hand, bet, phase, `pending_request_id`, `last_outcome`, bet-input buffer, status message, svc clone, events receiver
- Methods:
  - `new(svc, user_id, balance)` — subscribes to events
  - `tick(&mut self)` — drains `BlackjackEvent`, matches by `request_id`, transitions phase
  - `submit_bet(amount)` — mints `request_id`, fires `place_bet_task`, phase → `BetPending`
  - `deal_initial()` — on successful `BetPlaced`, deal 2+2, check naturals, transition to `PlayerTurn` or straight to `Settling`
  - `hit()` — draws card, checks bust
  - `stand()` — phase → `DealerTurn`, then immediately `run_dealer`
  - `run_dealer()` — draws until `!dealer_must_hit()`, runs `settle()`, fires `settle_hand_task`, phase → `Settling`, updates local balance optimistically
  - `next_hand()` — clears hands, phase → `Betting`
- Unit tests for: bust transitions, natural-vs-natural push, dealer-stands-on-17 loop, ace promotion in live play

**4. `blackjack/input.rs`** (~80 lines)
- **Betting:** digits → bet buffer, Backspace removes, Enter → `submit_bet`, Esc → leave
- **BetPending:** ignore all input
- **PlayerTurn:** `h` or Space → hit, `s` → stand, Esc → auto-stand + leave
- **DealerTurn:** ignore (auto-running, instantaneous in MVP)
- **Settling:** any key → `next_hand`, Esc → leave

**5. `blackjack/ui.rs`** (~120 lines)
Dead-simple 3-section layout. Uses `games::cards::AsciiCardTheme::Minimal` for compact one-row cards.

```
╭── BLACKJACK ─────────────────────────────╮
│                                          │
│  Dealer:  [A♠] [??]         (—)          │
│                                          │
│  You:     [10♥] [7♣]        (17)         │
│                                          │
│  Balance: 450    Bet: 50    PlayerTurn   │
│  [h]it   [s]tand   [Esc] leave           │
╰──────────────────────────────────────────╯
```

Hole card shown as `??` until `DealerTurn`. Outcome banner during `Settling`: **BLACKJACK! +75** / **You win! +50** / **Push** / **Bust** / **Dealer wins**. No animation, no split panes, no chat panel.

**6. `blackjack/mod.rs`** (4 lines declaring submodules)

**7. Wiring into the app shell** (~30 lines, scattered — *risky*)
- `games/mod.rs` — `pub mod blackjack;`
- Games-level state — add `blackjack: BlackjackState` field
- Games picker — add "Blackjack" entry; Enter routes into the game
- Games input router — route to `blackjack::input` when active
- Games UI router — route to `blackjack::ui` when active
- `main.rs` / startup — instantiate `BlackjackService` alongside existing services
- Session config — plumb the service into `BlackjackState::new` at session init

Haven't yet surveyed how the current games picker owns per-game state — a quick read of `games/mod.rs` + one existing game's wiring is needed before this step.

### Milestones

| M | Deliverable | Verification |
|---|---|---|
| **M1** | `ChipService::debit_bet` + `credit_payout` added | `cargo check -p late-ssh` clean |
| **M2** | `svc.rs` complete (`settle_hand_task` landed, `refund_bet_task` removed) | compiles |
| **M3** | `state.rs` runtime types + methods, unit tests for phase transitions | unit tests pass (pure logic, no DB) |
| **M4** | `input.rs` + `ui.rs` + `mod.rs` — module compiles as a unit | compiles in isolation |
| **M5** | Wired into Games screen, navigable (placeholder OK) | reachable over SSH |
| **M6** | Full playable loop: bet → deal → hit/stand → settle → next hand | actually play a hand end-to-end |

Estimated ~500–700 LoC across 5 files plus ~30 LoC of wiring.

---

## Phase 2: Multi-Seat Blackjack (after MVP ships)

Turn the MVP single-player table into a true multi-seat table. This is where the chat room wiring, migration 024, seat management, turn timers, and AFK handling all land — pattern-matching the earlier architecture discussion.

**Scope of Phase 2:**
- Bind a table to a `ChatRoom` of `kind='game'`, `game_kind='blackjack'`, single permanent row seeded at startup (slug `bj-001`, not `game-blackjack`, so the 1→N path is free)
- Move `BlackjackTable` from per-session `BlackjackState` into a shared `Arc<Mutex<HashMap<RoomId, BlackjackTable>>>` owned by `BlackjackService`
- Seat management: 5 seats, sit/leave independent from chat membership
- Turn timers: 15s per action, 20s for betting, 3-strike AFK unseat
- Hard-disconnect hook via `SessionRegistry` drop → auto-stand + free seat at end of round
- Per-table chat (reuses existing chat infra once the room is wired)
- Split-pane UI: game table on top, scoped chat on bottom
- Activity feed broadcasts for big wins (`🃏 @mat won 80 chips at Blackjack`)
- Extract shared host concerns (seat state, turn timer, disconnect handling) into `app/games/table_host.rs` — wait for Poker to confirm the abstraction

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
