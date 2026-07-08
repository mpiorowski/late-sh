# UP.md — taking late.sh to the next level

Working plan from the 2026-07-08 product discussion. Each step gets its own
design review before implementation; this file holds the problem, the
reasoning, and the ordered direction. Keep it updated as steps land.

## The problem

~30 concurrent users, healthy chat + music core, but engagement plateaus:

- Hard to ever fill a poker / blackjack / chess table; tournaments and events
  are impossible to run.
- People come for chat + music. Lateania works because it is simple and
  ambient. NetHack / Green Dragon / dopewars are underused.
- The shop barely sells. Chat cosmetics are almost never used; a few people
  buy aquarium fish. Chips pile up with nothing worth wanting.

## The diagnosis (why it's like this)

Everything that works (chat, music, Lateania, artboard, aquarium, drinks) is
**ambient, zero-coordination, interruptible** — it fits around work in a tmux
pane. Everything that struggles needs either **synchronous coordination**
(tables: at 30 concurrent, ~3 people want a game at any moment, spread across
7 game types — a liquidity problem, not a UX problem) or **deep exclusive
attention** (door games compete with the user's own compiler).

Key proof point: the bartender drink that tints your username background is
loved. It is consumable, ephemeral, performative, and visible in the highest
traffic pixels of the app (your name in chat). The permanent, invisible,
silently-bought shop items are the exact opposite on every axis.

**People don't want to own things in late.sh. They want to be seen in it.**

## The strategy

Games are the focus — not more games, but connecting the games we have to the
people we have. Chat grows by having more to talk about; games are the content
generator for chat. Music stays maintenance-mode atmosphere.

The unifying concept is one **presence layer**: username label in chat +
clubhouse avatar + game seat labels are a single canvas, and everything writes
to it — drinks, game wins, purchases, social acts. Almost everything money
buys should be ephemeral; permanent marks are reserved for real achievements.

North-star check for any idea: **does it ship a story into #lounge?**
Metric to watch: fraction of #lounge messages referencing something that
happened inside late.sh.

## The plan

### 1. Daily games / lobby / challenges
- Make multiplayer async-first: challenges you can post and walk away from.
  An open challenge board ("100 chips, chess, anyone") that persists until
  claimed; you can "wait forever" for an opponent instead of coordinating.
- `/challenge @user <game> [wager]` from the composer, with escrowed chips,
  turn notifications, correspondence pacing (games can span days).
- **Seeded score duels**: both players play the same seeded arcade run any
  time before midnight UTC, higher score takes the pot, result announced in
  #lounge. Async, reuses existing arcade games, works at any concurrency.
- Daily featured game + one-key join/spectate.

### 2. Door/MUD game events into #lounge
- System messages tiered by **rarity, not event type**: NetHack deaths and
  ascents are fine as-is; Lateania emits only server-firsts, boss deaths,
  level/depth milestones — never per-kill.
- Global budget (~4-6 system lines/hour, drop low-tier when over).
- Daily one-line digest for the grind volume ("2,314 mobs slain by 9
  adventurers; mira hit level 30").
- Styled dim/one-line, reactable/replyable like normal messages.

### 3. Surface games in the sidebar / clean up the sidebar
- A "Today" card: your open duels/challenges, today's featured game, live
  tables with one-key spectate ("2/4 at poker — join").
- Live game status leaks into shared space (clubhouse tables show occupancy;
  spectating is zero-commitment engagement that fills the next seat).
- Sidebar layout cleanup to make room — layout discussion pending (what
  stays: clock, music; what competes: visualizer, pet, bonsai, new Today
  card; experimentation welcome, e.g. pet moved to chat input area).

### 4. Chips: real reasons to spend
Benchmark everything against the drink (100-1000 chips, hours of visible
personal effect, bought performatively). Ideas agreed as strongest:
- **Username effects, 24h**: glows/gradients on your name everywhere.
- **Buy a round for the bar**: price scales with headcount, everyone gets a
  drink + buzz, announced, buyer glows "generous". Best single item.
- **Naming rights, time-boxed**: a named drink on the bartender menu for a
  week, name the lounge dog for a day. Expensive, visible, expires.
- **Wager escrow** on challenges/duels (circulation that makes chips matter).
- **Crowdfunded ultimates**: keep the 10M spells, let the community fill a
  shared pot; contributors named when it fires.
- Rework the emoji badge wall: keep flags; the rest become earned-only or
  rotating limited stock (scarcity = status). Aquarium badge idea: badge tier
  derived from number of fish owned (ownership becomes visible status).
- Audit notes: catalog today is ~95% permanent one-time buys (nothing recurs
  except drinks); room effects are ~50x worse chips-per-visible-second than
  drinks; Rubik's UI says 500 but pays 250; `last_stipend_date` is a dead
  column — if a stipend ever ships, make it a bartender login stipend.

### 5. Previously discussed, not yet scheduled (the "forgot" list)
- **One flagship weekly ritual**: e.g. Friday Night Poker 20:00 UTC, counted
  down in the sidebar all week. Scheduled scarcity is how thin communities
  fill tables. Async weekly tournaments (brackets over days) for the rest.
- **Green Dragon deep integration** (separate discussion): prioritize the
  social/PvP layer — attacking offline players, mail, bar gossip, custom
  avatars — over combat-math parity. It's the best async-multiplayer asset.
- **Presence-layer plumbing** as its own system (one flair/state model
  rendered in chat labels, clubhouse avatars, seats) rather than per-feature
  hacks.
- **Crews/teams** later: shared weekly goals add the missing retention force
  (other people depending on you).
- Event winners mint permanent profile awards + a trophy shelf in the
  clubhouse.

### 6. Chat-native RPG (Discord Pokémon-bot style)
Idea: creatures/encounters that spawn *in #lounge itself* — a wild creature
appears as a system message, first to type the catch command gets it; your
critters level ambiently while you're connected; `/battle @user` runs a short
auto-resolved fight rendered as a few chat lines with a chip wager. Fits
perfectly: zero-coordination, ambient, performative, lives in the highest
traffic surface, and gives collectors a chip sink (training, cosmetics for
your critter in the clubhouse). Could BE the presence-layer flagship — your
critter follows your avatar in the tavern. Design carefully vs. spam budget
(shares the #lounge system-message budget with step 2).

## Suggested order

1. Buy-a-round + 24h username effects (extends the proven drink pattern).
2. Score duels + challenge board (kills the liquidity problem cheaply).
3. Sidebar "Today" card + lounge event feed (steps 2-3 above).
4. Friday flagship event.
5. Green Dragon social layer (own design doc first).
