# The Thundersmith, a class design

Status: design only, not implemented. Drafted 2026-08-06; revised 2026-08-20 after
a code-verified balance pass. Every engine claim below was re-checked against
source; the benchmark class is now the Ranger (not the Rogue), measured at the
Lv30-60 band where the game is actually played, not at L100.

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
power is gated: being good costs Smithing ~50, materials, and a survived recon
fight per zone. An unlock gate on top adds annoyance, not depth.

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
| Counter-cells (ember / frost / holy / ...) | one line per school | one notch below the same tier's storm | crafted |

- Cells are consumed per shot. Combat runs one auto per 2s tick, so a 20-shot
  cell is ~40 seconds of fighting; cells therefore craft in **bandoliers**
  (batches), and cost is tuned as a rate, not a price (see §9).
- Tier-5 cells require Smithing ~50 and masterwork-grade materials. This is the
  power ceiling and the crafting system's endgame consumer.
- Scrap keeps the dry state playable but honest, and it stings twice: x0.90 on
  the multiplier, and Physical is the worst-expectation school in the game.
  Corrected census (116 authored profiles): Physical is 4th by resist count
  (Frost 23, Shadow 19, Fire 18, Physical 15) but **nothing in the world is weak
  to it** (0 of 116), giving it the near-worst expected multiplier (0.934).
  Notably, every Aelunor boss resists Physical: an entire continent already
  punishes the unprepared shot.
- Why lightning as the brand holds up: resisted by 2 profiles, a weakness on 9,
  expected multiplier 1.031, behind only Holy (1.140) and Fire (1.057). Nothing
  resists Arcane (0 of 116), but nothing meaningfully seeks it either (weak on 3).

## 4. The loop (recon fights)

No free information. A mob's resist/weak profile is never shown up front (true for
everyone; the engine only reveals it via the post-hit `defense_tag` log line).
The Thundersmith's edge is that he can *act* on what he learns.

1. **Probe.** Each school fired reveals its result (weak / resisted / neutral),
   reusing the existing tag machinery. Every probe round is a real combat round;
   recon costs HP and shells. Sharp players binary-search schools; sloppy ones
   empty a bandolier.
