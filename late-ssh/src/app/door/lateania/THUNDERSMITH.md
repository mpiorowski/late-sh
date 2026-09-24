# The Thundersmith, a class design

Status: design only, not implemented. Every engine claim below reflects the
landed world, including the world resist/weak pass and the weapon coats (spec:
`CONTEXT.md`, "The world resist/weak pass" and "Crafting depth"). The benchmark
class is the Ranger (not the Rogue), measured at the Lv30-60 band where the game
is actually played, not at L100.

*A bulky master smith with a storm-cell scattergun. He is not stronger than you.
He is prepared, and that's worse.*

---

## 1. Identity

Iron Man by way of a dwarven forge. Gloamwright glasscraft (the glass-and-obsidian
artificers of Kaelmyr's black deserts) fused with Stormheld storm-spire tech; both
peoples already exist in Kaelmyr's lore, so the craft has an in-world origin.

Doctrine: efficiency and smartness. He wins fights before they start, at the forge
and in his field notes, not in the exchange of blows. Lightning is his brand; the
crack and ozone of storm-cells is the sound of the class.

**The contract.** The class is the widest gap in the game between prepared and
unprepared. Fueled and informed he is the strongest class, full stop: +15-20%
over the Ranger, the current verified #1. Dry he is a tanky, honest, bottom-third
brawler. The spread is the identity, and the cost of the top end recurs per fight;
it is never a one-time unlock. If preparation can be paid once and forgotten, the
design has failed.

Not locked. The class is available at class select like any other. Access is free,
power is gated: being good costs Smithing ~50, materials, and a survived first
walk of every land he wants in the ledger. An unlock gate on top adds
annoyance, not depth.

## 2. Frame

| field | proposal | note |
|---|---|---|
| Resource | **Charge** (new `Resource` variant) | one enum arm + label |
| Primary score | Intelligence | smartness doctrine |
| max_hp | `44 + l*10` | bulky: between Berserker/Valewalker (42+10) and Paladin (46+11) |
| attack | `5 + l*2` | caster tier; the gun's damage comes from cells (multiplicative, see §3) |
| max_resource | `60 + l*4` | |
| resource_regen | `8` | mid: below Rogue 12/Monk 11, above Mage 7 |

Dry, this frame reads as a Warrior's bulk with a caster's damage: sturdy,
unremarkable, honest. Everything above mid-tier flows through cells.

## 3. The gun

His auto-attack is a scattergun shot whose damage school and power are decided by
the loaded cell. This is the genuinely new mechanic, verified against the engine:
every other auto-attack in the game is hardcoded Physical (the combat round calls
`profile.apply(atk, DamageType::Physical)`; the pet bite too), so he is the only
class whose basic attack can bypass a physical resist or land on a weakness.

**Cells are multipliers on `attack()`, never flat adders.** Verified: ability
magnitudes are flat table constants with no level, gear, or score term, which is
why every class's kit decays into irrelevance as gear scales (endgame gear alone
is ~+523 attack). A flat "+N per shot" cell would decay on the same curve and the
whole crafting gate would buy a rounding error by the Frontier. A multiplier rides
the gear curve instead, so tier-5 cells matter as much at the top as in the band.

| ammo | school | multiplier | supply |
|---|---|---|---|
| Scrap shot | Physical | **x0.90** | unlimited, free; the never-empty floor |
| Storm-cells t1-t5 (signature) | Lightning | x1.10 / x1.15 / x1.20 / x1.27 / **x1.35** | crafted |
| Counter-cells (ember / rime / blessed / gloom / rune / venom) | one line per remaining school | one notch below the same tier's storm | crafted |

- Cells are consumed per shot. Combat runs one auto per 2s tick, so a **60-shot
  cell is ~2 minutes of continuous fighting**, and a ~4-cell **bandolier** (cells
  craft in batches) is ~8 minutes of fueled combat. Cost is tuned as a rate, not
  a price (see §10): a cell three times longer costs three times the materials,
  so the gold-per-fueled-hour bill is unchanged and only the rhythm moves.
- **The unit of recurrence is the expedition, not the fight.** That is what §4
  already describes - provision at the forge, march, execute - and it is what
  keeps §1's contract intact while the cell is long. Cells were drafted at 20
  shots, which made re-chambering a chore that read as upkeep rather than
  preparation; the bill is identical and the feel is the loop the design wants.
  What must never happen is the bandolier surviving the expedition that bought
  it: if a single craft session fuels a night, the recurring cost is gone and
  with it the class.
- Tier-5 cells require Smithing ~50 and masterwork-grade materials. This is the
  power ceiling and the crafting system's endgame consumer.
