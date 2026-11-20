# The ring, as built (late-ssh/src/app/deadchannel/ring)

Parent: `../CONTEXT.md` §3e. Design and the reasons: `../GAME.md`, "The
arena pass". Live 1v1 card duels between two runners with a mark, called
on the wire, fought round by round at the RING shop while any standing
runner watches and bets bits on a corner, and owed at levels 3, 6, 9 and
12 of every climb after the first. The slice is `ring/` because the name
`arena` is the fight's balance harness (`fight/arena.rs`).

## File roles

| File | Owns |
|---|---|
| `data.rs` | The flat numbers: `ATTACK` 16 and `DEFENSE` 12 (`powers()`, through `Powers::of`, is a strike of 10 and a block of 7, no glyph hit and no noise), `SIGNAL` 45, `ROUNDS` 4, `SLOTS` 4, `CALL_SECS` 180, `BELL_SECS` 300, `ROUND_SECS` 30, `REVEAL_SECS` 6, `AFTERMATH_SECS` 60, `MAX_PRESSES` 3, `RAKE_PERCENT` and `PURSE_PERCENT` 10 each. The copy: the wire's `call_line`, `set_line` and `result_line`, `called_banner` for a runner called while somewhere else, and `reply_line`, the ring's word for every answer and refusal. |
| `state.rs` | Pure: `Duel` (the round, two `Corner`s in seat order, caller first, the `tape` of every resolved round as `Beat`s, the `Ending`: `Knockout`, `Points`, `Draw`), `place` / `clear` / `lock` (refused with a `Refusal`: over, locked, no slot, no card, laid already, no energy; the energy is walked through the slots in order, so a burn paying for a surge behind it counts), `call_time` (the clock ran out: what is laid locks, a corner that laid nothing gets `policy::auto`), `resolve` (the slots one at a time, both corners at once), `view_for` (every hand open, a corner's slots blank for everyone but that corner until the round comes out), `Picks` (one option of every draft: `opening`, `parse`, `flipped`, and `deck` with the runner's inks), `Pool`, `settle` and `pay` (the payout arithmetic). The shuffle's dice are handed in, and the whole duel is one serde value that lives on the row. |
| `policy.rs` | Pure: `obvious` (a burn first, the biggest guard when nothing stands and the other hand holds a heavy hit, then every hit the energy covers, the free ones first and the best rate after, then guards and static) and `auto` (the obvious round, with a mute laid across from the slot of the other corner's heaviest obvious hit, guessed from their open hand on `Duel::unlaid`, never from what they laid). Every card goes through `Duel::place`, so the machine decides what the energy covers. It is the floor: a person reading the open hands beats it. |
| `svc.rs` | `RingService`, the one ring layer touching the database, logs and metrics: the requests (`call_task`, `answer_task` (accept or decline), `deck_task` (read or store the picks), `bet_task`, `act_task` (place, clear, lock)), all through `run`, the one place a request's outcome is counted and logged; the sweeper (`start_sweeper_task`, `sweep`, `advance`, `settle_the_unsettled`); the settle after a bout (`settle`, `settle_fighter`, `pay_bet`); the notify worker into a process-shared `watch<RingBoard>` (`start_notify_worker`, `refresh`), fed by the tables' own triggers (migration 287): no write here notifies by hand. `Board` is the open calls, the bout in the ring or the last one out of it as a `LiveBout` (the row, the whole duel, the bets), and every runner's record. |
| `session.rs` | `RingSession`: the session's copy of the board cut by `seen_by` (other corners' slots blank), the picks it brings, the cursor over `roster` (the presence records joined with the runner directory, marks one or more, by name), the corner picked for a bet, the ring's last word, one request in flight, and the countdown's last painted second. `phase` reads the board at a moment: `Empty`, `Set` (accepted, before the bell), `On`, `Over` (ended, still on the board). `tick(app)` copies the board, drains the answer, re-reads the sheet when this runner's bits moved or their gate opened, and raises the banner for a call that lands while the panel is shut. Decides nothing. |
| `input.rs` | Keys while the panel is open (`city/input.rs` hands it every key): with the ring empty, ↑↓ (`k` `j`) walk the roster, `c` calls, `a` takes a call, `x` declines one or takes your own back, `1` to `4` flip a draft pick (marked runners only); with a bout set, ←→ (`h` `l`) pick a corner and `b` puts one press on it (a fighter's goes on their own); for a fighter from the accept on, `1` to `5` lay that hand card in the next empty slot or take it back, `e` or space locks the round. Enter leaves; `0`, Tab and `q` stay global, and so do the digits but a fighter's during a bout and a marked runner's at an empty ring; `?` is the guide's (taken in `city/input.rs` first); everything else is swallowed. |
| `ui.rs` | `RingView` and `panel_lines`, one function per phase: the empty ring (the gate owed in red, the deck you bring with the four picks and their keys, the marked runners on the wire with their records, the open calls, the keys), a bout set (both corners with their records and what they drafted, the pool by corner, your bet, the split, the bell's clock, a fighter's own round), the rounds on (both signal bars and blocks, locked or laying, both hands open, your own slots and energy, the round that last came out slot by slot, the clock), and the aftermath (the result, the pool's split, what your bet paid, the gate opened). Pure; `city/ui.rs::draw_panel` frames it and puts the ring's word at its foot. |

Tests sit beside each file. The rows' own test is
`late-core/src/models/deadchannel_bout_test.rs` (the three unique
indexes, the pool closing at the bell, the stake cap, each settle and
pay claim taken once).

## The rules

- **Who.** Only a runner with at least one mark (they put the Old
  Signal down) can call or be called: `call` reads both off the runner
  directory, and the roster lists only marked runners on the wire. Any
  standing runner can watch and bet.
- **Where.** The RING shop (`Landmark::Ring`, the magenta sign beside
  the blade shop). Enter opens its panel (`RingSession::open` reads the
  deck the first time), and the ring takes every key until Enter or Esc
  closes it. Watching is there and nowhere else.
- **The call.** `c` calls the marked runner under the cursor, who must
  be on the wire (a presence record). A call lasts `CALL_SECS` (180 s);
  one open call or bout per caller, and none while a bout is accepted
  and unfinished: one conditional insert decides both. The wire gets a
  line that mentions the runner called, and a session of theirs that is
  not looking at the ring gets a banner. `a` takes the call, `x` says no
  or takes your own back; a call nobody answers lapses (the sweeper
  ends it as expired, no wire line). The call, answer and pick keys
  work only while the ring is empty: no bout set or on, and the last
  one's minute on the board over.
- **The accept.** Both decks are read and locked into the row, round 1
  is dealt, and the bell is set `BELL_SECS` (300 s) out, round 1 due
  `ROUND_SECS` after it; the wire gets a line. One accepted bout at a
  time, and two runners fight each other once per UTC day whichever of
  them called: the accept is one conditional update, and the unique
  indexes `deadchannel_bouts_one_ring` and
  `deadchannel_bouts_one_per_pair` refuse it as `RingTaken` or
  `FoughtToday` (the second ends the call on the spot). A fighter who
  walked off since the call has no sheet, and the call ends as
  declined.
- **The deck.** The base ten, four picks chosen at the ring (one option
  of each of the four drafts, whatever the level: sixteen decks;
  `1` to `4` flip one, stored in `deadchannel_ring_decks`; a runner who
  never chose brings the first option of each), plus the other card of
  every draft the runner inked. Ink is the one thing off the road the
  ring keeps: level, kit, the marks bonus and the day's glass stay
  outside.
- **The duel.** Flat numbers for both corners: attack 16 and defense 12
  (a strike of 10, a block of 7, every other card its road share of
  those through `Powers::of`), signal 45. Four rounds at most; each
  round both corners hold a hand of five, open to everyone, and 3
  energy, and lay up to 4 cards into slots in the order they will come
  out. A corner's slots are hidden from everyone else until the round
  comes out (`Duel::view_for`, applied per session by
  `session::seen_by`; the process-shared board holds them all). When
  both are locked the slots come out in order, both corners' at once:
  the guards go up, a mute cancels the card across from it (two mutes
  cancel each other; a muted burn leaves the card it was paying for in
  the hand), both hits land together, a hit that gets through leaves a
  static card in the other deck, a siphon mends whoever still stands,
  and block carries between rounds. A signal at zero is a knockout, both
  at zero in the same beat a draw, and past round 4 the higher signal
  takes it on points (level is a draw).
- **Four slots, not three.** One more than the energy pays for at a
  card apiece, so a round is short of energy and never of slots: a
  burn's energy and a free card have somewhere to go. With three, the
  decks that feed on static won 60 to 79% on the clock.
- **The clock.** Round 1 has `ROUND_SECS` (30 s) from the bell; every
  later round has `REVEAL_SECS` (6 s) to watch the last one come out,
  then 30 s. Fighters can lay and lock round 1 before the bell. Both corners
  locked with the bell rung resolves at once (`act` does it under the
  same lock). At the deadline what is laid is locked and a corner that
  laid nothing is played by `policy::auto`, so a bout with bets on it
  always finishes. A corner that locked with nothing laid holds: holding
  is a move.
- **The bets.** Open from the accept to the bell (`Bout::add_to_pool`
  checks `bell_at > now` in its `WHERE`). One press is one cache's worth
  of bits at the bettor's level (`Rules::cache`), `MAX_PRESSES` (3) at
  most, one corner per bettor; a fighter backs only their own corner.
  The press is the pool, the bet row and `Command::Stake` on the sheet
  in one transaction, refused short or with a glyph waiting on the row.
  Parimutuel (`state::settle`, `pay`): 10% of the pool burned, 10% to
  the winning fighter (the purse), the rest split pro rata among the
  bets on the winner, rounded down and the dust burned. A draw, or bets
  on one corner only, refunds every stake. No bits are minted: the
  purse and the split together are the pool less the rake.
- **The gate.** A marked runner whose level has reached a draft level
  above `ring_cleared` owes a bout (`Sheet::ring_gate`): every road
  step, the crossing and the answer are refused with
  `Refusal::RingOwed`, the road panel says so in place of the node
  (`fight/ui.rs::ring_owed_lines`), and the ring's panel says it in red.
  Fighting a bout, win, lose or draw, clears every gate up to the
  current level (`Command::RingFought`, in the fighter's settle). A
  mark and the ledge put `ring_cleared` back to 0; a first climb never
  owes one. The balance sim takes the bout as fought
  (`fight/sim.rs`). The contract on the sheet's side is
  `../fight/CONTEXT.md`, "The ring's gate".
- **The settle.** Once a bout ends, each fighter's end is one
  transaction: the settle claim (`caller_settled_at` or
  `called_settled_at`), `Command::RingFought`, and the purse as
  `Command::Collect`. Each bet is one more: the pay claim (`paid_at`,
  `paid_bits`) and its `Collect` (nothing for a losing bet). Two
  replicas settling the same bout pay each runner once, and one
  runner's failure costs nobody else theirs. A runner who no longer
  stands is logged and counted as `pay_left`, the claim taken.
- **The wire.** Three lines from the `afterglow` voice: the call
  (mentions the runner called), the bout set (the bell's minutes, bits
  down at RING), and the result (who put whom down in which round, or
  took it on points, or a draw, and how many bits rode on it). Declines
  and lapsed calls say nothing.

## Replicas and locks

Nothing here is a process-local source of truth. A bout is a row; every
change to it is made under its row lock (`Bout::lock`: a round, a bet,
the sweeper's advance) or is one conditional statement (the call, the
accept, a decline, a lapse, the end); a fighter's settle and a bet's pay
are claims on rows taken in the same transaction as the bits they move,
through `FightService::apply_in`. A fighter's key carries the round it
was pressed on, and `act` refuses it (`ActRefusal::RoundGone`) when the
row has moved to the next: nothing is laid or locked on a hand its
runner has not seen. Every write to a bout or a bet notifies from its
table's trigger, the bout's id on `deadchannel_bout`. The board each replica holds is a copy
for drawing and for the sweeper's clock, and what the sweeper does it
does again under the lock.

Lock order, everywhere: the bout's row, then a bet's, then one runner's.
The fight never takes a bout's row, so the two cannot cross.

## The clocks

- **The sweeper** runs every second on every replica
  (`start_sweeper_task`), off its own board copy: it lapses the calls
  the copy shows past their time, and rings the bell or calls the round
  when the bout is due (`is_due`: the bell rung, and both locked or the
  deadline past), under the lock, a bout that moved on since the copy
  left alone. Every 15 ticks it also reads the rows themselves: lapses
  what the copy could not show, settles a bout a dead replica left
  unsettled once it has ended at least 20 s ago (long enough for the
  replica that ended it to pay in the ordinary case), and re-reads the
  board. A draining replica stops sweeping: the newer one owns the rows.
- **The board.** The notify channel `deadchannel_bout`
  (`Channel::DeadchannelBout`) carries the bout's id on every change:
  called, accepted, declined, lapsed, a round stored, a bet placed,
  ended, settled. The
  board is small, so every signal re-reads all of it (`refresh`: the
  open calls, the bout in the ring or the last one out of it, its bets,
  the records); a resync re-reads it through
  `read_until_ok(Refresh::RingBoard, ..)`. A finished bout stays on the
  board `AFTERMATH_SECS` (60 s).
- **The session.** A countdown owes one frame a second, and only while
  the panel is up. A session re-reads its sheet after its own stake,
  and for a bout it fought or bet on once the bout ends and again once
  everything of theirs on it is settled.

## Telemetry

`metrics::record_deadchannel_ring(RingBeat)`, the counter
`late_ssh_deadchannel_ring_total{beat}`: `called`, `call_refused`,
`accepted`, `accept_refused`, `declined`, `expired`, `bet`,
`bet_refused`, `locked`, `timed_out` (a round the clock called on a
corner that had not locked), `resolved`, `ended`, `settled` (a fighter's
end on their sheet), `paid` (a bet), `pay_left` (no standing runner to
pay), `settle_failed`, `deck_set`, `failed` (a request that erred, or an
advance).
The fight counter sees the ring's sheet commands on its own side:
`staked`, `collected`, `ring_fought`, and `ring_owed` (a step, the
crossing or the answer turned down at the gate, counted by itself
because there is no fallback opponent: how often the road is found shut
is the cost of the gate). `apply_in` counts nothing; the ring hands
each outcome to `FightService::landed` once its own commit is through,
so a beat is a write that landed. Spans: `deadchannel.ring.sweep`,
`deadchannel.ring.call_task`, `answer_task`, `deck_task`, `bet_task`,
`act_task`. Every log line about a bout carries its id; the Grafana
panel is "Ring Bouts" on the deadchannel row.

## Seams

`App.ring` (the session), one tick line in `tick.rs`
(`ring::session::tick`), the panel in `city/ui.rs` (`RingView` built in
`render.rs`, drawn through `ring_ui::panel_lines`), the key hand-off and
`RingSession::open` in `city/input.rs`, `Landmark::Ring` and its
`Enter::Panel` in `city/`, `State.ring_service` (built in `main.rs` with
its own `FightService`, the notify worker on `RingService::CHANNELS`
and the sweeper started there), `Channel::DeadchannelBout` and
`Refresh::RingBoard` in `pg_listener.rs`, the sheet's side
(`Command::Stake`, `Collect`, `RingFought`, `Sheet::ring_gate`,
`Refusal::RingOwed`, `FightService::settled_sheet`, `apply_in` and
`landed`; and `Bout::holds`, which the fight asks before the ledge: a
runner with a bet unpaid or a bout unsettled is refused the fall,
`Refusal::RingHolds`, so the ring never pays onto a sheet that started
over), and
the rows in `late-core/src/models/deadchannel_bout.rs` (migration 287:
`deadchannel_bouts`, `deadchannel_bout_bets`, `deadchannel_ring_decks`,
and `deadchannel_runners.ring_cleared`). The nuke
(`deadchannel_nuke_runners()`, `runner/CONTEXT.md`) takes the ring with
it: unpaid bets are void, open calls lapse, a bout being fought ends a
draw with both fighters settled, and `ring_cleared` goes back to 0. The
deck a runner brings stays.

## Known costs

- **No fallback opponent.** A marked runner at a gate cannot walk the
  road until another marked runner is online and accepts; with one mark
  on the street, they wait for a second. `ring_owed` on the dashboard
  is how that cost is watched.
- **A runner already past level 3 on a second climb** owes a bout
  before their next step on the road.
- **Calls post on the wire** to reach the runner called. One open call
  per caller, the 180 s expiry, and `CallResult::TooSoon` (the same
  runner is not rung again until the last call to them would have run
  out, however it ended) are the limits on that.
- **A mark is not held the way the ledge is.** A runner who bets and
  then puts the Old Signal down inside the same bout wakes on a fresh
  sheet and is still paid the bet: the kill cannot be refused the way
  the fall is.
- **Watching is at the RING shop only.** The wire carries the call, the
  bell and the result.

## Testing

`make test ARGS="-p late-ssh -p late-core --lib -E 'test(deadchannel::ring) | test(deadchannel_bout)'"`.
The balance contract is `policy_test.rs::no_deck_runs_away_with_the_ring_on_the_clock`:
every one of the 16 decks against the field, from both seats, on the
clock's own hand, each between one third and two thirds, and at least
half the bouts a knockout. Measured at these numbers: 34% to 63%, 72%
knockouts, 3.0 rounds a bout. The clock's hand plays a mute and a
bulwark worse than a person does, so decks built on them sit at the low
end. A number in `data.rs` moves until it passes; the band moves only
with a design decision. The gate's tests are in `fight/state_test.rs`.
