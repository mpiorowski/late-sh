# Pot Context

## Metadata
- Domain: the weekly pot, late.sh's parimutuel raffle and its largest concurrency-safe chip sink
- Scope: `late-ssh/src/app/pot/`, `late-core/src/models/pot.rs`, migration 160, the status HUD badge, the `/pot` composer commands
- Read this before: changing the ticket price or the cap, the draw hour, the payout split, the sweeper, or anything that reads `pots` / `pot_tickets`
- Related: root `CONTEXT.md` (routing, chips), `SHOP.md` phase 5 (the decided design and the fixed numbers), `late-ssh/src/app/crown/svc.rs` (the service shape this copies)

## 1. Summary

One pot is open at a time. Tickets cost a flat 100 chips, at most 10 per
player per UTC day (so a full week is 70, and nobody can buy them all on
Monday). Monday 21:00 UTC the pot draws: one ticket is pulled weighted by
holding, its owner takes 80% of everything the tickets paid in, and the other
fifth has no credit row anywhere. That gap is the burn. The next pot opens in
the same transaction, so there is always exactly one open pot and `/pot`
never has to answer "there isn't one".

There is no house wallet and no stored running total. A live pot's size is
`SUM(pot_tickets.count) * ticket_price`; `pots.ticket_count` and
`pots.payout_chips` are written once, at the draw, as the settled record.

## 2. Module map

| File | Owns |
|---|---|
| `late-core/src/models/pot.rs` | Both tables, the fixed numbers (`POT_TICKET_PRICE`, `POT_MAX_TICKETS_PER_DAY`, `POT_DRAW_WEEKDAY`, `POT_DRAW_HOUR_UTC`), and the pure money math: `payout_for`, `next_draw_at`, `draw_from_seed`. |
| `late-core/src/models/pot_test.rs` | The whole-state draw assertion, the weighting guard, the cap-in-the-insert test, the one-open-pot and one-sweeper-settles tests, the notify test. |
| `svc.rs` | `PotService`: the shared snapshot, the buy transaction, the sweeper, the Postgres listener, every refusal and every log line. |
| `state.rs` | `PotView` (the per-session projection the HUD badge draws) and the one countdown format. |
| `svc_test.rs` / `state_test.rs` | Adjacent tests for each. |

Commands are parsed in `late-ssh/src/app/chat/state.rs` (`parse_pot_command`,
`PotCommand`) and carried out in `App::tick_pot`
(`late-ssh/src/app/state.rs`), the same shape as the crown.

## 3. The fixed numbers

All of them live in `late-core/src/models/pot.rs` and are decided in SHOP.md's
fixed-numbers table. Change them there, not in a code comment.

| Dial | Value | Constant |
|---|---|---|
| Cadence | weekly (decided 2026-08-27; the design said daily, but at this size a daily pot is an arcade afternoon and its thresholds never fire) | `next_draw_at` |
| Ticket price | 100 chips | `POT_TICKET_PRICE` |
| Per-user cap | 10 tickets per UTC day (70 a week; decided 2026-08-27 so the raffle is about showing up, and 1,000 a day is reachable by anyone who plays) | `POT_MAX_TICKETS_PER_DAY` |
| Payout | 80% of the ticket sum, floored | `payout_for` |
| Draw | Monday 21:00 UTC (late evening in Europe, afternoon across the US) | `POT_DRAW_WEEKDAY`, `POT_DRAW_HOUR_UTC` |

## 4. Chips

Two `ChipMove` variants, both in the roster in `late-core/src/models/chips.rs`:

- `PotTicket`: debit, floor-guarded at `CHIP_FLOOR`, `source_ref` is the pot
  id. One ledger row per buy, not per ticket.
- `PotWon`: credit, `source_ref` is the pot id.

**Both are `counts_as_earnings = false`.** The win is excluded because a
lottery must not top the earners board; the ticket is excluded because if the
win is out and the ticket is in, buying into the pot would be a pure negative
on a board the winner cannot climb back up.

The burn is the gap between the two reasons, exactly like the gild's missing
third: there is no third ledger row and no wallet holding the fifth.

## 5. Concurrency

Three separate guards, none of which replaces another:

1. **The partial unique index** `pots_single_open` (`ON pots ((status =
   'open')) WHERE status = 'open'`) makes "at most one open pot" a table fact.
2. **`Pot::lock_open`** takes a `pg_advisory_xact_lock` before its
   `SELECT ... FOR UPDATE`. The advisory lock is what makes *opening* a pot
   exact: when none is open there is no row to lock, so two replicas sweeping
   the same second would both insert and one would die on the index. It is
   also what makes the losing replica see the successor pot: its `SELECT`
   statement starts after the winner commits, so it takes a fresh snapshot,
   finds the new pot, sees it is not due, and does nothing.
3. **The status transition** `UPDATE pots SET status = 'drawn' ... WHERE id =
   $1 AND status = 'open' RETURNING *` settles the draw. A replica that gets
   no row rolls its whole transaction back: no payout, no ledger row, no
   announcement.

