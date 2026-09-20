# Nightcap Context (late-ssh/src/app/nightcap)

## Metadata
- Domain: a small bar out back of the Clubhouse, top-level screen (contextual, not in the Tab cycle)
- Last updated: 2026-09-14 (added)
- Status: Active

## 1. Summary

Six sittable seats and a round of drinks — nothing else. Reached with `n`
from the Clubhouse (`clubhouse/input.rs`), Esc returns there (handled
centrally in `app/input.rs::dispatch_escape`, same as every other
contextual screen). Deliberately much smaller than the Clubhouse: no
walking, no floor plan, no AI bartender. See `clubhouse/CONTEXT.md` for
comparison — this module intentionally does not try to match its scale.

## 2. Module map

| File | Owns |
|---|---|
| `lobby.rs` | `SharedSeats`, the process-global `Arc<Mutex<..>>` seat state: who sits where, per-seat drink counts. |
| `state.rs` | Per-session view state: roster-refresh cadence, latest seat snapshot, the last action's flavor message. |
| `input.rs` | Number keys `1`-`6` sit/stand, `d` orders a drink. |
| `ui.rs` | Renderer: title, seat rows, footer. |

## 3. The shared seats (multiplayer contract)

- `crate::state::State.nightcap_lobby` is the single process-global
  `SharedSeats`, threaded into each session through
  `SessionConfig.nightcap_lobby` (same pattern as `clubhouse_lobby`).
  Single-replica by design, same constraint as the Clubhouse.
- Unlike the Clubhouse, nobody holds a seat by default: you are only in the
  room's shared state once you press a seat number. `App::tick_nightcap`
  (called from `tick.rs` alongside `tick_clubhouse`) only evicts
  disconnected occupants; it never auto-seats anyone.
- **A stool is held only while its owner is in the room.** Leaving the
  screen by any route (Esc, `0`, Tab, a page digit) runs
  `State::leave_screen` from `App::set_screen`, which vacates the seat. This
  is load-bearing, not tidiness: `SharedSeats::sync` evicts only users who
  dropped out of `active_users`, which means disconnected, so a seat kept
  across a screen change would outlive the visit and six of them would close
  the six-stool bar for the rest of those sessions.
- Pressing your own occupied seat's number again stands you up. Pressing a
  stool someone else holds is refused with `SeatChange::Taken`, which the
  footer prints; a bounced press is the one this room sees most, and
  silence there reads as a dropped keypress.
- Occupant names are re-read from the roster on every `sync`, never cached
  at sit time, so a rename reaches the stool. Root `CONTEXT.md` §8.1 names
  seat labels as the case not to build a per-feature username cache for.

## 4. Drinks are cosmetic (deliberately, for now)

- "Order a drink" (`d`) increments an in-memory per-seat counter shown as
  `●` marks next to the occupant's name (capped display at 5). It does
  **not** touch the real chip economy
  (`app::games::chips::svc::ChipService::buy_drink`) or the DB-backed
  drunk-level system the Clubhouse's `@bartender` flow uses
  (`late_core::models::drinks`).
- This was a scope choice, not an oversight: wiring a synchronous input
  handler into an async DB call requires understanding this app's
  task/channel dispatch pattern for that, which is real integration work
  beyond a small companion room. If this room earns its keep, hooking
  `order_drink` up to `ChipService::buy_drink` (spending real chips, feeding
  the same drunk-level decay math) is the natural next step — see
  `clubhouse/lobby.rs`'s `DrunkEntry`/`set_drunk_states` for the pattern to
  follow.

## 5. Testing

- `lobby_test.rs`: seat toggle/move/collision, vacating, drink counting,
  roster eviction and relabelling. Pure `SharedSeats` behavior.
- `state_test.rs`: the seat is given back on `leave_screen`, a taken stool
  reports itself, and the `ROSTER_REFRESH_TICKS` cadence plus the
  `enter_screen` override. `State::new` takes an `Option<SharedSeats>`, a
  `Uuid` and a name, so neither file needs an `App` fixture.
- No `ui_test.rs`/`input_test.rs`: both are thin enough that the behavior
  they carry is asserted a layer down.
