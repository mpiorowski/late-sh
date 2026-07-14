# Games Context

## Metadata
- Scope: `late-ssh/src/app/games`
- Last updated: 2026-07-08
- Purpose: shared game-domain primitives and services used across game surfaces (Arcade, the house tables in `app/lobby/house`, and the Daily correspondence domain in `app/lobby/daily`).

## Source Map
- `mod.rs` declares shared game modules only.
- `cards.rs` defines card ranks, suits, `PlayingCard`, and ASCII card rendering themes used by Solitaire plus room card games.
- `chips/svc.rs` owns the Late Chips economy adapter: login ensure, bet debits, payout credits, floor restore, Activity-driven daily puzzle rewards, and reward-template claims for room-game daily/cooldown/lifetime payouts. SQL stays in `late-core` models.
- `chess_core/` is the surface-agnostic chess kernel (extracted from the demolished rooms chess table; see `devdocs/FRD-DAILY.md`). Daily chess is its only consumer today:
  - `types.rs`: `ChessColor`, `ChessPieceKind`, `ChessPiece`, `ChessGameResult`, `ChessMoveSpec`, `ChessMoveRecord`, `ChessPieceRenderMode`, `piece_glyph`.
  - `rules.rs`: pure helpers over `cozy_chess::Board` (legal move generation, queen-promotion move resolution, SAN labels, piece-array projection, repetition counting).
  - `board_ui.rs`: the tiered board renderer (`Tier`/`pick_tier`, `BoardCtx`, `draw_board`, mouse `square_at`, `king_square`). Callers pass a plain `[Option<ChessPiece>; 64]` plus display context, never a table snapshot; piece-graphics image ids derive from a caller-supplied `placement_seed` Uuid (daily passes `match_id`; other surfaces pass their own stable id).
  - `piece_art.rs`: embedded PNG piece graphics for Kitty/iTerm2/Sixel plus tier thresholds.
  - `cursor.rs`: orientation-aware board cursor movement and legal-target filtering.

## Boundaries
- `games` must not depend on `arcade` or `lobby`.
- `arcade` owns solo Arcade screen/runtime/UI.
- `chess_core` here owns only rules, shared types, and the bare board renderer; the daily domain owns match lifecycle/persistence and the board screen chrome.
- Shared primitives belong here only when more than one game surface needs them.
- Do not move house-table registries, table settings, or runtime state into `app/games`. Those are Lobby-owned; `app/games` is only for cross-domain primitives/services such as cards, chips, and the chess kernel.