The buy path takes `Pot::lock_open_for_buy` (row lock, no advisory lock). That
row lock is what makes the daily cap exact: the cap is checked inside the
insert's `WHERE` (today's rows only, `created` on today's UTC date), and two
concurrent buys by one player serialize on the pot row instead of both
reading the same sum. The holding (`user_total`) is the whole pot; the cap
(`user_total_today`) is today. It also keeps a buy from landing in
a pot the sweeper is drawing: after the draw commits, the blocked buy
re-evaluates its `WHERE status = 'open'`, finds nothing, and refuses with
`PotRefusal::Closed` (uncharged).

## 6. Distribution

`pot_changed` is the Postgres notify channel, and it carries a `PotChange`
(`Bought` / `Drawn { .. }` / `Rolled`).

- Every replica LISTENs (`PotService::start_listener_task`), re-reads the open
  pot on any payload, and re-seeds after a reconnect, so a buy committed
  during the gap is not lost.
- The **winner's banner** rides the notify, not the sweeping replica's own
  broadcast, so it reaches the winner on whichever replica they are connected
  to and there is one code path for it. Same reasoning as the crown's deposed
  banner.
- The buyer's own receipt is sent by the replica that ran the buy
  (`PotEvent::Bought`), because that is the only replica the buyer is on.

The shared snapshot is a `watch<Arc<PotSnapshot>>`, read by every session on
the ~1s tick in `tick.rs` and projected into `App.pot_view`. No render ever
queries for the pot, and `/pot` is answered straight out of the snapshot with
no query at all.

`PotSnapshot` holds a private `HashMap<Uuid, PotHolding>` of holdings (the
whole holding plus today's part, both from the one `holders` query).
`holding_for(user_id)` is the only way out of it, and every caller passes
their own id: the field breakdown never leaves the service. `/pot` reads
`room_today` off it ("5 more today"), so the daily cap is visible before a
refused buy; today's part is stamped at query time, so across UTC midnight
it is stale until the next refresh (the sweeper's minute at most).

## 7. There is no sidebar panel

The pot had a two-row `RightSidebarComponent::Pot` panel until 2026-08-28; it
is gone, variant and all. The size and countdown live in the status HUD badge
below, and `/pot` answers in full. A stored `"pot"` key in a user's panel list
is dropped on read by `RightSidebarComponent::from_key`, exactly like the
retired `"activity"` and `"visualizer"` keys, so no settings migration is
needed.

## 7b. The status HUD badge

`render.rs::status_hud_title` carries the open pot as `pot 84,200 · 3h12m`,
placed right before the chips segment so the prize reads against the viewer's
own balance (`... | pot 84,200 · 3h12m | 1500 chips`). It reads the same
`App.pot_view` `/pot` does, so it costs no query and repaints on the same
~1s edge. The HUD is painted over the left title, so under a tight border it
degrades: countdown first (`pot 84,200`), then the whole badge, and it yields
before the pomodoro badge does because it is ambient and `/pot` still answers.
Absent before the first refresh and in a process with no pot service.

## 8. Feed lines

Two `ActivityKind` arms, both explicit in `filter::lounge_includes`:

- `PotDrawn { pot_id, payout, winner_tickets, total_tickets }` -> "mira won
  67,360 chips from the pot on 3 of 312 tickets". It is also the second arm of
  `filter::lounge_headline` (the crown is the first): a headline is a real
  #lounge row rather than a ticker line that is gone by the time an offline
  winner reconnects.
A pot that rolls empty announces nothing: no chips moved and nobody lost.

There are no mid-week size lines any more (migration 162 dropped
`pots.announced_threshold`, 2026-08-27): the size sits in the status HUD on
every screen all week, so a #lounge nudge only repeated what the border said.

## 9. Telemetry

`late-ssh/src/metrics.rs`: `record_pot_tickets_bought`,
`record_pot_buy_refused` (labelled by `PotRefusal`),
`record_pot_drawn`. Five counters, and the burn is readable as the gap between
`late_ssh_pot_chips_in_total` and `late_ssh_pot_chips_out_total`.

## 10. Gotchas

- **Only a test may fake the draw hour**, and only through the helper in
  `svc_test.rs` that moves `draws_at` into the past. `PotService` always
  schedules from `next_draw_at`; `Pot::open_in_tx` takes the hour explicitly
  rather than defaulting, so nothing can silently pick when money moves.
- **`next_draw_at` is strictly after `now`.** A pot settling at exactly Monday 21:00
  schedules tomorrow, not itself.
- **A winner who deletes their account** leaves `pots.winner_user_id` NULL
  (`ON DELETE SET NULL`); the drawn/rolled CHECK constraints tell the two
  apart by `ticket_count`, not by the winner, so a settled row survives the
  delete. The ledger keeps who was paid.
- **`SUM(count)` is `numeric`**, so the cap check inside the insert casts to
  `BIGINT` before comparing against a bound `i64`. Dropping the cast gets you
  "inconsistent types deduced for parameter".
- The composer parse is the boundary for the ticket count: only `1..=cap`
  (the daily cap) reaches the service, so nothing downstream re-checks the
  number; whether today still has room is the insert's `WHERE`.

## 11. Out of scope (SHOP.md phase 5)

A machine in the Late Lounge tavern, a sealed-bid twin for the marquee,
seeding the pot with minted chips, and more than one pot at a time.
