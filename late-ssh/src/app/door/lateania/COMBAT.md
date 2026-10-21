# Lateania: combat, classes and items

How a fight resolves once you are standing in one: classes and ability scores, abilities and the damage formula, the world resist/weak pass, combat rules, and items, shops and rewards. Part of the Lateania door: the land numbers and the balance audit are `PROGRESSION.md`, the runtime and its invariants are `CONTEXT.md`.

### Classes and scores

Playable classes (17; the first nine are the class-select `1-9` quick-pick):
- Warrior: Rage, `Unbreakable`, Strength primary.
- Mage: Mana, `Arcane Mastery`, Intelligence primary.
- Cleric: Mana, `Light of the Dawn`, Wisdom primary.
- Rogue: Energy, `Opportunist`, Dexterity primary.
- Ranger: Focus, `Hunter's Instinct`, Dexterity primary.
- Druid/Necromancer/Bard/Monk/Paladin/Warlock/Berserker: the next seven (Spirit/Souls/Tempo/Ki resources).
- Beastlord: Spirit, `Pack Bond` (empowers the taming/pets companion), Wisdom primary.
- Skald: Tempo, `War-Chant` (fast Tempo regen), Charisma primary.
- Runemaster: Mana, `Runic Overflow` (+arcane spell damage), Intelligence primary.
- Valewalker: Focus, `Reaping Harvest` (self-heal on melee hit), Strength primary.
- Spiritmaster: Souls, `Spirit Siphon` (health+Souls on kill), Charisma primary.
- Beastlord, Skald, Runemaster, Valewalker and Spiritmaster keep their rosters at ids 1700/1800/1900/2000/2100+, each with a level-50 capstone and two archetype paths at `ARCHETYPE_LEVEL`. Progression reads as tiered: staged ability unlocks across the curve, the L10 archetype specialisation, and the shared five-level named milestones.

