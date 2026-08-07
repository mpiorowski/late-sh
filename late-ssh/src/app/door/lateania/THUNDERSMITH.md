# The Thundersmith, a class design

Status: design only, not implemented. Drafted 2026-08-06 from a balance study of
the seventeen live classes. Numbers below reference the code as of that date
(post Warrior regen 6 -> 9, post Wildbound mount ladder fix).

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

Not locked. The class is available at class select like any other. The decision is
deliberate: access is free, power is gated. Being *good* costs Smithing ~50,
masterwork materials, and a survived recon fight per species (see the loop below).
An unlock gate on top adds annoyance, not depth.

## 2. Frame

| field | proposal | note |
|---|---|---|
| Resource | **Charge** (new `Resource` variant) | one enum arm + label |
| Primary score | Intelligence | smartness doctrine |
| max_hp | `44 + l*10` | bulky: between Berserker/Valewalker (42+10) and Paladin (46+11) |
| attack | `5 + l*2` | caster tier; the gun's damage comes from cells, not the stat |
| max_resource | `60 + l*4` | |
| resource_regen | `8` | mid: below Rogue 12/Monk 11, above Mage 7 |

## 3. The gun

His auto-attack is a scattergun shot whose damage school is decided by the loaded
cell. This is the genuinely new mechanic: every other auto-attack in the game is
hardcoded Physical (the combat round calls `profile.apply(atk, DamageType::Physical)`),
so he is the only class whose basic attack can bypass a physical resist or land on
a weakness.

| ammo | school | supply | strength |
|---|---|---|---|
| Scrap shot | Physical | **unlimited**, free | weakest; the never-empty floor |
| Storm-cells (signature) | Lightning | crafted, tiers 1-5 | his best vs any neutral foe (affinity bonus) |
| Counter-cells (ember / frost / obsidian / ...) | one line per school | crafted | out-damage storm only against that school's weakness |

- Cells are consumed per shot; one cell is a batch of ~20 shots (tune later).
- Tier 5 cells require Smithing ~50 and masterwork-grade materials. This is the
  power ceiling and the crafting system's endgame consumer.
- Scrap shot keeps the dry state playable but honest: Physical is the most
  resisted school in the game, so leaning on free ammo hits walls exactly where
  the class is supposed to think.

## 4. The loop (recon fights)

No free information. A mob's resist/weak profile is never shown up front (true for
everyone; the engine only reveals it via the post-hit `defense_tag` log line).
The Thundersmith's edge is that he can *act* on what he learns.

1. **Probe.** Each school fired reveals its result for that species (weak /
   resisted / neutral), reusing the existing tag machinery. Every probe round is
   a real combat round; recon costs HP and shells. Sharp players binary-search
   schools; sloppy ones empty a bandolier.
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

Per-species field notes, persisted like `visited` (a set of mob names; the one
schema bump in the design). What a probe reveals is recorded forever, per
character. First fight against each species is always the expensive recon fight;
every later meeting starts solved.