2. **Fighting retreat.** Signature utility ability: a concussion blast that stuns
   and withdraws cleanly (unlike flee's uncontrolled first-exit). Probe damage
   persists on the mob (shared-world HP), so the return pass faces a dented foe.
3. **Re-arm.** Swapping loaded cells from the carried bandolier is an
   out-of-combat action, doable at the boss door. Crafting *new* cells requires a
   craft station (forge in Embergate), so expeditions are provisioned in advance.
4. **Execute.** Return, auto-chamber (below), shred.

The loop self-balances: trash dies to anything, so the recon dance only activates
on fights that matter.

## 5. The Ledger

Field notes, **keyed by zone, not by species.** Verified data model: resist/weak
is per-zone in practice. Every generated region's regulars carry no profile at
all (Frontier, Reaches, Kaelmyr, archipelago, lakes, Broceliande, Aelunor's 100
creatures), the three dungeons and the three Wildbound biomes carry exactly one
profile per zone, and only ~116 authored spawns vary individually. A per-species
ledger over 426 regular foes would be almost entirely duplicate rows (the same
clutter wall that got per-species titles removed); a zone-keyed ledger makes the
first probe in a place a real "you've read this land" moment and shrinks the
schema bump to a small persisted set of zone keys.

**Auto-chamber (QoL):** on engage, if the zone is in the ledger and the
counter-school is carried, it loads itself with one log line ("You know this
plating. Ember rounds chamber with a click."). Priority: known weakness if
carried > storm (affinity) > whatever is loaded > scrap. Zero keypresses;
knowledge does the work.

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
Druid's neighborhood while dry. Re-verify the same table at Lv55 before shipping;
the multipliers are level-independent by construction, so drift there means a
frame or roster bug, not a cell bug. If the fueled number lands above +25%,
tighten the tier-5 multiplier or shots-per-cell, not the frame.

## 10. Costs, as rates

Every cost is a recurring rate, never a one-time gate. Smithing ~50 is the
*access* grind; cells are the *power* bill.

| gate | what it costs |
|---|---|
| Smithing ~50 + masterwork materials | access to tier-5 cells, the ceiling |
| Tier 2-3 upkeep | target ~10-15 min of gathering/smithing per hour of fueled combat |
| Tier-5 upkeep | steep by design; rationed for bosses, not for grinding |
| The recon fight, per zone | survive learning each land the hard way |
| PvP | pure burn, no Scrapwright rebate |

Scrapwright turns skill into margin: clean trash play runs a ~20% shell rebate,
sloppy play pays list price. Waste is the tax on ignorance, efficiency the
dividend on knowledge.

## 11. Engine cost map

Cheap where it counts, reusing existing patterns (all verified present):

- Loaded ammo: the `weapon_poison` transient pattern (`Some((school, mult_pct, shots)`-shaped),
  no save state for the chambered cell; bandolier contents are inventory items.
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

- Shots per cell (~20) x cells per bandolier craft (~5): together they set the
  minutes-of-prep-per-fueled-hour rate, the single most important dial.
- The tier multiplier ladder (x1.10 to x1.35 proposed) and the counter-cell
  notch-below rule.
- Doorway advantage: one denied strike, or two vs non-boss. First nerf lever.
- Scrapwright refund rate (1 shell per mob kill proposed, ~20% rebate).
- Whether probe results are account-wide or per character (per character
  proposed; the ledger is the character's story).

## 13. The world resist/weak pass (landed)

Formerly a full handoff brief; the pass shipped 2026-08-20, together with the
weapon oils (promoted from follow-up once it was clear placement alone left
the seven Physical-locked classes a mathematically zero matchup game). **The
spec now lives in `CONTEXT.md`, section "The world resist/weak pass"**: theme
vocabulary and tables, the two Physical rules, census bands, the routed
grind-rate budget, and the tests that are the contract. This section keeps
only what it means for this class.

### What the Thundersmith inherits

- **Real ledger data everywhere.** 126 themed zones; one profile per zone for
  regulars is now test-enforced, so the zone-keyed Ledger (§5) can never be
  wrong about a regular. Bosses keep authored profiles, so the ledger
  rightly does not cover them: the §4 recon dance survives exclusively for
  bosses, exactly where it should activate.
- **The monopoly is fenced in code.** Oils are flat riders, never a
  conversion of the auto's school, never a multiplier on `attack()`; both
  levers remain reserved for the cells. Everyone else plays matchups now
  (casters: rotations; martials: oils and geography); his edge stays
  legible as degree, not kind.
- **Boss walls are his signature moment by contract.** Physical resist now
  exists only on bosses, everywhere - the one wall the gun uniquely walks
  through.
- **The seven-cell rack is his exclusive reach** (proposed): oils cover four
  schools deliberately; Shadow/Arcane/Poison zone weaknesses (22/18/13
  zones) have no martial answer. Cells covering all seven schools makes
  "he industrializes matchups" a capability gap, not just a number gap.

### Re-price at ship time

The pass moved numbers this doc still states pre-pass:

- **Top-state uptime.** §9's "known weakness chambered" (+15-20%) was
  priced against a mostly-neutral world; now every zone has a weakness and
  auto-chamber makes it the default in known land. Re-run the sim; if the
  fueled edge lands high, the doc's own levers apply (tier-5 multiplier or
  shots-per-cell, never the frame).
- **The bar moved.** The Ranger benchmark (382/tick) predates oils; an
  oiled Ranger in a matched zone is ~+6.5%. Re-anchor §9 against the oiled
  Ranger or the stated gap silently shrinks by a third.
- **§3's census paragraph is stale** (Physical 4th by resist count at
  15/116): the world is now ~5,000 profiled regulars and boss-only Physical
  resists. The "every Aelunor boss resists Physical" line still holds.
- **§11's cost map** says the loaded cell reuses the `weapon_poison`
  pattern; that field is now the one-slot `weapon_coat`, and the cell must
  be a *separate* transient - a coat legitimately rides on top of a cell
  shot (flat rider + multiplied shot, each in its lane).
- **Extend the routed sim, don't exempt him.** Add a Thundersmith row where
  "routed" means fueled-and-informed and assert his band explicitly:
  prepared edge inside the §9 targets, dry state pinned to the bottom
  third, no state that reaches the top row without per-fight spend. The
  contract - OP only if prepared - becomes an assertion that fails the
  build, not a sentence in this doc.
