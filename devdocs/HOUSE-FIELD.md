# The house field (proposed design)

Status: proposed, not built. One Minesweeper board the whole house clears
together, a few opens per person per day, on a tile beside #lounge. As it
lands, move shipped behavior into the domain's own `CONTEXT.md` and delete
this file.

## Why

The measurements are in the root `AUDIENCE.md`; the ones this rests on:

- Solo dailies are the strongest ritual and do not grow: four new dailies
  doubled the boards finished and left the week's distinct players where
  they were.
- Anything live and synchronous starves (house tables, rounds, gilds).
- The one shape between the two, async and shared, is the Realm, and it is
  the door the most distinct people open.
- #lounge holds 69% of Home's hours; every door together holds 3% of all
  time.
- The 10 busiest authors write 80% of chat. A newcomer who wins a board on
  day one comes back on two days at 25%, one who chats at 12%. Most of the
  house has no way to take part that is not talking.

So: one shared object, in the room people already sit in, that a lurker or
a first-day account can add to with one key, and that gets better with
more hands instead of needing them at the same time.

## Why Minesweeper

- The house knows the rules: it leads the Arcade by boards finished.
- No content pipeline. A seed is a board.
- One open is a whole contribution, and a careful deduction is a better
  one, so a newcomer and a regular both matter on the same board.
- It writes its own stories: a mine hit has a name on it, and a forced
  guess needs someone to take it.

Weighed and not picked:

- **A nonogram of a gallery piece.** The best reveal, but member art is
  not line-solvable (the shipped packs only keep puzzles
  `nonogram::solve_by_lines` finishes) and a newcomer cannot contribute.
- **A jigsaw of a gallery piece.** Anyone can join, nothing to be good at,
  and sparse ASCII tiles look alike (Sliding Puzzle numbers its tiles for
  that reason).
- **A crossword.** Needs written clues every board and shuts out part of a
  multilingual room.

## The rules

- **One board stands at a time, until it is cleared.** Then the next one
  rises. Not one board per UTC day: a daily board would be emptied by
  whichever timezone wakes first, and would have a day that fails. Sized
  to last a few days.
- **A ration of opens per person per UTC day** (start at 5). A flood
  counts as one open. The ration resets with the dailies at 00:00 UTC and
  does not bank.
- **A mine ends your day.** The cell becomes a crater with your name on
  it, known to everyone from then on. Nothing else is lost: the board has
  no lives and cannot be failed.
- **The first open of a board is safe**, as in the Arcade.
- **Guesses stay in.** A random board will force a coin flip; that is the
  story, not a defect.
- **No chips.** The economy needs a sink, not another faucet. The reward
  is standing: your colour on the board, your count on your profile.

## Where it shows

- **A Home tile** (a new `TileKind`, `late-ssh/src/app/zen/CONTEXT.md`),
  drawn small beside #lounge: the board, how much is left, your opens left
  today. Enter opens it full size to play.
- **Each opened cell is tinted with its opener's username colour**, so the
  board reads as a map of who did what. Username colours are the Shop's
  one hit; this is a second place they are seen.
- **One #lounge line per cleared board** (hands, craters, who opened the
  last cell). Craters and ordinary opens stay on the board and its own
  log, never in the feed (`devdocs/FEED-BUDGET.md`).
- **The profile modal**: boards helped clear, cells opened, craters.

## Shape

A new domain with the usual file roles (`state.rs`, `svc.rs`, `input.rs`,
`ui.rs`), built to the multi-replica rule from the start:

- **The Realm's spine** (`late-ssh/src/app/door/realm/CONTEXT.md`): one
  row for the standing board, state with a revision, every open resolved
  once by the write that wins the CAS, a change trigger, the process
  listener re-reading into a shared `watch`. One live board enforced by
  the table, the next one opened in the transaction that clears the last
  (the pot's shape).
- **The ration is claimed in SQL**, in the same transaction as the open
  (the pot's per-day ticket cap in the insert). A move row per open is
  also the credit: who, which cell, what it did.
- **The mine map never leaves the service.** The view a session draws
  from is a type with no mines in it, so the UI cannot show one by
  mistake.
- **The kernel is shared, not reached into.** The generator, the flood and
  the safe first open are private to `arcade/minesweeper/state.rs`. A
  second user makes them a kernel under `app/games` (the `chess_core`
  shape); that lift is its own change, before this one.
- Telemetry with the feature: opens by outcome (opened, crater, refused by
  ration, refused as already open) and boards cleared, labels from closed
  enums.

## What would say it worked

- Distinct people who open a cell in a week, against the week's distinct
  daily winners (82 to 93 before the `Board` tour, 117 the week of
  2026-11-09). More opens from the same people is not the win.
- The share of those people outside the 20 busiest chat authors.
- First-day accounts that open a cell, and their two-day return against
  the board winners' 25%.

## Open questions

- **Board size and ration.** Run the generator offline, count the opens a
  board of each size needs, and size against the people who move in a day
  so a board lasts three to five days. One dial each; tune against the
  roster, not taste.
- **Flags.** Free and shared, carrying the flagger's name, cleared by
  anyone? Or none at all in the first version, with craters the only
  marks?
- **A gradient username on one cell.** Which colour it takes.
- **Buying an open with chips.** It would be a sink everyone can afford,
  and it lets a rich account take more of the board. Leaning no.
- **The tour.** An "open one cell" stop is the obvious use; untouched
  until the `Board` route's cohort reads are in.
- **Laying the field over a gallery piece**, so clearing it uncovers the
  art and credits the painter. A later version, once the plain one holds.
