# Games Context

## Metadata
- Scope: `late-ssh/src/app/games`
- Last updated: 2026-07-08
- Purpose: shared game-domain primitives and services used across game surfaces (Arcade, Rooms, and the upcoming Daily correspondence domain).

## Source Map
- `mod.rs` declares shared game modules only.
- `cards.rs` defines card ranks, suits, `PlayingCard`, and ASCII card rendering themes used by Solitaire plus room card games.
- `chips/svc.rs` owns the Late Chips economy adapter: login ensure, bet debits, payout credits, floor restore, Activity-driven daily puzzle rewards, and reward-template claims for room-game daily/cooldown/lifetime payouts. SQL stays in `late-core` models.
- `chess_core/` is the room-agnostic chess kernel extracted from `rooms/chess` (see `devdocs/FRD-DAILY.md`):
  - `types.rs`: `ChessColor`, `ChessPieceKind`, `ChessPiece`, `ChessGameResult`, `ChessMoveSpec`, `ChessMoveRecord`, `ChessPieceRenderMode`, `piece_glyph`.
  - `rules.rs`: pure helpers over `cozy_chess::Board` (legal move generation, queen-promotion move resolution, SAN labels, piece-array projection, repetition counting).
  - `board_ui.rs`: the tiered board renderer (`Tier`/`pick_tier`, `BoardCtx`, `draw_board`, mouse `square_at`, `king_square`). Callers pass a plain `[Option<ChessPiece>; 64]` plus display context, never a table snapshot; piece-graphics image ids derive from a caller-supplied `placement_seed` Uuid (rooms passes `room_id`; other surfaces pass their own stable id).
  - `piece_art.rs`: embedded PNG piece graphics for Kitty/iTerm2/Sixel plus tier thresholds.
  - `cursor.rs`: orientation-aware board cursor movement and legal-target filtering.

## Boundaries
- `games` must not depend on `arcade` or `rooms`.
- `arcade` owns solo Arcade screen/runtime/UI.
- `rooms` owns persistent multiplayer room runtime/UI, including the timed chess table runtime (`rooms/chess`: seats, ready flow, clocks, `runtime_state` persistence, room chrome). `chess_core` here owns only rules, shared types, and the bare board renderer.
- Shared primitives belong here only when more than one game surface needs them.
- Do not move `RoomGameManager`, `ActiveRoomBackend`, `RoomGameRegistry`, create modals, room settings, or runtime state into `app/games`. Those are Rooms-owned abstractions; `app/games` is only for cross-domain primitives/services such as cards, chips, and the chess kernel.
