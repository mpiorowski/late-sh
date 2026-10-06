# Balancing the fight (late-ssh/src/app/deadchannel/fight)

Parent: `../CONTEXT.md` §3c. Design and the reasons: `../GAME.md`. This
file is the manual for a balance pass: what balanced means here, the
numbers that can move, the instruments that measure them, how to run a
pass, and what the last one found. What is being balanced: the day's
road of ten steps, five of them a round of cards, and the four drafts
that change the deck on the way up (`../CONTEXT.md` §3c).

The rule of the house: **a balance problem is fixed with a number, not
with a wall.** A gate (a tier a level, a cap on the purse, a hard lock)
makes a reading come out right without making the game right, and hides
the number that was wrong. If a runner holds the top kit half way up,
the kit is too cheap for what a level pays; fix the price.

## 1. What balanced means: the targets

Every target is a band on one number of a `Reading` (`arena.rs`). The
bands live in `Reading::misses`; this table says why each is there.
Change a band here and there together, and only with a design decision.

Two runners are read throughout: one on the **`Auto` key** every turn
(`Hand::Auto`, the game's own `policy::auto`), and one **reading the
hand** (`Hand::Sharp`, `policy::sharp`). The key is the floor the game
promises; the gap between the two is what playing the cards is worth.

The named players all carry one deck, `sim::HOUSE_BUILD` (jab, bulwark,
ground, sever). The last two rows of the table hold every other deck.

| Target | Band | Why |
|---|---|---|
| The Old Signal is reached (level 15 with the exp to leave it) | day 14 to 17, read sharp and on the key | the boss is on the screen after about two weeks of days played, whoever plays the cards (decided 2026-10-06: day 16 is right) |
| Careful runner's first mark (reads the hand) | day 14 to 18 | GAME.md's two to three weeks, the near end |
| Ambient runner's first mark (the key) | day 16 to 21, never ahead of careful, 3 drops at most | the floor finishes inside three weeks |
| Reckless runner's first mark (the key, careless) | day 18 to 28, never ahead of ambient, with 1 to 8 drops | carelessness costs, and does not lock |
| Keen runner (plays the crystal pass) | 0 to 5 days ahead of careful | crystals are an edge, not a skip |
| Kit lead over the level fought at | -0.5 to +1.0 tiers, levels 3 to 15 | the kit tracks the climb: never the level 15 kit at level 8, never a tier short of a fair fight |
| Top kit first worn | level 14 or 15 | the armorer is a stop on every level, to the end |
| Gear's share of bits earned | 45% or more | the armorer is the main sink |
| Patch's share of the ambient runner's bits | 5 to 30%, and no more for the careful one | a cost that is felt by the runner who gets hit, not a second landlord; reading the hand is what makes it small |
| Lost to drops (careful) | 15% or less | a careful climb is not a lottery |
| Idle (earned, never spent or lost) | 30% or less | bits always have somewhere to go |
| Fair fight on the key, kit level with the runner | 85% or better at every level | the fair fight is the bread, not the gamble |
| Turns in a fair fight on the key | 2 to 5 | a fight is short: about three turns |
| Signal a fair fight leaves, read sharp over the key | 8 points or more | the hand is worth playing |
| Bright glyph on the key, same kit | 40% or better at every level | a risk, never a wall |
| Signal a bright glyph leaves the key | 45% or less | it costs most of a signal |
| Bright glyph read sharp | 85% or better, and 10 points over the key's worst | reading it is what makes it safe |
| Old Signal on the key, no marks | 35 to 60% | the gate is a boss, and the floor can pass it |
| Old Signal read sharp | 15 points over the key, 95% at most | the one fight to read |
| Old Signal on the key with static on ice | 10 points over dry | a glass is worth saving |
| Crystals a day (keen) | 0.8 to 2.3 | a glass a day, a cart piece every few |
| Every build, on its own | careful day 14 to 18; ambient day 15 to 22 with 3 drops at most; fair fight on the key 85% or better; bright glyph on the key 40% or better; Old Signal on the key 30 to 80%, and no worse read sharp | no deck is a trap, and none is the answer |
| Every draft's two options | within 1.5 days of each other to the mark on the key (the mean, over the builds that carry each), and within 25 points on the Old Signal | a draft is a choice only while neither card is the right one |

A reading plays all sixteen builds (`BuildReading`, 40 seeds each for
the careful and the ambient runner) beside the named players, so the
contract fails by name when a card is retuned into a trap.

`sim_test.rs` holds the three pace windows, their order, and the lock
(the neglectful runner gains a level within a week of its first drop) on
their own, so a change to a curve fails fast and prints the whole
ladder. It also holds one seeded `KEEN` climb whole
(`one_keen_climb_holds_still`): a change that moves the climb inside
every band still shows up there as a diff.
`arena_test.rs::the_live_rules_miss_no_target` holds every band.

## 2. The knobs: `data::Rules`

Every tunable is a field of `Rules` (`data.rs`); `RULES` is the live
game, built from the constants beside it. The machine takes the rules as
a value (`Sheet::apply_under`), so a candidate is tried by building a
`Rules`, never by editing a constant and recompiling.

| Field | Live | Moves |
|---|---|---|
| `pay_bits_percent` | 400 | bits per kill, over LoGD's creature table |
| `pay_exp_percent` | 540 | exp per kill: the pace of the levels |
| `price_percent` | 225 | the armorer's wall, over `COST_LADDER` |
| `trade_in_percent` | 75 | what the handed-back piece takes off |
| `patch_percent` | 100 | patch: missing signal times level times this, rounded up |
| `exp_keep_on_death` | 0.65 | what a drop costs in days |
| `foe_signal_percent` | 100 | a glyph's signal over the table's: how many turns a fight lasts |
| `hit_percent` | 185 | a glyph's hit: its attack less a quarter of your defense, times this |
| `block_percent` | 60 | a block card, as a share of your defense |
| `rest_mend_percent` | 35 | what a rest mends of the signal's max |
| `cache_percent` | 50 | a cache, as a share of the fair glyph's bits |
| `old_signal` | 290 / 39 / 22 | the Old Signal's signal, attack, defense |
| `crystal_drop_one_in` | 12 | crystals from plain kills |
| `bright_signal_percent`, `bright_edge_percent` | 150, 115 | how hard the bright glyph is |
| `bright_bits_times` | 2 | what it pays |
| `cart_crystals` | 3 | the blade shop's price |
| `jab_percent` | 75 | a jab, as a share of a strike |
| `siphon_mend_percent` | 50 | what a siphon mends, as a share of a strike |
| `bulwark_percent` | 250 | a bulwark, as a share of a block |
| `burn_energy` | 2 | the energy a burn gives before any static feeds it |
| `ground_percent` | 100 | what a ground adds for each static card in hand, as a share of a strike |
| `sever_percent` | 250 | a sever once the glyph is at half, as a share of a strike |

A drafted card's number is said in words in three places: its
`Card::rule` line (`cards.rs`, the draft's offer), its face (`ui.rs`),
and the guide (`guide/data.rs`). Move them with the knob.

Not in `Rules`, and why: the creature table, the exp ladder, and the
gear ladder are LoGD's curves (retune the percent over them first); the
rations a day, the signal a level, the deck, the hand, the energy, the
drafts' levels and what each replaces (`cards::DRAFTS`), and the road's
shape (`road.rs`: five fight steps, two bright nodes) are the shape of
the day; a glyph's `pattern` is its identity. To make any other
constant sweepable, add a field, read it from the rules where the
constant was read, and set it in `RULES`.

## 3. The instruments

**`sim.rs`: a player, a climb, and a journal.** A `Player` is a way of
playing: who plays the cards (`Hand::Auto` or `Hand::Sharp`), the card
it takes at each draft (`build`), when it runs, when it patches, when it
steps down, when it starts heeding (`Heeds`), whether it routes to the
bright glyph, drinks, when it shops for blades (`Carts`), whether it
borrows, and how it holds its purse (`Purse::SpendAll`, or
`Purse::KeepAPatch`: never buy gear with the bits the next patch needs).
`climb(player, seed, max_days, &mut Bench, &mut Journal)` plays it from
a fresh row through the real machine, a road a day, and returns a
`Climb`: the day of every level, every day played (`days`: the level,
the kit, the bits, the crystals, the cards at dusk, and the day's kills,
runs, drop, earnings, and spending), the kit each level was fought in
(`kit_on`), the gate, the mark, the drops, the turns and fights, and a
`Ledger` of bits and crystals.

Between two steps the runner shops in the order a person would
(`Run::shop`): the card owed, patch, the loan, the armorer, the blade
shop, the bar. `route` is how it picks a lane: a few steps read ahead, a
rest worth what the signal and the deck are missing, a cache worth a
cache, a bright glyph a detour for the player who takes them and a berth
for the one who does not. A runner under its run line stays in the fight
when the way out would be the end of the day. A `Bench` is the rules
plus a memo of the road's threat reads, shared across a batch
(`Bench::live()`, `Bench::under(rules)`).

With `Journal::On` the climb also writes down everything it did, in
order, as `Event`s: every dawn and dusk with the sheet, every shop
visit, every step and what it met, and every turn of every fight with
the glyph's move, the hand dealt, the cards played, and where both
stood after. `Journal::Off` is what the batches use. A journal changes
nothing about the climb (`sim_test.rs` holds that).

| Player | Plays like |
|---|---|
| `CAREFUL` | reads the hand, runs under 30%, patches under 75%, keeps a patch in the purse |
| `AMBIENT` | the same, on the `Auto` key |
| `RECKLESS` | the key, runs under 10%, patches under 35%, spends it all |
| `NEGLECTFUL` | the key, no armorer and no warning heeded until the first drop, careful after |
| `KEEN` | careful, plus the bright glyph when it reads even, static on ice, the blade shop |
| `HOARDER` | keen, but no glass and no blade under tier 11: every crystal for the top of the wall |
| `BORROWER` | ambient, plus the machine's loan whenever it puts the next piece in reach |

`sim::builds()` is all sixteen builds. A named player on another deck is
`Player { build, ..sim::AMBIENT }`.

**`arena.rs` (test-only): the instruments, smallest to largest.**

- `matchup(rules, &Recipe, Pick, Hand)`: one runner (a level, a `Kit` as
  a lead over the level, a glass, marks, the cards drafted) against one
  pick, the cards played by one hand, 400 fights to the end: odds,
  turns, signal left, static left in the deck, the patch after, the pay.
  `Recipe::level_kit(level, &build)` is the usual one: the kit level
  with the runner and the build's picks for the drafts that level has
  reached.
- `old_signal(rules, marks, drink, hand, &build)`: the same from the top
  of the wall with the gate open.
- `level_economy(rules, level)`: one level as arithmetic: the kills it
  takes, what they pay, what the next pair of pieces costs.
- `climbs(player, rules, seeds, max_days)`: `sim::climb` over the seeds.
- `BuildReading::take(rules, build)`: one build's numbers, and
  `draft_spreads`, each draft's two options side by side.
- `Reading::take(rules)`: every number the targets are stated in, 80
  seeds a named player and every build beside them, the batches on
  threads (`fan`). `Reading::misses()` lists the bands it is out of.
- `run(name, player, seed, rules)`: one journaled climb printed whole.

Everything is seeded: the same rules give the same reading.

## 4. Running a pass

1. `make deadchannel-arena` writes `late-ssh/target/deadchannel-arena.md`:
   the live reading with its misses, then every table (section 5).
   Start here to see where the game is. About a minute and a half.
2. `make deadchannel-run` writes `late-ssh/target/deadchannel-run.md`:
   three whole runs, start to finish. Read a day of it before trusting a
   table: a rule that reads right as a median can still be a bad day.
3. Put the candidates in `arena_test.rs::SWEEP`, each a name and a
   `Rules { field: value, ..RULES }`, the live rules first.
4. `make deadchannel-sweep` writes `late-ssh/target/deadchannel-sweep.md`:
   one reading per candidate, side by side, a miss count, every draft's
   two options per candidate, and the missed bands spelled out
   underneath. About fifteen seconds a candidate.
5. Move one knob at a time until the cause is clear, then sweep the
   neighbours of the pick: a setting whose neighbours miss is a knife
   edge and will not survive the next change.
6. Set the constants in `data.rs`. Run the targeted suite: the contract,
   `sim_test.rs`, and the tests that spell numbers out (`state_test.rs`,
   `ui_test.rs`, `city/ui_test.rs`, `svc_test.rs`). Read each moved
   number before re-blessing it.
7. Update the guide (`guide/data.rs`) and the card's rule line and face
   if a rule a player reads changed, and section 6 here with the new
   reading.

Why a pass takes minutes: a fight is small, but a reading is about
1,600 whole climbs (four named players on 80 seeds, sixteen builds on 40
seeds for two players) of about eighty fights each, the runner reading
the hand weighs every order of every affordable set of cards on every
turn, and the test build is not optimized.

## 5. Reading the report

- **The reading.** The targets' numbers, one row. `misses` is 0 when
  the game is in every band.
- **The climb, day by day.** The median level every player stands at
  when each day ends: the curve a habit is made of. Read it for how fast
  the first week runs and where the climb slows.
- **A day at a time.** Per player, every day until most climbs have
  marked: the level at its lowest, median, and highest over the seeds,
  the kit, the cards drafted, the kills and the bright ones, the runs,
  the share of climbs that dropped that day, what the day paid, what
  went on gear and on patch, and the bits and crystals on hand at dusk.
  Day one and day two are the rows to read first: they are what a new
  runner meets.
- **The drafts.** At each draft's level, kit level with the runner: the
  fair fight and the bright one on the key and read sharp, with the
  card the draft replaces kept, and with each option.
- **Every build.** Sixteen rows: the day of the mark read sharp and on
  the key, the fair fight and the bright one at their worst on the key,
  the Old Signal on the key and read sharp. Then every draft's two
  options, each averaged over the eight builds that carry it.
- **What reading the hand is worth.** At every level from a kit level
  with the runner, on the house build: the deck, the numbers printed on
  the cards (strike, block, the glyph's hit), then the fair fight and
  the bright one on the key and read sharp, each as odds, turns, signal
  left, and static left.
- **What gear is worth.** The fair fight on the key at every level by
  kit lead: bare hands, 4 to 1 tiers behind, level, 1 to 4 ahead.
- **Where a level's bits go.** Kills to clear the level and the days of
  fights that is, what they pay, the next pair's price against that pay
  (`pair / pay`: near 100% means a level buys its kit and little more),
  the patch after a fair kill against the kill's pay, and a cache.
- **The bright glyph and the step down.** On the key: the step down, the
  bright glyph a tier behind, level, a tier ahead, and with each glass.
- **The Old Signal.** Odds by marks: on the key, read sharp, and read
  sharp with each glass, and how many turns it takes.
- **Climbs.** Per player, the median day and kit of every level, then
  the mark, the drops, the boss tries, the turns a fight, and the ledger.

The printed run (`deadchannel-run.md`) is a different kind of reading.
Each day opens on the sheet, the deck, and the road (`g` a glyph, `B` a
bright one, `+` a rest, `$` a cache), lists every shop visit and step in
order, and closes on the sheet and the day's totals. The first two days,
every fight with the Old Signal, and every fight that was lost print
every turn: what the glyph showed, the hand, the cards played, and
where both stood.

## 6. Measured

The live reading (house build):

| boss reached, careful / ambient | careful | ambient (drops) | reckless (drops) | keen | lv after day 1 / 7 | kit lead | top kit at | gear | patch, careful / ambient | dropped | idle | fair low | turns | signal left, key / sharp | bright on the key (signal left) | bright sharp | boss key / sharp / key with a glass | crystals a day | builds: ambient day, boss on the key |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| day 16 / 16 | day 16 | day 17 (0) | day 20 (1) | day 16 | 3 / 10 | -0.5 to +1.0 | lv 14 | 75% | 3% / 11% | 0% | 22% | 100% | 2.6 to 3.8 | 77% / 91% | 64 to 100% (34%) | 93 to 100% | 50% / 90% / 94% | 1.9 | day 16 to 20, 50 to 65% |

The curve (median level at the end of the day):

| day | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 10 | 12 | 14 | 16 | 17 | 20 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| careful | 3 | 4 | 5 | 7 | 8 | 9 | 10 | 10 | 12 | 14 | 15 | marked | | |
| ambient | 3 | 4 | 5 | 7 | 8 | 9 | 10 | 10 | 12 | 14 | 15 | 15 | marked | |
| reckless | 3 | 4 | 5 | 7 | 8 | 9 | 10 | 10 | 12 | 14 | 15 | 15 | 15 | marked |

The first two days, on the key (medians over the seeds; every seed is at
the same level):

| day | level | kit at dusk | cards | glyphs down | earned | gear | patch | bits at dusk | crystals |
|---|---|---|---|---|---|---|---|---|---|
| 1 | 3 | weapon 2, armor 1 | 1 | 5 | 1,656 | 641 | 52 | 1,016 | 0 |
| 2 | 4 | weapon 4, armor 3 | 1 | 5 | 3,720 | 3,539 | 166 | 1,257 | 1 |

Every draft's two options on the key, each over the eight builds that
carry it:

| draft | card | mean day of the mark | Old Signal |
|---|---|---|---|
| lv 3 | jab / siphon | 19.8 / 19.7 | 55% / 57% |
| lv 6 | riposte / bulwark | 20.1 / 19.5 | 55% / 57% |
| lv 9 | burn / ground | 20.2 / 19.4 | 56% / 56% |
| lv 12 | sever / mute | 20.1 / 19.5 | 55% / 56% |

What the tables say:

- **The boss is on the screen on day 16, whoever plays the cards.** A
  level a day for the first week, then a level every day and a half;
  level 15 on day 14 and two days earning the exp that opens the gate.
  The hand and the build decide what happens at the gate, never when it
  opens: the road has five fights a day for everybody.
- **Day one ends at level 3 with the first card in the deck**, tier 2 and
  tier 1 on the wall bought, and about a thousand bits on hand; day two
  ends at level 4 in a tier 4 weapon. Nobody drops on either day who
  visits the armorer; the neglectful runner drops on day one.
- **The hand is worth about a third of the damage.** A fair fight on the
  key takes three turns and leaves 64 to 83% of the signal (77% on
  average); read sharp it leaves 91%. The key got better in this pass:
  it now puts energy it has left over on a block when something is
  landing, which is also what makes a burn and a bulwark worth drafting
  for a runner who never reads a hand.
- **No card is the answer and none is a trap.** Every draft's two
  options land within a day of each other on the key and within two
  points on the Old Signal. All sixteen builds mark on day 16 read sharp
  and on day 16 to 20 on the key, with the Old Signal at 50 to 65%.
- **Gear is still the fight.** Every card's number is a share of the
  strike or the block: the kit sits within a tier of the level all the
  way up and the top kit waits for level 14.
- **Patch is the key's bill.** 11% of what the ambient runner earns and
  3% for the careful one. The rests on the road are the free
  alternative.
- **The bright glyph is safe to read and a risk on the key.** Read sharp
  it goes down 93% of the time or better from level 4; on the key it is
  64% at its worst (level 9) and leaves about a third of the signal. The
  keen runner takes about two a day and marks with the careful one: the
  crystals buy tiers early, not days. The hoarder, who saves every
  crystal for the top third of the wall, marks two days later than the
  keen runner: spending them is right.
- **The Old Signal is the one fight to read.** On the key it is a coin
  toss (50%), read sharp nine in ten, and a glass of static on ice makes
  the key near sure (94%). A failed try is a dropped signal and three
  or four days of exp, which is the whole of the day between the
  careful runner's mark and the ambient one's median, and of the three
  days more for the reckless one.
- **The loan is small change.** The borrower takes about 5,500 bits over
  a climb for 555 in fees and marks with the ambient runner.
- **The neglectful runner** drops on day one, heeds the armorer from
  then on, reaches the gate on day 21, and marks about day 25.

How the draft was tuned. The first cut of the eight cards went in over
the round as it stood, and the first reading named the problems:

- **The game got easier by a deck.** The Old Signal read 82% on the key
  and 100% read sharp, and nothing dropped the reckless runner. Glyph
  hits went from 170% to 185%, the Old Signal from 245 signal to 290,
  and the bright glyph from 135% of the plain one's signal to 150%.
- **The burn was a trap.** It gave two energy and put a static card in
  the deck; the key played it every turn and choked. Builds with it
  marked on day 20 to 25 on the key against day 16 for the ground. It
  now only gives: two energy, and one more for each static card in
  hand, burned up.
- **The key could not use a card that was not a hit.** It only ever
  blocked on a heavy, so a bulwark and a burn bought it nothing, and the
  gap to a runner reading the hand at the Old Signal was 49% against
  98%. The key now spends leftover energy on a guard against whatever
  is landing. That one rule brought every draft's options together.
- **The jab and the sever were a shade light**: 50% of a strike and
  double. 75% and two and a half put them level with the siphon and the
  mute.
- **Two measures were wrong, not the game.** Patch was read off the
  careful runner, who is barely hit; it is the key's bill, so the band
  moved to the ambient runner. And a draft's options were compared by
  the median day of the mark, which moves a failed try (four days) at a
  time; the mean over the seeds is what is compared now.

The cliffs, all of them the same cause:

- **No dice means cliffs.** With only the shuffle random, a fight's
  odds move in steps: one more hit needed to put a glyph down is one
  more of its moves landing. The neighbours of the live numbers, each
  moved alone: the Old Signal at 280 signal reads 64% on the key and at
  300 reads 42% (290 is 50%); hits at 175% leave the reckless runner
  nothing to drop to, and at 195% the bright glyph falls to 38% on the
  key and two dozen bands go with it; blocks at 55% put four builds out
  of the pace. Every number near one of these is held by the contract,
  and a retune of strike, the creature table, the wall, or a card must
  re-read the boss, the bright glyph, and every build first.

## 7. Soft spots and the tests worth adding

Known, measured, and left for a pass of their own:

- **The first week runs fast.** A level a day to level 10. The exp
  ladder's first rungs are LoGD's; a `Rules` field for the early rungs
  would let a sweep slow them without touching the back half.
- **A bright glyph before the first draft is a wall.** At level 1 it
  reads 1% on the key and 15% read sharp (the reading holds it from
  level 4, where the first card is in the deck). The road never opens on
  one and says grim, but a new runner's first bright node is one to walk
  around, and nothing teaches that but the word.
- **The threat word is a cliff too.** It is the fight on the key from
  the sheet as it stands, so most fair fights read easy and a bright
  one reads easy or grim with little between.
- **The live numbers sit in a narrow valley.** Every neighbour in
  section 6 misses at least one band. The bands for every build and
  every draft are what made it narrow; they are also what caught the
  burn.
- **The house build is one of sixteen.** The named players carry it, so
  the pace bands are read from one deck and the build bands from all of
  them at half the seeds. A pass that moves a card should read the
  builds table, not only the misses.
- **The Old Signal with marks.** The report prints the odds by marks,
  but no climb runs a second season: `exp_to_advance` scaling and the
  mark bonus are untested as a pace, and with marks the boss is near
  sure on the key. By decision (2026-10-06) nothing past the first mark
  is tuned yet.
- **The step down** is in no player's rule except the neglectful one's
  recovery.
- **The road's route is read three steps ahead** by the sim. A player
  who plans the whole road (the bright glyph at eight, so the rest at
  seven) would say what the map is worth.
- **The reading is slow because the test build is not optimized.** An
  optimized profile for the arena's tests would turn a two-minute sweep
  into seconds.
