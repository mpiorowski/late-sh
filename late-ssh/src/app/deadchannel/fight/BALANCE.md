# Balancing the fight (late-ssh/src/app/deadchannel/fight)

Parent: `../CONTEXT.md` §3c. Design and the reasons: `../GAME.md`. This
file is the manual for a balance pass: what balanced means here, the
numbers that can move, the instruments that measure them, how to run a
pass, and what the last one found.

The rule of the house: **a balance problem is fixed with a number, not
with a wall.** A gate (a tier a level, a cap on the purse, a hard lock)
makes a reading come out right without making the game right, and hides
the number that was wrong. If a runner holds the top kit half way up,
the kit is too cheap for what a level pays; fix the price.

## 1. What balanced means: the targets

Every target is a band on one number of a `Reading` (`arena.rs`). The
bands live in `Reading::misses`; this table says why each is there.
Change a band here and there together, and only with a design decision.

| Target | Band | Why |
|---|---|---|
| Careful runner's first mark | day 18 to 24 | GAME.md's three weeks |
| Reckless runner's first mark | day 25 to 31, with 2 to 8 drops | care is worth a week; recklessness costs, and does not lock |
| Keen runner (plays the crystal pass) | 0 to 7 days ahead of careful | crystals are an edge, not a skip |
| Kit lead over the level fought at | -0.5 to +1.0 tiers, levels 3 to 15 | the kit tracks the climb: never the level 15 kit at level 8, never a tier short of a fair fight |
| Top kit first worn | level 14 or 15 | the armorer is a stop on every level, to the end |
| Gear's share of bits earned | 45% or more | the armorer is the main sink |
| Patch's share | 10 to 35% | a cost that is felt, not a second landlord |
| Lost to drops (careful) | 15% or less | a careful climb is not a lottery |
| Idle (earned, never spent or lost) | 25% or less | bits always have somewhere to go |
| Fair fight, kit level with the runner | 85% or better at every level | the fair fight is the bread, not the gamble |
| Bright glyph, same kit | 35 to 70%, levels 4 to 15 | a coin toss worth thinking about |
| Old Signal at no marks, dry | 30 to 50% | the gate is a boss |
| Old Signal with static on ice | 10 points over dry, 75% at most | a glass is worth saving, and does not hand it over |
| Crystals a day (keen) | 0.8 to 2.0 | a glass a day, a cart piece every few |

`sim_test.rs` holds the two pace windows and the lock (the neglectful
runner gains a level within a week of its first drop) on their own, so a
change to a curve fails fast and prints the whole ladder. It also holds
one seeded `KEEN` climb whole (`one_keen_climb_holds_still`): a change
that moves the climb inside every band still shows up there as a diff.
`arena_test.rs::the_live_rules_miss_no_target` holds every band.

## 2. The knobs: `data::Rules`

Every tunable is a field of `Rules` (`data.rs`); `RULES` is the live
game, built from the constants beside it. The machine takes the rules as
a value (`Sheet::apply_under`), so a candidate is tried by building a
`Rules`, never by editing a constant and recompiling.

| Field | Live | Moves |
|---|---|---|
| `pay_bits_percent` | 300 | bits per kill, over LoGD's creature table |
| `pay_exp_percent` | 300 | exp per kill: the pace of the levels |
| `price_percent` | 225 | the armorer's wall, over `COST_LADDER` |
| `trade_in_percent` | 75 | what the handed-back piece takes off |
| `patch_percent` | 50 | patch: missing signal times level times this, rounded up |
| `exp_keep_on_death` | 0.65 | what a drop costs in days |
| `crystal_drop_one_in` | 12 | crystals from plain kills |
| `bright_signal_percent`, `bright_edge_percent` | 135, 115 | how hard the bright glyph is |
| `bright_bits_times` | 2 | what it pays |
| `cart_crystals` | 3 | the blade shop's price |

Not in `Rules`, and why: the creature table, the exp ladder, and the
gear ladder are LoGD's curves (retune the percent over them first); the
rations a day and the signal a level are the shape of the day. To make
any other constant sweepable, add a field, read it from the rules where
the constant was read, and set it in `RULES`.

## 3. The instruments

