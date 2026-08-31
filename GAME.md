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

### First contact (the haunting, decided 2026-08-31)

- **The game is never announced; it arrives.** Onboarding as haunting: the
  bridge fiction says the city is behind the screen and chat is the leak,
  so first contact IS a leak. No tutorial, no reward, no explanation, and
  the first beats deliberately end with nothing: restraint is the magic.
- **The escalation ladder** (cheapest to loudest, each stage roughly an
  afternoon of render code, all pure client-side theater until the last):
  1. **Deniable.** Ambient corruption where the eyes already are, and
     the v1 target is decided: **the sidebar clock.** A glitch is only
     legible against stability, and the clock is the most stable,
     most-glanced-at element on screen (pinned core block, Home and
     Arcade); the visualizer is the trap choice (already chaos, a burst
     of static there reads as the visualizer being a visualizer), the
     ticker is busy text, the splash too brief. Spec for the
     implementer: replace one or two characters of the rendered HH:MM
     with characters drawn from the game's fixed glyph alphabet (the
     same set stage-4 spawns will render with, so the clock glitch is
     retroactive foreshadowing), hold roughly 200ms (one frame at 15fps
     is too fast to trust), restore. Rolled per session (independent
     dice, so two people almost never see it together: "did anyone else
     see that?" gets "no?" back, which is deniability and gossip in one
     move), rare (order of once per hours-long session, at most once or
     twice a day per user), render-layer only, no DB, behind the
     kill-switch and admin-scoped until the fuse is lit. Chrome, never
     content: chat message bodies stay untouched (a glitched clock is
     spooky, a glitched sentence reads as data corruption). NOT the
     clubhouse: the tavern is a hallway everyone tabs straight out of,
     and its only lingering audience is brand-new users, the one group
     the fuse must not spend itself on. Later variety (splash tear,
     ticker stamp, bonsai leaves) rides the same machinery. Nobody is
     sure they saw it.
  2. **Personal.** The corruption chooses you, and the v1 target is
     decided: **your own name.** Spec for the implementer: in the
     sender's session only, immediately after their message lands (the
     one moment of guaranteed attention: eyes always follow your own
     send), the author label of that just-landed message renders with
     one or two of its characters swapped for glyph-alphabet
     characters, holds roughly 300ms, heals. The body is never touched:
     the escalation over stage 1 is targeting, not content ("chrome,
     never content" still holds; your name is chrome that happens to be
     *you*). Rejected on purpose: corrupting the message echo itself,
     which plants "did that send garbled to everyone?", a
     data-integrity doubt and the panic rule violated in its most
     personal form. Chosen users only (the eligibility gate), rare
     (order of one in dozens of sends, capped once per UTC day, a
     handful of total hits per person), render-layer only, no DB beyond
     the per-user arming counter, kill-switch. Later variety (your name
     in the sidebar, the composer placeholder) rides the same
     machinery. Thematic payoff: when the stage-3 whisper says the
     static knows your name, it is describing what already happened.
  3. **The whisper (the held door).** Delivered on the splash screen,
     once per person ever: this one time it does not skip. Triggered by
     the chain, not the calendar: stage-2 hits arm it (the per-user
     counter), and it fires on the armed user's next fresh connect, so
     the beat is flickers one evening, then the door holds when they
     come back: the haunting follows you home. The splash is
     the liminal space (the doorway between outside and inside the
     machine), inherently private and per-session, so the whisper
     touches no chat surface at all. The load-bearing mechanic:
     **respond, don't ignore.** Esc must visibly do something, just not
     what it usually does: static surges, the skip hint itself corrupts
     and dissolves, the mysterious voiced line types itself in answer.
     Input acknowledged but control withheld reads as *something is
     holding the door*; input silently ignored reads as a hung
     terminal, which is the panic rule violated exactly. Hard time cap
     of a few seconds, then it releases on its own whether or not they
     pressed anything. Screenshot-bait, "glitches are coming" energy;
     still no game to play.
  4. **The invitation (decided 2026-08-31: the whole game is opt-in).**
     Not a breach: no game ever lands on anyone unasked. Some days
     after the held door, the contact goes real: a DM from the game's
     first voice, a character calling for help from the other side, not
     a system announcing a feature (name and copy at design review; a
     plea beats a pitch, it makes the reader the protagonist). It rides
     the proven ghost-user plumbing (@bartender's shape: dedicated DB
     user, fixed fingerprint) and, unlike stages 1-3, it persists on
     purpose: this is where the fiction goes real, and an invitation
     that vanishes cannot be followed three days later. It ends with
     the only instruction the entire haunting ever gives:
     `/join #deadchannel`. Typing the command IS the consent: runner
     created, the haunted channel under the clubhouse opens, the game
     exists for you and nobody else. The name decision (rooms ARE
     channels) becomes the consent mechanism. The invitation gates who
     learns first, never who may enter: anyone who overhears the
     channel's name in the lounge and types the command gets in, so
     the chosen are wave one and gossip converts the rest. The old
     stage 4 (a glyph flickering into #lounge, the room watching a
     runner put it down) is not dead, it is relocated: that is the
     game's public phase, once enough runners exist to make it a show.
