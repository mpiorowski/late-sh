# SOCIAL.md — events, tournaments, and the doors

Successor to the old UP.md (2026-07-08 product discussion), cut down to the
two things that matter now: an **events/tournaments system** and **deeper
MUD/door game integration**. Everything else from that discussion (shop
rework, chips sinks, sidebar cleanup, chat-native RPG, crews) is scratched or
parked. Each step still gets its own design review before implementation.

## Why (kept from the original diagnosis)

~30 concurrent users. Chat + music are healthy; anything needing synchronous
coordination starves (at 30 concurrent, ~3 people want a game at any moment,
spread across many game types: a liquidity problem, not a UX problem). What
works is ambient, zero-coordination, interruptible.

Core thesis: **people don't want to own things in late.sh, they want to be
seen in it.** Games are the content generator for chat.

North-star check for any idea: **does it ship a story into #lounge?**

## Landed from the old plan (for the record)

- **2026-07-09 / 2026-07-12 — sidebar cleanup + lounge feed v1.** Activity
  panel retired, presence block, bonsai as the flexible fill (the freed space
  is where the 1d countdown line goes); #lounge system feed + composer-gap
  ticker (see 2a for what's still missing).
- **2026-07-08 — buy-a-round (#417).** The bartender pours the house a round
  via `ChipService::buy_round`; every glass seeds the clubhouse glow
  immediately.
- **2026-07-15 — 24h username effects.** Glow 200 / Gradient 500 / Shimmer
  1000, buyer-picked colors, chat + clubhouse name labels, rebuy replaces,
  purchase announced in #lounge. All shop/pipeline detail lives in
  `late-ssh/src/app/hub/CONTEXT.md`. What matters for this doc: the flair
  pipeline (`NameFlairDirectory` → `App.name_styles`) is the first piece of
  the presence layer, and it is exactly what "champion flair" below should
  reuse.

## Pillar 1: Events & tournaments

The cure for thin liquidity is **scheduled scarcity**: instead of hoping 4
people want poker at the same random moment, concentrate everyone on one
moment ("Friday Night Poker, 20:00 UTC") or stretch one competition over days
so nobody has to coordinate at all.

### 1a. One flagship weekly ritual (live)
- Friday Night Poker 20:00 UTC (or whatever the data says is peak), counted
  down all week: sidebar slot + Hub Events tab + clubhouse signpost.
- One-key join from the countdown. Late joiners spectate (house tables
  already track occupancy and have seeded table chat/voice).
- The @dealer / @bartender ghosts host: table announcements, winner callouts
  into #lounge.
- Winner gets a permanent `profile_awards` row (the infra already exists:
  monthly leaderboard snapshots, Lateania/NetHack milestones all write them)
  and a chip pot.

### 1b. Async tournaments (brackets over days)
- Built on the daily correspondence infra (`DailyService`, `daily_matches`,
  deadlines/forfeit sweeper, your-turn notify). A bracket is just a set of
  daily matches with pairing logic on top; wagers/announcements are already
  listed as future hooks in `lobby/daily/CONTEXT.md`.
- Weekly cadence: sign up any time before Monday, one round per day, forfeit
  on deadline like daily games do today. Chess first (rules + board are done),
  then battleship/connect four.
- Bracket view: the World Cup screen already renders a knockout bracket;
  reuse that rendering for tournament state in the Hub Events tab.

### 1c. Seeded score duels & score windows
- Duel form: both players play the same seeded arcade run any time before
  midnight UTC, higher score takes the escrowed pot, result posted to
  #lounge. Reuses existing arcade games, works at any concurrency.
- Tournament form: a weekly "score window" (Mon-Sun) on one featured seeded
  game; everyone plays the same seed, top 3 get awards + chips. This is a
  tournament with zero pairing logic: cheapest possible v1 and a natural
  weekly rhythm between live flagship events.

### 1d. Events surface
- Hub Events tab becomes real: upcoming schedule, active brackets, past
  winners. (Hub CONTEXT already lists events as the known-gap surface.)
- Sidebar: one compact "next event" countdown line; candidate for the
  flexible space freed by the bonsai fill panel. Full "Today" card can come
  later, the countdown line is the v1.
- Every event lifecycle beat (opens, starts, final table, winner) ships one
  line to the #lounge system feed and the ticker. Event lines should be the
  highest-tier events in whatever budget lands (see 2a).

## Pillar 2: Door/MUD games woven in

The doors (Lateania, NetHack, Green Dragon, dopewars, Rebels) are the deepest
content in the app and the least visible. The activity-feed v1 (landed
2026-07-12) proved the direction: door stories in #lounge get reactions.
Deepen it.

### 2a. Finish the feed (v1 leftovers)
- The hourly system-line budget (~4-6/hour, drop low-tier when over) and the
  daily one-line digest ("2,314 mobs slain by 9 adventurers; mira hit level
  30") are still unbuilt; only the 30-minute per-user-per-shape repeat window
  exists. The digest doubles as a "yesterday in late.sh" morning post and is
  where grind volume goes so per-event lines stay rare.

### 2b. Green Dragon social layer (own design doc first)
- The best async-multiplayer asset in the codebase. Prioritize the
  social/PvP layer over combat-math parity: attacking offline players, mail,
  bar gossip about real player actions, custom avatars/taunts.
- Every PvP fight result is a natural #lounge story ("mira jumped tom while
  he slept. tom lost 340 gold.").

### 2c. Door games as event content
- **NetHack seeded race**: everyone runs the same seed for a week; deepest
  dungeon level (scraped from the vt100 status line, same mechanism as the
  existing Amulet/ascension awards) wins. Deaths post to #lounge as they
  already do, which IS the spectator experience.
- **dopewars high-score window**: dopewars is a pure score game; a weekly
  window with a #lounge result line is nearly free once 1c exists.
- **Green Dragon dragon-race**: first player to slay the dragon each week
  gets the line + award.
- **Lateania world events**: time-boxed server-wide happenings (a rare boss
  spawns for 48h, double frontier rewards on weekends) announced via the
  feed. Uses the shared-world runtime that already ticks for everyone.

### 2d. Milestone awards parity
- NetHack and Lateania already mint one-time `profile_awards` + lifetime chip
  payouts for their crowning achievements. Give Green Dragon (dragon slain,
  PvP champion) and dopewars (loan-shark cleared / score tiers) the same
  treatment so every door writes to the presence layer the same way.

## New ideas worth stealing (grounded in what exists)

- **The clubhouse is the venue.** Events should be physically visible in the
  tavern: a chalkboard signpost near the door with the next event + countdown
  (map is generated, add a landmark), the @bartender mentioning it in his
  pinned banner line on event day. Walking past the poker table during Friday
  Night Poker should feel like walking past a crowded table.
- **Trophy shelf in the clubhouse.** Event winners get a named trophy on a
  shelf behind the bar (rendered row in the tavern, tooltip/inspect shows
  winner + date). Permanent marks reserved for real achievements, in the
  highest-traffic shared space. Pairs with `profile_awards`.
- **Attendance streaks, not participation trophies.** A small chip bonus for
  playing N weekly events in a row (mirrors the daily-quest streak bonus that
  already exists in `QuestService`). Retention force without inflating awards.
- **Champion flair on the presence layer.** Reigning weekly champions get a
  temporary chat-label badge until the next event (the monthly leaderboard
  badge pipeline already does exactly this shape of thing). Ephemeral,
  performative, visible: the drink lesson applied to winning.
- **Spectate as the on-ramp.** One-key spectate from the countdown/Events tab
  into house tables. Spectators fill the next seat; zero-commitment
  engagement is how thin communities bootstrap live events.
- **Cross-door season.** A monthly "late.sh season": points for event
  placements across ALL games (poker night, score windows, seeded races,
  dragon kills), one season champion, one award, one #lounge coronation.
  Gives the disparate events a shared narrative spine and gives every game
  type a reason to exist in the calendar.

## Suggested order

1. **2a** — feed budget + daily digest (small, finishes landed work, and the
   event lines in everything below depend on the feed being trustworthy).
2. **1c** — weekly seeded score window (cheapest real "event": no pairing, no
   live coordination, exercises awards + feed + Events tab plumbing).
3. **1d + venue ideas** — Hub Events tab v1, sidebar countdown, clubhouse
   chalkboard.
4. **1a** — Friday flagship live event, once there's a surface to promote it.
5. **1b** — async brackets on the daily-games infra.
6. **2b** — Green Dragon social layer (write its design doc in parallel with
   3-5; it's the biggest lift and shouldn't block the events track).
7. **2c/2d** — door event formats + award parity, slotted into the calendar
   as they land.