**`sim.rs`: a player and a climb.** A `Player` is a way of playing: when
it runs, when it patches, when it steps down, when it starts heeding
(`Heeds`), whether it takes the bright glyph, drinks, shops for blades,
and how it holds its purse (`Purse::SpendAll`, or `Purse::KeepAPatch`:
never buy gear with the bits the next patch needs). `climb(player, seed,
max_days, &mut Bench)` plays it from a fresh row through the real
machine and returns a `Climb`: the day of every level, the kit each
level was fought in (`kit_on`), the gate, the mark, the drops, and a
`Ledger` of bits and crystals. A `Bench` is the rules plus a memo of the
picker's threat reads, shared across a batch (`Bench::live()`,
`Bench::under(rules)`).

| Player | Plays like |
|---|---|
| `CAREFUL` | runs under 40%, patches every point, keeps a patch in the purse |
| `RECKLESS` | runs under 15%, patches at half, spends it all |
| `NEGLECTFUL` | no armorer and no picker until the first drop, careful after |
| `KEEN` | careful, plus the bright glyph when it reads even, static on ice, the blade shop |

**`arena.rs` (test-only): four instruments, smallest to largest.**

- `matchup(rules, Recipe, Pick)`: one runner (a level, a `Kit` as a lead
  over the level, a glass, marks) against one pick, 400 fights to the
  end: odds, rounds, signal left, the patch after, the pay.
- `level_economy(rules, level)`: one level as arithmetic: the kills it
  takes, what they pay, what the next pair of pieces costs.
- `climbs(player, rules, seeds, max_days)`: `sim::climb` over the seeds.
- `Reading::take(rules)`: every number the targets are stated in, 80
  seeds a player. `Reading::misses()` lists the bands it is out of.

Everything is seeded: the same rules give the same reading.

## 4. Running a pass

1. `make deadchannel-arena` writes `late-ssh/target/deadchannel-arena.md`:
   the live reading with its misses, then every table (section 5).
   Start here to see where the game is.
2. Put the candidates in `arena_test.rs::SWEEP`, each a name and a
   `Rules { field: value, ..RULES }`, the live rules first.
3. `make deadchannel-sweep` writes `late-ssh/target/deadchannel-sweep.md`:
   one reading per candidate, side by side, a miss count, and the missed
   bands spelled out underneath. About ten seconds a candidate.
4. Move one knob at a time until the cause is clear, then sweep the
   neighbours of the pick: a setting whose neighbours miss is a knife
   edge and will not survive the next change.
5. Set the constants in `data.rs`. Run the targeted suite: the contract,
   `sim_test.rs`, and the tests that spell prices out (`state_test.rs`,
   `city/ui_test.rs`, `svc_test.rs`). Read each moved number before
   re-blessing it.
6. Update the guide (`guide/data.rs`) if a rule a player reads changed,
   and section 6 here with the new reading.

## 5. Reading the report

- **The reading.** The targets' numbers, one row. `misses` is 0 when
  the game is in every band.
- **What gear is worth.** The fair fight at every level by kit lead:
  bare hands, 4 to 1 tiers behind, level, 1 to 4 ahead. Read it for the
  cost of falling behind and the worth of running ahead.
- **Where a level's bits go.** Kills to clear the level, what they pay,
  the next pair's price against that pay (`pair / pay`: near 100% means
  a level buys its kit and little more), and the patch after a fair
  kill against the kill's pay.
- **The bright glyph and the step down.** The step down, the bright
  glyph a tier behind, level, a tier ahead, and with each glass.
- **The Old Signal.** Odds by marks, dry and with each glass.
- **Climbs.** Per player, the median day and kit of every level, then
  the mark, the drops, the boss tries, and the ledger.

## 6. Measured

The live reading:

| careful | reckless (drops) | keen | kit lead | top kit at | gear | patch | dropped | idle | fair low | bright | boss dry / glass | crystals a day |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| day 22 | day 29 (5) | day 20 | +0.0 to +0.0 | lv 15 | 57% | 23% | 3% | 16% | 94% | 42 to 60% | 40% / 58% | 1.2 |

What the tables say:

- **Gear is the fight.** From a kit level with the runner the fair fight
  is 94 to 100% with about two thirds of the signal left. A tier behind
  is 83 to 94%; two behind is 35% at level 3 and climbs to 90% by 15;
  bare hands are 3% or less past level 1. A tier ahead buys two or three
  points, so running ahead is worth little and falling behind early is
  worth a lot: prices, not a gate, are what keep the kit on the level.
- **A level pays for its kit.** At 225% the next pair costs 80 to 110%
  of what the level's kills pay from level 3 on. Levels 1 and 2 pay less
  than their pair (262%, 129%), but a tier behind there is still a fair
  fight to take, and both levels are gone inside the first day.
- **Patch is a growing tax.** The patch after a fair kill is 1% of the
  kill's pay at level 2 and 26% at 15.
- **The crystal pass is an edge.** The keen runner finds about 25
  crystals on the way up and spends nearly all, takes about 30 bright
  glyphs and puts down half, wears each tier a level early from the
  middle of the climb, and marks two days ahead.
- **The neglectful runner** sits at level 2 until its first drop (about
  day 30 at the median, because it runs from every fight it cannot win),
  then climbs a level a day. `sim_test.rs` measures the week after the
  drop, not the weeks before it.

Why 225% and 50%. With LoGD's prices as they stood (100%) and patch at a
bit a point, a level paid for two to four tiers: the careful runner
fought up to 4.5 tiers ahead, wore the top kit at level 11, and spent a
quarter of its bits on gear, a third on patch, and lost a third to
drops. The sweep, days to the first mark as careful / reckless, 40 seeds:

| prices \ patch | 100% | 75% | 60% | 50% | 40% |
|---|---|---|---|---|---|
| 100% | 20 / 26 | | | | |
| 150% | 19 / 29 | 20 / 25 | | 19 / 23 | |
| 175% | 26 / 29 | 23 / 22 | | 20 / 23 | |
| 200% | 40 / 30 | 23 / 28 | 22 / 28 | 21 / 25 | 21 / 35 |
| 225% | never / 33 | 29 / 39 | 27 / 35 | **23 / 30** | 23 / 31 |
| 250% | | | 28 / 31 | 26 / 29 | 26 / 29 |
| 275% | | | 46 / 34 | 34 / 32 | 28 / 28 |

- **Price alone is a knife edge.** With patch at a bit a point, raising
  prices walks off a cliff between 175% and 200%: a runner a tier behind
  takes more damage, the patch eats the bits the next tier needed, and
  the gap widens (patch's share reaches 64% and the climb never ends).
- **Halving patch flattens it.** At 50% the climb degrades by a day or
  two per 25 points of price instead of falling over, and gear becomes
  the main sink. 225% is the lowest price at which the kit sits exactly
  on the level all the way up and the top kit waits for level 15; 200%
  leaves the kit up to a tier ahead with the top kit at 14, 250% puts it
  up to a tier behind.
- **What did not move anything.** The cart at 4, 5, or 6 crystals reads
  the same as at 3. Crystals at one kill in 16 or 20 cost the keen
  runner a day or two. The drop keeping 70% of the exp instead of 65%
  moves the reckless runner one day.

## 7. Soft spots and the tests worth adding

Known, measured, and left for a pass of their own:

- **The first day runs fast.** Levels 1 to 3 fall on day one for every
  player. The exp ladder's first rungs are LoGD's; a `Rules` field for
  the early rungs would let a sweep slow them.
- **The blade shop's worth grows with the tier.** Three crystals buy 108 bits
  of gear at tier 1 and over 20,000 at tier 15. `KEEN` buys as soon as
  it holds three; a hoarder who saves every crystal for the last tiers
  is the player to add before trusting its price.
- **The purse rule is the careful runner's edge.** `Purse::KeepAPatch`
  is most of why careful beats reckless at high prices. A player between
  the two (spends all, patches every point) would say how much.
- **The Old Signal with marks.** The report prints the odds by marks,
  but no climb runs a second season: `exp_to_advance` scaling and the
  mark bonus are untested as a pace.
- **The step down and the loan** are in no player's rule except the
  neglectful one's recovery. A runner who borrows for a tier is the test
  of `LOAN_PER_LEVEL` (50 a level, small against the 225% wall).
