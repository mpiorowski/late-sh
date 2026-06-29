# Games Context

## Metadata
- Scope: `late-ssh/src/app/games`
- Last updated: 2026-06-17
- Purpose: shared game-domain primitives and services used by both Arcade and Rooms.

## Source Map
- `mod.rs` declares shared game modules only.
- `cards.rs` defines card ranks, suits, `PlayingCard`, and ASCII card rendering themes used by Solitaire plus room card games.
- `chips/svc.rs` owns the Late Chips economy adapter: login ensure, bet debits, payout credits, floor restore, Activity-driven daily puzzle rewards, and reward-template claims for room-game daily/cooldown/lifetime payouts. SQL stays in `late-core` models.

## Boundaries
- `games` must not depend on `arcade` or `rooms`.
- `arcade` owns solo Arcade screen/runtime/UI.
- `rooms` owns persistent multiplayer room runtime/UI.
- Shared primitives belong here only when both Arcade and Rooms need them.
- Do not move `RoomGameManager`, `ActiveRoomBackend`, `RoomGameRegistry`, create modals, room settings, or runtime state into `app/games`. Those are Rooms-owned abstractions; `app/games` is only for cross-domain primitives/services such as cards and chips.
