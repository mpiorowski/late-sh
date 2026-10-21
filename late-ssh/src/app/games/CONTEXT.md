# Games Context

## Metadata
- Scope: `late-ssh/src/app/games`
- Purpose: shared game-domain primitives and services used across game surfaces (Arcade, the house tables in `app/lobby/house`, and the Daily correspondence domain in `app/lobby/daily`).

## Source Map
- `mod.rs` declares shared game modules only.
- `cards.rs` defines card ranks, suits, `PlayingCard`, and ASCII card rendering themes (`Minimal`/`Boxed` one-line faces, `Outline` five-line faces, plus backs, empty slots, and a stock counter) used by Solitaire, the house card tables, and daily briscola, cribbage, and gin rummy. It renders cards and nothing else: there is no shared deck or shuffle here, and each game owns its own rank set and point values (briscola's ten-rank order generalizes to no other game; daily cribbage and gin share the 52-card deck in `app/lobby/daily/std_deck.rs`).
- `chips/svc.rs` owns the Late Chips economy adapter: login ensure, bet debits, payout credits, floor restore, Activity-driven daily puzzle rewards, and reward-template claims for room-game daily/cooldown/per-event payouts. SQL stays in `late-core` models. Every mutation names a `ChipMove` variant (closed roster in `late-core/src/models/chips.rs`: ledger reason, debit floor, source kind, earnings flag) and flows through `UserChips::apply` or a dedicated path (`restore_floor`, `transfer_gift`, game payout claims); the `chip_user_changed` notify comes from `user_chips` triggers (migration 128), never from call sites.
- `chess_core/` is the surface-agnostic chess kernel behind daily chess and chess960 (FEN is Shredder notation for both): rules over `cozy_chess`, types, the tiered board renderer, piece art, the cursor. Per-file detail in `chess_core/CONTEXT.md`.
- `pool_core/` is the surface-agnostic billiard kernel behind daily eight-ball, nine-ball and snooker: SI table specs, closed-form ball motion, a deterministic shot loop, the aim line, the rules as a closed enum, the half-block and shooter's-eye renderers. **No new dependencies** (no Rust billiards crate fits). Per-file detail in `pool_core/CONTEXT.md`.

## Boundaries
- `games` must not depend on `arcade` or `lobby`.
- `arcade` owns solo Arcade screen/runtime/UI.
- `chess_core` and `pool_core` here own only rules, shared types, and the bare renderers; the daily domain owns match lifecycle/persistence and the board screen chrome.
- Shared primitives belong here only when more than one game surface needs them.
- Do not move house-table registries, table settings, or runtime state into `app/games`. Those are Lobby-owned; `app/games` is only for cross-domain primitives/services such as cards, chips, and the chess kernel.