- **The eligibility gate is a whisper campaign.** Stages 2-4 target users
  with a filled bio, touched settings, and real tenure (thresholds at
  design review): the static chooses the invested. Side effects are the
  point: it pushes profile completion, and it makes the first glitch a
  social event (the chosen asking #lounge "did anyone else see that??"
  while half the room has no idea). The mystery ships a story into
  #lounge before the game has a single mechanic: north-star meets day
  zero.
- **Hard rules:**
  - **Never eat the message.** Enter on a written message is an act of
    trust in the app's healthiest feature. The haunting is theater
    *around* the send, never the send itself: the message always
    delivers untouched; only the sender's local echo, the idle
    composer, or the ambient frame get corrupted.
  - **Render-only, TUI-only.** No DB rows, no chat history, and IRC
    clients see nothing, which is the fiction stated as architecture:
    the city only leaks through the terminal.
  - **Aesthetic, never system.** The audience is terminal people; a
    hijacked input can read as "compromised server", and that panic is
    the failure mode. No fake errors, no fake disconnects, nothing
    resembling a real terminal failure. Static and corruption are
    obviously *voiced*, never mechanical.
  - **Admin kill-switch from day one**, and a ready answer for the
    inevitable "I found a display bug" report.
- **First contact is a nonrenewable resource.** It works exactly once per
  person. Two timing rules: never burn it on real users while it is
  admin-scoped scaffolding, and never light it until the breach is close.
  The whole ladder is a fuse of one to two weeks before the first real
  spawn, not a promise without a date: "glitches are coming" followed by
  months of nothing curdles into a broken feature.

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
   Now specced separately in **DIGEST.md** (2026-08-31): the same engine
   is the welcome-back paper, and the login news screen ships as a
   section of that paper rather than its own surface.
2. **Character layer + rations + login news.** Tests: do people do the
   daily loop.
3. **Chat spawns.** Tests: does the room engage. Spawns start inside
   #deadchannel for the opted-in; the #lounge leak (the relocated
   breach, the room watching a runner put a glyph down) is this phase's
   public beat, shipped only once enough runners exist to make it a
   show. The first-contact haunting ladder (see Core design) is the
   fuse: it ends at the invitation, so it must not fire for real users
   until phase 2's character layer can receive a `/join #deadchannel`;
   its machinery can be built and admin-tested any time earlier.
4. **Arena + betting + bounties.** Tests: spectacle and chip sink.
5. **Seasons** wrap it once the loop is proven.

## Open questions

- **PvP consent/grief model.** The base layer is now decided
  (2026-08-31): the whole game is opt-in via `/join #deadchannel`, so
  nobody who never joined can be ambushed, ever. Still open is the
  in-game tuning for people who did join: level bands? Shields after a
  loss? A way to retire a runner without deleting it? Needs an answer
  before the ambush engine ships.
- **V1's one visible surface.** Chat badge is almost certainly the
  cheapest; pick explicitly at design review.
- **First-contact tuning.** The eligibility thresholds (bio length,
  which settings count as touched, how much tenure), and the whisper
  copy pool: the voiced lines need the same variety discipline as feed
  templates, since a repeated whisper is a bug report, not a haunting.
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
