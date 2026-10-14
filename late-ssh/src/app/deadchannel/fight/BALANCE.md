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
| Ambient runner's first mark (the key) | day 16 to 22, never ahead of careful, 3 drops at most | the floor finishes in about three weeks; one patch a day costs the key a day or two and a drop over the climb, and that is the price of the day being a budget |
| Reckless runner's first mark (the key, careless) | day 18 to 28, never ahead of ambient, with 1 to 8 drops | carelessness costs, and does not lock |
| Keen runner (plays the crystal pass) | within 4 days of careful, either way | crystals are a side game, not a pace: the bright glyph pays the plain exp and its kit is priced like everyone's, so two real fights a day are paid for in signal and bought back in crystals |
| Inker (keen, the crystals to the needle first) | within 4 days of careful, either way | an ink is bought for the climbs after this one, and this one is not the price |
| The second climb on the key (one mark on the row) | 0 to 4 days sooner than the first | a mark is quicker on the way back up, and pays in the days a drop costs: the pace is set by the road (five fights a day pay a fixed exp), so only the runner who drops can be quicker, and that is the key |
| The sixth climb on the key (five marks, the bonus capped) | no slower than the second, at most 6 days sooner than the first | a veteran never outgrows the room, and the marks keep paying until the cap |
| The second climb on the key with every draft inked | 0 to 4 days sooner than the plain second | the needle pays on the climbs after, on the key, and never a tax; four inks are worth about as much as four more marks |
| A pour against the bright glyph on the key | never more than 8 points under dry, any pour, levels 4 to 15 | a card poured into the deck is never a trap for the floor; 8 points is what two 400-fight odds differ by on their own |
| Old Signal on the key with every draft inked | at most 25 points over dry | the needle helps at the gate without handing it over |
| Kit lead over the level fought at | -0.5 to +1.0 tiers, levels 3 to 15; the keen runner +2.5 at most | the kit tracks the climb: never the level 15 kit at level 8, never a tier short of a fair fight; the bright glyph's double bits and the crystals buy a tier or two, never the wall |
| Top kit first worn | level 14 or 15; the keen runner 13 or later | the armorer is a stop on every level, to the end |
| Gear's share of bits earned | 45% or more | the armorer is the main sink |
| Patch's share of the ambient runner's bits | 5 to 30%, and no more for the careful one | a cost that is felt by the runner who gets hit, not a second landlord; reading the hand is what makes it small |
| Days the careful runner spends its one patch | 40 to 90% of the days played | the patch is a decision most days and a formality on none (GAME.md, "One patch a day") |
| Lost to drops (careful) | 15% or less | a careful climb is not a lottery |
| Idle (earned, never spent or lost) | 30% or less | bits always have somewhere to go |
| Fair fight on the key, kit level with the runner | 85% or better at every level | the fair fight is the bread, not the gamble |
| Turns in a fair fight on the key | 2 to 5 | a fight is short: about three turns |
| Signal a fair fight leaves | 75 to 90% read sharp, 60% or more on the key | a day of five costs half a bar to a bar and a quarter read sharp, and two bars cover the key: the day is the unit of attrition, not the fight |
| Signal a fair fight leaves, read sharp over the key | 8 points or more | the hand is worth playing |
| Bright glyph on the key, same kit | 30% or better at every level | a risk, never a wall: risky at worst, never grim at a full bar |
| Signal a bright glyph leaves the key | 45% or less | it costs most of a signal |
| Bright glyph read sharp | 60 to 90% at its worst, and 10 points over the key's | reading it is what makes it takeable; what it costs is the signal, and the day has one patch |
| Old Signal on the key, no marks | 35 to 60% | the gate is a boss, and the floor can pass it |
| Old Signal read sharp | 15 points over the key, 65 to 90% | the one fight to read, and one a reader can lose |
| Old Signal on the key with static on ice | 10 points over dry | a glass is worth saving |
| Crystals a day (keen) | 0.8 to 2.3 | a glass a day, a cart piece every few |
| Every build, on its own | careful day 14 to 18; ambient day 15 to 25 with 3 drops at most; fair fight on the key 85% or better; bright glyph on the key 30% or better; Old Signal on the key 30 to 80%, and no worse read sharp | no deck is a trap, and none is the answer |
| Every draft's two options | within 2.5 days of each other to the mark on the key (the mean, over the builds that carry each; a drop on the key is a day and a half, so the mean moves in those steps), and within 25 points on the Old Signal | a draft is a choice only while neither card is the right one |

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
| `price_lead_percent` | 50 | what every tier above the level adds to a piece's price on the wall |
| `trade_in_percent` | 75 | what the handed-back piece takes off |
| `patch_percent` | 100 | patch: missing signal times level times this, rounded up |
| `exp_keep_on_death` | 0.65 | what a drop costs in days |
| `foe_signal_percent` | 100 | a glyph's signal over the table's: how many turns a fight lasts |
| `hit_percent`, `hit_ramp_percent` | 175, 3 | a glyph's hit: its attack less a quarter of your defense, times the first at level 1 and the second more for every level past it |
| `block_percent` | 60 | a block card, as a share of your defense |
| `cache_percent` | 50 | a cache, as a share of the fair glyph's bits |
| `old_signal` | 300 / 35 / 22 | the Old Signal's signal, attack, defense |
| `crystal_drop_one_in` | 12 | crystals from plain kills |
| `bright_signal_percent`, `bright_edge_level_percent`, `bright_guard_level_percent` | 150, 50, 0 | how hard the bright glyph is: its signal over the plain one's, and the shares of the level added to its attack and to its defense |
| `noise_level_step` | 5 | a noise turn throws one static card, and one more every this many levels |
| `bright_bits_times` | 2 | what it pays |
| `cart_crystals`, `cart_lead_crystals` | 3, 2 | the blade shop's price at or under the level, and what every tier above it adds |
| `ink_crystals` | 10 | what INK asks to ink a draft: about half of what a careful climb finds |
| `jab_percent` | 75 | a jab, as a share of a strike |
| `brace_percent` | 100 | a brace, as a share of a block |
| `siphon_mend_percent` | 75 | what a siphon mends, as a share of a strike |
| `riposte_percent` | 150 | a riposte's own hit, as a share of a block, before the block standing |
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
bright glyph, drinks, whether it inks (`Inks`), when it shops for blades
(`Carts`), whether it borrows, and how it holds its purse
(`Purse::SpendAll`, or `Purse::KeepAPatch`: never buy gear with the bits
the next patch needs). `climb(player, start, seed, max_days, &mut Bench,
&mut Journal)` plays it from `start` (`Start::FRESH`, or a row that has
marked before: the marks on it and the first `inked` drafts inked, the
second climb's bench) through the real machine, a road a day, and
returns a `Climb`: the day of every level, every day played (`days`: the level,
the kit, the bits, the crystals, the cards at dusk, and the day's kills,
runs, drop, earnings, and spending), the kit each level was fought in
(`kit_on`), the gate, the mark, the drops, the turns and fights, and a
`Ledger` of bits and crystals.

Between two steps the runner shops in the order a person would
(`Run::shop`): the card owed, patch, the loan, the armorer, INK, the
blade shop, the bar. `route` is how it picks a lane: a few steps read ahead, a
rest worth the static in the deck, a cache worth a
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
| `CAREFUL` | reads the hand, runs under 30%, spends the day's patch under 60%, keeps a patch in the purse until it is spent |
| `AMBIENT` | the same, on the `Auto` key |
| `RECKLESS` | the key, runs under 10%, the patch under 35%, spends it all |
| `NEGLECTFUL` | the key, no armorer and no warning heeded until the first drop, careful after |
| `KEEN` | careful, plus the bright glyph when it reads even and the signal is over 60%, static on ice, the blade shop |
| `HOARDER` | keen, but no glass and no blade under tier 11: every crystal for the top of the wall |
| `BORROWER` | ambient, plus the machine's loan whenever it puts the next piece in reach |
| `INKER` | keen, but the crystals go to the needle first: the first draft not inked, whenever the needle would take it |

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
- `old_signal(rules, marks, drink, inks, hand, &build)`: the same from
  the top of the wall with the gate open, with the drafts at `inks`
  inked.
- `level_economy(rules, level)`: one level as arithmetic: the kills it
  takes, what they pay, what the next pair of pieces costs.
- `climbs(player, start, rules, seeds, max_days)`: `sim::climb` over the
  seeds. `Reading::take` plays the ambient runner from `SECOND` (one
  mark), `VETERAN` (five), and `INKED` (one mark, every draft inked)
  beside the fresh starts.
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

The live reading (house build), under one patch a day:

| boss reached, careful / ambient | careful | ambient (drops) | reckless (drops) | keen | inker | second climb on the key: mark 1 / mark 5 / mark 1 inked | lv after day 1 / 7 | kit lead | top kit at | keen lead, top kit | gear | patch, careful / ambient | careful patch days | dropped | idle | fair low | turns | signal left, key / sharp | bright on the key (signal left) | bright sharp | pour low | boss key / sharp / key with a glass / key inked | crystals a day | builds: ambient day, boss on the key |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| day 16 / 17 | day 17 | day 21 (2) | day 24 (3) | day 17 | day 17 | day 20 / 16 / 16 | 3 / 9 | -0.5 to +0.5 | lv 15 | +0.5, lv 15 | 77% | 5% / 6% | 59% | 0% | 18% | 100% | 2.6 to 3.8 | 79% / 88% | 48 to 100% (38%) | 90 to 100% | -5 | 57% / 90% / 90% / 64% | 1.1 | day 20 to 24, 43 to 70% |

The curve (median level at the end of the day):

| day | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 10 | 12 | 14 | 16 | 17 | 21 | 24 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| careful | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 | 12 | 13 | 15 | 15 | marked | | |
| ambient | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 | 11 | 13 | 14 | 15 | 15 | marked | |
| reckless | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 | 11 | 13 | 14 | 15 | 15 | 15 | marked |

The first two days, read sharp (medians over the seeds; every seed is at
the same level):

| day | level | kit at dusk | cards | glyphs down | dropped | earned | gear | patch | bits at dusk | crystals |
|---|---|---|---|---|---|---|---|---|---|---|
| 1 | 3 | weapon 2, armor 2 | 1 | 5 | 0% | 1,728 | 1,066 | 30 | 673 | 0 |
| 2 | 4 | weapon 4, armor 3 | 1 | 5 | 4% | 4,044 | 3,114 | 57 | 1,169 | 1 |

Every draft's two options on the key, each over the eight builds that
carry it:

| draft | card | mean day of the mark | Old Signal |
|---|---|---|---|
| lv 3 | jab / brace | 24.1 / 23.0 | 54% / 53% |
| lv 6 | riposte / bulwark | 23.9 / 23.2 | 55% / 52% |
| lv 9 | burn / ground | 24.3 / 22.8 | 49% / 58% |
| lv 12 | sever / mute | 23.7 / 23.4 | 56% / 51% |

What the tables say:

- **The day is the unit of attrition now, not the fight.** A fair fight
  read sharp leaves 88% of the signal and 79% on the key, so five of
  them cost half a bar read sharp and a whole one on the key, before
  any bright glyph. With one patch a day that is a budget: the careful
  runner spends its patch on three days in five, and the fight it opens
  at 40% signal is the one it reads twice. Before this pass the careful
  runner patched before every fight for 3% of its bits and never ran,
  dropped, or stepped down in fifteen days.
- **The fight scales up.** The glyph's hit is 175% at level 1 and 217%
  at 15 (`HIT_RAMP_PERCENT`), and its noise throws one static card at
  level 4 and four at 15. The fair fight on the key is still 100% at
  every level with a kit level with the runner: the key is the floor,
  and what it pays now is signal.
- **The bright glyph is a fight all the way up.** On the key it goes
  down 48% of the time at its worst (level 9) and 60 to 84% at levels 7,
  11, 13 and 14, and it leaves about a third of the signal, which is
  the cost that matters under one patch. Read sharp it is 90 to 100% at
  a full bar; the sim's keen runner opens one on a full bar only, and
  at level 15 one read sharp still costs 72% of a bar. (At 60% signal
  it was a drop: the seeded keen climb dropped twelve times before that
  rule.) At level 1 to 2 it is 100% on the key, where the old rounding
  made it 1%; the first bright is not a wall any more.
- **Nobody holds the wall early.** The kit sits within half a tier of
  the level for the careful runner and the keen one alike, and both
  wear the top kit at level 15, because a piece above the level costs
  half as much again a tier (`PRICE_LEAD_PERCENT`) and the blade shop
  asks two crystals more a tier past the level. Before this pass the
  keen runner was four tiers over the level at level 9 and wore the top
  kit at 11, and at four tiers over nothing on the road was a fight.
- **The key pays a few days for the budget.** The ambient runner marks
  on day 21 with two drops (day 17 with none before), the reckless one
  on day 24 with three. The ambient band moved to day 16 to 22 and the
  builds' to 15 to 25 by decision: that is the price of the day being a
  budget, and the floor still finishes in about three weeks. The siphon
  at three quarters of a strike is what holds it there (half a strike
  put the key on day 24; `SIPHON_MEND_PERCENT`).
- **Crystals are a side game.** The keen runner marks with the careful
  one. Its kit is held to the level like everyone's and the bright glyph
  pays the plain exp, so two real fights a day are paid for in signal
  and bought back in a glass and the odd piece. The band is within four
  days of careful either way.
- **No card is the answer and none is a trap.** All sixteen builds mark
  on day 20 to 24 on the key with the Old Signal at 43 to 70%. The brace
  is a whole block for nothing (at half a block the brace builds were
  three days behind the jab ones on the key) and the riposte hits for a
  block and a half (at a block's worth its builds were three days behind
  the bulwark's).
- **The Old Signal is the one fight to read.** 300 signal and 35 attack:
  57% on the key, 90% read sharp, near sure with any glass. The ramp
  reaches it too, which is why its attack came down from 39 and why ten
  signal was the finer knob to finish on.
- **The second climb is a day quicker, and quicker again where drops
  are.** The pace is the road's: five fights a day pay a fixed exp, so
  the careful reader marks on day 17 fresh and on day 16 at one mark, at
  five, and with every draft inked alike: one mark is worth a day to the
  reader, and the cap and the needle add nothing past it. The ladder is
  the same on every climb: LoGD's per-mark exp scaling was dropped here,
  since with the pace fixed it took that day back, and more at five
  marks, for nothing. On the key the marks pay in the dropped days: day
  21 fresh, 20 at one mark, 16 at five. That is the paragon promise as
  measured: a veteran on the key climbs like a first-timer reading the
  hand.
- **The needle pays on the key, and costs nothing this climb.** The
  inker marks with the keen runner (day 17): the ten crystals come out
  of a side game. With every draft inked the second climb on the key
  marks on day 16, four days under the plain second and five under the
  first, the same day as the sixth climb: four inks are worth about what
  four more marks are. At the gate the inks are
  worth seven points on the key (57% to 64%) and nothing read sharp
  (already near sure).
- **A pour is never a trap.** Feedback (a surge) and sign-off (a mute)
  help the key against the bright glyph at nearly every level; a wipe
  poured into a clean deck hurt it (lv 9: 48% dry, 36% poured; lv 14: 69%
  to 45%), so squelch is a burn, which is free and never dead. The band
  holds every pour within the dice of dry. Read sharp, a pour moves
  the Old Signal from 90% to 88 to 95%: the pours are road cards, the
  glasses are the gate's.

## 7. Soft spots and the tests worth adding

Known, measured, and left for a pass of their own:

- **The first week runs fast.** A level a day to level 10. The exp
  ladder's first rungs are LoGD's; a `Rules` field for the early rungs
  would let a sweep slow them without touching the back half.
- **The bright glyph read sharp is near sure at a full bar.** 90% at
  its worst. What makes it a decision is the signal it takes and the one
  patch; a band of 60 to 90 read sharp says so. A pass that wants the
  reader to lose brights at a full bar has to move the attack edge, and
  the rounding at levels 3 to 4 (`BRIGHT_EDGE_LEVEL_PERCENT`, where 65
  put a cliff at the bottom) is the thing to read first.
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
  and the second-climb benches run from one and five marks, but no
  climb runs a second season in one sitting: a veteran's purse, locker
  habits, and the days between marks are not played.
- **The step down** is in no player's rule except the neglectful one's
  recovery.
- **The road's route is read three steps ahead** by the sim. A player
  who plans the whole road (the bright glyph at eight, so the rest at
  seven) would say what the map is worth.
- **The reading is slow because the test build is not optimized.** An
  optimized profile for the arena's tests would turn a two-minute sweep
  into seconds.
