# DIGEST.md: the feed budget and the paper

Status: **seed doc, vision + decisions.** Spun out of the GAME.md phase 1
prerequisite (2026-08-31 spitball session) after it grew a second job: the
same engine that keeps deadchannel from spamming #lounge is also the engine
behind the welcome-back screen. Nothing here is committed design; every step
gets its own design review before implementation. This file exists so the
next session starts from the thesis instead of re-deriving it.

Load-bearing for GAME.md: deadchannel phase 2 does not start until the
budget (step 2 below) is live. The game's whole output is feed lines; this
doc is the valve and the inbox they flow through.

## Why (the diagnosis)

- **#lounge is the only public story surface, and it has no global limit.**
  Today `activity/lounge.rs` posts every event `filter::lounge_includes`
  approves, guarded only by the per-(user, event-shape) 30-minute
  `REPEAT_WINDOW`. That is a per-person politeness rule, not a room-level
  one: ten different people doing ten different things in one hour is ten
  lines, all compliant. It works now by luck, because events are rare.
- **Rare is what makes a line a story.** "tom put 10,000 chips on mira's
  head" lands when it is one of five things that happened today. At one of
  eighty it is log output, and the room learns to skim past the `· ` prefix,
  which kills the north-star metric (stories shipped to #lounge) even while
  its raw count goes up. deadchannel multiplies event volume by design;
  without a budget it destroys the surface it was built to feed.
- **Dropped is currently the same as never happened.** Events live only on
  the in-process broadcast; the survivors persist as rendered chat text.
  There is no structured record to bundle, rank, or replay later, so the
  only two options per event are "ship it now" or "lose it". A budget
  without a memory just deletes stories.
- **The return moment is unowned.** `/summary` answers "what did I miss"
  well, but it is pull, and nobody pulls: the people it would help most
  (the ones who were gone) are exactly the ones who do not know to run it.
  Meanwhile the device mark (`user_ssh_keys`, the Two Marks in
  `chat/CONTEXT.md`) already knows precisely how long you were gone, and
  the app does nothing with that moment beyond a thin divider rule.
- **Half the population never "logs in".** Sessions stay open for days;
  their return moment is a keypress after hours of silence, not a
  connect. The AFK set (`state.rs::AfkUsers`, per-session `last_input_at`)
  already detects this edge and spends it on an indicator.
- **The retention read (from GAME.md discussion, 2026-08-31):** joins are
  constant but the daily count is flat, so churn eats the funnel. The
  lever for 40 → 80 daily users is D1/D7 return, and "come back tomorrow"
  needs a concrete payoff on arrival. The paper is that payoff, and
  deadchannel's login news screen is a section of it, not a separate
  feature.

## The idea in one sentence

One digest engine that every domain feeds: events persist first, a global
budget decides what #lounge sees live, and everything (shipped or dropped)
flows into a personal welcome-back paper shown when you return from a real
break, so the room stays calm and no story is ever lost.

## Decisions made (2026-08-31)

- **Budget before game.** The valve ships before deadchannel phase 2 sends
  a single line. Tuning spam pressure while building a game is how both
  jobs get done badly.
- **The paper is the reader for the digest, not a second system.** One
  engine, two outlets: the live #lounge line (budgeted, for the room) and
  the paper (complete, for you). deadchannel's "while you were gone"
  screen from GAME.md is a section of this paper, not its own surface.
- **Two triggers, one question.** The paper answers "what happened since
  this device last saw the app", so it rides the existing device mark, the
  same cursor a bare `/summary` reads. Fresh connect after a real break
  shows it; a keypress after hours of AFK silence offers it. Same window
  math, same honest answer on both paths.
- **The paper never blocks and never pads.** One keypress dismisses it. A
  brand-new user never sees it before the tavern (they have no gone-ness
  to summarize, and the first-visit tutorial owns that moment). A quiet
  night prints a two-line paper; an empty section is omitted, never
  filled. A padded paper trains people to dismiss unread, which is the
  whole feature lost.
- **No AI calls to render the paper.** The default paper is templates over
  structured events: instant, free, always available (an install with no
  AI key gets the full paper). AI stays where it already lives, in the
  on-demand `/summary` pipeline, which the rooms section links into
  rather than duplicates. This is what makes showing it on every return
  affordable.
- **News is human-curated, AI-assisted, in that order.** The news section
  ranks what people shared (ArticleService) plus your own RSS inboxes.
  No autonomous AI news-gathering: "what did my weird computer friends
  think mattered" is the moat, and the chip-paid sharing loop (500 chips
  a share) is its engine. An AI firehose would compete with the humans
  and the humans would lose. AI may summarize and rank what humans
  brought; it never sources.
- **AFK stays a trigger, not an audit.** No public split of "really
  online" vs idle; at this population the warm-body count is part of why
  the room feels alive. The AFK edge feeds the paper and nothing else
  changes.

## The budget (the valve)

- **Persist first, filter second.** Every event that passes
  `lounge_includes` writes a structured row (kind, user, game, payload,
  created, shipped flag) before the ship decision. The table is the
  paper's source and the budget's memory; dropping a line stops being
  deleting a story.