- Scrap keeps the dry state playable but honest, and it stings twice: x0.90 on
  the multiplier, and Physical is the only school the world never rewards.
  Post-pass census (test-enforced, no drift possible): nothing anywhere is
  weak to Physical, no regular anywhere resists it, and Physical resist
  lives on exactly 14 bosses (the Elder Treant - the game's first crown -
  the Fallen Paladin, and Aelunor's twelve). So scrap is safe on trash but
  worst-in-class everywhere: it misses the weakness every one of the 126
  themed zones now carries, and it halves against the walls that matter.
- Why lightning as the brand holds up, post-pass: Lightning is the widest
  weak lane in the world (25 of 126 themed zones - the coasts, the lakes,
  the glass countries) against 6 Storm-zone resists. The brand school is
  also the best default cell, which keeps storm affinity honest.
- **The rack covers all seven non-Physical schools, and that is the point.**
  The coat family covers five - the four oils (fire/frost/holy/lightning) plus
  the poison vial, now one curve and one price, differing only by school - which
  leaves **Shadow and Arcane** - 22 and 17 of the 126 themed zones, and every
  foe in them - with no martial answer but his. His counter-lines are also
  multipliers where coats are flat riders, and they convert the auto's school
  where coats only ride alongside it. Together that is the capability gap that
  makes "everyone plays matchups, he industrializes them" literal rather than
  rhetorical. **Coats got cheaper to fly, not broader**: the merge and the `C`
  key moved convenience, never coverage, and these two lanes are the fence.

## 4. The loop (provision, march, execute)

A correction from the landed world: **information is not the scarce good.**
The battle panel reads a targeted foe's school/weak/resist out loud for
everyone (`MobView.weak/resist`, both layouts), and every regular in a zone
shares the zone theme. The original "no free information" probe dance is
dead as designed. What stays scarce is the *answer*: having the right line
crafted, carried, and chambered. The loop is provisioning, not spying.

1. **Read the land.** The first engage in a new zone writes it into the
   ledger. Mid-fight the traits line tells anyone; only the ledger remembers
   it at the forge, forever. Walking a land once is still a real cost - a
   survived trip, not a wiki lookup.
2. **Provision.** Crafting new cells requires a craft station (forge in
   Embergate), so expeditions are provisioned in advance. The ledger is the
   shopping list: bandolier composition against the route ahead is the skill
   expression. Swapping loaded cells from the carried bandolier stays an
   out-of-combat action, doable at the boss door.
3. **Fighting retreat.** Signature utility ability: a concussion blast that
   stuns and withdraws cleanly (unlike flee's uncontrolled first-exit).
   Dealt damage persists (shared-world HP), so the return pass faces a
   dented foe. This is now chiefly the tool for **authored crowns**: since
   the boss-weakness pass, a generated zone boss wears its zone's weakness
   (the ledger already knows it), but the hand-authored crowns keep bespoke
   profiles and hit hardest, so the first pull on one is still a read paid
   in HP.
4. **Execute.** Auto-chamber (below), shred.

The loop self-balances: trash dies to anything, so provisioning discipline
only pays where fights matter - and it is paid per bandolier, never once.

## 5. The Ledger

Field notes, **keyed by zone, not by species** - and since the world pass
this is the enforced data model, not a design bet: every generated zone's
regulars share one `ZoneTheme` profile, guaranteed by
`every_generated_zone_spawn_wears_its_zone_theme`. A zone key therefore
captures every regular in the land, and since the boss-weakness pass it
captures the zone boss's weakness too (same test: a zone boss wears the
theme's weak and no resist). The **authored crowns** are the deliberate
deviation and are **not** ledger rows: a crown is read the hard way, every
character, every time.

Given that the battle panel already reads a targeted foe's profile aloud,
what the ledger buys is **distance**: mid-fight everyone knows; only he
knows at the forge, before the march, with zero keypresses at the door. The
ledger is §4's provisioning input and the auto-chamber key, not secret
knowledge. Persisted as a small set of zone keys - still the only schema
bump.

**Auto-chamber:** on engage, if the zone is in the ledger and the counter-school
is carried, it loads itself with one log line ("You know this plating. Ember
rounds chamber with a click."). Priority: known weakness if carried > storm
(affinity) > whatever is loaded > scrap. Zero keypresses; knowledge does the
work.

**This is no longer a convenience advantage, and the design must not lean on it
as one.** The `C` key (`svc::coat_best`) gives every class a one-keystroke coat
picked by the same logic - the foe's weakness first, then a school it does not
resist, then tier. Auto-chamber is now *faster*, not *unique*, and speed alone
is not an identity. What stays his is the shape of the commitment:

| | coated martial | Thundersmith |
|---|---|---|
| when it is chosen | once, then locked | **per engage** |
| cost of switching school | up to 39 wasted strikes | nothing but the shot |
| what it rides | a flat rider beside a Physical auto | the auto's own school and a multiplier |

A `COAT_CHARGES`-long coat makes that commitment *more* binding than the old
12-strike one did: a martial now picks a school at the zone gate and lives with
it for the run, which is exactly the trade coats should make. He answers each
foe. **Flexibility, not keystrokes, is the gap to protect** - so if auto-chamber
is ever cut for scope, the class loses polish; if per-engage re-chambering is
cut, the class loses its point.

## 6. Traits and systems

- **Scrapwright** (the passive class trait): mob kills refund one shell of the
  loaded cell type. A short trash fight spends 4-6 shells, so this is roughly a
  20% rebate on clean play. It fires on mob kills only, never in pvp (see §8).
- **Storm affinity:** the storm line runs one multiplier notch above every
  counter-cell line. Keeps storm the default answer and the brand.
- **Doorway advantage:** the foe's opening strike is denied on engage. Priced
  deliberately from verified mob damage: one denied hit is worth 50-95 damage
  per pull in the Lv30-60 band (overworld mobs land 25-75 per hit, x1.25 after
  dark) and 110-290 in the Frontier and beyond. That is roughly one tick of
  effective HP on *every single pull*, strictly better than Opportunist in a
  grind. It is the defensive half of the kit's budget alongside the bulky frame;
  if the class lands hot, this is the first lever to pull, not the cells.
- **Capacitor plating:** while charged, a small shield trickle between hits
  (existing `shield` field, no new machinery).
- **Overclock:** spend Charge to empower the next few rounds. Note: empower
  feeds `attack()` and rides the cell multiplier, which is correct and intended;
  the interaction is priced into the +15-20% ceiling.
- **Capstone (L100):** a rail-lance that dumps the entire remaining battery into
  one discharge, damage scaling with shells left.

## 7. Archetypes (level 10, two paths)

The engine forces the choice (`archetype_choices` gates the screen at
`ARCHETYPE_LEVEL`), so the class needs exactly two:

- `dps("siegesmith", "Siegesmith")`: "Every shot a breach. Your cells discharge
  far harder." (standard dps template: +18% attack)
- `tank("aegiswright", "Aegiswright")`: "Plate the rig and hold the doorway;
  what lands is turned aside." (standard tank template: +22% mitigation, +12% max HP)

The cell multiplier stacks multiplicatively with the archetype percent, same as
gear does. Priced into §9's targets.

## 8. PvP: the armor-breaker

Verified engine fact: `strike_player` reduces incoming damage by `armor/2`
against Physical but only `armor/4` against every other school. A chambered cell
therefore halves the armor term against plate-stacking duelists.

This is embraced, not patched: **the Thundersmith is the designated counter-pick
to armor.** Tank archetypes and masterwork-plate stackers are his prey. The
pricing that keeps it a matchup rather than dominance:

- Cells burn in pvp with **no Scrapwright rebate** (the refund is mob-kill only),
  so dueling is the most expensive thing he does. Winning a duel on tier-5 cells
  should feel like spending real money to make a point.
- Doorway advantage does not fire in pvp; players are not mobs to be ambushed at
  a threshold.
- No pvp-only bonus anywhere. His whole edge is the armor formula itself, which
  means glass casters (who never stacked armor; their `armor/4` was already
  nothing) fight him even or better. Rock-paper-scissors, not a throne.

## 9. Benchmark: the Ranger (the bar to clear)

The Ranger is the verified #1, not the Rogue. Hunter's Instinct is the only trait
that multiplies both auto-attacks *and* abilities (+25% below half health, ~x1.125
averaged over a fight), on the top martial attack tier (`6 + l*2`), with regen 9
and a dps/tank archetype choice. The Rogue's Opportunist doubles one auto per
engage: ~+33% on a three-tick trash pull, ~3% on a boss. Rogue wins short fights,
Ranger wins everything else.

Measured at the band (Lv45, realistic gear ~+100 attack, mid pet ~76/tick;
sustained damage per tick over 30 ticks, dps archetype, sim verified against the
combat round's actual hooks). Broceliande drops Frontier-tier gear (its loot
borrows Frontier tiers 0-9), so this gear level is genuinely reachable in-band:

| build | dmg/tick | note |
|---|---|---|
| Ranger + pet (the bar) | 382 | |
| Rogue + pet | 365 | |
| Mage + pet | 344 | |
| bottom three + pet (Paladin/Cleric/Druid) | 272-306 | |

A maxed pet is ~247/tick at cap (Aurora Worldserpent, pet level 10, all
auto-skills; recomputed from `pets.rs`/`taming.rs`, replacing the earlier 281
estimate), and any class can hold one, so the pet cancels out of every
comparison.

### Thundersmith targets (Lv45 anchor)

| state | target dmg/tick | vs Ranger |
|---|---|---|
| Tier-5, known weakness chambered | **440-460** | **+15-20%** |
| Tier-5, neutral foe | 415-430 | +9-12% |
| Tier 2-3 cells (grind fuel) | 355-375 | Rogue/Mage tier |
| Scrap shot (dry) | 280-300 | bottom three |

The spread is the design contract: unambiguous #1 while fueled and informed,
Druid's neighborhood while dry.

Two post-pass caveats on this table, both re-verified at ship time:

- **The bar is the oiled Ranger, and the oil no longer lapses.** The 382
  figure predates the coats; a matched one is worth ~+6.5%, so in exactly the
  zones where prepared players live the bar is ~405. That was once an average
  over a coat that ran dry and got forgotten; with `COAT_CHARGES` strikes and a
  one-keystroke re-application it is the **sustained** state for anyone who
  bothers. Measure against ~405 always, never against 382, or the stated gaps
  silently shrink by a third.
- **The top row's uptime is now standard, not a spike.** Every zone carries
  a weakness and auto-chamber loads it in known land, so "known weakness
  chambered" is his *sustained* state wherever the ledger reaches. Judge
  the +15-20% ceiling as an hourly rate, not a lucky matchup.

Re-verify the same table at Lv45 and Lv55 before shipping; the multipliers
are level-independent by construction, so drift there means a frame or
roster bug, not a cell bug. If the fueled number lands above +25%, tighten
the tier-5 multiplier or shots-per-cell, not the frame.

## 10. Costs, as rates

Every cost is a recurring rate, never a one-time gate. Smithing ~50 is the
*access* grind; cells are the *power* bill.

| gate | what it costs |
|---|---|
| Smithing ~50 + masterwork materials | access to tier-5 cells, the ceiling |
| Tier 2-3 upkeep | target ~10-15 min of gathering/smithing per hour of fueled combat - **the dial that must not move**, whatever shots-per-cell is set to |
| Tier-5 upkeep | steep by design; rationed for bosses, not for grinding |
| The first walk, per zone | survive reaching each land once to ledger it |
| PvP | pure burn, no Scrapwright rebate |

Scrapwright turns skill into margin: clean trash play runs a ~20% shell rebate,
sloppy play pays list price. Waste is the tax on ignorance, efficiency the
dividend on knowledge.

## 11. Engine cost map

Cheap where it counts, reusing existing patterns (all verified present):

- Loaded ammo: its own transient field in the `weapon_coat` mold
  (`Some((school, mult_pct, shots))`), separate from `weapon_coat` itself -
  but **the gun never holds a coat**: using any coat as a Thundersmith is
  refused ("the shardgun's heat would cook it off"). Stacking a matched rider
  on top of a matched cell shot was too much buff on the one class that needs
  none; coats stay the mundane martial's lever, cells stay his, and the two
  never combine. No save state for the chambered cell; bandolier contents are
  inventory items.
  - **Price this exclusion at what coats are now worth, not what they were.**
    He is the only class that cannot press `C`, and what he declines has grown:
    a `COAT_CHARGES`-strike auto-picked rider, not the twelve-strike one dug out
    of a panel that this design was drafted against. That is the argument for a
    generous shots-per-cell (§3) and against ever trimming the cell ladder to
    close a gap - trim the multiplier or doorway advantage instead.
- Cell application: the two auto-attack call sites (the combat round's mob strike
  and the pvp strike) swap `DamageType::Physical` for the chambered school and
  scale `attack()` by the cell's percent. Two call sites, one helper.
- Probe reveal: existing `Defense`/`defense_tag` machinery.
- Auto-chamber: a hook in `engage`.
- Doorway advantage: skip the mob's first strike after engage (flag like
  `opening_strike`), pve only.
- Capacitor plating: existing `shield` field, topped in the upkeep loop.
- The Ledger: one persisted set of zone keys (the only schema bump).
- Standard new-class wiring: an arm in every `match self` in `classes.rs`
  (name / primary_score / resource / tagline / description / trait_name /
  trait_desc / stats_at / as_key / from_key), entry in `ALL`, a 10-ability
  roster in `abilities.rs` (ids 2200+), the two archetypes, trait hooks in
  `svc.rs` (engage, combat round, kill_mob for Scrapwright, strike_player for
  plating), cell items + recipes in `items.rs`/`crafting.rs`.

## 12. Open tuning knobs

- Shots per cell (~60) x cells per bandolier craft (~4): together they set the
  minutes-of-prep-per-fueled-hour rate, the single most important dial. Move
  them against each other, or against per-cell material cost, so §10's rate
  holds; moving shots-per-cell alone is a straight buff or nerf wearing a
  convenience costume.
- The tier multiplier ladder (x1.10 to x1.35 proposed) and the counter-cell
  notch-below rule.
- Doorway advantage: one denied strike, or two vs non-boss. First nerf lever.
- Scrapwright refund rate (1 shell per mob kill proposed, ~20% rebate).
- Whether ledger entries are account-wide or per character (per character
  proposed; the ledger is the character's story).
- Whether the counter-rack ships all six lines at once, or the
  Shadow/Arcane lines (the two lanes no coat covers at all) unlock deeper
  in the Smithing climb - staging his exclusivity as a late reward.
- Fueled uptime: with every zone weak to something, the top state is the
  default in read land. If the sim says that runs hot, shots-per-cell is
  the dial that prices uptime without touching the per-shot feel.

## 13. The world resist/weak pass (landed)

The pass shipped together with the weapon coats, which were promoted from
follow-up once it was clear placement alone left the seven Physical-locked
classes a mathematically zero matchup game. **The
spec lives in `CONTEXT.md`, section "The world resist/weak pass"**: theme
vocabulary and tables, the two Physical rules, census bands, the routed
grind-rate budget, and the tests that are the contract. Its consequences for
this class are folded into the sections above (the census and the
seven-school rack in §3, the provisioning loop in §4, the guaranteed ledger
model in §5, the oiled bar and uptime caveats in §9, the coat exclusion in
§11, the new knobs in §12). What remains here:

### What the class inherits

- **The monopoly is fenced in code, and the fence runs both ways.** Coats
  are flat riders, never a conversion of the auto's school, never a
  multiplier on `attack()`; both levers remain reserved for the cells. In
  exchange he is the one class that cannot coat a weapon at all (§11).
  Everyone else plays matchups now; his edge stays legible as degree, not
  kind.
- **The coat family merged, and it cost him convenience, not coverage.**
  Poison and the four oils are now one curve, one charge count and one price,
  separated only by school, and `C` applies the best-matched one in a
  keystroke. Nothing in that touched the two fences above, and no school was
  added, so Shadow and Arcane remain his alone. What it did take is the
  *feel* of being the prepared one: every class now arrives at a zone gate
  with the right school on its weapon. §5 answers that - his edge is
  per-engage re-chambering against their locked-in choice - and §3's longer
  cells answer the rest, by making the price he pays for the coat exclusion
  proportional to what coats became.
- **Every boss carries a weakness** (`every_boss_carries_a_weakness`):
  zone bosses inherit their zone's weak lane, authored crowns carry
  bespoke ones. For him that means the rack always has a boss answer -
  there is no fight in the game where a chambered counter-cell reads
  "nothing", and the Shadow and Arcane boss lanes are answerable by casters
  and him alone (the coat rack reaches the other five schools, not those).
- **Boss walls are his signature moment by contract - but never a group
  tax.** Physical resist exists on exactly 14 bosses and zero regulars,
  everywhere, test-enforced, and every one of those bosses guards an
  optional prize or sits at the low band where a tier-0 oil already answers
  the fight (the solo rule in CONTEXT.md, pinned by
  `physical_walls_never_gate_the_long_road_past_the_treant`). An oiled
  martial always clears the wall - slower, roughly two-thirds pace - so
  the Thundersmith's edge here is comfort and speed, not access. Do not
  "improve" his signature by adding Physical resists to road crowns.

### Verify at ship time

- Re-run the §9 table at Lv45 and Lv55 with both §9 caveats applied (the
  oiled-Ranger bar, top-row uptime as the sustained state). Levers if hot:
  tier-5 multiplier, shots-per-cell, doorway advantage - never the frame.
- Extend the routed world sim in `world_test.rs`, don't exempt him: add a
  Thundersmith row where "routed" means fueled-and-informed, asserting the
  prepared band inside the §9 targets, the dry state in the bottom third,
  and no path to the top row without per-fight spend. The contract - OP
  only if prepared - becomes an assertion that fails the build.
