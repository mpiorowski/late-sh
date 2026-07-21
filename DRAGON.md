# DRAGON.md — Green Dragon as THE game of late.sh

Status: **seed doc, why + how.** Successor to SOCIAL.md (dropped 2026-07-21;
its events/tournaments pillar is parked, see the graveyard note at the bottom).
Nothing here is committed design: every step gets its own design review before
implementation. This file exists so future spitballing starts from the thesis
instead of re-deriving it.

## Why

Carried over from the SOCIAL.md diagnosis, still true:

- ~30 concurrent users. Chat + music are healthy; anything needing synchronous
  coordination starves (~3 people want a game at any moment, spread across
  many game types: a liquidity problem, not a UX problem). What works is
  ambient, zero-coordination, interruptible.
- **People don't want to own things in late.sh, they want to be seen in it.**
  Games are the content generator for chat.
- North-star check for any idea: **does it ship a story into #lounge?**

New conclusions on top of that:

- **Doors as destinations have a ceiling.** Nostalgia asks (TradeWars etc.)
  convert to "saw it exists, played twice, back to chat". A door you have to
  travel to competes with the chat for attention and loses. The answer is not
  more doors; it's ONE game tangled into the surfaces people already live in.
- **The BBS doors were daily-ration games by design.** LORD's forest fights
  per day, TW2002's turns per day: one phone line created the constraint, the
  constraint became the design. Log in, spend your rations in 10-15 minutes,
  get ambushed while offline, come back tomorrow to read what happened to you.
  That is the same shape as bonsai watering and daily quests. The all-evening
  version of these games never existed; "heavy text game" is an illusion of
  distance.
- **Green Dragon is the only game we fully own.** Native Rust remake, no
  upstream, no license, no PTY proxy. NetHack/DCSS/Usurper/dopewars are
  foreign terminals we can only frame; the dragon we can bend around late.sh:
  its pacing, its UI, its identity system, where its surfaces live. That
  freedom is the whole opportunity, and it exists nowhere else in the roster.

## The idea in one sentence

Green Dragon stops being a door you visit and becomes the persistent
character layer of late.sh: your character exists in chat and the clubhouse
whether or not you're "playing", daily rations gate a 10-minute loop, and
other players' actions against you generate tomorrow's stories.

## How (directions to spitball, not commitments)

### The daily ration loop
- X forest fights, Y PvP attacks, one dragon attempt per UTC day. Fixed reset
  time so "tomorrow" is a concrete promise.
- A session spends rations in 10-15 minutes and is interruptible at any point.
- Ration status lives in the same mental slot as quests/streaks: sidebar
  line, maybe a bartender mention when unspent. Streak bonuses mirror the
  existing daily-quest streak shape in `QuestService`.

### Tangled into the presence layer (chat, clubhouse, profile)
- Character identity visible where people already are: level/class/title as a
  chat badge (the award-badge + `NameFlairDirectory` flair pipelines already
  do exactly this shape), character sheet on the profile, maybe the avatar
  visible in the clubhouse.
- Avatars: we own the renderer, so characters can have picked/earned looks.
  Cosmetics are "be seen" fuel and a natural chip sink.
- Slow-tutorial onboarding: no manual. New users get one nudge; after that
  the feed is the funnel ("tom slew the dragon" in #lounge makes people ask
  how). Possibly one guided first-fight beat in the existing clubhouse
  tutorial.

### Chat encounters (the third surface, and the onboarding funnel)
- The Discord-bot lesson (Mudae, Pokécord): those games won because they don't
  compete with chat for attention, they ARE chat content. Something spawns in
  the room, whoever's around reacts, everyone else watches. Zero coordination,
  works at any concurrency, stronger the more people idle in chat: exactly our
  population shape.
- Applied here: a monster wanders into #lounge, first person to hit it fights
  with their dragon character, gold lands on their sheet, the kill line ships
  to the feed. Someone who never opened the door now owns a level-1 character
  and has a reason to visit the forest.
- One character system, multiple surfaces: the door screen and the chat are
  two views of the same sheet. Never a separate progression/wallet for the
  chat game; that's two identity systems fighting for one presence layer.
