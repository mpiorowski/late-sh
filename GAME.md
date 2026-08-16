# GAME.md — deadchannel, the character layer of late.sh

Status: **seed doc, vision + decisions.** Successor to DRAGON.md (removed
2026-08-07; its thesis survives here, its "extend Green Dragon" framing does
not). Nothing here is committed design: every step gets its own design review
before implementation. This file exists so future spitballing starts from the
thesis instead of re-deriving it.

This is an **experiment**. It can fail, and the success metrics below define
what failing looks like. Theme and name are decided: **deadchannel**.

## Why (the diagnosis, carried over and sharpened)

- ~30 concurrent users. Chat + music are healthy; the door games hold maybe 5%
  of players. Anything needing synchronous coordination starves (~3 people
  want a game at any moment, spread across many game types: a liquidity
  problem, not a UX problem). What works is ambient, zero-coordination,
  interruptible.
- **People don't want to own things in late.sh, they want to be seen in it.**
  This is now confirmed by revenue data, not just theory: the shop's only hit
  is username colors/gradients (permanent, visible in every message, pure
  status). Chat effects don't sell because they're a moment: fire once, half
  the room misses it, nothing accumulates. Same category on paper, opposite
  outcomes, and the only variable is persistence + audience.
- **Ambient care loops decay.** Bonsai/pet/aquarium engagement is visibly
  fading. They share two missing axes: no audience (nobody sees your bonsai
  unless they go looking) and no variance (day 40 is identical to day 4).
  Lesson for everything below: a feature that can't be seen from chat, the
  profile, or the feed follows the aquarium's curve no matter how charming.
- **The chip economy is deflated.** People amassed fortunes with nothing
  meaningful to want. The game must be the sink, not another faucet.
- **Doors as destinations have a ceiling.** A door you travel to competes
  with chat for attention and loses. The 5% number does not move by making
  any door better; it moves when the game happens in the room you're already
  in.
- **LORD's real retention engine was never its mechanics.** Solo, the game is
  a skeleton (grind forest, buy armor, kill dragon, reset). It survived for
  years on: other players as content (ambushes, the daily news screen,
  rivalries), sysop-driven seasonal resets, and IGM bolt-on content cadence.
  We rebuild those three engines and skip faithful-mechanics parity entirely.

North-star check for any idea: **does it ship a story into #lounge?**

## The idea in one sentence

A new game that is not a place you visit but the persistent character layer
of late.sh: your character lives in chat, things happen to it while you're
offline, daily rations gate a 10-minute ritual, and the game's entire job is
to generate stories, stakes, and status for the surfaces people already
live in.

## Decisions made (2026-08-07 spitball session)

- **New game, new name, not a Green Dragon extension.** After stripping the
  door and LORD's simulated-social features, the salvage was too thin to
  justify the constraint. The Green Dragon door **stays untouched** as-is in
  the Games hub; this game is a separate thing. No migration question:
  everyone starts fresh at season one.
- **Reuse the LoGD balance data 1:1.** Combat curves, price ladders, gem
  chances, level pacing: twenty-years-tested numbers, and they're
  fiction-free (rename the items, keep the math). Deviations from the curves
  need a stated reason. This is the real salvage from the dragon work, plus
  the pure-resolver + per-user save schema *shapes* as a parts bin.
- **No full-screen game destination.** Interaction surfaces are: a
  lightweight modal (Lobby-modal shape) for spending rations / setting fight
  plans / buying looks, the character sheet on the profile, the chat badge,
  and the feed. What died is the place you travel to, not the UI.
- **Strip the simulated-social 90%.** No gossip, flirting, gardens, outhouse
  equivalents: LORD faked a community because BBSes had one phone line; we
  have a real one and the fake one competes with it. Keep cheap *world*
  flavor (named monsters, injuries, weird events) because that's raw
  material for feed lines and the announcer. Strip fake socializing, not
  texture.
- **V1 mechanics frozen at LORD-simplicity.** One attack loop, a handful of
  gear tiers, stances for the arena, nothing else. Every "we could add"
  mid-build goes on the season roadmap, not into scope. With a new game the
  main risk is no longer a boring base, it's the blank page plus "we could
  add so much": scope is how solo projects die at 70% done.

