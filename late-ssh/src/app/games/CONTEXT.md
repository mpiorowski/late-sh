# Games Context

## Metadata
- Scope: `late-ssh/src/app/games`
- Last updated: 2026-07-28 (daily briscola joined `cards.rs`'s consumers; it owns its own rank set in `lobby/daily/briscola.rs` and uses this module only to draw faces, backs, and empty slots)
- Purpose: shared game-domain primitives and services used across game surfaces (Arcade, the house tables in `app/lobby/house`, and the Daily correspondence domain in `app/lobby/daily`).

## Source Map
- `mod.rs` declares shared game modules only.
- `cards.rs` defines card ranks, suits, `PlayingCard`, and ASCII card rendering themes (`Minimal`/`Boxed` one-line faces, `Outline` five-line faces, plus backs, empty slots, and a stock counter) used by Solitaire, the house card tables, and daily briscola. It renders cards and nothing else: there is no shared deck or shuffle here, and each game owns its own rank set and point values (briscola's ten-rank order generalizes to no other game).
- `chips/svc.rs` owns the Late Chips economy adapter: login ensure, bet debits, payout credits, floor restore, Activity-driven daily puzzle rewards, and reward-template claims for room-game daily/cooldown/lifetime payouts. SQL stays in `late-core` models. Every mutation names a `ChipMove` variant (closed roster in `late-core/src/models/chips.rs`: ledger reason, debit floor, source kind, earnings flag) and flows through `UserChips::apply` or a dedicated path (`restore_floor`, `transfer_gift`, game payout claims); the `chip_user_changed` notify comes from `user_chips` triggers (migration 128), never from call sites.
- `chess_core/` is the surface-agnostic chess kernel (extracted from the demolished rooms chess table; see `devdocs/FRD-DAILY.md`). Daily chess is its only consumer today:
  - `types.rs`: `ChessColor`, `ChessPieceKind`, `ChessPiece`, `ChessGameResult`, `ChessMoveSpec`, `ChessMoveRecord`, `ChessPieceRenderMode`, `piece_glyph`.
  - `rules.rs`: pure helpers over `cozy_chess::Board` (legal move generation, queen-promotion move resolution, SAN labels, piece-array projection, repetition counting). **Castling has two gestures, one encoding.** cozy-chess is Chess960-native, so it generates a castle as the king capturing its own rook (`e1h1`): select the king, then the rook. `legal_moves` additionally emits the lichess/chess.com pair (the king pushed two squares toward the rook, `e1g1`) for every generated castle, so both squares light up as legal targets, and `legal_move_for` maps that pair back onto the rook square via the board's stored castle rights. Everything downstream (SAN label, `board.play`, the persisted `move_history` from/to) sees only the king-captures-rook move, so history and last-move highlights stay uniform however the player castled.
  - `board_ui.rs`: the tiered board renderer (`Tier`/`pick_tier`, `BoardCtx`, `draw_board`, mouse `square_at`, `king_square`). Callers pass a plain `[Option<ChessPiece>; 64]` plus display context, never a table snapshot; piece-graphics image ids derive from a caller-supplied `placement_seed` Uuid (daily passes `match_id`; other surfaces pass their own stable id).
  - `piece_art.rs`: embedded PNG piece graphics for Kitty/iTerm2/Sixel plus tier thresholds.
  - `cursor.rs`: orientation-aware board cursor movement and legal-target filtering.

## Boundaries
- `games` must not depend on `arcade` or `lobby`.
- `arcade` owns solo Arcade screen/runtime/UI.
- `chess_core` here owns only rules, shared types, and the bare board renderer; the daily domain owns match lifecycle/persistence and the board screen chrome.
- Shared primitives belong here only when more than one game surface needs them.
- Do not move house-table registries, table settings, or runtime state into `app/games`. Those are Lobby-owned; `app/games` is only for cross-domain primitives/services such as cards, chips, and the chess kernel.
