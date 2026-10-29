# chess_core (`late-ssh/src/app/games/chess_core`)

Part of the shared games domain (`../CONTEXT.md`). The match lifecycle,
persistence and screens are the daily domain's
(`late-ssh/src/app/lobby/daily/CONTEXT.md`, `GAMES.md`).

`chess_core/` is the surface-agnostic chess kernel. Daily chess and daily chess960 use it, differing only in the opening position, and the Arcade's Chess Puzzle (`late-ssh/src/app/arcade/chess_puzzle/`) plays its puzzles through the same rules, cursor and board renderer, PNG pieces included (its own fixed `placement_seed`; ASCII once a puzzle is finished, see `late-ssh/src/app/arcade/GAMES.md`):

## Files

- `types.rs`: `ChessColor`, `ChessPieceKind`, `ChessPiece`, `ChessGameResult`, `ChessMoveSpec`, `ChessMoveRecord`, `ChessPieceRenderMode`, `piece_glyph`.
- `rules.rs`: pure helpers over `cozy_chess::Board` (legal move generation, queen-promotion move resolution, SAN labels, piece-array projection, repetition counting), plus `random_chess960_board` (the shuffled back rank, bishops on opposite colours and king between its rooks, mirrored for Black) and `fen`, the one way a position is written down. **Castling has two gestures, one encoding.** cozy-chess is Chess960-native, so it generates a castle as the king capturing its own rook (`e1h1`): select the king, then the rook. `legal_moves` additionally emits the lichess/chess.com pair (the king pushed two squares toward the rook, `e1g1`) for every generated castle, so both squares light up as legal targets, and `legal_move_for` maps that pair back onto the rook square via the board's stored castle rights. **That second pair is offered from e1/e8 only** (`castle_king_landing` and `castle_rook_target` share the guard): a Chess960 king starting anywhere else has ordinary one-step moves onto c1/g1, so there it castles by taking its rook and nothing else. Everything downstream (SAN label, `board.play`, the persisted `move_history` from/to) sees only the king-captures-rook move, so history and last-move highlights stay uniform however the player castled.
- **FEN is Shredder notation for both variants** (`rules::fen`, i.e. `{board:#}`): castling rights name the rooks' own files, `HAha` where standard FEN writes `KQkq`. `KQkq` cannot name a rook that does not stand on a1/h1, so a Chess960 position written that way does not read back at all; one notation for both means no caller has to know which variant it is holding. `str::parse` accepts either spelling, so stored FENs in `KQkq` form keep loading.
- `board_ui.rs`: the tiered board renderer (`Tier`/`pick_tier`, `BoardCtx`, `draw_board`, mouse `square_at`, `king_square`). Callers pass a plain `[Option<ChessPiece>; 64]` plus display context, never a table snapshot; piece-graphics image ids derive from a caller-supplied `placement_seed` Uuid (daily passes `match_id`; other surfaces pass their own stable id).
- `piece_art.rs`: embedded PNG piece graphics for Kitty/iTerm2/Sixel plus tier thresholds.
- `cursor.rs`: orientation-aware board cursor movement and legal-target filtering.
