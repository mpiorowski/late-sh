# FRD: Daily Games (correspondence chess v1)

Design doc for the daily-games system agreed in the 2026-07-08 discussion
(UP.md step 1, scoped to v1). Status: draft for review, no code yet.

## Goal

Async-first multiplayer that works at ~30 concurrent users. A daily game is a
correspondence match between two fixed players: you post a challenge, walk
away, and play one move whenever you're around. Chess only in v1. The system
lives in the right sidebar on Home (the highest-traffic surface), not behind
the Tables directory.

Non-goals for v1: wagers/escrow, spectating, games other than chess,
tournaments. The schema leaves room for all of them.

## Product shape

- **Open lobby is the centerpiece.** Anyone can post an open challenge;
  anyone can claim it; claiming starts a match. Open challenges persist until
  claimed or cancelled ("wait forever for an opponent").
- **Directed challenges ride along**: `/challenge @user chess` from the chat
  composer, or from the modal. Same row, `target_user_id` set.
- **24h per move** (fixed in v1). Missing the deadline forfeits.
- **Cap: 5 active entries per user** (active matches + your open challenges
  combined). Keeps the panel honest, prevents challenge spam.
- **Winner payout** via the existing reward-template path (no peer wagers).
  That payout is the entire economy/social footprint of v1: no @dealer
  wiring, no #lounge announcements, no `ActivityEvent` publishing (so no
  quest integration). The sidebar panel is the only broadcast surface.

## Architecture decision: separate domain, shared chess core

Daily matches do NOT live in `game_rooms`. A daily match is a relationship
between two people, not a place: no seats, no AFK timers, no embedded chat
room, no `open/in_round` lifecycle, no hourly idle sweep. Forcing it into the
rooms system means special-casing every one of those behaviors (chess already
carries two special cases in `GameRoom::delete_inactive_open` and
`reconcile_in_round_after_restart` today).

What IS shared is the chess itself. Rules come from the `cozy-chess` crate
(workspace dep, 0.3.4); the repo code around it is pure helpers and a mostly
pure board renderer. Those get extracted once and used by both runtimes:

```
late-ssh/src/app/games/chess_core/   <- new shared module (games/ CONTEXT.md
                                        already scopes this dir to shared
                                        primitives)
late-ssh/src/app/rooms/chess/        <- shrinks to the live-table shell
late-ssh/src/app/daily/              <- new domain (correspondence runtime)
late-core/src/models/daily_match.rs  <- persistence
```

### chess_core extraction (PR 1, zero behavior change)

Peel out of `rooms/chess/`:

| chess_core file | Moves from | Contents |
|---|---|---|
| `rules.rs` | `chess/svc.rs` | `legal_moves`, `legal_move_for`, move application producing SAN label (`display_san_move`) + new FEN, threefold-repetition count over a `Vec<Board>` history, status mapping (`GameStatus` -> result). Pure functions over `cozy_chess::Board`. |
| `types.rs` | `chess/state.rs` / `svc.rs` | `ChessColor`, `ChessPieceKind`, `ChessMoveSpec`, `ChessMoveRecord`, the board-cell snapshot data the renderer consumes. |
| `board_ui.rs` | `chess/ui.rs` | `draw_board` + `BoardCtx` (orientation, cursor, selected, last move, check square). Already driven by snapshot + plain params. The one coupling to break: piece-graphics image ids are keyed by `room_id` (`piece_placement_id`); parameterize the id seed (any `Uuid` works, daily passes `match_id`). Keep the `terminal_image` dependency, it's crate-internal. |
| `piece_art.rs` | `chess/piece_art.rs` | Moves wholesale. |
| `cursor.rs` | `chess/state.rs` | Board cursor/selection/legal-target navigation, shared by table view and daily board view. |

Stays in `rooms/chess/`: `ChessService`/`SharedState` (seats, ready-up,
countdown clocks via `tokio::sleep_until`, `RoomGameEvent`, `ChipService` /
`ActivityPublisher` wiring, `game_rooms.runtime_state` persistence), manager,
create modal, settings, room chrome in `ui.rs`. All of it now importing
chess_core. The duplicated `orienting_color` default in `state.rs`/`ui.rs`
collapses into chess_core while we're there.

## Persistence

One table, one model. Single-table over challenge+match pair because a
challenge IS a match waiting for its second player, and the sidebar panel
wants exactly one query.

### Migration `102_create_daily_matches.sql`

```sql
CREATE TABLE daily_matches (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    game_kind TEXT NOT NULL DEFAULT 'chess',
    status TEXT NOT NULL DEFAULT 'open'
        CHECK (status IN ('open', 'active', 'finished', 'cancelled')),
    challenger_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    opponent_id UUID REFERENCES users(id) ON DELETE CASCADE,
    target_user_id UUID REFERENCES users(id) ON DELETE CASCADE,
    turn_user_id UUID REFERENCES users(id) ON DELETE CASCADE,
    turn_deadline_at TIMESTAMPTZ,
    winner_user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    result TEXT NOT NULL DEFAULT '',
    state JSONB NOT NULL DEFAULT '{}'::jsonb
);
CREATE INDEX idx_daily_matches_status ON daily_matches(status);
CREATE INDEX idx_daily_matches_turn_user
    ON daily_matches(turn_user_id) WHERE status = 'active';
```

