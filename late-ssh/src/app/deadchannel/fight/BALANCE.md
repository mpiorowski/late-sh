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

The live reading:

| careful | ambient (drops) | reckless (drops) | keen | lv after day 1 / 7 | kit lead | top kit at | gear | patch | dropped | idle | fair low | turns | signal left, key / sharp | bright on the key (signal left) | bright sharp | boss key / sharp / key with a glass | crystals a day |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| day 16 | day 20 (1) | day 25 (4) | day 16 | 3 / 10 | -0.5 to +0.5 | lv 15 | 79% | 9% | 0% | 12% | 100% | 3.0 to 3.9 | 71% / 88% | 62 to 100% (27%) | 99 to 100% | 55% / 92% / 97% | 2.1 |

The curve (median level at the end of the day):

| day | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 10 | 12 | 14 | 16 | 20 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| careful | 3 | 4 | 5 | 7 | 8 | 9 | 10 | 10 | 12 | 14 | 15 | marked | |
| ambient | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 | 12 | 13 | 14 | 15 | marked |

What the tables say:

- **A level a day for the first week, then a level every day and a
  half.** Level 3 falls on day one for every player and level 10 by the
  end of the first week. The careful runner reaches 15 on day 14 and
  spends two days earning the exp that opens the gate.
- **The hand is worth about half the damage.** A fair fight on the key
  takes three to four turns and leaves 59 to 89% of the signal (71% on
  average); read sharp it leaves 76 to 100% (88%). Over a five-fight
  road that is the difference between needing a patch and not.
- **Gear is still the fight.** The weapon is the strike and the armor
  is the block and the hit: the kit sits within half a tier of the
  level all the way up and the top kit waits for level 15.
- **Patch is felt and never a spiral.** At a bit a point it is 9% of
  what a careful runner earns and about 13% for the runner on the key.
  The rests on the road are the free alternative, which is what makes
  taking one a choice against a cache.
- **The bright glyph is safe to read and a risk on the key.** Read
  sharp it goes down 99% of the time or better at every level; on the
  key it is 62% at its worst (level 14) and leaves about a quarter of
  the signal. The keen runner takes about two a day and marks with the
  careful one: the crystals buy tiers early, not days.
- **The Old Signal is the one fight to read.** On the key it is about a
  coin toss (55%), read sharp nine in ten, and a glass of static on ice
  makes the key near sure (97%). A failed try is a dropped signal and
  two or three days of exp, which is the whole of the four days between
  the careful runner's mark and the ambient one's.
- **The neglectful runner** drops on day one, heeds the armorer from
  then on, and marks about day 22.

How the round was tuned. The first cut carried the old exchange loop's
feel over (hits at 70% of the formula, blocks at 35% of defense): a fair
fight on the key left 90% of the signal and nothing on the road could
drop a runner. What moved it, in order:

- **Hits.** A fight is about three turns, so the glyph only gets two
  moves in. For those to matter a plain hit has to be worth about a
  fifth of the signal, a heavy twice that: 170%.
- **Blocks.** At 55% of defense a block is worth about what a strike is
  and reading the hand buys little; at 70% the sharp runner walls up and
  takes nothing. 60% leaves the question open every turn.
- **Patch.** With rests on the road patch at half a bit was 4% of the
  bits; a bit a point is 9%.
- **Patterns.** A glyph that opens on two plain hits (the old crackle
  and test pattern) took half a signal from the key before it could
  act; one that opens on noise and a wind-up (the old interference)
  took nothing. Those three were rewritten; the spread on the key is
  now 59 to 89%.

Soft spots found on the way, all of them the same cause:

- **No dice means cliffs.** With only the shuffle random, a fight's
  odds move in steps: one more strike needed to put a glyph down is one
  more of its moves landing. The Old Signal at 245 signal is 55% on the
  key; at 250 it is 26%. The bright glyph at 135% signal reads 62 to
  100% on the key; at 145% it reads 0% at some levels. Every number
  near one of these is held by the contract, and a retune of strike,
  the creature table, or the wall must re-read the boss and the bright
  glyph first.

## 7. Soft spots and the tests worth adding

Known, measured, and left for a pass of their own:

- **The first week runs fast.** A level a day to level 10. The exp
  ladder's first rungs are LoGD's; a `Rules` field for the early rungs
  would let a sweep slow them without touching the back half.
- **The threat word is a cliff too.** It is the fight on the key from
  the sheet as it stands, so most fair fights read easy and a bright
  one reads easy or grim with little between. It is honest, and it is
  less of a dial than it was under dice.
- **The blade shop's worth grows with the tier.** Three crystals buy 108 bits
  of gear at tier 1 and over 20,000 at tier 15. `KEEN` buys as soon as
  it holds three; a hoarder who saves every crystal for the last tiers
  is the player to add before trusting its price.
- **The Old Signal with marks.** The report prints the odds by marks,
  but no climb runs a second season: `exp_to_advance` scaling and the
  mark bonus are untested as a pace, and with marks the boss is near
  sure on the key.
- **The step down and the loan** are in no player's rule except the
  neglectful one's recovery. A runner who borrows for a tier is the test
  of `LOAN_PER_LEVEL` (50 a level, small against the 225% wall).
- **The road's route is read three steps ahead** by the sim. A player
  who plans the whole road (the bright glyph at eight, so the rest at
  seven) would say what the map is worth.
