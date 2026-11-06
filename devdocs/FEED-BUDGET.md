# The feed budget (parked design)

A global valve on the #lounge system feed: events persist first, a
lines-per-hour dial decides what the room sees live, and nothing dropped is
lost. Parked until deadchannel's lounge theater goes public
(`late-ssh/src/app/deadchannel/GAME.md`, "The three surfaces" and the
sequencing in "Experiment framing"): until the public flip the game's
output lands on the wire in #deadchannel, which is unfiltered by design, so
the budget is the gate for phase 3, not for anything shipped today.

What exists today: `activity/lounge.rs` posts every event
`filter::lounge_includes` approves, guarded only by the per-(user,
event-shape) 30-minute repeat window. That is a per-person politeness rule,
not a room-level one, and it holds because events are rare. Why rarity is
the whole point, and the measurements behind it, are in the root
`AUDIENCE.md`.

## What this design is not

The first draft of this idea carried a second job: a per-reader
"while you were gone" digest built from the same events table, with a
personal section, a rooms section, a news desk and an AFK-return trigger.
That half is dead. The Late Edition (`late-ssh/src/app/paper/CONTEXT.md`)
shipped as a newspaper instead: one edition per public room per UTC day,
printed once, read by everyone at login, with `/summary` left as the
per-reader window. deadchannel's "while you were gone" news lives on the
wire (`GAME.md`, "The three surfaces"), not in a paper section. Nothing
below depends on the paper.

## The valve

- **Persist first, filter second.** Every event that passes
  `lounge_includes` writes a structured row (kind, user, game, payload,
  created, shipped flag) before the ship decision. The table is the
  budget's memory: dropping a line stops being deleting a story, and a
  later bundle ("overnight: 3 ambushes, the pot hit 40k") has something
  to read.
- **Two classes, from the closed `ActivityKind` roster, mapped
  exhaustively** so a new kind must choose or the build breaks:
  - **Headline class ships always.** The rare, once-ish events that are
    the product: pot drawn, crown taken, boss slain, went live, burn
    milestones. Already self-limiting; a budget that delays the pot draw
    is a bug.
  - **Texture class ships while the hour has room.** Joins, sit-downs,
    daily results, rentals, and the deadchannel volume once it reaches
    #lounge: spawns, crossings, fight results. Over budget, texture drops
    to the table silently.
- **One global lines-per-hour number,** owned by the lounge feed task
  beside the repeat window. Deliberately dumb: no per-kind quotas, no
  priority scores, one dial that makes "is the room too noisy" a
  one-number conversation. Cleverness waits for evidence.
- **Bundling is a digest's job, not the live feed's.** The live line is
  always a single event. A live bundle line is an admission the dial is
  set wrong. Where a bundle would be read (a daily #lounge line, a wire
  recap, a paper section) is open; the table makes any of them cheap.

## Success metric

The #lounge system-line count per day stays flat while deadchannel event
volume grows. Not lines shipped: the budget makes that number a dial, not
a result.

## Open questions

- **The number.** Start opinionated (8 to 12 texture lines an hour) and
  tune against the metric, not taste.
- **Retention of the events table.** A recap needs days; leaderboard
  seasons might want a month. Pick a TTL at design review so the table
  never becomes an accidental analytics warehouse.
- **A public morning line.** With the table, one daily #lounge bundle
  ("yesterday in the city: ...") is cheap. Worth doing only if the valve
  leaves the room blind to dropped texture; decide after the dial has run.