- **Two classes, from the closed `ActivityKind` roster, mapped
  exhaustively** (a new kind must choose or the build breaks):
  - **Headline class ships always.** The rare, once-ish events that are
    the product: pot drawn, crown taken, boss slain, went live, burn
    milestones. These are already self-limiting; a budget that delays
    the pot draw is a bug.
  - **Texture class ships while the hour has room.** Joins, sit-downs,
    daily results, rentals, and (later) the deadchannel volume: spawns,
    ambushes, fight results. Over budget, texture drops to the table
    silently and surfaces in papers and bundles instead.
- **The budget is one global lines-per-hour number,** owned by the lounge
  feed task alongside the existing repeat window. Deliberately dumb in
  v1: no per-kind quotas, no priority scores, one dial that makes "is
  the room too noisy" a one-number conversation. Cleverness waits for
  evidence.
- **Bundling is the digest's job, not the live feed's.** The live line is
  always a single event. "overnight: 3 ambushes, the pot hit 40k" is
  paper grammar; a live bundle line is an admission the budget is set
  wrong.

## The paper (the inbox)

Sections, each omitted when empty, in a fixed order so regulars can read
it in two seconds:

- **Masthead.** Date and how long you were gone. Sets the window every
  section below answers for.
- **The city desk (world).** Headlines and bundled texture from the
  events table since your mark: who took the crown, what the pot did,
  who went live, door milestones. Later, deadchannel's world beats land
  here.
- **You.** The personal ledger: your daily-match turns waiting, quest
  streak state, pot tickets and the draw result, and later the
  deadchannel character section (GAME.md's "mira ambushed you, your
  bounty is now 8k" lines live here). This section is why the paper is
  per-user and not a broadcast.
- **Your rooms.** Unread shape per pinned/favorite room, with the
  existing `/summary` as the drill-down (the paper shows what moved, the
  AI call stays opt-in per room). Not a wall of message counts: names
  and one-line "who was loudest about what" metadata, cheap fields the
  snapshot already carries.
- **The news desk.** Articles people shared since your mark (title +
  sharer), then unread heads from your own RSS inboxes. Human shares
  outrank your feeds on purpose (see decision above).

Surface: the existing shared `Overlay` the `/summary` pipeline already
renders into is the v1 shape; a dedicated screen is a v2 question. On
fresh connect after a break the paper opens over the tavern landing; on
an AFK return it never steals the screen mid-anything, it offers itself
as a one-line banner plus a keypress.

## Success metrics (named now so it can fail honestly)

- Paper open → dwell: are returning users reading it or reflex-dismissing
  it (dismiss-under-two-seconds rate is the padding alarm).
- D1/D7 return of new users, before and after. The paper is a retention
  bet; this is the number it must move. Prerequisite: actually
  instrumenting D1/D7, which does not exist yet and ships with step 1.
- #lounge system-line count per day staying flat while deadchannel event
  volume grows (the valve working).
- `/summary` invocations from the paper's rooms section (is the
  drill-down real).
- NOT lines shipped. The budget makes that number a dial, not a result.

## Sequencing (each step useful alone)

1. **Persist the events.** The table plus the write in the lounge drain.
   Ships nothing visible; makes every later step possible. D1/D7
   instrumentation rides along here.
2. **The budget.** Class mapping plus the hourly dial in
   `activity/lounge.rs`. Visible result: a calm #lounge no matter what
   lands upstream. This is the GAME.md phase 1 gate.
3. **The paper v1.** Connect-after-break trigger, masthead + city desk +
   You sections from the events table, rooms section as unreads +
   `/summary` tip.
4. **The news desk.** Shares and RSS since the mark.
5. **The AFK-return trigger.** The banner-and-keypress path for
   always-open sessions.
6. **deadchannel plugs in.** Phase 2's login news screen arrives as the
   You section growing a character block, not as new plumbing.

## Open questions

- **The name.** The surface wants a masthead. Candidate: **The Late
  Edition** (the newspaper pun is exactly the register, and "late" is
  the brand). Decide at design review; the engine does not care.
- **The break threshold.** How long is a real break before the paper
  shows on connect (12h? 8h?), and how long a silence arms the AFK
  trigger. Feels like one number each, tuned once real papers exist.
- **The budget number.** Lines per hour for the texture class. Start
  opinionated (something like 8-12) and tune against the metric, not
  taste.
- **A public morning digest line.** GAME.md's phase 1 mentions a morning
  digest; with the events table, one daily #lounge bundle ("yesterday in
  the city: ...") is cheap. Worth doing only if the paper alone leaves
  the room blind to dropped texture. Park until step 3 is observed.
- **Interest tags.** Per-user topics steering the news desk ranking.
  v2 at the earliest; needs the desk to exist and prove people read it.
- **Retention of the events table.** The paper needs days; leaderboard
  seasons might want a month. Pick a TTL at design review so it never
  becomes an accidental analytics warehouse.