## The design gate (apply to every proposed mechanic)

**Does another player see it?** Fight plans pass (your rival reads your
history). Ambushes pass (feed line). Cosmetics pass. A skill tree fails:
it's you, alone, in a menu. Combat depth is content players burn through in
two weeks and nobody in chat ever sees; it is explicitly not the retention
lever and gets no investment beyond the reused LoGD curves.

## Theme (decided 2026-08-07)

**The neon undercity inside the machine.** Blade Runner register, machine
substance. The city is what the inside of the machine looks like: rain that
falls as static, alleys of dead channels, bars where the signal is warm, and
out in the dark, the **glyphs**: creatures made of the same characters the
terminal renders.

- **You are a runner**: the version of you that stays in the city when you
  log off. This keeps theme-explains-mechanics: offline ambushes and the
  login news screen are diegetic (of course things happened, your runner
  never left), and Blade Runner hands us bounty hunting as the fantasy
  centerpiece, so the bounty system is fiction, not just economy.
- **The screenshot test (hard language rule).** Every game noun must be
  legible from what's on screen, never from a man page. In: glyph, static,
  signal, runner, dead channel, flicker. Out: kernel, daemon, segfault,
  root, heap. Rationale: a large share of users are not devs; static is
  folklore (haunted TVs, Matrix, vaporwave), the kernel is homework. The
  deep-lore season boss is an Old Signal shape (something broadcasting at
  the bottom of the city since before anyone connected), never a Unix
  internal.
- **Diegetic spectacle.** Enemies are made of the medium itself: a fight
  where a swarm of block characters corrupts the fight panel, a boss whose
  presence tears the frame border for everyone watching. Screen-tear as a
  boss mechanic costs a render function, not an art team, and no one needs
  it explained: they can see it.
- **The bridge fiction.** The clubhouse is the surface, the city is what's
  behind the screen, and chat spawns are the leak between them: a glyph
  flickers into #lounge, someone's runner puts it down, the room watches
  the static clear. The fiction of "the game bleeds into chat" and the
  architecture of it are the same sentence.
- The noir voice is the announcer's voice.

### Name (decided 2026-08-07)

**deadchannel.** Three layers in one word: the Neuromancer opening line
("the sky above the port was the color of television, tuned to a dead
channel"), the exact aesthetic DNA of the theme; fully non-dev legible (a
dead TV channel looks like static, everyone knows it); and late.sh speaks
IRC, rooms ARE channels, so the game's home room can literally be
**#deadchannel**: the haunted channel underneath the clubhouse. The name
gives us a place in the app for free, and place is what this design is
about.

Runner-up, kept on record because it also fits: **afterglow** (what a CRT
does when switched off, the image that keeps burning when you stop
looking, which is the offline-runner fiction in one word; warmer and more
melancholy-cozy than deadchannel). A candidate for naming something
inside the world later rather than the game itself.

