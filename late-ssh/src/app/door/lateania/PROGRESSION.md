# Lateania: the progression map

Where every land sits, what it pays, what opens it, and the numbers to read
before re-tuning any of it. Split out of `CONTEXT.md` because it is the one
block an agent doing balance or content work needs whole, and the one block
everything else can ignore.

**This file owns these numbers.** `CONTEXT.md` keeps no copy of the land
levels, the loot tiers or the power ladder, on purpose: a second copy is a
copy that goes stale, which is how the Thornveil rows sat at "estimated" for
a release and how an Archipelago tier got picked by comparing literals across
two tuning bands. Change a land here, and nowhere else.

Regenerate every measured number in this file with the atlas yardstick:

```
make test-llm ARGS="-p late-ssh --run-ignored all --no-capture -E 'test(region_atlas_yardstick)'"
```

Combat mechanics (classes, abilities, the resist/weak pass, where a
character's damage comes from) stay in `CONTEXT.md` §7: this file is about
where a fight happens, that one is about how it resolves.

---

## The shape of the game [VOLATILE]

One place to look for "where does this land sit, what lives in it, and how do you get there". **The level over a foe's head means "come at this level".** A crown reads the target it is tuned to fall at (`world::CROWNS`); everything else reads by its *bite* off the crown ladder (`MobSpawn::level` → `level_for_bite`: the level of the prepared character whose crown hits like this, discounted by `TRASH_BITE_PCT` 70 for a regular and `BOSS_BITE_PCT` 85 for a zone boss). Health never enters it: a sponge is a longer fight, not a deadlier one. The old `max_hp + damage * 4` power estimate is gone. Every table below is read off the engine (`region_progress` bands, the atlas yardstick, and the `arena_report_extra` roster `Every boss as the engine fields it`). None of it is a source of truth; `world.rs` is. Re-derive after a retune rather than trust.

### The crowns and the story they encode [STABLE]

The grind to 100 is long by design (75% of the xp curve is 50→100), so **the last crown falls to a prepared L80 and 80-100 is prestige** (the Archipelago, the Wildbound Waste, titles); **the first crown is a real fight at L12 with the right prep** (the Treant teaches the oil), not a one-shot. "Prepared" = the tier's kit (a smithed weapon and plate plus authored pieces under the tier's rarity cap, `arena::Gear::Kit`), the oil the crown is weak to, three draughts, and from the Reaches on a maxed companion. `world::CROWNS` is the table (14 rows: the authored core's seven, the three seals, the King, Yssgar, the two Kaethyrs), applied after `tune_spawn_balance` by `tune_crowns`, so nothing upstream decides what a crown is:

| crown | falls at | kit |
|---|---|---|
| the Elder Treant | L12 | kit 1 |
| the Bone Tyrant · Lich Vael · Magma Colossus · Wyrm · Fallen Paladin | L16 · 20 · 24 · 27 · 30 | kit 2 · 3 · 3 · 4 · 4 |
| the Archdemon | L35 | kit 5 |
| the three living-dark seals | L40 | kit 5 |
| the King | L55 | Frontier-10, shop pet |
| Yssgar | L65 | Reaches-10, maxed tame |
| Kaethyr the Unquenched | L75 | Kaelmyr-10, maxed tame |
| Kaethyr Ascendant | L80 | Kaelmyr-15, maxed tame |

**Every row is derived, not authored by feel**: `max_hp` = the median prepared dps at that kit × `CROWN_KILL_TICKS` (14); `damage` = the median prepared health pool / `CROWN_SURVIVE_TICKS` (11) + what the kit's armor blunts (half for a Physical striker, a quarter otherwise). The inputs come from `arena_crown_yardstick`; the outcome is the contract `every_crown_falls_to_a_prepared_character_and_not_to_a_walk_in` (every calling wins prepared, the median kill is 8-40 ticks, a walk-in six levels lower in the previous tier with no prep loses). Re-derive a row when the player curve moves; the contract says when.

**Lands agree with their crowns through `tune_spawn_balance`'s band rows** (`Band`: Overworld / LivingDark / Frontier / Reaches / Kaelmyr / Archipelago / Thornveil × boss-or-regular, one row each, matched exhaustively; `Thornveil` rides the same row as `Kaelmyr`). The three crowned endgame lands are calibrated at their deepest zone against the crown that stands there: a regular dies in ~3 prepared ticks and needs 15+ to kill you (casters included, armor blunts a school by a quarter), a zone boss ~8 and ~14. The Frontier's generator was re-sloped for that (`extend_frontier`: entry = a prepared L40 out of the living dark, deep = the King's L55) and its row is 1:1; the Reaches and Kaelmyr keep their generator slopes and scale by row. The Archipelago's row is 1:1 too, for the same reason the Frontier's is: it is ungated, portal-reachable and deadly by design, and the one land that runs past the end of the crown ladder is the one that can least afford a multiplier between what its generator says and what a player meets. Its bosses read Lv82-100, one level per island. Contract: `the_trash_on_a_crowns_doorstep_is_in_band`; yardstick: `arena_doorstep_yardstick`. An out-of-band land is fixed in its row, never mob by mob.

### The lands

Every `REGIONS` entry: what it scales to, what it pays, and how you get in.
Three tables, because the first question about any land is which of the three
it is. **The road** is the crowned critical path, fourteen crowns from Lv12 to
Lv80, each step opened by a title. **Side country** is ungated: it holds no
crown and nothing is gated behind it. **Safe ground** has no combat at all.

Columns: *rooms* is the count, with the id base in brackets. *trash* and
*bosses* are displayed levels (`MobSpawn::level`, a reading of damage alone).
*hp* is median regular health, the cost of a kill, and the number a land's gear
tier answers to rather than its level (see the measured power ladder below). *xp/hp* is what it pays per
point of that. *gear* is the shared realm-ladder tier (`items::realm_loot`,
Frontier 1-20, Reaches 21-40, Kaelmyr 41-60) or a land's own catalog.
Regenerate the numbers with `region_atlas_yardstick` (command at the top of this file).

**The road** (14 crowns; a title opens each step)

| Land | Rooms | Trash | Bosses | hp | xp/hp | Gear | Opened by |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Embergate & the King's Road | 198 `[1]` | Lv3-35 | 15, Lv10-35; **7 crowns Lv12-35** | 94 | 1.03 | authored | your home |
| The Sunken Catacombs | 96 `[5000]` | Lv26-35 | **The Bonewright Lich Lv40** | 715 | 0.77 | authored | Archdemon's Bane |
| Thornwood Hollows | 96 `[5200]` | Lv26-35 | **the Elder Dryad Lv40** | 732 | 0.70 | authored | Archdemon's Bane |
| The Drowned Caverns | 75 `[5400]` | Lv29-37 | **the Abyss-Thing Lv40** | 530 | 0.27 | authored | Archdemon's Bane |
| The Frontier · 20 zones | 1000 `[2000]` | Lv37-53 | 20, Lv38-55; **the King Lv55** | 1510 | 0.39 | t=1-20 | all four Banes |
| The Sundered Reaches · 20 zones | 1000 `[10000]` | Lv50-65 | 20, Lv55-65; **Yssgar Lv65** | 3054 | 0.57 | t=21-40 | the King's Bane |
| Kaelmyr, the Ashen Reach · 20 zones | 2048 `[12000]` | Lv63-78 | 20, Lv69-80; **Kaethyr Lv75, Ascendant Lv80** | 4102 | 0.89 | t=41-60 | Yssgar's Bane |

The road ends at Kaethyr Ascendant. Nothing is gated behind him, and the gear
ladder ends with him at t=60: that is the deepest set a shop will ever stock
(`MARKET_TIER_MAX`).

**Side country** (no crowns, nothing gated behind them; difficulty is the only gate)

| Land | Rooms | Trash | Bosses | hp | xp/hp | Gear | Reached by |
| --- | --- | --- | --- | --- | --- | --- | --- |
| The Overworld & Capitals | 100 `[600]` | Lv6-14 | 7, Lv19-24 | 72 | 1.02 | authored | the roads |
| The Wildbound Waste · 3 biomes (pvp) | 1086 `[30000]` | Lv10-57 | 3 apex, Lv36-58 | 1368 | 0.18 | t=1-5 / 10-14 / 25-29 per biome | walk, the Sand-Wyrm's Maw |
| The Sunderlakes · 14 zones | 1114 `[16000]` | Lv12-41 | 14, Lv20-43 | 444 | 0.32 | fish only; finds t=1-14 | walk, the Melvanala lake |
| Aelunor, the Faewood · 12 zones | 306 `[25000]` | Lv14-43 | 12, Lv29-48 | 507 | 0.22 | t=1-15, high end a rarity roll | walk, the Amber Savanna |
| Broceliande, the Greenwood · 20 zones | 1788 `[22000]` | Lv18-51 | 20, Lv26-56 | 660 | 0.27 | t=1-10; finds t=1-20 | walk, the Verdant Highlands |
| Thornveil Falls · 12 zones | 1152 `[34000]` | Lv58-69 | 12, Lv64-68 | 3167 | 0.86 | t=29-40; finds t=41-52 | walk, the World-Oak Crown |
| The Shattered Archipelago · 20 islands | 1000 `[20000]` | Lv80-100 | 20, Lv82-100 | 6124 | **1.40** | **t=61-80**, its own catalog | portal only, no title |

Two side lands are worth knowing by heart. **Thornveil Falls** reads late
(Lv58-69) but charges Reaches-grade health, which is why it pays Reaches gear;
its twenty-four signature finds at t=41-52 are the reason to go. **The
Shattered Archipelago** is the only land past t=60 and the only one that pays
better than Kaelmyr per point of health, so it is both the best gear in the
game and the fastest climb to the cap. It runs past the end of the road rather
than on it: islands 0-9 hit softer than Kaethyr Ascendant, 10-19 harder.

**Safe ground** (no mobs)

| Land | Rooms | What it is |
| --- | --- | --- |
| Wayfarer's Hollow | 5 `[40000]` | the new-player tutorial; one Lv1 practice foe |
| City Districts | 20 `[3000]` | shops off the three capitals |
| Hearthward Close | 16 `[9000]` | player housing, off Market Row |
| Silvael | 8 `[26000]` | Aelunor's own city |
| Portal Villages · 4 | 4 `[8000]` | the Archipelago's safe landings |

The authored core's seven-boss ladder. Every one is a crown, fielded at its `CROWNS` row (the level is the target it falls at, not a reading):

| Boss | Lv | Room | Grants |
| --- | --- | --- | --- |
| the Elder Treant | 12 | 28 | `FIRST_DUNGEON_GATE_TITLE` |
| the Bone Tyrant | 16 | 44 | |
| the Lich Vael | 20 | 62 | |
| the Magma Colossus | 24 | 77 | |
| the Wyrm of Frostspire | 27 | 92 | |
| the Fallen Paladin | 30 | 103 | |
| the Archdemon Mal'gareth | 35 | 110 | `FRONTIER_GATE_TITLE` |

**The ladder**, weakest to strongest by mob level. Bars are ordinary mobs; two levels per column.

```
                          1        20        40        60        80        100
                          +---------+---------+---------+---------+---------+-
the authored core          ################                                     7 crowns Lv12-35, 8 side bosses Lv12-35
the Overworld & capitals    ####                                                7 bosses, Lv19-24
the Sunderlakes                ###############                                  14 bosses, Lv20-43
Aelunor                         ###############                                 12 bosses, Lv29-48
the living dark x3                    ######                                    3 seals, all Lv40
Broceliande                       #################                             20 bosses, Lv26-56
the Wildbound Waste (pvp)     ########################                          3 apex, Lv36-58
the Frontier                                ########                            20 bosses, Lv38-55; the King Lv55
the Sundered Reaches                              ########                      20 bosses, Lv55-65; Yssgar Lv65
Kaelmyr                                                  ########               20 bosses, Lv69-80; Kaethyr Ascendant Lv80
Thornveil Falls                                       #######                   12 bosses, Lv64-68; deepest Thornveil
the Archipelago (portal)                                         ###########    20 bosses, Lv82-100
```

The road is the right-hand half: the seven crowns to L35, the seals at L40, the Frontier to the King at L55, the Reaches to Yssgar at L65, Kaelmyr to the two Kaethyrs at L75 and L80. Everything to the left of the Frontier is ungated side country where a character levels between crowns; the Archipelago is the prestige farm past the last one, and it now measurably sits there: one boss level per island from Lv82 to the cap, every island reading past Kaethyr Ascendant's Lv80 (see the measured power ladder below). Thornveil Falls sits beside Kaelmyr rather than past it - a second road through the same stretch, not a longer one.

### The gate spine [VOLATILE]

**Five hard gates, all of them in one function**: `Service::can_cross_progression_gate`. Nothing else in the world checks a title to let you walk somewhere. The seven `*_GATE_TITLE` consts feed those five checks, since the Frontier stair needs four titles at once.

```
  Elder Treant ──────────────▶ the first dungeon ladder
                               FIRST_DUNGEON_GATE_FROM -> _TO

  Archdemon Mal'gareth ──────▶ all three living-dark descents
                               Tasmania's square  -> the Sunken Catacombs
                               Melvanala's square -> the Thornwood Hollows
                               Matlatesh's square -> the Drowned Caverns
                                 |
                                 |  one seal per zone boss:
                                 |  Bonewright Lich, Elder Dryad, Abyss-Thing
                                 v
  all four of the above ─────▶ the Frontier stair, Embergate's Town Square
                               THE FRONTIER, 20 zones, nothing gated inside
                                 zone 20 holds the King Who Was Promised Nothing

  the King's Bane ───────────▶ the Matlatesh sea-gate
                               THE SUNDERED REACHES, 20 zones, nothing gated inside
                                 zone 20 "Sundering Deep" holds Yssgar

  Yssgar's Bane ─────────────▶ Down, out of Yssgar's own chamber
                               KAELMYR, 20 zones, nothing gated inside
                                 zone 19 Kaethyr the Unquenched
                                 zone 20 Kaethyr Ascendant  <- the last crown,
                                         and nothing is gated behind him
```

Nine named bosses sit on the critical path; five of them hold a key.

- **A hard gate is a title check; the 20-zone realms are not gated at all.** The Frontier, the Reaches, and Kaelmyr are each a chain of 20 zones where **each zone's boss room holds the `Down` exit into the next zone** (`extend_frontier`, `extend_reaches`, `extend_kaelmyr`). Reaching Yssgar means walking through 19 prior boss chambers, but no title is checked on any of them, so nothing forces a player to actually kill them. Depth and a boss standing in the doorway are the whole obstacle.
- **The last two gates each hand over a continent.** One title opens 20 zones. King's Bane -> all of the Reaches, Yssgar's Bane -> all of Kaelmyr. This is deliberate; the gauntlet, not the gate, is what paces the back half.
- **Kaelmyr has exactly one door and it is inside Yssgar's room.** `kaelmyr_seagate_room` finds the room where `Yssgar, the Sundering Deep` spawns and hangs `Dir::Down` there. There is no second entrance, so Kaethyr is unreachable without the Yssgar title by either walking or the Ways.
- **The Ways carry no gate rules of their own.** `CONTINENT_WAYSTONES` used to hold a third field naming the title each far gate wanted, and `svc::travel` enforced it a second time - the same rule in two places, free to drift. Both are gone: a waystone destination is now offered on `world::waystone_is_known` (have you stood there) and nothing else, so **every title check in the game lives in `can_cross_progression_gate`**. Fast travel still cannot skip a gate, because it only ever returns you somewhere you already walked.

### The connections

Three link kinds and no others: a **plain walk**, a **title gate** (see the gate spine above), and a **waystone portal** (`i` at a waystone, `portal_destinations()`). Everything that is not on the spine hangs off the Overworld as an ungated branch.

```
  Wayfarer's Hollow
    | walk (Embergate's square)
    v
  EMBERGATE & THE KING'S ROAD ---walk--- Hearthward Close (housing)
    |  the seven-boss ladder ends at the Archdemon, room 110
    | walk (the Greatroad, west)
    v
  THE OVERWORLD ---walk--- Tasmania / Melvanala / Matlatesh ---walk--- City Districts
    |
    +== [Archdemon's Bane] ==> the living dark, one seal each:
    |     the Sunken Catacombs .. the sealed boneyard stair, Tasmania's square
    |     Thornwood Hollows ..... off Melvanala
    |     the Drowned Caverns ... off Matlatesh
    |
    +--walk--> THE SUNDERLAKES ....... the Melvanala high lake
    +--walk--> BROCELIANDE ........... the Faerie Hollow, Verdant Highlands (room 688)
    |            +--walk--> THORNVEIL FALLS .. off the World-Oak Crown, Broceliande's
    |                       deepest chamber. No title gate: a long walk, not a seal.
    +--walk--> SILVAEL --walk--> Aelunor .. the Amber Savanna (see below)
    +--walk--> THE WILDBOUND WASTE ... the Sand-Wyrm's Maw, Sahra Wastes (room 751)

  == [all four Banes] ==> THE FRONTIER ...... the sealed stair, Embergate's Town Square
       deepest zone: the King Who Was Promised Nothing Lv55
  == [the King's Bane] ==> THE SUNDERED REACHES ... the Matlatesh sea-gate
       deepest chamber: Yssgar, the Sundering Deep Lv65
  == [Yssgar's Bane] ==> KAELMYR ............ the ash-gate below Yssgar's chamber
       KAETHYR ASCENDANT Lv80  <-- the last crown. Nothing is gated behind him.
```

The Ways (`portal_destinations()`) run in parallel to all of the above and are the **only** way into the Archipelago:

```
  CONTINENT_WAYSTONES (6 mainland gates)          opens when
    Embergate, the Town Square ................... you have stood there
    the Sunderlakes landing ...................... you have stood there
    Broceliande, the forest gate ................. you have stood there
    Last Watch, the Wildbound Waste .............. you have stood there
    the Sundered Reaches sea-gate ................ you have stood there
    Cinderfall Shore, Kaelmyr .................... you have stood there
  + 4 Portal Villages (8000+) and 20 island landings (20000+) .... always
```

**The Ways carry no progression rules of their own.** `waystone_is_known` (`world.rs`) is the whole rule: a mainland gate answers once `player.visited` holds it, and the archipelago always answers because its rooms have no directional exits and a visited rule would orphan them. `svc::travel` and the panel both filter through it, and the panel reports `known/total` far gates rather than listing what is missing. There is deliberately **no** title check here: the two sealed continents need none, since a visited set cannot hold a Reaches or Kaelmyr room unless `can_cross_progression_gate` already let the player walk in. That keeps every progression rule in one function.

Embergate's square needs no special case despite the visited seed being `tutorial_start_room()`, not room 1: a waystone can only be used by standing on it, and standing in room 1 is what marks it visited, so the home anchor is always known by the time it could matter. Pinned by `the_ways_only_carry_you_where_you_have_already_stood`, `a_gate_title_alone_does_not_open_the_ways`, and `the_archipelago_answers_without_a_title_or_a_prior_visit` in `svc_test.rs`.

Two things fall out of that table and are easy to miss:

- **Aelunor is not on the network, and it sits behind Silvael.** Every other far country has a waystone; Aelunor and Silvael are reachable only by walking from the Amber Savanna. `extend_silvael` splices the city into that road rather than hanging it off the end, so the walk is savanna -> **Silvael** -> Aelunor and nothing steps from the overworld straight into the Faewood. Pinned by `silvael_stands_between_the_overworld_and_aelunor`.
- **The Archipelago is ungated endgame, and that is intended.** Its islands hold Lv80-100 mobs and Lv82-100 bosses on a ramp of one boss level per island (`extend_archipelago`, band row 1:1), so the shallow isles are somewhere to grow if the last crown is walling you and the deep ones are the pinnacle past him. It is the farm past the end of the road, not a rung on it. Every landing is portal-reachable with no title at all, so a low-level character can step directly into content above the Frontier; difficulty is the only gate, and `the_archipelago_answers_without_a_title_or_a_prior_visit` pins that on purpose (the islands have no directional exits, so a visited rule would orphan the region). Nothing in progression routes through it (no island grants a gate title or a Long Road crown), so it stays open on purpose while every mainland gate is visited-gated. The Wildbound Waste has the same property in milder form, running to Lv57 in its third biome behind nothing but a walk.

**How the level scale reads, and where it stops resolving.** Everything comes from `MobSpawn::level` and bites the UI, not the engine:

- **A crown reads its target, everything else reads by bite.** `level_for_bite` walks the `CROWNS` ladder: linear between neighbouring crowns, extrapolated past either end, clamped to 1..100. A regular's damage is read at `TRASH_BITE_PCT` (70) of a crown's, a zone boss's at `BOSS_BITE_PCT` (85), so a land's trash reads a few levels under its crown by construction. Retuning a foe's damage therefore always moves its level; retuning its health never does. There is no knee any more: the scale is only as uneven as the crown ladder itself (five levels per crown through the core, then 15, 10, 10, 5 across the endgame).
- **Saturation at 100.** Anything biting past the Ascendant's 397 extrapolates on the last segment (five levels per 29 damage) and clamps at 100. Only the Archipelago gets there, and only at its very top: islands 18 and 19 share `Lv100` bosses and the deepest rooms of the last two islands read `Lv100` regulars. Everything below that resolves, one boss level per island from `Lv82`. Keep it that way - when the whole region saturates, twenty islands collapse to one ceiling and there is no reason to sail past the first. `the_archipelago_ramps_past_the_last_crown_instead_of_sitting_flat_at_the_cap` asserts the entire ladder rather than its endpoints, because the flat version had both endpoints looking correct.
- **A doorstep can read past its crown.** A land's band row is calibrated at its deepest zone against the crown standing there, so the last zone bosses before it read close to it: Kaelmyr's zones 17-18 read Lv77-80 with the Unquenched at Lv75 next door. The crown is still the harder fight (twice the health); the level over the head only says how hard it hits.

**Where the time goes.** `xp_for_level` is cubic to `XP_KNEE_LEVEL` (50) and then a flat `XP_PER_SUMMIT_LEVEL` (75,000) per level to 100. Total climb is 4,967,282 xp: **1,217,282 to reach 50 (24%), then 3,750,000 for 50 to 100 (75%) at a rate that never changes.**

**Known gaps in the shape** (see also `CONTEXT.md` §11):

- The authored core holds 6,276 xp in total, which is about Lv11 of progress, while its own last boss falls at L35 with the tier's kit (L32 bare kit, L20 with a maxed tame; the arena's Long Road table). Players are therefore pushed out into the ungated side countries to level and come back over-levelled, which is why the approach to the Throne plays as trivial even though its trash reads a few levels under its crown by construction.
- Quest content clusters hard: 5 starter steps at Lv1-10, then **2 bounties across Lv10-30**, then 32 quests unlocking at once at Lv30-35, then **nothing authored across Lv35-52**, 8 bounties to Lv78, and **nothing past Lv78** but the last two crowns. The five side countries carry no quests at all despite being the de facto bridge from the core to the Archdemon.

---

## Frontier, Reaches, Kaelmyr, and Archipelago loot

- `items::FRONTIER_TIERS = 20`, one tier per Frontier zone; `items::REACHES_TIERS = 20`, one per Sundered Reaches zone; `items::KAELMYR_TIERS = 20`, one per Kaelmyr zone; `items::ARCHIPELAGO_TIERS = 20`, one per Archipelago island.
- Generated Frontier item IDs are `3000..3200`; generated Reaches IDs are `3200..3400`; generated Kaelmyr IDs are `3400..3600`; generated Archipelago IDs are `5000..5200` (all four realms are 20 tiers times 10 slots, built by the shared `build_generated_items`).
- `item(id)` searches authored `ITEMS`, the generated Frontier/Reaches/Kaelmyr/Archipelago catalogs, `regional_finds()` (Sunderlakes/Broceliande/Archipelago/Thornveil finds), materials, crafted, and fish.
- Reaches spawns drop `reaches_loot(zone)`; the Reaches power curve continues the Frontier's (tier 0 lands just above Frontier tier 19). Kaelmyr spawns drop `kaelmyr_loot(zone)` with `power_offset = FRONTIER_TIERS + REACHES_TIERS`, so Kaelmyr tier 0 lands just above Reaches tier 19 — a real gear step past Yssgar. Archipelago spawns drop `archipelago_loot(isle)` with `power_offset = FRONTIER_TIERS + REACHES_TIERS + KAELMYR_TIERS` (60), landing just above Kaelmyr tier 19; this replaced a bug where Archipelago mobs dropped `reaches_loot` outright, a gear step *behind* what players had already farmed by the time they reached it, not ahead. Archipelago boss loot additionally weights in the island's two `archipelago_find_ids` at increasing odds through the back half/quarter of the 20-island chain (islands `0..10` ×1, `10..15` ×2, `15..20` ×4) - the "more high-power drops toward the end" fix, layered on the base `archipelago_loot` table rather than replacing it.
- Frontier mob and boss loot tables use `frontier_loot(zone)`, which includes representative weapon, head, chest, hands, ring, draught, and relic entries for the zone tier.
- Frontier item generation now starts at post-living-dark power and climbs hard across all 20 tiers; regional boss loot is authored, meaningful post-Archdemon gear, while Frontier remains the best long-term gear path.
- Early Frontier regulars are tuned as endgame mobs: tests keep the first Frontier regular above the strongest living-dark boss damage while still below the first Frontier boss.
- Thornveil Falls has no generated gear catalog of its own (the 200-item budget went to the Archipelago, replacing its reused-Reaches-loot bug); it borrows `reaches_loot` at an offset tier plus its own 24-item regional-finds set - see the Thornveil Falls bullet above.

---

## The measured power ladder

**Read this before touching any tier.** `tier` is a generator input, not a power level, and **is not comparable across bands**: `tune_spawn_balance` applies a different hp/dmg/xp row per `Band`, so two lands at the same `tier` can be a factor of three apart. Always compare *displayed* levels (`MobSpawn::level`) and post-row damage. Measured on this branch:

| land | trash lvl | boss lvl | median trash hp | xp per hp | deepest boss dmg |
|---|---|---|---|---|---|
| The Frontier | 37-53 | 38-55 | 1510 | 0.39 | - |
| The Sundered Reaches | 50-65 | 55-65 | 3054 | 0.57 | - |
| Kaelmyr (`z + 32`) | 63-78 | 69-80 | 4102 | 0.89 | 348 |
| Thornveil (`z + 30`) | 58-69 | 64-68 | 3167 | 0.86 | ~254 |
| Archipelago, `isle + 14` (pre-port) | 69-100 | 75-100 | ~4200 | 0.38 | 555 |
| Archipelago, `isle + 52` (rejected) | 100 flat | 100 flat | 10524 | 0.38 | 1187 |
| **Archipelago, the ramp (current)** | **80-100** | **82-100** | **6124** | **1.40** | **443** |

Regenerate the whole table with the atlas yardstick, which prints every
region's rooms, level bands, median regular health and xp per point of it:

```
make test-llm ARGS="-p late-ssh --run-ignored all --no-capture -E 'test(region_atlas_yardstick)'"
```

The Archipelago ladder, as the arena prints it ("Every boss as the engine
fields it", `lateania-arena-extra.md`): Lv82 Vitreon at 15000hp/348 through
to Lv100 The Sundering, Made Flesh at 29912hp/443, one level per island. It
crosses the last crown's 397 bite at **island 10** (Your Own Reflection,
Wrong, Lv92, 398), so islands 0-9 hit softer than Kaethyr Ascendant and
10-19 harder. Island 0's boss is strictly weaker than him on both axes
(15000hp/348 against 24542/397), which is what makes the shallow isles
somewhere to grow when he walls you: the arena has every class beating him
in Kaelmyr-top gear, so the same character clears island 0.

**What the arena does and does not cover.** It fights `CROWN_TARGETS`, and
neither the Archipelago nor Thornveil holds a crown, so no simulated fight
is ever run against either; the boss table above is printed stats, not a
fought result. It also measures no xp, so the xp-per-health figures in this
section come from the spawn table rather than from play, and no kit column
exercises Archipelago gear (the arena's deepest is `L100 kael20`, t=60).

Reference points: the final crown Kaethyr Ascendant is fielded at **397 damage / 24542 hp, L80**; Kaethyr Unquenched at 368 / 22722, L75. Two consequences worth knowing before re-tuning:
- **A boss cannot read L100 without out-hitting the final crown.** `level_for_bite` discounts a boss's bite by `BOSS_BITE_PCT` (85) and a regular's by `TRASH_BITE_PCT` (70), so trash reads L100 at 359 damage but a boss needs 436, which is past the Ascendant's 397. **This is settled: the Archipelago is allowed past him.** It is off-road content reached only by portal, gated by nothing and granting no title, so it runs past the end of the crown ladder rather than sitting on a rung of it (the Path of Exile shape: the campaign boss is not the hardest thing in the game). Kaethyr Ascendant remains the strongest thing *on the road*, which is the claim that actually matters, and every island out-reads him by design. Islands 0-9 still hit softer than he does; islands 10-19 hit harder.
- **Never raise the final crown's *damage* to make it "the strongest".** Past L80 `level_for_bite` extrapolates on the slope between the last two crown rows (5 levels per 29 damage). Widening that gap flattens the slope and pushes every displayed level above 80 *down*. Raise its `max_hp` instead (via `CROWN_KILL_TICKS` for that row): health never enters `MobSpawn::level`, so a longer fight costs nothing in the level ladder.
- **XP does not follow damage, and a tier moves both at once.** This is why the Archipelago could not be fixed by re-picking its tier: `tier` scales hp, damage *and* xp together, so pulling the tier down to tame the damage also cut the xp base, and pushing it up to pay better xp is what flattened every island to L100. The two were one knob. They are now separate: the Archipelago's band row is **1:1** and `extend_archipelago` authors hp, damage and xp per island directly, so each can move without dragging the others. Measured xp per point of mob health: Kaelmyr **0.89**, Archipelago **1.40** (it was 0.38, making the deadliest ground in the game also the slowest to level on). Gold is `xp/5` (`gold_for_kill`), so it follows. Pinned by `the_archipelago_is_the_fastest_ground_in_the_game_to_reach_the_cap_on`.
- **Displayed level says nothing about what a land costs to clear.** `MobSpawn::level` is a reading of *damage* only; health never enters it. So a land's loot tier must be set against the health it charges, never against the level it displays. Measured health the road charges per realm tier: t=1 **960**, t=10 **1455**, t=20 **2005**, t=30 **2773**, t=40 **3573**, t=60 **4376**. Checked against that, Broceliande (924hp at its deepest, paying t=10) and Thornveil (2760-3288hp, paying t=29-40) are priced correctly and look under-paid only if you compare their levels; the Wildbound's Scorched Flats charged **3960hp** and paid **t=18**, which was a real two-fold under-payment and is now t=25-29.