- `status='open'`: a challenge. `opponent_id` NULL, `target_user_id` NULL for
  open-lobby or set for directed. No expiry in v1 (`cancelled` by creator is
  the only exit besides claim).
- Claim: `open -> active` sets `opponent_id`, assigns colors (random, stored
  in `state`), sets `turn_user_id` = white, `turn_deadline_at = now + 24h`.
  Claim must be a guarded UPDATE (`WHERE status='open' AND opponent_id IS
  NULL`) so two simultaneous claims can't both win.
- `result`: `checkmate`, `draw`, `resign`, `timeout`, plus `''` while running.
- `state` JSON mirrors the proven `ChessRuntimeState` shape minus room
  concepts: `{ version: 1, revision, fen, colors: {white: uuid, black: uuid},
  move_history: [{label, from, to, at}], position_history: [fen] }`.
- Writes go through `DailyMatch::update_state` with the same SQL monotonic
  revision guard as `GameRoom::update_runtime_state` (apply only when stored
  revision <= incoming). Model registers as `pub mod daily_match;` in
  `models/mod.rs`, using `model!` with bespoke methods for claim/move/finish.

Deadlines are durable by construction: `turn_deadline_at` is a DB timestamp,
not an in-process `sleep_until` (the rooms-chess clock approach explicitly
does not survive restarts; this one must).

## Service: `late-ssh/src/app/daily/svc.rs`

`DailyService`, process-global singleton like `RoomsService`:

- **Snapshot**: one global `watch::Receiver<Arc<DailySnapshot>>` with
  `open_challenges: Vec<...>` and `active_matches: Vec<...>` (id, players,
  turn_user_id, deadline, move count). At a 5-per-user cap and this scale
  that's tens of rows; per-session UI state filters for "mine". Refresh from
  DB on every mutation plus a slow poll (60s) as backstop.
- **Events**: `broadcast<DailyEvent>`: `ChallengePosted`, `ChallengeClaimed`,
  `MovePlayed { match_id, by_user_id, label }`, `MatchFinished { match_id,
  winner, result }`, targeted `Error { user_id, message }` for banners.
- **Mutating tasks** (fire-and-forget, per repo convention):
  `post_challenge_task` (enforces the 5-cap), `claim_challenge_task`,
  `cancel_challenge_task`, `play_move_task` (validates via
  `chess_core::rules` against `state.fen`, bumps revision, flips
  `turn_user_id`, resets deadline), `resign_task`.
- **Deadline sweeper**: one spawned loop, every 60s:
  `WHERE status='active' AND turn_deadline_at < now()` forfeits (winner =
  other player, result `timeout`). Survives restarts for free.
- **No live actor per match.** Every move loads state, validates, persists.
  No `HashMap<id, Service>`, nothing to reconcile after restart.

Payout: on `MatchFinished` with a decisive winner, credit via the existing
`ChipService::credit_cooldown_reward_template` with a new template row seeded
in the migration: key `daily_chess_win_payout`, 500 chips, `claim_policy =
'cooldown'`, 3600s, params `{"game":"daily_chess","payout_kind":"win"}`
(mirrors `chess_win_payout` from migration 056).

## UI

Three surfaces, one system of record.

### 1. Sidebar panel (passive, fixed height)

New `RightSidebarComponent::Daily` (key `"daily"`, label `Daily Games`).
Touches, per the existing pattern: `late-core/src/models/user.rs` (variant,
`ALL`, count const, `as_str`/`from_key`/`label`; the JSON round-trip is
hand-rolled so both directions matter), `common/sidebar.rs` (height const
`DAILY_HEIGHT = 8`, `component_height`, render arm, `SidebarProps` field),
`render.rs` (populate props from daily UI state). Settings dialog rows are
data-driven, no change.

Fixed 8 rows, stable chrome: slots render dashes when empty, the panel never
changes height between states.

```
 daily games ──────────
 ► mira        your turn     <- glows; sorted: your turn first,
   c0ld        waiting          then nearest deadline
   ─
 lobby: 1 open · c0ld        <- open-lobby activity, glows when
   ─                            new since last modal open
 g games · c challenge
```

The lobby line is the liquidity engine: other people's open challenges are
visible exactly where everyone idles. New-challenge glow clears when the
modal is opened.

Defaults: `default_right_sidebar_components()` order becomes
`[Visualizer, Pet(disabled), Bonsai, Daily, Music]`. Cut-from-top shrink then
drops visualizer first and keeps daily + music longest (matches the agreed
shrink priority; pet leaves the default set but stays available in settings,
and existing users' stored orders are respected, with `normalize_*`
backfilling `Daily` enabled at the end of their list).