Adding a class means an arm in every `match self` here (name/primary_score/resource/tagline/description/trait_name/trait_desc/stats_at/as_key/from_key), an entry in `ALL`, an ability roster in `abilities.rs`, and (if the trait needs runtime behaviour) a hook in `svc.rs`: upkeep loop for regen (Druid/Paladin) and Tempo (Bard/**Skald** War-Chant); `kill_mob` for harvest (Necromancer/Warlock/**Spiritmaster** Spirit Siphon); `strike_player` for Monk mitigation; `spell_damage` for Mage/**Runemaster** overflow; the combat round for Berserker frenzy and **Valewalker** heal-on-hit; the pet-bite step + `fire_pet_skills`/`wound_pet` for **Beastlord** Pack Bond (empowers the taming/pets companion - stronger bite, tougher, faster auto-skills). **Every level grants something:** the curve grows each level (surfaced by `check_level_up`, which logs the concrete +HP/+attack/+resource gains per level), plus `level_milestone`/`milestone_hp_bonus` add a named milestone (Blooded…Ascended) with a permanent +HP every fifth level, a pure function of level, so no extra save state; `current_milestone(level)` shows on the character sheet. **Archetypes:** at `ARCHETYPE_LEVEL` each class offers two paths (the `ARCHETYPES` data table; `archetypes_for`/`archetype_by_key`), each carrying a `Role` (Tank/Healer/DPS) and four percent modifiers (`attack_pct`/`mitigation_pct`/`heal_pct`/`max_hp_pct`). The modifiers apply at existing combat hooks in `svc.rs` (DPS in `attack()`+`spell_damage`, Tank in `strike_player`, Healer in `heal_player`, max-HP in `max_hp()`); no engine changes; the chosen `&'static ArchetypeDef` is held on `PlayerState` and persisted by key.

Progression:
- Level cap is `Class::MAX_LEVEL = 100`.
- `xp_for_level` keeps early levels quick, then adds a much steeper post-level-8 term so midgame and Frontier progress target roughly week-scale casual play instead of a 1-2 sitting clear; `level_for_xp` caps at `MAX_LEVEL`.
- `Class::stats_at(level)` computes HP/resource/attack/resource regen.
- Ability scores are rolled before class selection, persist after class choice, and grow by placed points: `stats::points_earned(level)` is one point per `POINT_EVERY_LEVELS` (4) levels, so 25 over the whole ladder; what is left to place is that less the saved `score_points_spent`, a pure function of level so a save can never drift and a character saved before points existed simply has them all to place (`a_character_saved_before_points_existed_has_them_all_to_place`). A point raises any score up to `SCORE_CAP` (20); `spend_score_point` keeps the point when the score is capped and says so.
- Every score feeds exactly one mechanic through its D&D modifier, all of it pure in `stats.rs` and applied at one hook each in `svc.rs`: STR `swing_pct` (+2%/mod on `swing()`), DEX `crit_pct` (+2%/mod chance a swing crits for double, or below 10 glances for half; `crit_outcome` rolled in both combat rounds), CON `hp_bonus` (mod × (4 + level/2) max HP), INT `spell_power_pct` (+2%/mod on `spell_power_of`, so every ability), WIS `regen_bonus` (+1/mod a tick, `PlayerState::regen`), CHA `price_pct` (3%/mod off `buy_price`, on top of `sell_price`) and `tame_pct` (3%/mod on `tame_chance`). Nothing touches `attack_rating` itself. An 18 is therefore worth ~8-12% on its axis, and a placed point on an even score moves nothing until the next one (the point screen says so). Pinned by `every_ability_score_moves_the_number_it_promises` and `every_score_moves_one_number_and_says_so`.
- `Class::primary_score` is only the score a calling leans on for damage (the row the sheet glows): STR for every martial and hybrid, INT for every caster, DEX for Rogue/Ranger/Monk, WIS for Cleric/Druid. No class has CHA as its primary.
- The creation screen prints the rule and the rolled reading of all six under the attribute rows (`ui::attribute_rule_lines`); the point screen (`ui::score_point_lines`, a gate like the archetype crossroads) shows each score's reading now, after the point, and the rule. Nothing about the scores is hidden from the player.
- The sheet and the class-select screen glow the primary score's row in the class accent. Both the label (`ui::primary_label`) and the accent (`ui::class_accent`, `ui::class_emblem`) are matched exhaustively over `Class`, so a new calling breaks the build instead of rendering with no attribute highlighted and a nameless "Adventurer" bust.

### Abilities and damage

- `AbilityEffect` variants: `Strike`, `DamageOverTime`, `Heal`, `HealOverTime`, `Empower`, `Ward`, `Stun`, `Finisher`.
- Every class has a level-1 ability, a level-50 capstone, and ten more at every fifth level from 55 to 100 (see the roster-cut item in `CONTEXT.md` §11): 23 abilities for the first five classes in `Class::ALL`, 21 for the next seven, 20 for Beastlord/Skald/Runemaster/Valewalker/Spiritmaster. Slots past the 1-9/0 hotbar cast from the Abilities panel.
- Offensive abilities require a target. Heals, buffs, and wards do not.
- Damage schools: Physical, Fire, Frost, Holy, Shadow, Poison, Arcane, Lightning.
- `DamageProfile` lets each mob deal one attack type, resist up to one incoming school, and be weak to up to one incoming school.
- Resist halves damage, weak adds 50 percent, and minimum damage is 1.
- Auto-attacks are physical and still pass through mob resistances.
- **Both halves of the matchup are on screen.** A foe's nature line names its attack school and its weak/resist; the ability rows name theirs, via `Ability::effect_label` ("fire damage over time", "arcane stun"), in the `v` panel and in the battle side panel's roster alike - both render the one `AbilityView.effect` built in `svc`, so there is a single place to change it. The school comes from `Ability::school`, which returns `None` for `Ward`/`Empower`/`Heal`/`HealOverTime`: `damage_type` is a plain field on every row of the roster, but only `Strike`/`DamageOverTime`/`Stun`/`Finisher` reach `damage_target`, so the rest carry an inert school that must never be shown as though it were a lever. `only_the_effects_that_resolve_damage_carry_a_school` pins that split against the runtime's own dispatch.
- **The damage formula.** One shared **attack rating** (`PlayerState::attack_rating`: class curve + gear + active empower + primary score; `attack()` is that with the archetype's `attack_pct`, the number the sheet calls "attack"). What the rating feeds is decided per calling by `Class::damage_weights()` (`classes.rs`, `DamageWeights { auto_pct, spell_pct }`, three closed shapes: **casters** Mage/Runemaster/Necromancer/Warlock/Spiritmaster/Cleric at 50/60, **hybrids** Druid/Paladin/Bard/Skald/Beastlord at 95/45, **martials** Warrior/Rogue/Ranger/Monk/Berserker/Valewalker at 100/35):
  - the Physical auto lands for the **swing** = `attack() * auto_pct / 100` (`PlayerState::swing`; a Mage swings at half, a Warrior in full);
  - every ability lands for `magnitude + spell_power * ability_coef_pct(effect) / 100` (`ability_power`), where **spell power** = `attack_rating() * spell_pct / 100` (`PlayerState::spell_power`) and the coefficient is per effect (`svc::ability_coef_pct`: Strike 100, Finisher 150, Stun 50, DoT 30 per tick, Heal 50, HoT 20 per tick, Ward 60, Empower 25). Class traits (+20% Mage/Runemaster, +25% Ranger vs wounded) and then the archetype apply once to the whole hit in `ability_damage`; an Empower is fed the rating *without* the running empower so a buff never compounds itself. The table magnitude is the flat floor an ability keeps at level 1.
  - a `Stun` never shortens one already on the foe: `mob_stuns`/`pvp_stuns` keep the longer of the two (`a_shorter_stun_does_not_cut_a_longer_one_short`).
  - Consequences, measured in the arena: gear and level lift abilities as well as the swing, so a caster's output rides its schools (and the resist/weak board) rather than a Physical swing; on the neutral dummy martials sit at 55-67% auto, casters at 67-75% abilities, and the best-to-worst calling spread in the same gear is under 1.6 at every ladder step. Contracts: `casters_lean_on_abilities_and_martials_on_the_auto`, `classes_kill_at_a_similar_pace_in_the_same_gear` (`arena_test.rs`), `abilities_scale_with_spell_power_and_the_auto_swings_by_calling` (`svc_test.rs`). The sheet shows attack, swing, and spell (`PlayerView.swing`/`spell_power`).
  - **Resource regen grows with level**: `stats_at` adds `l / REGEN_LEVELS_PER_POINT` (one point per four levels) on top of every class's base. The bases were tuned for level-1 costs and the roster's costs climb to ~49 by level 100; without this a summit caster cast once every five ticks and fell back on its swing.
- Every generated zone carries a themed resist/weak profile on its regulars: see the dedicated section below, **The world resist/weak pass**.

### The world resist/weak pass [STABLE]

Every generated zone gives its regular mobs a themed
resist/weak profile, so school choice is a real lever across the whole Lv30-60+
band instead of only against ~116 authored spawns. Data-only: the engine
multipliers (resist halves, weak +50%, minimum 1) and the one-resist-one-weak
`DamageProfile` shape are unchanged.

**The theme vocabulary.** `damage::ZoneTheme` is a closed 16-variant enum; each
variant maps exhaustively to `(resist, weak)`. Physical never appears in either
slot, and every theme carries a weakness (weak-forward: the right school is a
reward; walls are events, and rare).

| theme | resist | weak | flavor |
|---|---|---|---|
| Ashen | Fire | Frost | magma-born flesh |
| Sunscorched | - | Frost | heat without fire-born flesh |
| Frozen | Frost | Fire | ice country |
| Verdant | - | Fire | burnable greenwood |
| Tidal | - | Lightning | open water, wet ground |
| Drowned | Frost | Lightning | the cold deep |
| Storm | Lightning | Arcane | storm-born |
| Resonant | - | Arcane | song, echo, standing wards |
| Undead | Shadow | Holy | barrow-flesh |
| Haunted | - | Holy | ghosts without the flesh |
| Profane | Holy | Shadow | god-cults, the profaned divine |
| Fae | - | Shadow | glamour |
| Beastwild | - | Poison | living beasts and vermin |
| Fungal | Poison | Fire | spore and rot |
| Construct | Poison | Lightning | bloodless made things |
| Crystal | - | Lightning | glass and shard |

**Placement.** One theme table per generated region, beside its zone data and
in the same order: `FRONTIER/REACHES/KAELMYR/LAKES/BROCELIANDE/AELUNOR_ZONE_THEMES`
in `world.rs`, `ISLAND_THEMES` in `archipelago.rs`; 126 themed zones. Regulars
inherit the theme at spawn build (an Aelunor spawn wears its zone theme
whatever affix it rolls). **Every boss carries a weakness** - bosses are the
fights players actually prepare for, so the prep mechanic must exist there:
generated zone bosses inherit their zone theme's weak but **never** its
resist (a weakness is pure reward; a resist on a boss is a class tax with no
counterplay, so boss resists stay rare authored events - the 14 Physical
walls and the elemental crowns). Authored crowns keep their hand-picked
profiles; the zone teaches the school, the boss is the exam, and the oil
already in your bag is the answer.

**The two hard rules** (global, no exceptions): nothing anywhere is weak to
Physical, and no regular anywhere resists it - a Physical resist on a regular
is a zone-wide tax on the seven Physical-locked classes with no counterplay.
Authored regulars that would resist it are themed elsewhere
(constructs/stone to Poison, wraiths/shades to Shadow, cold-sea creatures to
Frost), each keeping its weakness.

**The solo rule** (this is a solo game, no grouping fallback): a Physical
resist on a boss may only guard an optional prize or sit at the low band
where a tier-0 oil's flat rider out-punches the halving. Exactly 14 bosses
wear one - the Elder Treant (the road's teaching fight, ~L5-8, where a 20g
Sparkseed Oil already beats hitting a neutral boss dry), the Fallen Paladin
(optional Sunken Citadel), and Aelunor's 12 zone bosses (optional region,
weak Holy, so the blessed-oil rider lands at 150%). The mandatory Long Road
past the Treant never demands a school a Physical-locked class can't bring;
never add a Physical resist to a road crown. Pinned by
`physical_walls_never_gate_the_long_road_past_the_treant` (`svc_test.rs`).

**Census bands** (declared, test-enforced): per school 10..=30 weak zones and
<=10 resist zones; Holy keeps >=4 resist zones (the Profane predators - without
them the two mono-Holy classes silently win the school game); resists <= a
third of any region; >=5 weak schools per region; no school owns more than a
quarter of a region's weaknesses. Nothing resists Arcane.

**The martial lever** is the weapon-coat family (the poison vial plus the four
Alchemy oils, five schools on one curve; mechanics in Crafting depth): flat,
charge-limited school riders on the Physical auto that ride `seed_mob_dot`'s
baked-in resist/weak multiplier.
Coats are never a conversion of the auto's school and never a multiplier on
`attack()` - both are reserved for the planned Thundersmith class
(THUNDERSMITH.md), whose whole identity is industrializing this system.

**The balance budget**, enforced by the routed grind-rate model in
`world_test.rs`: 75% auto / 25% abilities from the real roster mix, plus the
coat rider; "before" is exactly 1.0 everywhere since regulars were
`(None, None)`. Bands: per-zone swing within +-15%; per-class themed-zone
average within +-3%; a >=+5% best zone-and-coat answer for every class in
every region (the meaningfulness floor); a <=+18% routed ceiling with <=12
points of spread between the best- and worst-served class. The mono-Holy
classes top the table by design (holy oil stacking with their own school in
Undead/Haunted lanes): the deliberate buff to today's weakest two.

The rider in that model is **derived from the engine, never declared**:
`COAT_PER_TICK` over `TIER_ATTACK_BAR` (both in `svc.rs`, both pinned to a live
character by `svc_test.rs`), converted through `AUTO_SHARE`. It lands near 14%
of output. **That figure is for a character with no
companion**: `AUTO_SHARE` has no pet term, so a player running a strong tame
sees less of it (`PROGRESSION.md`, "Where the world pass's own model
diverges"). The rider must stay derived from a live character: a hardcoded
share is a model measuring a constant, not the game.

**Tests** (`world_test.rs`): `every_generated_zone_spawn_wears_its_zone_theme`
(regulars wear the theme, zone bosses wear its weak and no resist),
`no_regular_resists_physical_and_nothing_is_weak_to_physical`,
`every_boss_carries_a_weakness`,
`the_school_census_stays_inside_its_declared_bands`,
`the_world_pass_redistributes_grind_rates_but_never_rebalances_a_class`; in
`svc_test.rs`, `the_attack_bar_still_matches_a_real_character` and
`the_coat_curve_stays_inside_its_share_of_the_bar`. Re-theming a zone means
editing its table row and letting these judge the census and budget; they, not
prose, are the contract.

**Visibility**: the targeted foe's traits line (`rank · strikes with X · weak
to Y · shrugs off Z`) in both battle surfaces, plus the per-hit log tags.
Deliberately no pre-fight display: the first swing is the probe, and pre-fight
knowledge is reserved as the future Thundersmith Ledger's territory.

### Combat rules

- `engage` targets the first alive mob in the current room unless the room is safe. Engaging a boss opens with a one-line bark; a blow worth ≥25% of the foe's max HP logs as "crush into" instead of "strike"; a foe first dropping below 25% logs a one-time "staggers" line - flavor only, no mechanics.
- Movement and recall are blocked during combat; flee clears target and moves through the first available room exit, or only breaks combat if no exit exists. **Flee is never free**: the foe you turn your back on lands one more blow as you run (`strike_player` at its normal damage, skipped while it is stunned), and a blow that fells you ends the flight where you stand. Breaking off also takes you off that foe's `engaged_by` list, which starts its recovery clock (see below); it keeps its wounds when you run, so ducking next door and coming straight back resumes the same fight.
- **Who counts as still fighting** (`MobInstance::engaged_by`, `mobs_in_a_fight`): drawing on a mob puts you on its list, and you stay there until you flee, fall, or leave its room. Fleeing and falling (`fall_out_of_every_fight`) take you off at once, so a fighter raised where they fell, by a healer's rite or a veteran charge, rises out of the fight and is not struck until they draw on the foe again. A foe with nothing to shed (never wounded, stunned, or festering) that nobody fights forgets its whole list on the next sweep instead of lying in wait for whoever walks back in. A mob is in a fight while anyone holds it as a target **or** anyone on its list is alive in its room - the second half is the load-bearing one, because the combat round only ever resolves the mob a player is *targeting*. So a foe you turned from **keeps swinging at you while you stand in its room** (a second strike pass in the tick, stun-checked and Brute-enraged like the round proper), and a mob in a fight does not roam. Without it, "still in the room" would have been a free place to stand untargeted, take nothing, and heal up.
- **Foe recovery** (`MOB_RESET_TICKS` = 10, ~20s, `recover_abandoned_mobs` in the tick): any living mob that is wounded, stunned, or festering with **nobody left fighting it** for ten consecutive ticks recovers in full (health, stuns, DoTs, and its `engaged_by` list), and everyone in its room is told. The clock only runs once every fighter has left: dying, disconnecting, fleeing, walking off. Switching your lock onto the add it just summoned, or onto a pack-mate it dragged in, is the same fight and costs the foe nothing - that hole let a solo boss heal to full in six seconds while you killed its own add. The grace is deliberately wider than the shared potion cooldown (5 ticks) by a single round trip, so a hit-and-run buys two gulps at most and the engage / stun / flee / repeat loop stays closed. Pinned by `fleeing_costs_a_parting_blow_and_the_foe_recovers`, `a_stunned_foe_cannot_strike_at_a_fleeing_back`, `a_wounded_foe_nobody_fights_recovers_after_a_few_ticks`, `a_foe_someone_else_still_fights_keeps_its_wounds_when_you_flee`, `a_boss_keeps_its_wounds_while_you_fight_the_add_it_summoned`, `a_foe_you_turned_from_keeps_swinging_while_you_stand_in_its_room`, `stepping_out_of_the_room_starts_the_recovery_clock`, `a_fighter_raised_where_they_fell_is_out_of_the_fight`, `a_full_health_foe_forgets_a_fighter_who_walked_off` (`svc_test.rs`) and the two arena exploit pins (`arena_test.rs`). The Rogue's opening strike still re-arms on every engage; against a foe that is always at full health that is harmless.
- Rogue opening strike doubles the first auto-attack after engaging.
- Mage offensive spell damage is boosted by `Arcane Mastery`.
- Cleric healing is amplified by `Light of the Dawn`.
- Ranger damage is boosted against wounded targets below half health.
- Warrior survives the first lethal blow of each life at 1 HP.
- Veteran accounts, checked on join by account age, can resurrect in place while charges remain; fountains refresh charges.
- **Combat companions.** A pet bought from a capital Stable (`buy_pet`, one at the heel at a time; a new purchase sends the old one home to the **kennel**, see below) rides on `PlayerState` and so is always in its owner's room. In the combat round it **bites the owner's target** after the owner's strike (crediting the kill to the owner) for `Pet::attack()` (species base + an eighth per loyalty level, at least 1) **plus `PET_COEF_PCT` (20%) of the owner's attack rating**, the same flat-floor-plus-share shape abilities have, so the companion multiplies the build instead of replacing it (12-33% of output on a band-appropriate beast, pinned by `a_companion_is_a_share_of_the_fight_not_the_fight` and the exact number by `a_companion_bites_off_its_owners_rating`); when the owner is struck, `wound_pet` splashes `PET_WOUND_PCT` of the blow onto it (alongside `wound_escort`), **but only on survivable hits**, since the death branch takes no `wound_*` (combat is over once you fall). A pet at 0 HP is **downed** and stops fighting until **fed** (`G` from anywhere, or `~` when no stray is being courted; both land in `feed_owned_pet`, costing `PET_FEED_COST`). **Loyalty is capped at `MEALS_PER_DAY` (4) meals a UTC day** (`PlayerState::pet_meals`, persisted, on the player so a new beast does not reset the count): a meal is `FEED_LOYALTY` + `Pet::mend`, and past the cap a hurt or downed pet is still mended for the fee with no loyalty, while a well pet with no meal to have is turned away free (`Meal::Sated`). A pet at `PET_MAX_LEVEL` has no meal to have either (`NoMeal::FullyGrown`, beside `MealsSpent`): loyalty past the cap raises nothing, so it is mended when hurt and otherwise never charged, pinned by `a_healthy_companion_at_the_level_cap_is_turned_away_free`. Every pet takes 36 meals to reach `PET_MAX_LEVEL`, so nine days. Pinned by `a_companion_grows_on_four_meals_a_day_and_is_only_mended_past_them`. The room panel promotes `G` into `You can` only for a hurt or downed pet (`mend`/`rouse companion`); otherwise `pet_feed_chips` puts `G feed 20g · 45% to Lv4 · 2/4 today` under the pet, dropping the `G feed` chip when `G` would be turned away (meals spent, or max level). Loyalty raises the pet's level (more HP/attack). Persisted by species key + loyalty.
  - `PetSpecies` carries a closed `source: PetSource` (`Stable { price, rung }` or `Wild { tame_level }`; `rung()` places both on one companion ladder) and its own `skills: &'static [taming::PetSkill]` ladder (every pre-Aelunor species points at the shared `taming::PET_SKILLS`; Aelunor's five each carry their own, so different pets really do have different spells); `pet_species_by_key` searches `PET_SPECIES`, `taming::TAMEABLE`, **and** `taming::AELUNOR_TAMEABLE` so a saved pet of any kind reloads. The world wiring (buying/feeding/wounds, the bite each round, and the level-gated **pet auto-skills**) lives in `svc.rs`. Persisted by species key + loyalty (HP restored full on load). `Kennel` holds the owned pets not at the heel, one per species, and decides where a bought or tamed pet goes.
- **The kennel.** `pets::Kennel` on `PlayerState` holds every companion the player owns but is not leading, at most one per species and never the species at the heel, so each owned species exists exactly once (`adopt_tamed`, `adopt_bought`, `call_out`, `admit`; pinned in `pets_test.rs`). Nothing is ever released: a purchase sends the old companion home, a tame lands there when the heel is taken, and the Stable refuses to sell a species you already own (`Bought::AlreadyOwned`, no charge). Kennelled pets keep their loyalty and their runtime hp/downed state (a downed pet stays downed at home, so the kennel is no free heal). The Stable panel (`p`) lists **Your kennel** first, strongest rung first, then **For sale** (owned species show `owned` instead of a price); `Enter` on a kennel row is `call_out_pet`, which swaps it with the pet at the heel, only at a Stable and never mid-fight. Pinned end to end by `a_new_companion_sends_the_old_one_to_the_kennel_and_it_comes_back`.
- **Death & resurrection.** A lethal blow with no Warrior death-save and no veteran charge leaves the player a **corpse where they fell** (`dead = true`, hp 0, target/shield/empower cleared, 20% carried gold lost, escort lost; banked gold protected). The corpse lingers (`respawn_at = now + CORPSE_LINGER_SECS`). The player chooses: **wait** for a resurrection, or **release** to the temple now (`release_to_temple`, `r`/Enter while dead). If neither happens by the deadline the tick auto-releases them. **Resurrection** is a rite of the holy/nature callings (`Class::can_resurrect` → Cleric/Paladin/Druid): a living caster in the same room spends `RESURRECT_COST` to raise the nearest corpse **in place** at `RESURRECT_HP_PCT` of max (`resurrect_nearest`, `g` key). The snapshot exposes `dead`, `can_resurrect`, `corpse_here`, and per-occupant `alive` so the UI shows the fallen overlay, a `(fallen)` roster tag, and the rez hint. The dead state is **transient** (not persisted; a reload returns the character alive at a safe room).
- `seed_world()` applies a balance scaler after every spawn is generated: `tune_spawn_balance` scales each spawn by its `Band` row (see "The crowns and the story they encode" in the shape section), then `tune_crowns` re-fields the fourteen crowns at their `CROWNS` numbers. Each land has its own pair of rows calibrated against its crown.

### Items, shops, and rewards

- Equipment slots: Weapon, Head, Chest, Legs, Hands, Feet, Ring, Trinket.
- Item rarities: Common, Uncommon, Rare, Epic, Legendary.
- Item kinds: Equipment, Consumable, Valuable.
- Valuables, including Frontier relics, show a `valuable / sell Xg` stat line in inventory/shop UI so players know they are sell loot; generated Frontier relic descriptions also state that they have no combat use.
- **An item's stat line says what using it does, never what selling it pays.** For `ItemKind::Utility` (every weapon oil and poison in the game) a sell price would say nothing about what separates Sparkseed, Chillrime and Blessed Oil. `Item::coat_effect` reads the very constants the service applies in `use_item`/`coat_weapon` (`svc::COAT_PER_TICK`/`COAT_CHARGES`) and states the school, the rider and the window: `coat a weapon: +15 fire a strike for 40 strikes`. Deriving it rather than authoring it is what stops the shelf promising a number combat does not deliver. Cooked meals do the same for their lasting half (`+5 hp a tick for 8 ticks`), through `items::food_well_fed` - which is also what `svc::use_item` applies, so the number the pack promises and the number the tick grants are one place. Pinned by `a_coat_says_which_school_it_adds_and_for_how_long_not_what_it_sells` and `a_cooked_meal_says_the_regen_it_leaves_behind_as_well_as_the_heal`.
- Gear rows in the inventory and shop panels carry a **comparison line** vs. what's worn in that slot (`InvView::compare`/`ShopEntryView::compare`, built by `svc::compare_to_worn`): green upgrade / red downgrade / amber trade-off, or "new slot"; empty for the worn item and non-gear (`ui::compare_line`).
- Starter inventory is a Rusty Shortsword and two Minor Healing Draughts. Starting gold is 120.
- Shops are in Embergate: Ember Forge, Outfitter, Apothecary, and Curio Cart.
- **The shops scale with the character (the market tier).** Authored stock alone would leave gold buying nothing after about L37, so each gear shop *also* stocks its own slots from the generated realm catalogs at `PlayerState::market_tier`, so no new item catalog exists: the Forge sells Weapon/Hands, the Outfitter Head/Chest/Legs/Feet, the Curio Cart Ring/Trinket (`Shop::market_slots`), drawn through `items::market_item_id`, which walks Frontier 1-20 → Reaches 21-40 → Kaelmyr 41-60 (`MARKET_TIER_MAX`). The market stops at the end of the **road**, not at the end of the gear: `archipelago_items` is a full eight-slot Legendary set at t=61-80 above it, and it stays unbuyable on purpose, because the Archipelago is off-road content where the best gear is earned. `market_title_cap` makes that structural anyway (it ladders on the three gate titles and the Archipelago grants none), and `market_tier_base` would index past Kaelmyr's catalog if the ceiling were lifted without teaching it the fourth realm.
- **The market tier is derived from two factors, the lower binding.** *Level* reads `expected_kit_tier`, interpolated between `world::CROWNS`' own kit rows (L40 authored → L55 Frontier-10 → L65 Reaches-10 → L75 Kaelmyr-10 → L80 Kaelmyr-15), less `MARKET_LAG_PCT` (20). *Gate titles* cap the band, so gold can never buy out of a continent the character has not opened: Archdemon's Bane → Frontier top, King's Bane → Reaches top, Yssgar's Bane → Kaelmyr top. The clamp to `MARKET_TIER_MAX` is applied **before** the lag, never after, or a level past the ladder's end creeps back up to Kaelmyr's deepest drop. Contract: `the_shops_deep_stock_always_trails_the_kit_the_level_expects` (a bought piece is weaker than its own land's drop in every slot, at every rung), plus `gold_cannot_buy_a_kit_out_of_a_continent_you_have_not_opened` and `a_market_piece_you_have_not_earned_cannot_be_bought`.
- The result is a shop that reads about **-9% to -18%** against the kit the crown ladder expects, at every level from 45 to 100: a floor under a straggler, never a replacement for the hunt. `MARKET_MARKUP_PCT` (200, i.e. 3x list) makes it the late game's repeating gold sink: a full eight-slot set runs 21,600g at L55 and 103,200g at L85+. It also fills Head/Legs/Hands/Feet, which the Archipelago's finds never drop.
- **`PlayerState::shop_offers` is the single authority** on what is on sale and what it costs; both the `ShopView` builder and `buy` read it, so a stale panel or a hand-sent item id cannot buy out of band, and the panel can never quote a price `buy` disagrees with.
- **Consumables are deliberately not on the market.** A deep-realm draught heals `120 + 20t`, which at the L80 market tier is a 1,000+ heal, unlimited and on tap in town, against the Phoenix Tonic/Wyrmfire Cordial players already finish the game on. The heal curve is what the whole combat tuning rests on, so the Apothecary stays authored (`Shop::market_slots` is empty for it) and sustain stays earned.
- Shop economy intentionally includes expensive late-game gold sinks: masterwork weapon/armor/head/hands, premium curio gear, and the repeatable Phoenix Tonic. The masterwork shop pieces are shop-stock, not boss drops, so gold remains useful after normal boss clears.
- Apothecary consumables are tuned as the pressure valve for harder combat: early draughts are affordable recovery, Elixir of Renewal covers mid/late mixed HP/resource recovery, and Phoenix Tonic is a repeatable expensive late-game recovery sink. **Every heal/restore consumable shares one cooldown** (`QUAFF_COOLDOWN_TICKS` = 5, `PlayerState::quaff_cd`, transient): a second gulp inside it is refused ("still queasy") and nothing is spent. Pinned by `a_draught_needs_a_breath_between_gulps`.
- Authored boss loot tables include head and hand upgrades across tiers; living-dark bosses add controlled post-Archdemon unique gear, while their regular mobs mostly drop regional relics and sustain consumables.
- Bosses always drop one item from their loot table. Regular mobs have a modest chance if their table is non-empty.
- Mob kills grant XP, reduced gold, possible loot, and titles. Boss XP and Frontier quest XP/gold bounties are intentionally damped so boss chains do not skip too much of the level curve.
- Boss title format is `Bane of ...`; lesser foes grant a derived `...bane` title.
- Frontier boss kills complete their zone quest, award XP/gold, and grant `Champion of the <zone>`. The payout and the title's level key off `world::frontier_zone_level(zone)` (a straight line from the seals' L40 to the King's L55, the ends the generator is sloped between), never the level displayed over the boss's head: that reads by bite and moves with every retune, and a one-time payout must not (`a_zone_boss_bounty_pays_by_the_zones_target_level_not_the_number_over_its_head`).
- Defeating the authored final boss, the Archdemon Mal'gareth, pays 10,000 chips and grants the `LMG` profile-award badge. **The badge is once per account; the chips repeat** behind two gates at once (migrations 158 and 207): once per `mud_characters.id`, and at most one crown of that kind every 30 days per account. Both gates or neither: a refusal writes nothing and pays nothing. A full run of the four crowns is 40,000 a month, in band with the other boss payouts.
- Defeating the final Frontier boss, the King Who Was Promised Nothing, pays 10,000 chips on the same two gates and grants the `LKN` profile-award badge.
- Defeating the final Reaches boss, Yssgar, the Sundering Deep, pays 10,000 chips on the same two gates and grants the `LYS` profile-award badge. Defeating Kaelmyr's last boss, Kaethyr Ascendant, Who Sang the God Awake, does the same for `LKA` (the Unquenched Throne's Kaethyr the Unquenched carries no achievement; only the Ascendant form at the Sundering Wound does). The four crowns pay the same on purpose: the lockout is what stops the reroll farm on the easy pair, and the hard pair needs a leveled character every time either way. `BossAchievement.payout` is still an `Option` so a future badge-only crown stays expressible. Badge codes are named after the boss (Mal'Gareth, King/Nothing, YSsgar, KAethyr Ascendant), and chat author labels collapse to the highest crown (`LKA` > `LYS` > `LKN` > `LMG`).
- Every mob kill emits a Lateania activity win event (dashboard/quest tier only; excluded from the #lounge feed). Only the **four named realm crowns** — the ones `boss_achievement_for` recognizes (Archdemon Mal'gareth, the Frontier King, Yssgar the Sundering Deep, Kaethyr Ascendant) — publish a structured `BossSlain` event to #lounge; the ~9 regional/zone bosses (`MobSpawn.boss` without an achievement) fall too often and stay dashboard-only. `publish_kill_outcome` therefore gates the `BossSlain` on `outcome.achievement.is_some()`, since `KillOutcome` carries no `boss` flag. A player materializing in the world via `join` publishes `GameStarted`, which also ships to #lounge through `app/activity/lounge.rs`. Final-boss kills route through `ChipService::credit_run_cooldown_reward_template`; if a gate refuses, activity still records the defeat without the chip/badge detail. The character the crown is keyed on is resolved in `publish_kill_outcome` from the service's `live_slot` binding (falling back to `active_slot`, the same rule `publish` uses) rather than carried on `KillOutcome`: the world state has no slot, and the read happens one step after the tick that produced the kill.
