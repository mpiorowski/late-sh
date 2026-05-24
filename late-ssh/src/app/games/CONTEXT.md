# Games Context

## Metadata
- Scope: `late-ssh/src/app/games`
- Last updated: 2026-05-22
- Purpose: shared game-domain primitives and services used by both Arcade and Rooms.

## Source Map
- `mod.rs` declares shared game modules only.
- `cards.rs` defines card ranks, suits, `PlayingCard`, and ASCII card rendering themes used by Solitaire plus room card games.
- `chips/svc.rs` owns the Late Chips economy service used by Arcade rewards and room-game settlements.

## Boundaries
- `games` must not depend on `arcade` or `rooms`.
- `arcade` owns solo Arcade screen/runtime/UI.
- `rooms` owns persistent multiplayer room runtime/UI.
- Shared primitives belong here only when both Arcade and Rooms need them.