Rejected: *downtime* (collides with ops vocabulary: "downtime tonight at
20:00" must never be ambiguous between maintenance and a fight card),
*static*/*signal* (too generic to own), *phosphor* (borderline on the
screenshot test), *the flicker* (reserved as fauna vocabulary alongside
glyphs and the Old Signal).

## Core design

### The daily ration loop (the ritual floor)
- X forest-equivalent fights, Y PvP attacks, one boss attempt per UTC day.
  Fixed reset so "tomorrow" is a concrete promise.
- A session spends rations in 10-15 minutes and is interruptible at any
  point. Thin combat is a feature at this session length.
- Ration status lives in the same mental slot as quests/streaks: sidebar
  line, streak bonuses mirroring the `QuestService` daily-streak shape.

### The login news screen (the killer feature)
- "While you were gone: mira ambushed you, you lost 340 gold, your bounty is
  now 8k." Every login opens with consequences.
- My ten minutes creates content for your session tomorrow. Offline PvP,
  arena results, boss kills, bounty changes, all rendered as morning news
  and (budget permitting) #lounge lines.
- Story variety over combat depth: fifty feed-line templates instead of five
  is cheap and is the difference between gossip and a cron log.

### The visibility layer (be-seen fuel)
- Customizable ASCII character: we own the renderer, looks are picked/earned/
  bought. Level/class/title as a chat badge; character sheet on the profile.
- One identity system, multiple surfaces. Never a separate progression or
  wallet for any sub-surface; that's two identity systems fighting for one
  presence layer.
- Cosmetics are the proven shop category (see Why), so lean in hard:
  top-end looks priced absurdly (a 100k-chip legendary look is the point,
  not a bug), rotating/limited seasonal stock for scarcity and shop
  check-ins.

### Chat encounters (the onboarding funnel)
- The Mudae/Pokécord shape: something spawns in a room, first to react
  fights it with their character, reward lands on their sheet, the line
  ships to the feed. Zero coordination, works at any concurrency, stronger
  the more people idle in chat: exactly our population shape.
- Solves onboarding without a manual: someone who never opened a modal now
  owns a level-1 character and has a reason to look at it.
- The Mudae warning: those bots won on pre-loaded attachment (anime
  characters, Pokémon). A homegrown collectible set has none, so the
  attachment object here is *your own character*, not the spawns.
- Spawn cadence is a feed-budget question (event, not wallpaper);
  encounters mint curiosity for the ration surfaces rather than consuming
  rations.
- **The spawn/event mechanism stays generic** (spawn → first-reactor claims
  → feed line): it's a content pipeline, not a feature. A new encounter
  type is an afternoon of data (name, art, numbers, feed lines). This is
  the anti-staleness plan (world beats: a troll week, a plague, a two-day
  siege, on a cadence we control) and the door to pet-style spawns later
  without a second identity system.

### Offline PvP / ambush (the story engine)
- Attacking sleeping players. Risk and consequence are what make the news
  screen worth reading and the feed worth gossiping about.
- Needs a consent/grief model before launch: level bands, shields after a
  loss, possibly opt-out (see Open questions).

### The arena (the spectacle and the chip sink)
- **Skill lives in preparation, not execution** (the autobattler insight).
  Owners pre-commit a secret fight plan: a stance (small RPS triangle:
  aggressive > cautious > reckless > aggressive) plus a gambit slot or two
  ("open with the big swing", "hold the potion until under 30%"). Plans are
  secret; fight *history* is public. Betting becomes reading people, not
  looking up stats.
- Keep the plan-space small (three stances, a handful of gambits). Fifteen
  knobs make plans unreadable and betting collapses into coinflips; the
  at-a-glance readability of the meta IS the product.
- **Shaped luck:** tune the resolver so the on-paper favorite wins ~65-70%.
  Script drama into the event stream deliberately (crits, near-death rally,
  botched gambit): not to change outcomes, but because the play-by-play
  needs moments people retell.
- **Parimutuel betting, never a bookmaker.** Everyone bets chips into a
  pool, winners split pro-rata, house rakes ~10% and *burns it* (chip sink
  working on every fight). Odds emerge from the crowd; showing the live
  pool split ("70% of chips on mira") provokes contrarians and is itself
  content.
- **Nightly fight card at a fixed hour.** An appointment without
  coordination: present spectators watch the play-by-play land line by
  line, everyone else gets a highlight feed line and the morning news.
  Challenge → announced in feed → betting window (hours) → resolution.
  Rides the existing daily-games deadline/your-turn plumbing.
- **Announcer ghost** on the proven @dealer plumbing, pointed at the fight
  event stream: flavor that doesn't come from a template pool, directly
  attacking line-staleness.
- Fighter side-stakes (both escrow chips, winner takes pot), title fights
  for rank-1 flair.
- Expectation-setting: at ~30 concurrents the card is 2-5 fights a night
  and that's fine. Matchmaking never gates on "enough players": challenges
  are person-to-person, plus maybe one house-arranged match daily between
  willing characters so the card is never empty.

### Bounties
- Pay chips to put a price on someone's character; whoever takes them down
  (ambush or arena) collects a cut, the rest burns. Chip sink and story
  generator in one move: "tom put 10,000 chips on mira's head" is the best
  feed line the system can produce, and it converts idle rich-player wealth
  into drama for everyone else. Bounties funnel targets onto fight cards.

### Seasons (the "what's after the boss" answer)
- Rides the existing monthly leaderboard/awards rails: monthly cycle,
  plaque resets, permanent `profile_awards` for the month's top placements,
  weekly first-slayer flair (champion-flair shape).
- LORD's prestige cycle (kill boss → reset with a mark) is already a season
  mechanic; LORD just never put a calendar on it. The reset is what makes
  the race exist.
- Season cadence is also the content cadence: each season is where
  new-game energy safely lands (a new spawn family, a world event, a
  profession) once the loop has proven people care.

### Economy rules (hard lines)
- **Chips never buy power. Only visibility (cosmetics) and stakes (bets,
  bounties, buy-ins).** A 200k-chip whale buying extra fights or better
  gear kills the game in a week.
- Game-internal gold stays internal. Chips flow *in* as sinks; out only at
  rare milestone awards (boss-slain / champion `profile_awards` + chip
  payouts, the shape NetHack and Lateania already use). Beware building a
  printing press into an already-flooded economy.

### The retention model (honest version)
- A player burns hot for 2-4 weeks, sees the content, then settles into
  ambient mode: rations in 5 minutes, the occasional spawn tap, keeps the
  badge, reads the news. **Ambient players are fully valuable**: still a
  body to ambush, a name in the arena, a line in the feed. Retired
  characters are content too. Design for that curve, not for infinite
  grind; the treadmill answer (more levels, more tiers, skill trees) is
  expensive and targets players who were leaving anyway.
- Four renewable reasons to log in tomorrow: the ration ritual (floor),
  the monthly season race (ceiling), arena/ambush rivalries (renewable
  middle: other players don't deplete), world beats (authored surprise on
  our clock).

## Experiment framing

Success metrics, named now so the experiment can fail honestly:
- stories shipped to #lounge per day (the north star)
- daily ration check-ins (ritual retention)
- bet participation (does the spectacle + sink work)
- NOT hours-in-game. Two good gossip lines a day and a 5-minute ritual for
  40 people is total success at this population.

Sequencing, each phase testing something before paying for the next:
1. **Feed budget + daily digest.** Still the hard prerequisite (global
   lines/hour budget and morning digest remain unbuilt; only the per-user
   30-min repeat window exists). Everything above multiplies feed volume.
2. **Character layer + rations + login news.** Tests: do people do the
   daily loop.
3. **Chat spawns.** Tests: does the room engage.
4. **Arena + betting + bounties.** Tests: spectacle and chip sink.
5. **Seasons** wrap it once the loop is proven.

## Open questions

- **PvP consent/grief model.** Level bands? Shields after a loss? Opt-out?
  Needs an answer before the ambush engine ships.
- **V1's one visible surface.** Chat badge is almost certainly the
  cheapest; pick explicitly at design review.
- **Professions as interdependence (v2 at the earliest).** As "+10% yield"
  they fail the design gate; as interdependence (only the blacksmith
  repairs gear, only the healer shortens recovery) they pass, because
  players need each other by name. Economy design, gated on the core loop
  proving out.

## Graveyard note

- **DRAGON.md (2026-08-07):** the dragon-as-layer thesis lives on in this
  file; "extend Green Dragon" died because the salvage (content, not
  curves) was too thin to justify the constraint. The Green Dragon door
  itself remains in the Games hub, untouched.
- **Space trade/fight MMO:** parked, not wrong. The most liquidity-hungry
  genre there is; coordination machinery for a population we don't have
  (the SOCIAL.md mistake in a spacesuit). A fine idea for a late.sh with
  500 concurrents.
- **Card/pet collection game as a separate thing:** folded into the generic
  spawn mechanism. If chat encounters work, pet-style spawn families ride
  the same pipeline and the same identity system later; never a parallel
  progression/wallet.
- **SOCIAL.md events/tournaments pillar:** still parked (see prior
  reasoning: coordination machinery for liquidity we don't have). The
  seeded score window remains the first thing to un-park once the
  character layer gives people a reason to show up daily.