**Auto-chamber (QoL):** on engage, if the species is in the ledger and the
counter-school is carried, it loads itself with one log line ("You know this
plating. Ember rounds chamber with a click."). Priority: known weakness if
carried > storm (affinity) > whatever is loaded > scrap. Zero keypresses;
knowledge does the work.

## 6. Traits and systems

- **Scrapwright** (the passive class trait): kills refund a shell of the loaded
  cell type. Mastery lowers the upkeep cost of being OP; waste is the tax on
  ignorance, efficiency the dividend on knowledge.
- **Storm affinity:** lightning-school shots get a percent bonus (the shape of
  Arcane Mastery, narrowed to one school). Keeps storm the default answer and
  the brand.
- **Doorway advantage:** the foe's opening strike is denied on engage. Range,
  converted into the only currency a room-based engine has: a round where you
  hit and they do not. (Inverse of the Rogue's Opportunist.)
- **Capacitor plating:** while charged, a small shield trickle between hits
  (existing `shield` field, no new machinery).
- **Overclock:** spend Charge to empower the next few rounds.
- **Capstone (L100):** a rail-lance that dumps the entire remaining battery into
  one discharge, damage scaling with shells left.

## 7. Archetypes (level 10, two paths)

The engine forces the choice (`archetype_choices` gates the screen at
`ARCHETYPE_LEVEL`), so the class needs exactly two:

- `dps("siegesmith", "Siegesmith")`: "Every shot a breach. Your cells discharge
  far harder." (standard dps template: +18% attack)
- `tank("aegiswright", "Aegiswright")`: "Plate the rig and hold the doorway;
  what lands is turned aside." (standard tank template: +22% mitigation, +12% max HP)

## 8. Benchmark: the Rogue (the bar to clear)

Why the Rogue is the current best, so the targets below stay honest. The game's
top spot is decided by three levers: resource regen (funds ability uptime),
off-sheet damage (pet, opener), and multiplicative traits.

| lever | Rogue value |
|---|---|
| resource_regen | 12 (best; funds ~86% uptime on a 14-cost/tick rotation) |
| Opportunist | opening auto doubled; ~+48/tick at L100 amortized over a 5-round fight |
| best sustained Strike | L90, mag 153, cost 28, cd 2 |
| attack tier | `6 + l*2` (top martial tier, 204 at L100) |
| max_hp | `34 + l*8` (426 at L50, 826 at L100) |

Measured damage per tick (model: auto every round + best regen-affordable Strike
x uptime, DPS archetype x1.18, opener amortized; pet is a maxed Aurora
Worldserpent, a flat ~281/tick at cap that any class can hold):

| build | L50 | L75 | L100 |
|---|---|---|---|
| Rogue solo | 167 | 273 | 366 |
| Rogue + pet | 361 | 517 | **647** |
| Runemaster + pet (next caster) | 330 | 464 | 578 |
| Beastlord + pet (post ladder fix) | 371 | 505 | 615 |

### Thundersmith targets

| state | target at L100 (with pet) | vs Rogue+pet |
|---|---|---|
| Tier-5 cells, ledger known | **~686** | **+6%** |
| Tier-5 cells vs physical-resistant foe | further ahead (school bypass) | biggest gap |
| Scrap shot (dry) | ~605 | -7% |

The +6% ceiling is the design contract: best in the game while fueled and
informed, mid-tier on scrap. If implementation lands the fueled number above
~+10%, tighten cell power or shots-per-cell, not the frame.

## 9. Gates summary

| gate | what it costs |
|---|---|
| Smithing ~50 + masterwork materials | tier-5 cells, the power ceiling |
| The recon fight, per species | survive learning each boss the hard way |
| Ongoing: cell economy | OP-ness has an upkeep bill; Scrapwright refunds reward clean play |

## 10. Engine cost map

Cheap where it counts, reusing existing patterns:

- Loaded ammo: the `weapon_poison` transient pattern (`Some((school, power, charges))`),
  no save state.
- Probe reveal: existing `Defense`/`defense_tag` machinery.
- Auto-chamber: a hook in `engage`.
- Doorway advantage: skip the mob's first strike after engage (flag like
  `opening_strike`).
- Capacitor plating: existing `shield` field, topped in the upkeep loop.
- The Ledger: one persisted `Vec<String>` of known species names (the only
  schema bump).
- Standard new-class wiring: an arm in every `match self` in `classes.rs`
  (name / primary_score / resource / tagline / description / trait_name /
  trait_desc / stats_at / as_key / from_key), entry in `ALL`, a 10-ability
  roster in `abilities.rs` (ids 2200+), the two archetypes, trait hooks in
  `svc.rs` (engage, combat round, kill_mob for Scrapwright, strike_player for
  doorway/plating), cell items + recipes in `items.rs`/`crafting.rs`.

## 11. Open tuning knobs

- Shots per cell (~20 proposed) and material cost per tier decide whether OP
  upkeep is minutes or hours per session.
- Doorway advantage: one denied strike, or two vs non-boss.
- Scrapwright refund rate (1 shell per kill proposed).
- Whether probe results are account-wide or per character (per character
  proposed; the ledger is the character's story).