### 2. Daily Games modal (all interaction)

Follows the Hub/bonsai-modal pattern: `App`-level `show_daily` +
`daily/state.rs` modal state, input intercepted while open. Opened by `g`
(not composing) and from the panel hint. `g` appears unbound globally today;
verify against `handle_global_key` during implementation and fall back to
`Ctrl+J`-style chord only if a collision surfaces.

Sections (j/k navigate, single scrollable list, no tabs):

- **Your matches**: opponent, color, move count, deadline countdown, your-turn
  marker. `Enter` opens the board.
- **Lobby**: every open challenge (yours marked, cancellable with `x`).
  `Enter` claims with a confirm prompt.
- **Footer actions**: `c` post open challenge, `C` post directed challenge
  (username prompt), `Esc` close.

Composer command `/challenge @user chess` (chat input) posts a directed
challenge through the same `post_challenge_task`; `/challenge chess` posts an
open one.

### 3. Board screen

Full-screen match view, reusing `chess_core::board_ui` + `cursor`, with a
minimal frame: players/colors, move list, deadline, result banner. Entered
only from the modal; `Esc` returns to Home. Implemented like the door games:
a `Screen::DailyMatch` outside the Tab cycle, not a room backend
(`RoomGameManager`'s one-room-one-live-table shape doesn't fit many
concurrent matches per user, confirmed in exploration).

Move flow: cursor + Enter/click exactly like table chess, then
`play_move_task`; the optimistic UI shows the move immediately and reconciles
on the next snapshot.

## Notifications

- **Your-turn desktop notify**: daily UI state holds a cloned
  `notify::Notifier` (same wiring as chat/rooms producers) and pushes a
  `Kind::GameEvents` notification on the became-my-turn edge while connected.
- **On-login awareness**: the panel itself (glow + "your turn" rows) is the
  v1 login nudge. No login modal.
- **Explicitly NOT in v1** (deferred by decision, 2026-07-08): #lounge
  announcements via @dealer, and `ActivityEvent` publishing (quest wiring).
  When announcements do land later, they go through a `DailyEvent`
  subscriber posting via `ChatService::send_lounge_message_task`, preserving
  the "games never post to chat directly" boundary.

## Tables cleanup (last PR)

- Remove `Daily` from `TIME_CONTROL_OPTIONS` in `rooms/chess/settings.rs` so
  new tables are Blitz/Rapid only. Keep the `ChessTimeControl::Daily` variant
  and `from_id` parsing so any existing daily table rows deserialize; they
  finish or idle out under the existing chess TTL and the variant gets
  deleted in a later pass.
- The chess special cases in `delete_inactive_open` /
  `reconcile_in_round_after_restart` stay (they also protect live
  blitz/rapid games mid-flight).

## Delivery plan

1. **PR 1, chess_core extraction.** Pure refactor, table chess behavior
   identical. Riskiest to review, so it carries nothing else.
2. **PR 2, daily backend.** Migration 102 + `daily_match.rs` model +
   `DailyService` (snapshot, events, tasks, sweeper) + reward template +
   integration tests under `late-ssh/tests/daily/` (svc: post/claim race,
   move validation, revision guard, sweeper forfeits, 5-cap) via
   `test_db()`.
3. **PR 3, UI.** Sidebar panel + settings default changes, modal, board
   screen, `/challenge` command, keybindings (help modal `data.rs`, footer
   lines, CONTEXT.md shortcut table per the keybinding checklist).
4. **PR 4, glue.** Desktop your-turn notify, Tables cleanup,
   `daily/CONTEXT.md` + root CONTEXT.md routing row.

Unit tests inline and pure per test policy (rules helpers, snapshot
filtering, deadline math); no `cargo test` runs by agents, `cargo check
--tests` as verification.

## Open questions (non-blocking)

- Move deadline: fixed 24h shipped first; a 1d/3d choice on the challenge is
  a one-enum follow-up if people ask.
- Draw offers: not in v1 (draws happen via stalemate/repetition/insufficient
  material only). Resign exists. Revisit if requested.
- Rematch one-key ("play again, colors swapped") is cheap and probably worth
  sneaking into PR 3 if the modal has room.

## Future hooks this design leaves open

- #lounge announcements (@dealer subscriber on `DailyEvent`) and quest
  wiring (`ActivityEvent` publishing): both deferred from v1, both pure
  additions to the event stream with no schema impact.
- Wager escrow: add `wager` column + hold/settle in `ChipService`; the
  claim/finish paths are the only touch points.
- Seeded arcade score duels: same table (`game_kind='duel_snake'` etc.),
  `state` holds seed + submitted scores, sweeper settles at deadline.
- Spectating: render `state.fen` read-only from any row; no live service
  needed.
- More correspondence games: anything expressible as "state JSONB + whose
  turn + deadline" slots in without schema change.