- This solves the dragon's hardest problem (onboarding without a manual) and
  the chat game's hardest problem (no depth behind the tap) in one move. It is
  the headline feature of the dragon launch, not an add-on.
- Spawn cadence is a feed-budget question (rare enough to be an event, not
  wallpaper); encounters probably don't consume daily rations, they mint
  curiosity for the surfaces that do.

### Offline PvP as the story engine
- Attacking sleeping players, player mail, bar gossip about real player
  actions. My ten minutes creates content for your session.
- Every PvP result is a natural #lounge line ("mira jumped tom while he
  slept. tom lost 340 gold."). Deaths, dragon kills, level-ups likewise,
  budgeted so lines stay rare enough to be read.
- Milestone parity with the other doors: dragon slain / PvP champion mint
  one-time `profile_awards` + chip payouts like NetHack and Lateania already
  do.

### We-own-it liberties (raw, unsorted)
- Classes/titles that reference late.sh itself rather than straight LORD
  parity; combat-math parity is explicitly NOT the goal, the social layer is.
- **The arena (The Pit's soul).** A town arena where your character fights
  other players' characters, results called into #lounge. Gladiators-fighting
  is a mechanic, not a door: this absorbs the whole "host The Pit" idea
  (DOOR.md red list) for free inside the game we own. Could double as the
  consent-friendly face of PvP (arena is opt-in spectacle, ambush is the
  risky wild).
- Economy bridge: gold vs chips needs a deliberate decision (probably keep
  gold internal, pay chips only at milestone/award moments; beware creating a
  printing press).
- Weekly dragon race: first slayer of the week gets the line + flair until
  the next one (champion-flair shape).
- World beats: rare 48h happenings announced via the feed, using the fact
  that we control the world clock.
- Lobby/daily-games hooks: a PvP challenge could ride the existing daily
  correspondence infra (deadlines, your-turn notify) instead of new plumbing.

## What to figure out before building anything

- Where the character actually lives on screen: badge only? sidebar panel?
  clubhouse sprite? Pick the ONE cheapest visible surface for v1.
- Ration sizes and reset UX (what does day one feel like vs day thirty).
- PvP consent/grief model: attacking offline players is the content engine
  but needs a fairness story (level bands? shields after a loss? opt-out?).
- Feed budget: the per-user 30-min repeat window exists; a global lines/hour
  budget and a daily digest do not. The dragon multiplies feed volume, so
  the budget likely lands together with this work.
- Migration: current Green Dragon characters/saves carry over or reset?

## Salvaged from SOCIAL.md: worth implementing regardless of the dragon

- **Feed budget + daily digest (was 2a).** The hourly system-line budget
  (~4-6/hour, drop low-tier when over) and the one-line morning digest
  ("2,314 mobs slain by 9 adventurers; mira hit level 30") are still unbuilt;
  only the 30-min per-user repeat window exists. Small, finishes work that
  already landed, and it's a hard prerequisite here: the dragon multiplies
  feed volume, and every idea in this doc assumes the feed stays readable.
  This is the one piece to build first.

## Graveyard note

SOCIAL.md (2026-07-21) contained an events/tournaments pillar (weekly flagship
poker night, async brackets on the daily-games infra, seeded score windows,
Hub Events tab, clubhouse chalkboard/trophy shelf, cross-door season).
Scratched for now, not because it's wrong but because it's coordination
machinery for liquidity we don't have; the dragon-as-layer bet comes first.
Scheduled scarcity remains a valid tool to revisit once the dragon gives
people a reason to show up daily; the seeded score window (everyone plays the
same seed all week, top 3 get awards, zero pairing logic) is the first thing
to un-park when that day comes. TradeWars/twclone research lives in DOOR.md
(spike done, parked).

Future ref, bigger than the dragon: **chat-native games as a genre** (the
Mudae/Pokécord shape: spawn in chat, react to play, watch others play). The
dragon's chat encounters are our first instance of it, deliberately fused to
one character system. If they work, the genre generalizes: other spawn types,
seasonal chat events, maybe games that live ONLY in chat. Keep the mechanism
(spawn → first-reactor claims → feed line) generic enough to reuse.
