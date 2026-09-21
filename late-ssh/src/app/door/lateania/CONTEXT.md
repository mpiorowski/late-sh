# Lateania Game Context

## Metadata
- Scope: `late-ssh/src/app/door/lateania` plus Lateania screen lifecycle in `late-ssh/src/app/door`
- Domain: Lateania, the persistent D&D-style MUD inside late.sh
- Primary audience: LLM agents changing the Lateania game runtime, content, UI, combat, or persistence
- Status: Active
- Parent context: `../../../../../CONTEXT.md`
- Stability note: Sections marked `[STABLE]` should change rarely. Sections marked `[VOLATILE]` are expected to change when gameplay/content changes.

---

## 0. Context Maintenance Protocol [STABLE]

Read this file after root `CONTEXT.md` whenever a task touches Lateania's landing page, launch/leave behavior, reset prompt, active-world input capture, game runtime, content, UI, combat, or persistence.

**Companion files in this directory, both beside this one:**

- **`PROGRESSION.md`** - the world map: every land's levels, health, xp rate and gear tier, the fourteen crowns, the gate spine, the connection graph, and the measured power ladder to read before re-tuning any of it. **It owns those numbers and this file keeps no copy**, so a land's figures are changed there and nowhere else. Read it for any task touching where a land sits, what it drops, or what opens it.
- `THUNDERSMITH.md` - a class design, not implemented.

- Keep this file aligned with game behavior, keybindings, save shape, world/content invariants, and known gotchas.
- Never add a changelog, a "last updated" line or dated history to any of these files. They say what is true now; `git log` says when it changed.
- Update root `CONTEXT.md` when routing, global keybindings, persistence contracts, activity events, or cross-domain behavior changes.
- Treat tests and code as authoritative when comments drift. Patch stale comments or this file before handoff.
- Do not add `pub use` re-export layers; `mod.rs` should stay declaration-only.

---

## 1. Summary [STABLE]

Lateania is a persistent, shared, terminal MUD rendered inside the SSH app. It is not an Arcade game. The surrounding `door` folder is only the historical/generic place where larger door-style games live; Lateania is the current first-class game there.

Core shape:
- `Screen::Lateania` has no top-level number key. It is reached by selecting the Lateania card in the Games hub (page `3`) and pressing `Enter`, which switches the screen and joins the live world in one step.
- The Games hub renders Lateania's landing copy and launches the live world on `Enter`; saved-character reset confirmation (`d`) is handled in the hub input.
- One shared `LateaniaService` owns authoritative `WorldState` behind a Tokio mutex.
- Each connected session owns a lightweight `state::State` with a cached `MudSnapshot`, local side-panel state, and a list cursor.
- Commands are fire-and-forget service tasks. The UI renders snapshots and may briefly show old state.
- The world ticks every 2 seconds for combat rounds, effects, cooldowns, mob/player respawns, idle drops, and activity feed kill events.
- Character state and shared world state persist separately.

Current game scale:
- `seed_world()` starts at Embergate room `1`.
- The world holds ~11110 rooms: 198 base/extension, 100 overworld, 1000 Frontier, three living-world regions (96-room Sunken Catacombs, 96-room Thornwood Hollows, CA-sized ~75-room Drowned Caverns), the **Hearthward Close** housing district (rooms `9000+`, `extend_housing`), **20 city-district rooms** (`3000+`, `extend_cities`), the **Sundered Reaches**, a *second ~900-room continent* (rooms `10000+`, `extend_reaches`, hung off Matlatesh), **Kaelmyr, the Ashen Reach**, a *third ~2000-room continent* (rooms `12000+`, `extend_kaelmyr`, hung off Yssgar's chamber in the Reaches), the **Sunderlakes**, a peaceful *~1200-room water country* (rooms `16000+`, `extend_lakes`, hung off the Melvanala high lake), **Broceliande, the Greenwood**, a *fourth ~2000-room continent* (rooms `22000+`, `extend_broceliande`, hung off the Verdant Highlands' Faerie Hollow), **Aelunor, the Faewood**, a *fifth ~300-room continent* (rooms `25000+`, `extend_aelunor`, hung off the Amber Savanna) plus its own eight-room city **Silvael** (rooms `26000+`, `extend_silvael`), the **Shattered Archipelago** (`archipelago.rs` + `extend_villages`/`extend_archipelago`): four safe **portal villages** (rooms `8000+`) and a *~900-room island region* (rooms `20000+`, 20 islands, each a maze/cavern with a named boss, its own 200-item generated catalog at `5000..5200`), reached only by **waystone portals** (`FeatureKind::Portal`, a runtime feature layer over the static `FEATURES`), not by walking (the reachability test follows `portal_destinations()`), and **Thornveil Falls**, a *sixth ~1150-room continent* (rooms `34000+`, `extend_thornveil`, hung off Broceliande's own deepest chamber), a second endgame track riding `Band::Thornveil` (the same tuning row as `Band::Kaelmyr`) so Kaelmyr isn't the only late-game grind. **Each Reaches, Kaelmyr, Sunderlakes, Broceliande, Aelunor, and Archipelago zone is carved as a braided maze (`carve_maze`) or an organic cavern (`carve_cavern`), never a uniform grid** (`reaches_zone_is_cavern`/`kaelmyr_zone_is_cavern`/`lakes_zone_is_cavern`/`broceliande_zone_is_cavern` pick the cave-like ones; Aelunor is cavern-only, every zone; Thornveil is maze-only, every zone - see §9); zones chain deepest-room→next-entrance, mobs are behaviour-driven by maze-role (dead-ends ambush, junctions swarm, corridors patrol/cast), and `frontier_desc`/`kaelmyr_desc`/`lakes_desc`/`broceliande_desc`/`thornveil_desc` supply paragraph prose (Aelunor reuses `broceliande_desc`). The room-count test checks each region range; `is_reaches_room`/`is_kaelmyr_room`/`is_lakes_room`/`is_broceliande_room`/`is_aelunor_room`/`is_thornveil_room` mirror `is_frontier_room`; shape tests assert every continent has dead-ends and varied branching (not square blocks).
- **The Sunderlakes** (rooms `16000+`, mob ids `980000+`, no generated gear catalog): a large, *peaceful*, mid-game-friendly water country of flooded caverns, reed labyrinths, island-dotted meres and drowned valleys, hung off the Melvanala high lake by a normal walk (lightly gated, effectively open). 14 zones (11×8 cell fields; 10 braided reed-mazes + 4 flooded caverns) of ~1200 rooms, chained lake-to-lake. The draw is **fishing**: **40 distinct fish species** (items `4600..4700`) are caught at Fishing-gated resource `NODES` spread across the maze zones (four fish per zone, gates rising with depth so the prized deep-water catches need a trained angler). Every zone landing is a safe haven; mobs are fewer and far weaker than the endgame (a zone notable + a scatter of lake-wildlife). Fish sell for a wide price spread (a few gold to hundreds); ~a third are edible `Consumable`s and a handful of legendaries grant a well-fed `HealOverTime` "special" (see `fish_well_fed`/`use_item`).
- **Broceliande, the Greenwood** (rooms `22000+`, mob ids `990000+`, no generated gear catalog — loot borrows the Frontier tiers via `broceliande_loot`): a vast, *moderate* (tougher than the lakes, well below Kaelmyr) Dark-Age-of-Camelot country of deep-green oakwoods and steaming jungles, druid circles and briar mazes, standing stones and faerie rings, moss-grown keeps and vine-choked ruins. 20 zones (11×9 cell fields; 14 braided briar-mazes + 6 organic fern-caverns/glades, `broceliande_zone_is_cavern`) of ~2000 rooms, chained forest-gate to forest-gate, hung off the Verdant Highlands' Faerie Hollow (room `688`) by a normal walk; every zone's forest gate is a safe haven. Its through-line is the enchanted-wood dream: from the woodward holt on the eaves, past the druid circles and sleeping keeps, down into the jungle heart and the World-Oak at the centre. **Excluded from the endgame `tune_spawn_balance` scaler** (mob ids `990000..` are cut out of the `kaelmyr` band there) so it keeps gentle overworld multipliers. It is the **home of the animal-taming trade**: the sixty tameable beasts (the classic fifty plus the ten Wildbound summit beasts; see the taming subsection) gather at the zone forest gates.
- **Aelunor, the Faewood** (rooms `25000+`, mob ids `1600000+`, no generated gear catalog — loot borrows the Frontier tiers via `aelunor_loot`/`aelunor_notable_loot`): a *moderate* sprawling forest of twelve organic glades - never a maze, never a grid, every zone `carve_cavern`-only - home to elves, high elves, druids, and court fae, some friendly (Silvael's own folk) and most genuinely hostile (`AELUNOR_CREATURES`, twenty base names - Hollow-Elf Raider, Faerie Trickster, Wild Druid, Dryad Handmaiden, and so on - crossed with the five item-rarity affixes Uncommon/Rare/Epic/Legendary for a 100-name roster). **The affix is a lottery, not a depth stamp**: the roll's bands are fixed and depth only nudges them, so a Legendary stays a rare find at every depth (~1% at the eaves, ~5% in the Deep Heart, 4% of the wood's spawns overall), and it is the affix, not the zone, that carries the reward - `aelunor_loot` walks depth at half a tier per zone (Broceliande's slope) and jumps three tiers per affix step, so a Legendary spawn drops from the catalog's Legendary band while its plain neighbours stay in the Uncommon/Rare one, and a named zone boss always pays as though it were an Epic spawn (`aelunor_notable_loot`). That split is load-bearing: the wood is an ungated walk off the Amber Savanna and keeps gentle overworld multipliers, so a *reliable* high tier here would hand out at ~660hp what the Frontier guards at ~3280hp behind four Bane titles. The affix also buys **teeth**, not just a table: the hp/damage/xp premium is quadratic in the affix and flat across zones, so a Legendary spawn is a ~1100-1350hp mini-boss (two to three times its glade-mates, above its own zone boss in the early glades) wherever it rolls - the prize doesn't shrink near the eaves, so neither does the guard. Pinned by `aelunor_high_end_loot_is_a_lucky_find_not_the_default_drop` and `a_legendary_aelunor_spawn_is_an_elite_that_guards_its_prize`. Twelve zones (9×8 cell fields, ~300 rooms total) chained Wood-Gate to Wood-Gate (each zone's real entrance, found via `aelunor_entrances()` - **never assume offset 0**, unlike Broceliande's maze-only `wild_beasts()` convention, since `carve_cavern` forces the whole grid border to rock), hung off the Amber Savanna's terminal room by a normal walk. **Excluded from the endgame `tune_spawn_balance` scaler** the same way Broceliande is (mob ids `1600000+` fall outside every named band, so gentle overworld multipliers apply by default). Home to **five more tameable beasts** (`taming::AELUNOR_TAMEABLE`), each with its **own** auto-skill ladder (`PetSpecies::skills`, see the Animal Taming section) rather than the shared `PET_SKILLS` ladder every earlier species uses.
- **Silvael** (rooms `26000+`, `extend_silvael`): the Faewood's own city, a small hand-authored eight-room haven (the Starlit Square, the Wildwood Gate, the Canopy Market, the Green Larder, the Moonwell, the Druids' Circle, the High Elm Terraces, the Beastkeeper's Hollow) spliced onto the exact seam `extend_aelunor` used to hang the wood off the Amber Savanna - so the walk in now runs anchor → Silvael's square → its own Wildwood Gate → the wood's first zone, never assuming the splice direction was East (it re-derives the real anchor room/direction by searching for whichever *non-Aelunor* room links to Aelunor's zone-0 entrance; the entrance cell also has ordinary in-zone neighbours, so the search must exclude `is_aelunor_room` or it can match one of those instead). Every room is safe - the city is the "friendly" half of "some friendly, some foe", the wood outside it is the hostile half. Not in `region_layout`/`biome_of` (same as the fixed capitals and the Wildbound gate towns - only the procedurally-carved grids get map coordinates), but it does get a `REGIONS` atlas entry. Aelunor's five tameable beasts are never sold here or anywhere - they are wild-tame only, same as every other Broceliande/Aelunor beast; the Beastkeeper's Hollow says so outright.
- **Thornveil Falls** (rooms `34000+`, mob ids `1000000+`, no generated gear catalog: loot borrows `reaches_loot` at an offset tier plus its own 24-item regional-finds set at `3900..3924`): a hidden second wood past the known Greenwood - twelve zones of mist-wrapped canopy, root-choked hollows, and a descending chain of waterfalls, hung off Broceliande's own deepest chamber (the World-Oak Crown) by a plain walk, not a title gate. Exists because player feedback said the last stretch before max level was spent entirely grinding Kaelmyr - Thornveil's tier (`z + 30`) deliberately overlaps Kaelmyr's own *lower* half (`z + 32..51`) rather than extending past it, so it reads as a real alternative, not a harder or easier clone; `band_of` gives its `1000000+` mob band its own `Band::Thornveil`, tuned to the identical row as `Band::Kaelmyr`. **Every zone is a maze, never a cavern** (`thornveil_zone_is_cavern` always `false`) - unlike every other mixed continent here - because each zone's safe entrance room id has to be known exactly for the static `VILLAGERS` table, and a `carve_cavern` entrance cell is RNG-picked rather than fixed at cell 0 the way a maze's is. **Not a crown**: its final boss, "Thornveil, the Voice Beneath the Falls," is an ordinary named boss like an Archipelago island boss, not registered in `CROWNS`/`CROWN_TARGETS` - see §9.
- **Kaelmyr, the Ashen Reach** (rooms `12000+`, mob ids `960000+`, loot ids `3400..3600`): a burnt continent torn loose from the seabed when Yssgar was slain and the seas drained into the wound he left. 20 zones (13×9 cell fields; 16 braided mazes + 4 organic calderas) of ~2048 rooms total, each with a named boss, chained west-to-east and down. Five peoples run through the prose and mob/boss names: the **Emberkin** (ash-shamans of the western calderas), the **Cinderbound** (shackled dead who labour the ash), the **Gloamwrights** (glass-and-obsidian artificers of the black deserts), the **Stormheld** (sky-clans of the storm-spires), and the **Hollow Choir** (the final drowned-god cult at the wound). The continent ends at the **Unquenched Throne**, ruled since the Sundering by **Kaethyr the Unquenched, the Ashen King**, and the deepest zone (**Sundering Wound**) holds his ascended form, **Kaethyr Ascendant, Who Sang the God Awake** (the fourth realm crown: `LKA` profile badge once per account, 20,000 chips per character behind a 7-day account lockout). Gated behind the **Bane of Yssgar, the Sundering Deep** title (`KAELMYR_GATE_TITLE`) with the same transient two-step warning as the Reaches sea-gate. An ash-cairn board sits in the safe entry hub (Cinderfall Shore ash-gate, room `12000`) carrying board quests 17–22.
- Frontier has 20 zones, each 10 by 5 rooms, starting at room `2000`.
- Three deterministic living-world regions (fixed-seed `MazeRng`, identical every boot), each hung off a capital via a free direction:
  - **Sunken Catacombs** (rooms `5000+`, off `TASMANIA_SQUARE`): braided maze (`carve_maze` + `extend_catacombs`); undead.
  - **Thornwood Hollows** (rooms `5200+`, off `MELVANALA_SQUARE`): braided maze (`carve_maze` + `extend_thornwood`); beasts/fae.
  - **Drowned Caverns** (rooms `5400+`, off `MATLATESH_SQUARE`): organic cellular-automata cave (`carve_cavern` + `extend_caverns`), NOT a maze: noise smoothed into chambers, then only the largest connected pocket is kept (so no unreachable rooms); rooms are sparse within the cell field. Aberrations.
- The living-world regions are a hard post-Archdemon arc: their capital entrances require `Bane of the Archdemon Mal'gareth`, their regular mobs are capped below local boss damage, and their boss titles act as the three living-dark seals for Frontier access.

---

## 2. Module Map [STABLE]

| File | Responsibility |
|---|---|
| `../game.rs` | Minimal host-facing door-game contract: id/title/description, render/input/leave hooks, optional activity mapping, and generic outcome events. |
| `mod.rs` | Module declarations and Lateania credits. Keep declaration-only. |
| `screen.rs` | Top-level Lateania screen shell and `DoorGame` implementation: landing page, launch/reset/leave input, and active-world render delegation. |
| `state.rs` | Per-session client wrapper: snapshot receiver, local `Panel`, cursor, join retry, action delegation. Never mutate game truth here. |
| `input.rs` | Active-world key routing after launch. App-level launch/reset/leave handling belongs in `screen.rs`. |
| `ui.rs` | Ratatui rendering for class select, log, compact mode, side panels, minimap, hints. The Character panel expands to a full-width dashboard (accent-tinted class portrait, dot-rated ability scores, vitals/XP meters) when the area is at least 72x18, else falls back to the narrow side panel. **That dashboard has no scroll of its own** (`[`/`]` only reach `draw_side`), so every column has to fit: keep unbounded lists (titles) last and capped, and Experience above them. Foes/Adventurers/Follow render as aligned roster rows with HP meters. Lock-free, snapshot-only. |
| `worldmap.rs` | The overhead world map's derived coordinate field, streaming viewport, fog of war, POI index, and camera. Pure and process-global; see §5.1 for its invariants. |
| `svc.rs` | Authoritative runtime: service tasks, `WorldState`, player/mob state, combat, movement, following, shops, persistence, snapshots, activity events. |
| `world.rs` | Immutable world data and generation: rooms, exits, mobs, features, wildlife, minimap, overworld, Frontier. Also **resource nodes** (`NODES`/`ResourceNode`/`nodes_at`/`node_index`): trees, ore veins, fishing spots and herb/skinning patches keyed to rooms, modelled exactly like `WILDLIFE` (static data + a per-node service cooldown), each with a skill, tier, min-level gate, and derived yield item. |
| `classes.rs` | Seventeen playable classes (Warrior/Mage/Cleric/Rogue/Ranger/Druid/Necromancer/Bard/Monk/Paladin/Warlock/Berserker/**Beastlord/Skald/Runemaster/Valewalker/Spiritmaster**), resources (incl. Spirit/Souls/Tempo/Ki), passive traits, level 1-50 stat curves, XP curve. Adding a class means an arm in every `match self` here (name/primary_score/resource/tagline/description/trait_name/trait_desc/stats_at/as_key/from_key), an entry in `ALL`, an ability roster in `abilities.rs`, and (if the trait needs runtime behaviour) a hook in `svc.rs`: upkeep loop for regen (Druid/Paladin) and Tempo (Bard/**Skald** War-Chant); `kill_mob` for harvest (Necromancer/Warlock/**Spiritmaster** Spirit Siphon); `strike_player` for Monk mitigation; `spell_damage` for Mage/**Runemaster** overflow; the combat round for Berserker frenzy and **Valewalker** heal-on-hit; the pet-bite step + `fire_pet_skills`/`wound_pet` for **Beastlord** Pack Bond (empowers the taming/pets companion - stronger bite, tougher, faster auto-skills). **Every level grants something:** the curve grows each level (surfaced by `check_level_up`, which logs the concrete +HP/+attack/+resource gains per level), plus `level_milestone`/`milestone_hp_bonus` add a named milestone (Blooded…Ascended) with a permanent +HP every fifth level, a pure function of level, so no extra save state; `current_milestone(level)` shows on the character sheet. **Archetypes:** at `ARCHETYPE_LEVEL` each class offers two paths (the `ARCHETYPES` data table; `archetypes_for`/`archetype_by_key`), each carrying a `Role` (Tank/Healer/DPS) and four percent modifiers (`attack_pct`/`mitigation_pct`/`heal_pct`/`max_hp_pct`). The modifiers apply at existing combat hooks in `svc.rs` (DPS in `attack()`+`spell_damage`, Tank in `strike_player`, Healer in `heal_player`, max-HP in `max_hp()`); no engine changes; the chosen `&'static ArchetypeDef` is held on `PlayerState` and persisted by key. |
| `abilities.rs` | Ability roster and unlock helpers. Effects are data, resolved in `svc.rs`. |
| `housing.rs` | Player housing data + address arithmetic. `TIERS` (5 homes Hut→Tower: price/ground/upper rooms), the 50+-piece `FURNITURE` catalogue, `HOUSING_BASE`/`plot_base`/`plot_of_room`/`is_housing_room`. Homes are **static rooms** (generated in `world.rs::extend_housing` as Hearthward Close off Market Row); only **ownership** (`plot_owner`) and **furnishings** (`house_furniture`) are dynamic side-state on `svc.rs`, so movement/visiting/snapshot work unchanged and the homes are public shared-world plots. |
| `appearance.rs` | Character appearance/bio. `FIELDS` (Build/Hair/Eyes/Bearing/Origin/Mark/Manner, each with a menu of options) + `compose_bio`. The TUI has no free-text, so a player customises by cycling preset options (`e` opens the Appearance panel; `Enter`/`x` cycle a field). Stored as `[u8; N_FIELDS]` on `PlayerState`, persisted (new fields default cleanly for old saves), shown on the sheet and when profiling another adventurer (Follow panel). Also `portrait(class_key, sel) -> Vec<String>` (`PORTRAIT_ROWS`): a composed ASCII bust assembled from the player's own Build/Hair/Eyes/Bearing choices plus a class-flavoured headpiece (helm/hood/circlet/laurel/wild-band by class key) - pure glyph rows, coloured by `ui.rs::composed_portrait` with the class accent + per-feature tints. Shown on the character sheet (`sheet_identity`, replacing the old shared `class_portrait`), when profiling another adventurer (Follow panel, keyed by their snapshot `class_key`/`appearance_idx`), and live in the `e` appearance builder as a preview. Snapshot carries `class_key`/`appearance_idx` on `PlayerView` and `OccupantView` (lock-free/snapshot-only). |
| `archipelago.rs` | The **Shattered Archipelago** data + address arithmetic: the four portal `VILLAGES` (rooms `8000+`), the 20 `ISLANDS` theme table (rooms `20000+`), `island_entrance`/`village_room`/`is_archipelago_room`/`has_waystone`, and `portal_destinations()` (the fast-travel menu). Rooms are generated in `world.rs`; the portal teleport (`travel`) lives in `svc.rs`. |
| `pets.rs` | Combat companions. `PetSpecies` data table (`PET_SPECIES`, `pet_species_by_key`) of buyable beasts, and the live `Pet` (held on `PlayerState`, always co-located with its owner). Loyalty (earned by feeding) drives the level via a pure function; `max_hp`/`attack` scale with level. `PetSpecies` carries a closed `source: PetSource` (`Stable { price, rung }` or `Wild { tame_level }`; `rung()` places both on one companion ladder) and its own `skills: &'static [taming::PetSkill]` ladder (every pre-Aelunor species points at the shared `taming::PET_SKILLS`; Aelunor's five each carry their own, so different pets really do have different spells); `pet_species_by_key` searches `PET_SPECIES`, `taming::TAMEABLE`, **and** `taming::AELUNOR_TAMEABLE` so a saved pet of any kind reloads. The world wiring (buying/feeding/wounds, the bite each round, and the level-gated **pet auto-skills**) lives in `svc.rs`. Persisted by species key + loyalty (HP restored full on load). `Kennel` holds the owned pets not at the heel, one per species, and decides where a bought or tamed pet goes. |
| `taming.rs` | The **Animal Taming** trade. `TAMEABLE` = sixty tameable `PetSpecies` of Broceliande, small→large with `tame_level` rising 1..54 across the classic fifty and 55..100 across the ten Wildbound summit beasts (harder and harder); `AELUNOR_TAMEABLE` = five more, native to Aelunor. `wild_beasts()` places every beast in **both** pools at a real safe room (Broceliande's zone forest gates, Aelunor's zone Wood-Gates, via `world::aelunor_entrances()`) and returns one combined list; `WildBeast.species` indexes the **combined** pool (`TAMEABLE` then `AELUNOR_TAMEABLE`), resolved through `beast_species(index)` rather than indexing `TAMEABLE` directly. `beasts_at(room)` filters it. `tame_chance(xp, beast)` drives the success roll (40% at the required level, +9%/surplus level, capped 95%, 0 if under-level); `tame_xp` scales the reward. **Pet auto-skills**: `PET_SKILLS` (Savage Bite L2 / Rend L4 / Intimidating Roar L6 / Loyal Guard L8 / Killing Pounce L10, `pet_skills_at(level)`) is the shared ladder every pre-Aelunor species uses; the five Aelunor species each carry a distinct `PetSkill` array of their own (see the Animal Taming section), including a `Mend` effect (a direct heal) no earlier pet had. `PetSkillEffect` is resolved in `svc.rs::fire_pet_skills`/`fire_pet_skills_pvp`, both of which now take the firing pet's own `skills` slice as a parameter rather than assuming the shared ladder. Only data + pure maths; the action/panel/combat wiring is in `svc.rs`/`state.rs`/`ui.rs`. |
| `items.rs` | Item catalog, equipment slots, consumables, valuables, shops, generated Frontier loot. Also the **raw-material catalog** (`materials()`/`material_id`/`MATERIAL_BASE = 4000`): 5 skills x 5 tiers of gathered materials (logs/ores/fish/herbs/hides), `Valuable` kind (immediately sellable), IDs `4000..4100` (skill index x 20 + tier). The **crafted-goods catalog** (`crafted()`/`CRAFTED_BASE = 4200` + the `*_id(tier)` helpers): intermediates (ingots/planks/leather) and finished goods (weapons/armor/potions/poisons/oils/food), IDs `4200..4600` (the four oil families sit at `oil_id`, `4500..4566`). And the **Sunderlakes fish catalog** (`fish()`/`FISH_BASE = 4600`/`FISH_COUNT = 40`): 40 species with a wide sell-price spread, ~a third `Consumable` (edible), the rest `Valuable`; `fish_well_fed(id)` gives the well-fed regen for the legendary "special" fish. All chained into `item()`. |
| `skills.rs` | **Gathering skills** (`GatherSkill`: Woodcutting/Mining/Fishing/Foraging/Skinning) and **crafting skills** (`CraftSkill`: Smithing/Woodworking/Leatherworking/Alchemy/Cooking), both on one 1-50 xp curve (`xp_for_skill_level`/`skill_level_for_xp`/`skill_progress`), independent of class level and steepening past level 10. Persisted per-player as (skill key, xp) for each set. |
| `crafting.rs` | **Recipes** (`Recipe`/`recipes()`/`recipe(i)`/`recipe_indices_for(skill)`): inputs -> output, gated by a `CraftSkill` + level. 50 recipes (10 per tier x 5 tiers) that **chain** (ore -> ingot -> weapon). Data only; `svc::craft` resolves and applies them. Built at runtime and cached (inputs are `Vec`, not a leaked slice). |
| `damage.rs` | Damage schools, mob resistance/weakness profiles, damage multiplier math. Also `ZoneTheme`: the closed 16-variant theme vocabulary of the world resist/weak pass, each an exhaustive const mapping to `(resist, weak)` (see the Abilities and damage section). |
| `stats.rs` | D&D-style ability scores: 4d6-drop-lowest rolls, modifiers, the six pure hooks (`swing_pct`, `crit_pct` + `crit_outcome`, `hp_bonus`, `spell_power_pct`, `regen_bonus`, `price_pct`/`tame_pct`), the wording that explains them (`Score::rule`, `AbilityScores::effect`), and the point economy (`points_earned`, `raise`, `SCORE_CAP`, `ScoreOfferView`). |
| `persist.rs` | JSON schemas for durable character saves and shared world saves. Versioned (`SCHEMA_VERSION`); new fields use `#[serde(default)]` so old saves load (e.g. `board_progress`/`board_done` for quests). |

### Board quests [VOLATILE]

`BOARD_QUESTS` (in `svc.rs`) is a static table of bounties posted on a `FeatureKind::Board` in each capital square (Tasmania/Melvanala/Matlatesh) plus the **Kaelmyr ash-cairn board** at the Cinderfall Shore ash-gate (room `12000`, quests 17–22: reach the ash-gate, cull the cinder-dead/Emberkin/Choir, salvage shore relics, reach the Ashen King). Each has an `Objective`: `Bounty{name_contains,count}`, `Collect{item,count}`, `Reach{zone}`, or `Escort{npc,dest_zone}`, a `Repeat` (`Once`/`Daily`/`Weekly`), plus a `hint` (where the work is and how to walk there, shown on the board and in the journal), a `suggested_level` (a fair-fight estimate, `~LvN` on the posting), and `requires` (gate titles its hunting ground sits behind; empty = open country). A posting whose `requires` the player doesn't hold reads **sealed** (dim, `[sealed]` tag, via `board_quest_locked`) and `accept_board_quest` refuses it with the missing titles - a fresh adventurer can no longer carry bounties for ground that refuses them at the door. Per-player state: `board_progress` (accepted counters), `board_done` (one-offs claimed), `quest_cooldowns` (id→Unix seconds when a repeatable was last claimed), all persisted; plus a transient `escort: Option<EscortState>` (not persisted).

A board is a **picker menu** (`Panel::Board`, `BoardView`/`BoardEntryView`), not an auto-assign: selecting the board feature in Examine opens it (`state.rs`'s `Panel::Examine` arm still runs `interact_task` on a `kind == "board"` row, so the board reads out its description like every other feature, and then also calls `set_panel(Panel::Board)` - there's no key left to spare for a dedicated binding, every letter and the sensible symbols are taken, see §9). `WorldState::board_entries` lists every ready-to-claim counter-bounty for the room first, then every still-open one (`board_quest_available`); Enter on a row calls `claim_board_task`/`accept_board_task` depending on `entry.ready`. This replaced "examine silently posts the next available quest in static-list order" - a fresh adventurer could get handed a bounty for a foe several zones above them with no preview or way to decline. One-offs claim into `board_done`, repeatables into `quest_cooldowns` (re-available after `DAY_SECS`/×7 via `board_quest_available_at`). Counter progress still ticks via `bump_quests` from the kill / loot / room-enter paths, unaffected by any of this. **Escorts** spawn a transient escortee that travels with the player; it is wounded by chance when the player is struck (`wound_escort`) and lost immediately on player death; reaching `dest_zone` with it alive completes the quest (`check_escort_arrival`, in `describe_room_context`). The escortee and active board quests surface in the room panel / quest journal, each quest-journal row now with a `desc` line (`blurb` + `Objective::describe()` + `hint`) so what a bounty actually asks for is never lost once the one-time accept-time log line scrolls off.

### Starter chain, the Long Road, and the next-step line [VOLATILE]

- **Starter chain** (`STARTER_QUESTS`/`StarterGoal`/`bump_starter` in `svc.rs`): an auto-granted, strictly sequential five-step new-player line - reach Embergate → slay 3 on the King's Road → reach Whisperwood → slay the Elder Treant → reach Duskhollow - that hands a fresh character from Wayfarer's Hollow to the first real gate title. No board involved: `bump_starter_reach` fires from the room-enter path (beside the `Objective::Reach` bump) and `bump_starter_kill` from the kill path (matching `SlayIn` by the player's current zone, `SlayNamed` by the foe's name). Completing a step pays gold+xp and logs the next step; completing the chain points at the Long Road and the boards. State is `starter_stage: u8` + `starter_kills: u32` on `PlayerState`, persisted (schema v19, `#[serde(default)]`); on hydrate, pre-v19 saves at level ≥ 10 get the chain marked complete so veterans never see it.
- **The Long Road** (`LONG_ROAD`/`RoadMilestone`/`road_view`): the realm's nine-boss spine (Elder Treant → Archdemon → the three living-dark seals → the Frontier King → Yssgar → Kaethyr the Unquenched → Kaethyr Ascendant), rendered in the journal as `[x]`/`[>]`/`[ ]` rows. Purely derived from `player.titles` via `title_for(boss, true)` - no save state - so it can never disagree with what a kill actually grants; the drift test `the_long_road_matches_the_real_gates_and_tracks_titles` pins every gate const to the table. `boss` strings must match spawn names exactly.
- **The next-step line** (`next_step_for`): the active starter step, else the Long Road's first unconquered milestone. Logged as a `Next - ...` line on every join/class-choice. It was briefly also a standing `next` row in the room side panel, but a multi-sentence goal wrapped to five rows there and drowned the panel - the durable answer now lives in the journal, and the join log re-orients returning players.
- **The journal is gated by progression**: `PlayerView.quests` holds the starter step (`QuestKind::Starter`), accepted bounties (`Board`), and the twenty Frontier zone quests (`Frontier`) *only when* `frontier_open` (the four `FRONTIER_REQUIRED_TITLES`); sealed, the panel shows a single "The Frontier - sealed" line instead of twenty endgame rows drowning a level-2 journal.

---

## 3. Screen Lifecycle And Input Capture [STABLE]

- Lateania is no longer a top-level tab. It is launched from the Games hub (`late-ssh/src/app/door/hub`, page `3`), a selector that renders the selected door game's full landing; Lateania's landing is drawn by the now-`pub` `screen::draw_landing`, a single-column layout (logo, stats, actions) matching the NetHack/DCSS style, used both by the hub and the standalone screen fallback. `Screen::Lateania` is a live-world-only screen reached by pressing `Enter` on the selected Lateania card; that one keypress both switches the screen and joins the world (no intermediate standalone landing).
- `d` while Lateania is selected in the hub opens a destructive confirmation prompt to delete the current user's saved Lateania character. `Enter`/`Y` confirms; `N`, `d`, or `Esc` cancels (handled in the hub input, not the standalone landing).
- The landing's character list lives in the service (`LateaniaService::character_slots` -> `SlotList`), because the render path cannot await a query. The database is read **once per account** (`fill_slots_task`, kicked from `set_screen` into the landing or the Games hub, which is also the retry if that read fails); a read never replaces a list that is already `Ready`, so a query landing late cannot walk a fresher row backwards. Every change after that read rides the write that made it: `persist` and `delete_character_task` write their own slot's row (`publish_slot`). On top of the stored rows, `character_slots` lays any staged-but-unwritten save (`prepared_saves`, always newer than the row on disk) and then the live character's row off the world snapshot, so a character created minutes before its first save is never drawn as an empty slot. `SlotList::Loading` is its own state: an unread list reads "loading" and refuses `d`, rather than inviting you to start a new character over one it has not heard about. `App::tick` compares the list on the two screens that draw it and pays a frame when it moved, so a change made by another task is on screen within one idle tick (500ms); the `App::repaint_signal` handed to `fill_slots_task` and to a delete only makes those two immediate.
- Launching Lateania creates `lateania::state::State`, subscribes to the shared service snapshot, and joins the persistent world.
- Leaving the active Lateania world drops its per-session state. `State::Drop` sends the service leave event.
- Navigating away from the Lateania screen also drops active Lateania state.
- Lateania is not an Arcade game and should not use `App::is_playing_game`; the app tracks active state by whether `App::lateania_state` is present.

Input capture contract:
- The Lateania landing page behaves like the Arcade lobby: screen switching and global shortcuts remain available unless the landing page itself handles the key.
- Active Lateania captures ordinary key input, including number keys, `Tab`, `Shift+Tab`, `q`, and single-byte global shortcuts.
- Active Lateania still allows `Esc` to leave the active world; it now returns to the Games hub (page `3`), not a standalone landing page.
- Backtick in the active world detaches onto the workspace cycle: `input::handle_key` returns `InputAction::Detach` (after the chat-compose capture, so `` ` `` still types into a say line; no confirm gate, and mid-combat is allowed since Esc-Esc permits the same leave), `screen::handle_active_lateania_key` arms `App::lateania_detached_at` and calls `App::detach_door_game`. The hop-out is still a full leave (autosave, world removal); the armed 5-minute window (`App::lateania_recently_active`) is what keeps Lateania a stop on the cycle, and hopping back in calls `enter_lateania()` to re-join the remembered slot, skipping character select. An explicit Esc-Esc leave or a slot delete clears the window.
- Reserved/global modal shortcuts that run before screen dispatch remain allowed, including `Ctrl+O`, `Ctrl+G`, `Ctrl+/`, and other app-level modal paths.
- `?` still opens the global help modal, selecting the Lateania guide tab when the current screen is Lateania.
- Class selection is cursor-based (`w`/`s` move, Enter chooses; `1`-`9` quick-pick the first nine of the seventeen). The `draw_class_select` screen shows one row per class (it reads `Class::ALL`) plus a detail block for the highlighted one. Those keys must not switch top-level screens while Lateania is active.
- **Archetype selection** is a second one-time gate: at `ARCHETYPE_LEVEL` (10) the snapshot exposes a non-empty `archetype_choices`, which makes `draw_archetype_select` take over the screen and routes `1`/`2` to commit one of the two per-class paths. The choice is permanent and releases the gate once made.

---

## 4. Runtime Architecture [STABLE]

### Service and snapshots

- `LateaniaService::new` seeds the static world, creates the `watch` snapshot channel, starts world load, tick loop, character autosave loop, and shared-world autosave loop.
- `LateaniaService::mutate` spawns async command tasks, locks `WorldState`, applies one mutation, touches activity, and publishes a fresh snapshot.
- `WorldState` is the only gameplay truth. `PlayerView`, `MobView`, `QuestView`, `WildlifeView`, and other `*View` structs are derived snapshot data for rendering.
- `State::tick` drains the watch receiver into the session cache. UI code only reads the cache.
- `State::ensure_player_present` retries join after a short delay if the player is missing from the snapshot.

### Tick loop

Every `TICK_SECS = 2`, `WorldState::tick`:
- advances the world clock (`world_ticks`), which derives `TimeOfDay` (Dawn/Day/Dusk/Night, `PHASE_TICKS`) and `Weather` (Clear/Rain/Fog/Storm, `WEATHER_TICKS`), surfaced on `PlayerView` and shown in the room panel;
- runs the wandering world-boss lifecycle: notes when the reigning boss has died (clearing `world_boss`, scheduling the next at `+WORLD_BOSS_INTERVAL`) and raises a new one (fixed id `WORLD_BOSS_ID`, a roaming Hunter boss) only after an online player has the Archdemon title plus all three living-dark boss titles, announced server-wide via `log_all`;
- reaps runtime-only mobs (`id >= SUMMON_ID_START`: summoner adds and the dead world boss) and respawns authored mobs (resetting roamers to `leash_home` and re-hiding Ambushers);
- moves roamers (`move_roamers`): Wanderers/Patrollers drift in-zone, Hunters prowl only after dark (the world boss can roam across endgame living-dark/Frontier space at any hour);
- applies mob damage-over-time stacks and kills mobs if DoTs finish them;
- auto-releases lingering corpses to `TEMPLE_ROOM = 4` once their `respawn_at` deadline (`CORPSE_LINGER_SECS = 90` from death) passes and no one has resurrected them (`send_to_temple`);
- regenerates class resources and decrements buffs, shields, HoTs, stuns, and cooldowns;
- resolves one combat round for each engaged player, then per-mob behavior (`resolve_mob_behavior`): Caster bolts (storm-boosted), PackHunter gang-ups, Summoner adds, Brute enrage, Thief steal-and-flee, Skirmisher flee; all mob damage is scaled by `TimeOfDay::mob_damage_pct` (the dark hits harder) and Ambush reveals are fog-boosted;
- removes idle players after `PLAYER_IDLE_TIMEOUT_SECS = 10 * 60`, exporting their save;
- increments snapshot generation when dirty and drains kill outcomes for `ActivityGame::Mud`.

### Active sessions

- Active sessions are tracked per user and session UUID. Multiple sessions for the same user should not remove the player until all sessions leave. Character resets publish a per-user reset version in snapshots; any still-open Lateania session that observes its user's version advance stops auto-rejoining and tells the user to return to the Games hub, preventing an existing world screen from silently becoming a fresh class-select character.
- `State::Drop` calls `leave_task`; parent navigation away from Lateania drops active state.
- Character reset clears active sessions, removes the player, strips mob DoTs owned by that user, deletes only that user's character row, and does not wipe shared world state.
- Loading a saved character reconciles level from total XP while never lowering an already-higher saved level, so stale saves still restore current status, stats, and unlocked abilities.
- Character saves use per-user persist versions, prepared saves, and per-user persist locks so stale logout/autosave writes do not overwrite newer reset or join state. Shared-world load is skipped if live mutations already advanced `world_revision`. `flush_all()` best-effort persists present characters and dirty shared world state during graceful shutdown.

---

## 5. Input And UI [VOLATILE]

### Class selection

Before class choice:
- `1-5`: choose Warrior, Mage, Cleric, Rogue, Ranger.
- `r`: reroll 4d6-drop-lowest ability scores.
- Other ordinary game keys are ignored.

While an attribute point waits to be placed (`PlayerView::score_offer` non-empty, which the view keeps empty while the level-10 archetype crossroads is open):
- `1-6`: place the point on STR/DEX/CON/INT/WIS/CHA (`Score::ALL` order). Every other ordinary key is ignored until the points are placed, like the archetype gate; the action-bar chips are inert behind both gates too. Unplaced points are bounded by `AbilityScores::headroom` (a point with no score below `SCORE_CAP` to go in is not owed, so the gate can always be satisfied), the offer is empty while dead (the corpse view and `r` win), and the screen collapses to one line a score when the area is too short for the full five-row layout.

### Active game keys

- Movement: `w/a/s/d`, `h/l` for west/east, and arrow keys for cardinal directions; `<` or `,` for up; `>` or `.` for down.
- The Matlatesh sea-gate into the Sundered Reaches requires `Bane of the King Who Was Promised Nothing` and uses the same transient two-step warning as the Frontier descent.
- The ash-gate down from Yssgar's Reaches chamber into Kaelmyr requires `Bane of Yssgar, the Sundering Deep` (`is_kaelmyr_gateway`, `KAELMYR_GATE_TITLE`) and uses the same transient two-step warning. It is the deepest end-game gate in the game.
- The first dungeon descent from Whisperwood into Duskhollow requires `Bane of the Elder Treant`.
- Living-dark entrances from the three capitals require `Bane of the Archdemon Mal'gareth`.
- The Town Square Frontier descent requires `Bane of the Archdemon Mal'gareth`, `Bane of The Bonewright Lich`, `Bane of the Elder Dryad`, and `Bane of the Abyss-Thing`; after those title gates, it still uses a transient two-step warning: the first `>` logs that the Frontier is older, meaner country for seasoned adventurers, and the next `>` confirms descent. Service-backed non-movement actions clear the pending warning.
- Combat: `space`, `x`, or Enter attacks when not in a list panel; `z` flees.
- Abilities: `1-9` use unlocked ability slots unless a list panel is open; `0` uses slot 10. The Abilities panel is a list panel: Enter casts the highlighted ability, which is the only way to reach rosters deeper than ten (the classic classes' late slots).
- World actions: `y` works a resource node in the room (chop/mine/fish/forage/skin - the highest tier you qualify for); `u` opens the crafting panel where a craft station stands; `i` opens "the Ways" fast-travel menu when standing on a waystone portal (moved off `y`, which gather uses); `m` cycles the **map**: closed -> the graphical overhead field (§5.1) -> the **land map** (§5.2) -> closed, via `state::cycle_map` and `state::MapMode`; on the field page `x` marks the crosshair room as a destination and the room panel carries a `heading` line naming the next exit to take until you arrive, and `q` toggles the active-quest overlay (see §5.1); below 50x14 it falls back to the text **World Atlas**: `World::region_progress` scores each of the `REGIONS` for visited/total rooms + boss count and flags the region the player stands in (`RegionProgress.here`, a `◈ you are here` marker), rendered as meters + `◆N` loot markers by `ui.rs::atlas_panel`); `r` recalls to Embergate's Town Square when out of combat; `;` **retreats to the nearest safe haven** (`svc::retreat_to_haven`: a BFS over walkable exits to the closest `safe` room, refusing mid-combat and never expanding through a progression gate the player's titles wouldn't pass, via the silent `gate_blocks` twin of `can_cross_progression_gate`) - deep in a maze it reads as "back to this zone's gate"; `f` toggles the Follow panel; `g` casts the Resurrection rite on the nearest fallen adventurer in the room (Cleric/Paladin/Druid only); `p` opens the Stable (companion vendor) where one stands; `q` opens the **Animal Taming** panel where a tameable wild beast roams (Enter attempts the tame); `n` opens the housing ledger (at the clerk, or inside a home you own); `e` opens the appearance/bio builder; `o` (Examine) on a quest board reads it out and opens its picker (`Panel::Board`, see the Board quests section above) instead of assigning a bounty blind. In the Inventory panel `A`/`C`/`J` batch-sell all loose gear / commons / non-upgrades (keeping worn gear and every `Consumable`/`Utility` item - poisons included, not just potions); inventory and shop rows show both a stat-delta line and a coloured `▲+N%` / `▼-N%` upgrade tag vs. what's worn, plus the item's own description line.
- Local chat: `'` opens a **say** compose line (`state.chat_buffer`; input capture runs at the top of `handle_key`, before the Esc-leaves check, so Esc cancels compose). Enter sends via the existing world-local `say` (room occupants, `LogKind::Say`); backspace edits; the prompt renders on a reserved bottom row in `draw_page`. **Lateania chat is world-local and never reaches late.sh's global feed** (`say` only `log_to`s in-world players; it does not publish to activity/#lounge).
- While dead (a corpse): all normal keys are suppressed; only `r`/Enter (release to the temple) and `Esc` (leave) respond, until a resurrection or the auto-release deadline.
- Panels: `c` character, `v` abilities, `t` inventory, `b` shop where a merchant exists, `o` examine/look, `k` titles, `j` quest journal (a list panel: Enter tracks/untracks the highlighted quest's target - it sets the same `map_dest` the map's `x` does, so the compass line guides to it), `f` follow, `!` leaderboard.
- List panels: `w/s` or up/down move cursor; `1-9` jump and activate; Enter activates. The view auto-scrolls to keep the highlighted row within a small scroll-off margin (top and bottom).
- Cursor-less text panels (character/leaderboard): `[` / `]` scroll. Both scroll offsets share one interior-mutable `list_scroll` on `state::State`, clamped to content by the render pass and reset on panel change.
- Inventory panel: `x` sells the selected inventory row when a shop is present.
- Follow panel: Enter follows/stops the selected in-room adventurer; `x` stops following whoever is currently followed, including absent/separated targets.
- `Esc` leaves active Lateania and returns to the Games hub.

### Panels

**The room panel's shape (2026-09-16).** Three things used to compound into a panel that overflowed and lost its tail on a large terminal:
- **The rail was a flat 34 columns** above one `area.width >= 84` test, whatever the screen. `ui::side_width` now scales it (`total_width / 4`, clamped to `SIDE_WIDE`..`SIDE_MAX` 34..60, `SIDE_NARROW` below 84), because on a 190-column terminal a third of the panel's *height* was wrap damage (one exits line wrapping to three, a wildlife entry to three) while the main column sat half empty.
- **Eighteen lines of static keybinds trailed every room** (`footer_hints`), duplicating the `?` guide. What is specific to the room now **leads** the panel instead: `room_actions` renders a `You can` block right under the vitals, in a louder style (`action_hint`, green key + bright label), for the keys that only work where you stand (`g` raise, `b` shop, `p` stable, `u` craft, `y` gather, `q` tame, `i` ways, `n` ledger, `o` board, `G` mend or rouse a hurt pet). Empty in a room with none of them. `footer_hints` is now four dim compact lines of the standing keys plus `? all keys`, including the waypoint pair `: waypoint` and `/ warp`, while say (`'`), ranks (`!`) and leave (`Esc`) live only in the `?` guide (pinned by `what_this_room_offers_leads_the_panel_and_the_standing_keys_stay_short`); mid-fight it is just the four combat keys. Anything trimmed from it was added to the `?` guide (`help_modal/data.rs::lateania_help_lines`), which is the one place the full reference lives.
- **Nothing in the rail may run past its width.** `draw_room_side` paints pre-wrapped lines with no `Wrap`, so a line built without knowing the width is chopped at the edge (`f follow` rendered as `f follo`). The standing keys are therefore short `key label` chips packed to the measured width by `pack_hint_chips` (at most 8 lines, and fewer as the rail widens), and `action_hint` labels are kept short enough to fit the narrowest rail. Both the packer and anything sizing a chip against the rail subtract the same `RAIL_INDENT`. The one chip built from world data is the warp key: with a waypoint fixed it reads `/ warp <zone>` (`PlayerView::waypoint` carries the zone name, not a bare flag), trimmed with `truncate` to whatever the rail leaves it. That replaced a permanent `waypoint set (/ to warp)` line in the room panel, and took `f follow`'s chip slot; follow lives in the `?` guide (`help_modal::data`), which is where a key with nothing to report about its own state belongs. Pinned by the width sweep in `what_this_room_offers_leads_the_panel_and_the_standing_keys_stay_short`.
- **The bottom log strip was capped at 7 rows** (`height / 4`), so a tall terminal showed no more events than a laptop: a room description and one look, then the text was gone. `ui::log_strip_height` is `height / 3` capped at 12, and always under half the height so the field keeps the larger share.
- **The standing keys are anchored to the floor of the rail, not trailing the content.** `room_panel` returns a `RoomPanel { body, footer, .. }`, and `draw_room_side` reserves the footer's wrapped-row count at the bottom (`ui::footer_anchor`) so the room's state reads top-down from the vitals and the key block sits at a fixed edge, instead of the panel ending wherever the content happened to stop with dead space below. The berth is only taken while it leaves the room at least half the rail; below that the footer goes back to being ordinary content at the end of the body, where it scrolls. `foe_hits`/`player_hits` index into `body` and their click rects are measured against `body_area`, not the whole panel.
- **A key shown in `You can` is not repeated in the standing keys.** `room_action_entries` is the single `(key, label)` table; `room_actions` renders it and `footer_hints` filters its chips against it, so `o` and `i` appear once, in the loud place. `o look / interact` was also a `hint` sitting *inside* the `Of note` list, styled like one of the features you could walk up to; it is an entry in the table now (`read the board` where a board is present, since `o` opens the look list and picking the board out of it opens the board).
- **`[` / `]` scroll every panel, always, with no conditions.** `State::scroll_up`/`scroll_down` branch on one thing: a panel with a cursor (`list_len() > 0`) scrolls by **walking its cursor** one row, since the render pass already keeps the selection in view and paging it drags the view along; a cursor-less panel (room, character sheet, atlas, leaderboard) shifts `list_scroll` directly and the render pass clamps it. There is deliberately no manual-vs-auto scroll mode: one source of truth for where a panel sits, so the keys can never be a silent no-op the way they used to be in every list panel.
- **It was the only panel that could not scroll.** `draw_side` short-circuits to `draw_room_side` before reaching the scroll path, and the panel rendered into a fixed rect, so overflow was silently dropped. `ui::scroll_room_panel` now fits it: the vitals block (down to the first spacer) stays **pinned** so HP is never something you scroll to find, the rest scrolls on the same `[`/`]` + `list_scroll` cell every cursor-less panel uses, and a `+N more  ] to scroll` marker takes the last row when content is hidden. It is pure and unit-tested because the arithmetic is fiddly: it counts **wrapped rows, not logical lines**, and the marker row is only reserved when something is genuinely hidden (reserving it unconditionally pushed the last line off the final page and then reported it as `+1 more`, which no scrolling could reach). `draw_room_side` maps a line index to its screen row through `pinned`/`off` before recording click targets; getting that wrong aims clicks at the wrong foe.

- **A tutorial line that reprints forever is only a tutorial the first time.** An adoptable critter's `feed it daily (~) and it may take to you as a stray` cost two or three wrapped rows in every room that held one. `WildlifeView.adopt_streak` carries the courting progress, so the sentence shows until the first feed and then gives way to `(~ 2/5 days)`, which is shorter and says something the player does not already know. `adoptable` is also false once a stray is kept, because `~` stops courting critters then (`feed_target`) and the panel must not advertise a key that does nothing.

`state::Panel` variants:
- `Room`: current room, vitals, exits, mobs, occupants, wildlife, features, minimap, hints. The zone line carries the zone's derived mob-level band (`zone_with_band`, from `World::zone_band` - computed at seed time from the spawns, so it can't drift). Foe rows wrap the full name (no more `a scrawny …`) with a wider meter + numbers (plus `bleed xN`/`stunned` tags) on their own line. **In the field layout, while a fight is on** (a targeted mob or duel occupant), `draw_room_side` swaps the room summary for `battle_side_panel`: vitals, a Battle section (the locked foe's full name, wide meter, rank · attack school · weak/resist, afflictions), your `shield/empowered/stunned` effects, your companion with its auto-skills, the **full ability roster** (slot key, name, cost, effect, ready/dim - the detail the bottom action bar's 7-char chips have no room for), the room's other foes under "Also here", and the combat keys. Rows carry their own click actions (`Vec<(usize, ClickAction)>`): foe rows switch the lock, ability rows cast, same as their keys. The classic layout keeps the room summary here because its main column already swaps to `battle_context`.
- `Character`: class, trait, scores, stats, titles, resurrection charges.
- `Abilities`: unlocked abilities, cost/readiness/effect.
- `Inventory`: pack items plus equipped items as rows. Enter on a row is context-sensitive via `state::inv_action`: worn gear comes **off** (`unequip_task`), loose gear goes on, a consumable is used. Selling checks the `equipped` row specifically, not the item id: a worn item refuses with a reason, but a loose duplicate of that same item id sells fine while the worn copy stays on (`svc::sell`). **On terminals ≥ 100x20 it expands to a full-screen master-detail layout** (`draw_inventory_screen`, the shop screen's twin): one line a piece (name, sell value, the `▲/▼` tag or `worn`) under the collapsible headers on the left, and on the right the highlighted piece against what is worn in its slot (`InvView::worn_name`/`worn_stats`), its description, what Enter does to it (`inv_action`), and whether `x` sells it here. The sell keys are only offered as live when `view.shop` is present. The pack covers the field and is the panel opened mid-fight, so its title line carries HP, the class resource, and `fighting <foe>` (`pack_vitals_spans`), ahead of the counts so a long line clips the purse, not the fight. Both screens build their rows and detail through the shared `item_row_line`/`item_detail_lines`, so they cannot drift apart. The sidebar `inventory_panel` serves anything smaller.
- `Map`: two pages on one key, chosen by `state::MapMode`. `Field` is the graphical overhead world map (§5.1); `Lands` is the land map (§5.2). Either page falls back to the text atlas in the side panel when the body is too small (`map_fits` is 50x14, `lands_fit` is 76x12).
- `Shop`: merchant stock if `shop_at(room)` exists. **On terminals ≥ 100x20 it expands to a full-screen master-detail layout** (`draw_shop_screen`: the stock left at one line per item - name, price, upgrade tag - under the same collapsible category headers, and the highlighted piece right, stood directly against what it would replace: its stats, the worn item's name and stats, the delta, the overall percent, and the buy prompt); the sidebar `shop_panel` serves anything smaller. The side rail renders every listing as four wrapped lines, which a ten-item shop already outgrows and the market tier's stock outgrows badly. Same rows, same cursor, same keys in both renderings. `draw_shop_screen` takes its data (rows, `ShopView`, cursor, purse) rather than the `State`, so the layout renders in a test without a service behind it. Entries arrive **sorted best-first within each category** (by `Item::power`, then price) from the `ShopView` builder, not from the authored stock order. Row cells are fixed-width: sizing the name from a row's own price/tag once clipped `▲+77%` down to `▲+7`.
- `Examine`: room features; fountains can restore vitals.
- `Titles`: earned titles; selecting active title again clears it.
- `Quests`: the journal - the active starter step, accepted bounties, the Long Road, and (once `frontier_open`) the Frontier zone list. A list panel whose cursor walks the quests *and then the Long Road milestones* (`list_len` = quests + road), so w/s can scroll the whole panel even with a single active quest; Enter tracks the highlighted quest's `target` - or a crown's lair (`RoadStepView.target`, resolved once at world build by `road_targets` from the boss spawn's home room) - on the compass/map. The key hints (`w/s` move, Enter track, `j` close) sit at the *top* of the panel - the Long Road makes it the longest panel in the game, so bottom hints scrolled out of sight. **On terminals ≥ 100x20 the journal expands to a full-screen three-column layout** (`draw_journal_screen`: In progress | The Long Road | The Frontier, each column scrolled to keep the cursor visible) following the character-sheet pattern; the sidebar `quests_panel` serves anything smaller. Same cursor indices and keys in both renderings.
- `Board`: **on terminals ≥ 100x20 the board expands to a full-screen master-detail layout** (`draw_board_screen`: the postings list left, the highlighted posting's full story - blurb, task, hint, reward, sealed/READY state - right); the sidebar `board_panel` serves anything smaller.
- `Follow`: current occupants, follow target tag, stop-follow action.
- `Crafting`: recipes worked at the craft station(s) in the room; select and Enter to craft.
- `Taming`: the tameable wild beasts roaming the room (Broceliande), each with its required Animal Taming level and your odds; select and Enter to attempt a tame.
- `Leaderboard`: read-only, scrollable with `[`/`]`, opened/closed with `!` (not `?`, which late.sh reserves globally across every door game for a cross-door help overlay - a Lateania binding on `?` is intercepted before the door's own `handle_key` ever runs). Top ten currently-connected, classed adventurers by level, lifetime pvp kills, and total gold (carried + banked). Built once per `snapshot()` (`WorldState::build_leaderboard`, identical for every viewer) and shared into every `PlayerView` via `Arc<LeaderboardView>` rather than recomputed/cloned per player. This is a **live, in-session** ranking over `self.players`, not a persistent all-time record - an offline character's best-ever level doesn't appear once they log out. Unclassed characters never appear on any board.

### 5.1 The overhead world map (`worldmap.rs`)

`m` opens a full-view, biome-coloured overhead map centred on the player. It is derived, never stored: the world is deterministic, so `worldmap::derive_coords` gives every room an `(x, y, z)` once per process behind a `LazyLock`, and `LateaniaService::new` warms it (plus the POI index) on a blocking task so the first opener does not pay a world-gen on the render thread.

Two placement mechanisms:

- Procedurally-generated zones decode straight from the room id via `world::region_layout` (`id = base + zone*stride + cell`), so each zone is an exact `w x h` block at its own reserved origin and is collision-free by construction.
- Hand-authored rooms (capitals, roads, villages, housing, archipelago) have no grid and are walked out by BFS over exits, one connected component at a time.

#### Geometry and safeguards (post-unfold)

The picture is honest by construction, not by filtering. Generated zones are
reserved blocks that cannot overlap; the hand-authored core is authored so its
summed-step embedding stays truthful: each descent zone sits on its own
z-level behind Up/Down seam exits (Whisperwood 0, Duskhollow -1, the Crypts
-2..-4, the Mines -5, Frostspire climbing -4 to -3, the Citadel -4, the
Throne -5), the Wildbound Waste decodes as blocks (`world::wildbound_layout`),
and the town/road wings fan away from what they once folded over. There is no
hide/filter/exempt layer on top of any of this - fog of war and the block
margins are the only things that keep a room off screen, so what is drawn one
cell away really is a few moves away.

The safeguards, in `worldmap_test.rs` unless said otherwise:

- `no_zone_presses_against_another_it_has_no_gate_into`: the fold detector
  (`worldmap::zone_interleaves`) proves no two zones draw within one cell of
  each other while more than `FOLD_WALK_LIMIT` real moves apart. Content that
  re-folds the field fails it with a report naming both rooms, their coords,
  and the walking distance.
- `every_walkable_room_is_reachable_from_the_start_room`: the exit graph is
  connected (portal-only lands excepted), so a severed wing cannot ship;
  `world::link` backs it up by asserting on an occupied exit instead of
  skipping it silently.
- `generated_zones_are_collision_free_and_the_core_stays_tight`: no generated
  room may ever collide; the hand-authored stack tail is measured and capped.
- `one_screen_never_shows_two_reserved_blocks`: `COMPONENT_MARGIN` beats a
  full pan plus a full viewport.
- `a_foe_beyond_the_cell_window_stays_off_the_field` (`svc_test.rs`): the live
  `†`/`☺` lists are scoped by the field's cell window and nothing else.
- `ui::level_label` captions the viewed z; an open-sky zone the ladder pushed
  below z 0 (Frostspire Ascent, the Saltwind Wharves) names its own layer
  instead of reading "underground".

Invariants to keep:

- **`COMPONENT_MARGIN` must stay >= `MAX_VIEWPORT_COLS`.** Blocks are unrelated places; a seam between two of them must never share a screen. At the original margin of 4, an 80-column map showed five zones side by side with a forest slab against Embergate's town square. Locked by `one_screen_never_shows_two_reserved_blocks`.
- **The player always wins their own cell; failing that, the room matching where they stand wins.** Since the unfold the field's collisions are a handful of same-zone stacks (a named wing folded back over its own zone's side room: Frostspire 92/425, Emberpeak 77/395), 0.04% of rooms, measured by `generated_zones_are_collision_free_and_the_core_stays_tight`. `resolve_collision` (shared by `viewport_explored` and `map_canvas`) still resolves each contested cell as player-room first, then a room in the *same region the player currently stands in*, then lowest *visited* id as a last, purely-deterministic tie-break; the region preference is what keeps the answer following the player if a cross-region stack ever returns. `viewport` (fog-less, lowest id wins, no player/region context) is for dumps and tests only.
- **The map never lies about distance: no zone presses against another it has no gate into.** The old flat embedding drew places many moves apart one cell apart (the Sunken Glade corridor beside Embergate's square, Duskmire Wood draped over the descent), papered over by a live region filter plus `exempt` sets; the geometry was fixed instead and that machinery is gone. The full picture and the test suite that pins it live in "Geometry and safeguards (post-unfold)" above.
- The collision tail is measured, not assumed: `generated_zones_are_collision_free_and_the_core_stays_tight` fails if any *generated* room collides at all, and caps the hand-authored tail at 3% of rooms.

**What the map can and cannot do, measured.** Over the built world's 22,839 exits, 22,827 (99.95%) land exactly where the direction walked says they should, so the embedding does *not* lie about direction and "I walked north and the map moved me south" is not a real failure mode. What is real: **198 rooms have an exit whose two ends sit further apart in the field than a whole screen** (median ~5,600 cells, max ~60,000), because `COMPONENT_MARGIN` reserves a block per zone and almost every zone chains to the next one by a *vertical* exit. Crossing one replaces everything on screen at once. That is the whole of "you enter one node and you're in a totally different graph", and no amount of rendering fidelity addresses it - the three things that do are below, and all three work by adding information the picture structurally cannot carry.

- **A stub is sided by its exit's own `Dir`, never by the coordinate delta to its destination.** Across reserved blocks that delta carries no meaning at all: it only records which block `derive_coords` laid down first. The Timber Longhouse door faces *east* onto Hearthward Close, which sits 5,622 cells *west* of it in the field, so siding by delta drew a corridor stub to the west; the field drew its own honest east stub from `PlayerView.exits` at the same time, giving `─@─` with one real path and one phantom, indistinguishable. Players walked into the phantom. Inventing a path is the worst thing this map can do, and "I thought there was a path there that wasn't actually there" was a real player report, not a hypothetical (`a_scattered_links_stub_follows_the_exit_not_the_coordinate_delta`).
- **`Tile::Stair`** (`⇓` a way down, `⇑` a way up, `⇕` both; bold double arrows so a stair reads at a glance against the terrain, kept clear of the compass's thin `↑↓` and black `⬆⬇`, single-width with no emoji form). A flat level has no direction in which to draw a vertical link, so `map_canvas` used to `continue` past them and the map showed everything about a zone *except* the way out of it. Stairs are drawn in each room's own up-right corner cell (odd column, odd row): rooms sit on even/even and corridors on the odd cell between two of them, so every room owns exactly one free corner and no two rooms can claim the same one (`stair_corners_never_collide_with_rooms_corridors_or_each_other` pins the whole layering rule). The marker says only "there is a way through here", never what is on the far side.
- **The chain breadcrumb.** `RegionPlacement` carries `zone_count`, so the map header reads `Broceliande · zone 7 of 20 · Oakheart Grove`. A ~20-zone continent is 20 unrelated blocks in the field; the picture can never say you are 7 zones into a run of 20, and that sentence is what turns "lost in a forest" back into a position. Regions that aren't procedurally chained simply omit it.
- **The compass: marked destinations and routing** (`worldmap::route`, `state::Heading`). `x` on the map marks the crosshair room (`⚑`, outranking every other marker on that cell); the room panel then carries a `compass` line directly under `exits`, because it answers the question the exits raise: they say what is available, it says which one to take. It is always present once a destination is marked (`Dir::compass_glyph` - ↑↓→←⬆⬇, one per `Dir`, deliberately distinct from the `▴`/`▾` stair glyphs - plus a room-count, e.g. `→ the Vigil House · 4 rooms · take east`). The walk is restricted to `visited` rooms, which is what makes it honest rather than a spoiler machine: it can only retrace ground already covered, it can never reveal an unexplored shortcut, and it needs no gate check because a room can only be in `visited` if the player legitimately walked into it. It is computed client-side (the world graph is process-global in `worldmap`) and memoised on the `(standing in, heading for)` pair, so the search runs once per room actually entered rather than once per keystroke. `Heading` is a closed enum - `Toward`/`Arrived`/`Unreachable` - so a mark that can no longer be reached says so instead of showing a confident direction. There is no automatic quest-objective targeting: `BoardQuest` objectives are not exposed on `PlayerView`, only the older Frontier-zone `quests: Vec<QuestView>` list is, so a player marks their own destination with `x` rather than the compass auto-tracking a bounty - a deliberate scope cut, not an oversight.

Camera: `worldmap::MapCamera` is a pure value (offset from the player + level offset) held on `state::State`, clamped to the field's `bounds()` so panning cannot walk off into blank. `wasd`/`hjkl`/arrows pan, `<`/`>` change level, `x` marks/unmarks the crosshair room as a destination, Enter re-centres, `m` closes; the header flags a panned camera because it names where you *stand* while the inspector names the crosshair. Fog of war comes from `PlayerView.visited`, shared as an `Arc<HashSet<RoomId>>` because `State::view()` clones a whole `PlayerView` on every keystroke and every frame.

Overlay markers come from `worldmap::pois()`: `★` a zone boss (with its guaranteed drops in the inspector), `♥` a tameable wild beast. The marked destination `⚑` outranks all of them, and an active quest's target room draws a `!` (below `⚑`, above `★`) when the quest overlay is on (`state.map_quests`, toggled with `q` on the map, default on; targets come from `QuestView.target`). Border arrows carry a deliberate two-color contract: **amber arrows are the world's** (off-screen bosses/tames to go find, always on, `PAN_LIMIT`-honest), **the green arrow is yours** - only the *tracked* destination gets one, drawn by the same straight-line `PAN_LIMIT` rule as the amber ones (`quest_arrows` over `&[dest]`). Deliberately **not** route-based: `worldmap::route` refuses any destination not in `visited` (its first line), so a route-driven arrow would vanish for exactly the case tracking exists to serve - a boss you have not found yet. A straight-line direction needs no `visited` at all and is honest within a land, which is where it is drawn. **Any destination in the world can be tracked, however far**, because the arrow aims along the real walk rather than at the target's coordinates: `worldmap::track_aim` BFS-walks the shortest path over the *whole* world (not `visited`, same reasoning as above) and cuts it at the first step that leaves the player's floor or land, returning a closed `TrackAim` - `Target` (the walk stays on this floor and in this land, so aim at the destination itself), `Stair { room, climb }`, or `Crossing { room }` (a flat exit into another reserved block, e.g. Melvanala onto the Sunderlakes). The aim is therefore always inside `PAN_LIMIT` on the player's floor, the only place a direction means anything, which `track_aim_always_aims_inside_the_players_land` pins. `State::dest_track_aim` caches it on the `(standing in, heading for)` pair exactly like `route_cache` (a whole-world sweep for an unreachable target is ~10ms in a debug build, so it must never run per keystroke). Once the aimed-at room is in view its stair corner swaps its `⇓`/`⇑` for the walk's single `↓`/`↑` in the tracked colour (`ui::paint_track_aim`; a recolour alone was invisible, since every stair is already drawn in `SUCCESS` green), and a `Crossing` room's glyph just takes the colour. While the map views another floor (`<`/`>`) the arrow aims straight at the destination, as before. `track_aim` walks exits as authored and ignores title gates, so a crown behind a sealed gate is aimed at that gate; a waystone-only land (`portal_lands`) has no walk at all, so it gets no arrow. The journal's `ui::quest_place_note` names the target's region and how the walk sits relative to the player ("in Whisperwood", "- 2 floors down", "- beyond this land", "- reached by waystone"). Untracked quest targets get in-view `!` markers only - no arrows - and `quest_arrows` over all targets still supplies the count of cross-land ones, which the marker-legend line reports as "N quests beyond this land (track: j)" instead of drawing a dishonest arrow. `worldmap::poi_arrows` additionally projects every *off-screen* boss/tameable onto the map border as a direction arrow (`hug_poi_arrows` pulls them in to hug the explored cluster), but **only within `PAN_LIMIT` of the player** - since `COMPONENT_MARGIN = PAN_LIMIT + MAX_VIEWPORT_COLS`, distinct reserved blocks always sit further apart than that, so a delta within `PAN_LIMIT` is guaranteed to be a POI in the *same* block (a real spatial relationship), and anything farther is dropped rather than pointed at with a meaningless direction - the camera could never pan there anyway, since `MapCamera::pan` clamps to the same limit. **These arrows are global map only** - the live field draws none, so a glyph beside `@` can never masquerade as a movement affordance.

`Tile` has two distinct "there's more this way" stubs, not one - both a plain `─`/`│` half-stub of corridor, never an arrow (arrows read as controls; a line means "walkable path" and nothing else): `Hint` is an exit into genuine fog (unvisited), styled dim; `HintKnown` is a link to a room you've *already visited* that the flat grid can't draw adjacent to you (a scattered hand-authored branch, or a jump into a whole other reserved block - the Sunderlakes hanging off Melvanala is the canonical example), styled brighter/amber so a known non-Euclidean jump reads differently from the true edge of your exploration. This is the map's answer to "I can't tell paths from dead ends" and "the water area only shows up once I'm already there": once discovered, the connecting room now visibly says "goes somewhere you know" every time you look, rather than looking identical to unexplored fog. The footer legend (four lines: controls, symbols, markers, terrain) explains both stubs plus `@`/corridors/stairs/the reversed-cell cursor explicitly - it used to cram controls and markers onto one line and silently omit `@`/corridors/cursor from the legend entirely.

UI uses a two-column layout with compact fallback for terminals narrower than 50 columns or shorter than 9 rows. The left column splits current room context (`Now`) from newest-first action scrollback (`Recent`) with a visible divider; **while a fight is on** (a `targeted` mob, or a duel via a `targeted` occupant) the `Now` block is replaced by the **battle frame** (`ui::battle_context`): the foe's full name, its nature line (rank · attack school · weakness/resist, from the new `MobView.school/weak/resist`), wide HP meters for both sides, `afflicted:` (DoT stacks/stun) and the player's `shield/empowered/stunned` effects (new `PlayerView` fields), reverting the moment the fight ends. Note the battle frame lives in the *classic* layout's left column only - the wide-terminal field layout has no `Now` block, so there the fight reads from the side panel's foe roster (the targeted foe's expanded traits line, see the Room panel note in §5). The `Now` region wraps the room description naturally and only truncates the whole context as a last resort to preserve recent-event space. Service room-description lines use `LogKind::Room` and are filtered out of `Recent` so movement does not bury combat, loot, chat, and system events. Arrivals use compact `LogKind::Travel` breadcrumbs so Recent still shows where the player has just been. Consecutive identical recent events are collapsed with an `xN` suffix so repeated blocked-movement warnings do not flood the split.
In the Room panel, the minimap is rendered in a separate bottom-aligned side-panel region, not appended to the room detail lines; keep it anchored so changing foes/features/hints does not make the map jump vertically.
Room-panel variable text rows (zone, exits, features, foes, occupants, wildlife) should use the side wrapping helpers in `ui.rs` so long labels wrap within the side column instead of clipping against the border.
Non-Room side panels are rendered through `side_paragraph`, which enables Ratatui wrapping for long quest, inventory, shop, title, and ability rows.

### 5.2 The land map (`worldmap::land_links`, `ui::land_map_lines`) [VOLATILE]

The map's second page, and the only view that answers **"how do I get there"**. The overhead field (§5.1) is deliberately local: `COMPONENT_MARGIN` is wider than any terminal, so two regions can never share a screen. The text atlas is a flat list with no links. Neither draws a road.

The eighteen atlas regions are drawn as an **atlas**: every country sits at a hand-set place on a character grid, the two hubs every road runs through are walled keeps, and each road between two lands is a line joining them.

```
THE LANDS OF LATEANIA   5 of 18 walked
Every line is a road you can walk. Numbers are zones you have entered.

    Aelunor  0/12 ── Silvael ──╮
    Wildbound Waste  0/3 ──╮   │               Wayfarer's Hollow ──╮
   Sunderlakes  0/14 ──╮   │   │       ╭── City Districts ──╮      │
                    ╭──┴───┴───┴───────┴──╮             ╭───┴──────┴──────╮
 Broceliande  0/20 ─┤      OVERWORLD      ├─────────────┤    EMBERGATE    │
                    ╰──┬───┬───┬───────┬──╯             ╰──┬───────┬──────╯
    Sunken Catacombs ──╯   │   │       │  Frontier  3/20 ──╯       │
       Thornwood Hollows ──╯   │       │        Hearthward Close ──╯
             Drowned Caverns ──╯       │
                            Sundered Reaches  0/20
                                       │
                                       │
                                 Kaelmyr  0/20

Only the Ways reach:  Portal Villages · Shattered Archipelago

walked  ·  not yet  ·  where you stand
```

Three earlier attempts are worth knowing about, because all three failed the same way and the next one will too. An **indented tree** was correct and unreadable: the Overworld's ten roads out rendered as a vertical list, exactly what the picture exists to avoid. A **fan** drew a bare `|` down the middle with unattached names either side, which reads as three unrelated columns. A **trunk with spurs** attached each name to the trunk with its own rule, which fixed the attachment but still stacked the Overworld's spurs into a column. The lesson: *a layout algorithm cannot draw this world*. One node holds ten of the sixteen roads, so any generic arrangement collapses into a list. The map is therefore drawn by hand and checked by machine.

Load-bearing decisions, all of them deliberate:

- **The placement is authored; which lands touch is not.** `KEEPS`/`PLACES`/`ROADS` in `ui.rs` are a hand-set picture: rows, columns, and the legs each road runs. But `ROADS` may only name a pair `worldmap::land_links` derives from the room graph, and must name every such pair. `the_atlas_draws_every_road_in_the_world_and_invents_none` compares the two sets and fails the build if they ever disagree, so the map can neither invent a road nor lose one. **Adding a country means finding it a place in `PLACES` and a road in `ROADS`** - which is the point: a map nobody placed it on is a map that quietly dropped it.
- **It reads the room graph and region membership, and nothing else.** `derive_land_links` records an edge wherever one room's exit lands in another region (`region_atlas_entry`). It never looks at a title, a boss, or a level band, so it **cannot drift out of step with `can_cross_progression_gate`**, and it cannot leak a gate rule. Kaelmyr simply sits at the bottom of the long road south under the Sundered Reaches; a player who walks to the Sundering Deep and finds Yssgar in the doorway draws their own conclusion. Naming the gate would answer the question before it was asked.
- **The two hubs are keeps, and that is what makes the shape drawable.** The Overworld carries ten roads and Embergate five. A bare name has one row above and one below; a box has a whole wall, so each road leaves at its own junction (`┬`/`┴`/`├`/`┤`, worked out in `Canvas::stroke` from whatever the road lands on) and no two spurs share a line. `City Districts` sits between the two keeps because it opens off both of them - the one cycle in an otherwise tree-shaped world.
- **North of the road is the country you walk to; south of it is the dark.** The living-dark dungeons, the Frontier, the Reaches and Kaelmyr all hang below; the Sunderlakes, Broceliande, Silvael/Aelunor, the Wildbound Waste, Wayfarer's Hollow and the Close sit above. Nothing enforces it but the layout, and `the_atlas_lays_the_realm_out_the_way_it_is_walked` pins it.
- **A chain reads outward from the hub.** Aelunor is reached through Silvael, so it is drawn the far side of it (`Aelunor ── Silvael ──╮`, the road arriving on Silvael's side); the Reaches and Kaelmyr stack downward for the same reason. Distance on the page is distance from the hub, not reading order.
- **Every land is named, walked or not.** The name of a country was never the secret; the road to it is. Undiscovered lands render faint, not as `???`, and a road runs faint until both lands it joins are walked. The footer legend says what the three colours mean, since colour is the only thing carrying it.
- **Depth is in zones, not rooms, and is a number rather than a bar.** `RegionProgress.chain` counts zones with at least one visited room, from `LAND_CHAINS` (names written out, every number read from the generator's own consts). A country can be three zones deep on 2% of its rooms, and depth is the number that says how far in you are. A land absent from `LAND_CHAINS` draws with no depth at all. A shaded `▓░░░` bar was tried and removed: a run of block glyphs renders as one solid rectangle in plenty of terminal fonts, so it read as noise rather than progress.
- **Labels are anchored to their road, not to a column.** `At::Ends`/`Starts`/`Centered` fix the end that meets the road, so a counter gaining a digit (`3/20` -> `20/20`) pushes the name *away* from the road rather than shoving the road it is attached to. Both `the_land_map_stays_inside_the_narrowest_terminal_it_draws_into` and `no_land_on_the_atlas_is_written_over_by_another` render the fully-walked atlas as well as the fresh one, so a two-digit counter cannot silently overflow or overwrite a neighbour.
- **Names are shortened to fit** (`land_chip_name`: `land_label` plus a leading `The` stripped). The picture is a fixed 76x13 grid ending at column 74, so `lands_fit` (76x12) refuses to draw it into anything narrower and the text atlas serves instead, the same way the overhead field falls back below 50x14.
- **The scroll hint is only shown when there is something below the fold**, since a hint for a key that does nothing is worse than no hint. That is the only thing `land_map_lines` uses its `height` argument for.
- `LAND_LINKS` is a `LazyLock` forced by `worldmap::warm()` alongside the coordinate field and the POI index; a player never pays for it on the render thread.
- `land_map_lines` is split from `draw_land_map` so the layout is read in tests rather than only on a screen. Pinned by `the_land_graph_is_read_off_the_room_graph_and_covers_every_region` (`worldmap_test.rs`) plus six in `ui_test.rs`.
---

## 6. World And Content [VOLATILE]

### Room graph

- `World` is immutable after seeding: `rooms`, `spawns`, and `start_room`.
- `RoomId` is `u32`. Exits are `HashMap<Dir, RoomId>`.
- `Dir` supports cardinal and vertical movement. `Dir::delta_2d` returns `None` for up/down because minimap is flat.
- `World::minimap` BFSes visited rooms around the current room, draws visited/current/frontier/corridor cells, highlights the previous room plus connector when available, and separately flags vertical exits.

### Authored and generated areas

- Base authored path starts in safe Embergate and descends through King's Road, Whisperwood, Duskhollow Caverns, Drowned Crypts, Emberpeak Mines, Frostspire Ascent, Sunken Citadel, and Obsidian Throne.
- Embergate's west temple path is intentionally a safe sanctuary endpoint, while the Town Square down stair is signposted as sealed old danger/Frontier access so it does not read like a normal early side path.
- `extend_world` adds authored deeper exploration wings.
- `extend_overworld` adds 100 rooms including Greatroad, Tasmania, Melvanala, Matlatesh, Sapphire Coast, Verdant Highlands, Mistfen, Fungal Hollow, Sahra Wastes, Amber Savanna, and Skyreach Mesas.
- The Mistfen sinkhole is signposted as a Fungal Hollow side-delving, not a relic altar or empty hole.
- Safe capital squares are `TASMANIA_SQUARE = 620`, `MELVANALA_SQUARE = 660`, and `MATLATESH_SQUARE = 720`. Each must remain safe and carry a fountain plus dedication plaque.
- `extend_frontier` adds 20 Frontier zones. Each zone is a 10 by 5 grid with a safe entrance cell, regular mobs on even-indexed cells, a boss in the last cell, generated names/descriptions, and down/up links between zones.
- Frontier remains hung off Embergate's Town Square for reachability, but its exit label renders as `down (dangerous Frontier)`, entry is gated behind the Archdemon title plus the three living-dark boss titles, and the Town Square/class-choice guidance points new players toward the South Gate first.
- `extend_wildbound` adds the Wildbound Waste (rooms 30000+), Lateania's first **pvp** continent: three chained biomes behind three small gate towns, hung off the Sahra Wastes' Sand-Wyrm's Maw. Ungated, no title required - see §6 "The Wildbound Waste (pvp)" below for the full shape and §7 for the combat rules.
- Wayfarer's Hollow (rooms 40000+, `TUTORIAL_BASE`) is the new-player tutorial zone every brand-new character actually spawns in - see "Wayfarer's Hollow (new-player tutorial)" below.

### Features

- `FEATURES` contains lookable room features.
- `FeatureKind::Fountain` restores HP/resource and refreshes veteran resurrection charges only when examined in a safe room.
- `FeatureKind::Bank` toggles deposit/withdraw of all carried gold at the Embergate banker's grille. Banked gold is safe from death loss but must be withdrawn before shopping.
- `FeatureKind::Stable` (one per capital) is the **companion vendor**: `p` opens the Stable panel where `Enter` buys the selected beast and its hint points at `G` for feeding. **The Stable has no feed key of its own**: it used to have `x`, routed through `~`'s stray-first choice, and Embergate's Stable shares the square with two adoptable critters, so a healthy pet could never be fed there. `G` (`feed_companion`) is always your own companion (pinned by `feed_companion_feeds_a_healthy_pet_even_with_a_stray_in_the_square`). `room_has_stable` gates `buy_pet` (feeding works anywhere with `~`). All four Stables sell the same `pets::PET_SPECIES`, cheapest and weakest first, each a `PetSource::Stable { price, rung }` priced to match the wild beasts at its rung: War Hound (120g, rung 4), Dire Wolf (320g, 9), Moor Hawk (650g, 12), Cave Bear (1100g, 16), Sabre Cat (2000g, 20), Emberdrake (3200g, 26), Ironhide Rhino (5000g, 30), Stormhide Basilisk (8000g, 35). Gold buys a real mid-game pet; rungs 36-100 stay taming-only. `buy_pet` refuses a wild species key (`PetSpecies::price` is `None` for them). **Adding a feature shifts `features_at` indices; tests must find features by kind, not position** (a stale hardcoded index broke the bank test when the stable was added).
- `FeatureKind::Housing` (the clerk at Hearthward Close) is the **housing ledger**: `n` opens it. At the clerk it lists **deeds** (`buy_deed` claims a free plot of that tier; one home per name); inside a home you own it lists the **furniture catalogue** (`buy_furniture` places a piece in the current room, shown to everyone via the room description). Placed furnishings live in `house_furniture` keyed by room; ownership in `plot_owner` keyed by tier/plot index.
- **Interactable features stand out by colour** (`ui.rs::interactable_color` + `is_actionable_feature`): things you *act on* (fountain green; bank/board/stable/clerk gold + bold + a `◆` marker) pop like loot, while purely lookable scenery (plaque/vista) reads a softer cyan with a `·` marker.
- `FeatureKind::Portal` is a **waystone**: `i` opens the fast-travel menu (`world::waystone_destinations()` = `CONTINENT_WAYSTONES` - Embergate's square plus each continent's safe gate room (Sunderlakes landing, Broceliande forest gate, Reaches sea-gate, Kaelmyr's Cinderfall Shore) - followed by `archipelago::portal_destinations()`, the villages + island landings); `travel` teleports out of combat. **Gated continents keep their locks through the Ways:** each `CONTINENT_WAYSTONES` entry carries the walking gate's required title (a drift test in `svc.rs` pins them to the gate consts), the snapshot marks locked entries `sealed` (rendered dim), and `svc::travel` re-checks the title server-side. Portal features for the runtime rooms are synthesised in `waystone_features()` (a `OnceLock` layer over the static `FEATURES`), since those rooms are generated, not authored.
- Plaques and vistas are descriptive.
- Room descriptions intentionally mention only feature names; the detailed text is revealed by `o` / Examine.

### Wildlife

- `WILDLIFE` is separate from combat mobs.
- `CritterKind::Skittish` is ambient.
- `CritterKind::Game` can be hunted by attacking when no combat mob is present. Hunted game grants small XP and is hidden by a per-world 40-second cooldown keyed by global wildlife index.
- `CritterKind::Boon(Perk)` applies on room entry. Perks are `Embolden`, `Mend`, and `Quicken`. `Mend` heals to full in one visit (not a small top-up) - it used to grant only `max_hp/8 + 2`, which meant walking in and out of the room repeatedly just to fully heal.
- Wildlife appears in the Room panel; game critters show as huntable only while off cooldown.
- **Genesys stray adoption** (`feed_wild_critter`, feeding an adoptable critter `STRAY_ADOPTION_DAYS` days running to win it over) tracks **real calendar days at UTC midnight** (`now_unix_secs() / 86_400`), a completely different, much slower clock than the visible in-game Dawn/Day/Dusk/Night cycle (`TimeOfDay`, ~16 real minutes per full cycle) - the two read as "a day" in casual conversation but are unrelated systems, which confused players. Every feed message now spells out a concrete countdown to the next UTC reset via `time_until_next_utc_day()` rather than just saying "today"/"tomorrow" and leaving the boundary to be guessed.
- `TimeOfDay` also carries a `glyph()` (the same `●○` dot family the character sheet's ability scores use) and `is_dark()`, both surfaced on `PlayerView` (`time_of_day_glyph`, `time_of_day_dark`) so the room panel's world-clock line reads at a glance and turns a danger colour during dusk/night (when `mob_damage_pct` is 125%), instead of being indistinguishable dim flavour text.

### Gathering and skills [VOLATILE]

- Five gathering trades (`skills::GatherSkill`) - Woodcutting, Mining, Fishing, Foraging, Skinning - each levelled 1..=50 on its own steepening xp curve, tracked as a `skill -> total xp` map on `PlayerState` and persisted (schema v12).
- `world::NODES` seeds harvestable nodes (trees/ore veins/fishing spots/herb & skinning patches) across the overworld, tiered 0..5 by area difficulty (roadside starters near town; the best materials deep in the harder wings and capital waters). Each node has a min skill level and a yield item. **Two node constructors:** `node(...)` derives its yield from `(skill, tier)` via `items::material_id` (the classic tiered materials); `node_yielding(..., yield_item, ...)` stores an **explicit** catalog item id. The Sunderlakes fishing spots use `node_yielding` to hand out a specific one of the 40 fish species (ids `4600..4700`), gated by Fishing level — the gather flow (`svc::try_gather`) reads `yield_item` directly, so no new mechanic is needed. The node test exempts fish-yielding nodes from the derived-material check.
- `y` works the highest-tier node in the room the player qualifies for (`svc::gather`/`try_gather`): it grants the raw material to the pack plus skill xp, then depletes for `NODE_RESPAWN` (45s, tracked in `WorldState::gathered`, mirroring `hunted`). Under-skilled or regrowing nodes log why and yield nothing. No combat and no safe/unsafe gate - gathering works anywhere a node stands.
- Raw materials (`items::materials`, IDs `4000..4100`) are `Valuable` today, so they are immediately sellable ("tradeable"); the crafting update turns them into gear/consumables and further recipe chains.
- The Room panel shows a **Resources** section (like Wildlife) with a `◆`/`·` marker per node and a gatherable/reason tag; the character sheet + narrow panel show a **Trades** block (each skill's level and progress) with the `y` hint.

### Crafting [VOLATILE]

- Five crafting trades (`skills::CraftSkill`) - Smithing, Woodworking, Leatherworking, Alchemy, Cooking - level 1..=50 on the same curve, tracked as a separate `craft_skills` map on `PlayerState` and persisted (schema v13).
- `world::FEATURES` places the five **craft stations** (`FeatureKind::CraftStation(CraftSkill)`) in Embergate's Market Row (room 3): a forge, workbench, tannery, alchemy lab and cooking fire. `craft_stations_at(room)` gates crafting and builds the panel. Stations read as actionable gold in the room (`ui::is_craft_station`).
- `u` opens the **Crafting** panel (`Panel::Crafting`) where any station stands; it lists every recipe worked at the stations here, each flagged craftable/gated (station + skill level + materials). `Enter` crafts the selected recipe (`svc::craft` / `craft_task`): it consumes the inputs (`PlayerState::consume`/`item_count`), adds the output, and trains the craft skill. Recipes **chain** - smelt ore -> ingot, then forge ingot + plank -> sword.
- Crafted outputs are ordinary items, so they equip / are consumed / sell through the existing systems (weapons & armor equip, potions & food heal/restore, poisons and oils coat the weapon - see Crafting depth below).
- The **Trades** block shows all ten trades (gather then craft); the recipe `inputs` are summarised as `"3x Copper Ingot, 1x Oak Plank"`.

### Crafting depth [VOLATILE]

- **Weapon coats (poisons + oils)**: using a crafted poison (`items::poison_tier`) or one of the four **weapon oils** (`items::oil_id`/`oil_school_tier`/`OIL_SCHOOLS` = Fire/Frost/Holy/Lightning, 6 tiers each, Alchemy recipes with a school-flavored second ingredient) routes out of the normal consumable path in `use_item` and coats the weapon - `PlayerState::weapon_coat = Some((school, per_tick, charges))` (transient, one slot: any new coat replaces the last; `POISON_CHARGES = 5`, `OIL_CHARGES = 12`). Each landed melee strike seeds a DoT of the coat's school via `seed_mob_dot`, which bakes the foe's resist/weak multiplier into the per-tick up front, so a matched oil is a real matchup lever, and spends a charge. Coats seed pvp dots in duels the same way, and the active coat shows in both battle panels' `you:` effects line via `PlayerView.coat` ("fire coat x8"). Oils are always flat riders added to the Physical auto, never a conversion of its school and never a multiplier on `attack()` - that line is reserved for the planned Thundersmith class (THUNDERSMITH.md).
  - **One wound per coat, refreshed** (`svc::DotSource`): a coat re-seeds on every landed strike, at the very cadence its DoT ticks, so it keeps a single stack per attacker and refreshes it in place. Ability DoTs still stack, because a cooldown rations how often they can be cast. This is load-bearing, not housekeeping: while coats pushed a stack per strike, `POISON_DOT_TICKS` of them were live at once and every coat was silently worth three times its rider (`a_coat_keeps_one_refreshing_wound_however_many_swings_land`). The wound's source is persisted (`SavedMobDot::from_coat`) so a reload cannot untag a live coat and let a second stack open beside it. Only the opening of a wound is logged, never a refresh.
  - **Two shapes, priced against the same bar**: `svc::TIER_ATTACK_BAR` is a *measured* auto-attack yardstick (a real character at each crafting gate wearing that tier's crafted weapon, pinned by `the_attack_bar_still_matches_a_real_character`), and both coat curves are written as a share of it. The oil sustains ~20% of the auto for 12 strikes; the poison bursts ~30% of it for 5, costs two herbs against the oils' three items, and owns the one school no oil covers. So a vial is roughly three quarters of an oil's damage in half the window for two thirds of the materials - cheaper, shorter, sharper, never simply worse. `the_coat_curves_stay_inside_their_share_of_the_bar` holds every tier of both curves to its band, which is what stops a curve quietly outgrowing the attack curve (it had: the oil rider was documented at 15% of output while really running three to six times that).
- **Cooking buffs**: eating crafted food (`items::food_tier`) heals/restores as a normal consumable *and* pushes a `HealOverTime` self-effect (well-fed regen, `WELL_FED_TICKS`), reusing the ability HoT tick.
- **Masterwork sinks**: two Legendary smithing recipes (`items::masterwork_id`, level 45) consume a heap of top-tier intermediates (8-10 mithril ingots + ironbark planks / dire leather) for gear a clear step above the tiered craftables - the endgame material sink.
- None of this adds save state (`weapon_coat` is transient); no schema bump.

### Animal Taming [VOLATILE]

- **The trade.** `Animal Taming` is an eleventh trade (`skills::TamingSkill`), levelled 1..=100 (`SKILL_MAX_LEVEL`) on the same shared curve as gathering/crafting. Its xp is a single `taming_xp: i64` on `PlayerState` (there is only one taming trade), persisted (schema **v14**, `#[serde(default)]`), shown as the last row of the character-sheet **Trades** block.
- **The sixty (plus five) beasts.** `taming::TAMEABLE` is sixty tameable `PetSpecies` ordered **small → large** (hare → hedgehog → … → wolf → direwolf → cave-bear → jungle-drake → … → treant/World-Oak scion, then the ten Wildbound summit beasts, palfrey → Aurora Worldserpent). Each is a `PetSource::Wild { tame_level }` whose level **rises across the list** so taming gets harder and harder: the classic fifty span 1..54 (two per level from 46 up, one hitter and one wall: Green Wyrm of the World-Oak 39/580 and Scion 35/720 at 54) and the Wildbound ten 55..100. Stats scale with size (a bigger beast is a stronger companion), under the one-ladder rule in §9 (`no_companion_is_out_classed_by_an_easier_one`). The Wildbound beasts are ordinary companions: there is no riding. `taming::AELUNOR_TAMEABLE` adds five more, native only to Aelunor: a Faerie (Mend + a haste-ish buff), a Sapling-kin (a slow-tanky Guard specialist), an Owl (a Rend/burst hybrid), a Fox (fast, high Pounce uptime), and a Hound (a Roar/empower specialist) - each with its **own** `PetSkill` ladder rather than the shared one, so "different spells and abilities" is real per-species data. `wild_beasts()` homes every beast in **both** pools at a real safe room - Broceliande's zone forest gates, Aelunor's zone Wood-Gates (`world::aelunor_entrances()`, never offset 0 - see that function's doc comment for why a cavern zone's cell 0 is always solid rock) - spread by difficulty across their region's zones, and returns one combined list; `WildBeast.species` indexes the combined pool and must be resolved via `beast_species(index)`, never `TAMEABLE[index]` directly. **Keys are `wt_*`/`ae_*` and persisted — never reorder/rename.**
- **The action + panel.** Standing where a tameable beast roams, `q` opens the **Taming panel** (`Panel::Taming`, `TamingView`/`TameEntryView`): it lists the beasts here with each one's required level and your live odds (or "needs Taming N" / "spooked"). `Enter` attempts the selected tame (`svc::tame`/`tame_task`). Success is a roll against `tame_chance` (40% at the required level, +9% per surplus level, capped 95%; refused outright when under-level). On **success** it trains Animal Taming xp and the beast joins you through `Kennel::adopt_tamed`: at your heel if it is free, else home to the **kennel**, and not at all when you already own that species (the tame then only trains the trade). **A tame never displaces the companion at your heel**, so the trade can be farmed on small beasts without losing a leveled pet. Tamed pets use the same runtime `Pet` (fights/fed/persisted identically). On **failure** the beast bolts and is `spooked` for `TAME_COOLDOWN` (30s, per-player-per-beast in `tame_cooldowns`). Clear log feedback throughout ("eyes you warily…", "You've earned its trust!", "shies, then bolts into the briars"). The room panel shows a **Wild beasts** section (`◾`/odds + the `q` hint). None of these beasts are ever sold at a Stable - taming in the wild is the only way to get one, Aelunor's five included (Silvael's Beastkeeper's Hollow says so outright).
- **Pet auto-skills.** A companion unlocks abilities as it levels, firing **automatically** in the combat round on their own cooldowns, resolved by `svc::fire_pet_skills`/`fire_pet_skills_pvp` in the pet-bite step against **that pet's own `species.skills` ladder** (cooldowns keyed by `world_ticks` in `pet_skill_cd`, lock-free/snapshot-only). Every pre-Aelunor species shares `taming::PET_SKILLS`: **Savage Bite** (L2, bonus damage), **Rend** (L4, a `seed_mob_dot` bleed), **Intimidating Roar** (L6, owner empower), **Loyal Guard** (L8, owner shield/splash mitigation), **Killing Pounce** (L10, heavy burst that can finish the foe → credits the owner), all reachable within `PET_MAX_LEVEL` (10). Aelunor's five each carry a distinct ladder built from the same effects plus a new one, **Mend** (a direct heal on the owner via `heal_player`, magnitude scaling with the pet's attack) - the fae/druidic pets' signature. Magnitudes scale with the pet's attack. Unlocked skills surface in the room-panel pet line and the snapshot `PetView.skills`.

### The Wildbound Waste (pvp) [VOLATILE]

- **First real pvp.** Before this, Lateania was pure PvE - no player could target another. `Room::pvp` (never `true` together with `safe`) marks a room as contested ground; `svc::engage_player`/`engage_player_task` let a player lock onto another adventurer standing in the same `pvp` room, mirroring `engage_mob`. The victim auto-retaliates (`pvp_target` set both ways) if they weren't already mid-fight with anything. `PlayerState::in_combat()` (`target.is_some() || pvp_target.is_some()`) is what movement/recall/waypoint/travel actually gate on now - a duel holds you in place exactly like a mob fight always has.
- **The exchange.** The tick's pvp-fighters pass (right after the mob-fighters pass) resolves one auto-attack per engaged pair per tick, reusing `attack()`/`strike_player` as-is (`strike_player`'s `mob_name` argument is just a display string, so a player-vs-player blow needed no changes there). Rogue opening strike, Berserker frenzy, and Ranger's wounded-target bonus all still apply to the attacker; armor mitigation, shields, Monk/Tank mitigation, the Warrior death-save, and veteran in-place resurrection all still apply to the victim exactly as they do against a mob.
- **`target` and `pvp_target` are mutually exclusive, and both halves are load-bearing.** `engage_player` clears `target`; `set_target` (shared by `engage`/`engage_mob`) clears `pvp_target` and logs "You break off the duel." Every resolver reads pvp first, so a player holding both at once would have their abilities damage the rival while `Stun` landed its stun on the mob - and in the Waste, where contested ground and the mob roster share the same rooms, one auto-attack keypress used to be enough to get there (`locking_onto_a_mob_breaks_off_the_duel_so_abilities_hit_the_mob`).
- **Abilities and pets also reach a `pvp_target` now.** `damage_target` (used by `Strike`/`Finisher`) checks `pvp_target` before `target` and routes through the new `strike_pvp_target` (a thin wrapper around `strike_player` that also handles pvp kill-crediting - shared by the auto-attack pass, abilities, pet bites, and pvp dots, so that logic lives in exactly one place). `DamageOverTime` seeds a `pvp_dots` entry (parallel to `mob_dots`, but keyed by victim id and carrying its own `DamageType` per stack, since a player's `strike_player` needs the real school every tick rather than a resist/weak multiplier baked in once); the tick resolves it the same way as mob dots, just through `strike_pvp_target`. `Stun` inserts into `pvp_stuns` (parallel to `mob_stuns`), checked at the top of each attacker's own turn in the pvp-fighters pass - a stunned adventurer skips their swing that round, same as a stunned mob does. A companion's bite and its unlockable auto-skills (`fire_pet_skills_pvp`, a pvp sibling of `fire_pet_skills`) fire against `pvp_target` too: `SavageBite`/`Pounce`/`Rend` route through `strike_pvp_target`/`seed_pvp_dot`; `Roar`/`Guard` are pure self-buffs and needed no changes. Coated weapons (`weapon_coat`, poisons and oils alike) work in duels too: the pvp auto-attack pass seeds a `pvp_dots` entry of the coat's school and spends a charge, mirroring the mob path.
- **Death and spoils.** A real pvp kill (not a death-save or a spent veteran charge) reuses the ordinary death path unchanged - the victim becomes a corpse, keeps `CORPSE_LINGER_SECS` to be resurrected or release, and loses the same `carried_gold_death_loss` cut as any death. The only pvp-specific step is crediting that lost gold to the killer (diffed before/after the `strike_player` call, not a new field) alongside a flat xp bonus, incrementing persisted `pvp_kills: i64` (schema **v18**, `#[serde(default)]`), and awarding the reaver title track (`pvp_title_for`: Blooded → Reaver of the Waste → Dread of the Wildbound → Warlord of the Waste → Deathless Sovereign of the Waste at 1/10/50/150/500 kills) via the existing `award_title`.
- **UI.** `OccupantView` gained `attackable`/`targeted`; the "Adventurers here" list marks a hostile row (`hostile`/`duel` tag, a `»` marker on your current target) and, only in a `pvp` room, is clickable (`ClickAction::AttackPlayer`) exactly like a foe roster row. `PlayerView.pvp`/`pvp_kills` expose the room flag and lifetime count. `OccupantView` also carries `level` now: both the "Adventurers here" list and the Follow panel prefix the name with `Lv{N}` (`roster_row`, same convention `MobView` already uses) and append a three-letter class abbreviation (`ui::class_abbrev`, hand-picked - not a naive truncation, since e.g. Warrior/Warlock would otherwise both read "WAR") into the status-tag slot, combined with any active status (`hostile·WAR`) rather than replacing it - so a foe's kit is visible before you ever engage.
- **The continent.** `extend_wildbound` (rooms 30000+) builds three chained biomes - Duskmire Wood (cavern, levels ~13-60), the Hollowdeep (braided maze, ~44-67), the Scorched Flats (cavern, ~65-83) - each ending in one named apex boss (Wychelm Sovereign / Deathless Warden / Apex Sandwyrm, **levels 62 / 70 / 85** derived from their `boss_stats` through `Spawn::level()`; an earlier note here claimed 65/78/100, which overstated all three and wrongly implied the Sandwyrm was the world's ceiling - Kaelmyr and the deep Reaches are, see §7), carved with the same `carve_maze`/`carve_cavern` never-a-grid machinery as Broceliande. Every field room is `pvp: true`; the only havens are three small four-room gate towns (Last Watch, Barrowgate, Ashhold), one per biome, chained gate → field → next gate. Regular mobs are 20 base creature names per biome crossed with a shared five-tier affix ladder (`Lesser/·/Greater/Elder/Ancient`) - a 300-entry template pool, ~600 live spawns. Loot borrows `frontier_loot` at an offset tier per biome rather than a bespoke catalog (`broceliande_loot`'s shortcut): every table, **the apex boss's included**, is `loot_base + n`, the boss one affix ladder past its own deepest regular (tiers 5 / 12 / 18), and `wildbound_loot` clamps at `FRONTIER_TIERS - 2` so the Waste can never reach the catalog's top table. That clamp is load-bearing - the boss branch used to return `FRONTIER_TIERS - 1` outright, so all three apexes dropped the King Who Was Promised Nothing's own tier, guaranteed (`roll_loot` never rolls for a boss), off a 1500hp Duskmire mob reached by an ungated walk at gentle overworld multipliers. Pinned by `a_wildbound_apex_boss_pays_off_its_own_biome_not_the_frontier_crown`. Hung off the Sahra Wastes' `WILDBOUND_GATEWAY` (room 751, the Sand-Wyrm's Maw) by a plain walk south; `Last Watch, the Wildbound Waste` is a `CONTINENT_WAYSTONES` entry with no title gate.

### Wayfarer's Hollow (new-player tutorial) [VOLATILE]

- **Where new characters actually land.** `join()` places a brand-new character at `world::tutorial_start_room()` (== `TUTORIAL_BASE = 40000`), **not** `World::start_room` directly - `World::start_room` stays Embergate's square (1) unchanged, so map anchoring, `recall`, the temple, and every other "home is room 1" assumption elsewhere is untouched. A returning character's saved room (`hydrate`, from `SavedCharacter.room`) is unaffected either way. `choose_class`'s welcome message was updated to describe the Hollow and explicitly say `r` (recall, already game-wide and unmodified) leaves for Embergate anytime - that's the whole "leave with a key anytime" mechanic; no new key was added.
- **Layout.** Five hand-authored rooms, hung off the Gilded Flagon (room 2, `Dir::North`) rather than the square itself - room 1 has no free direction left (`Dir::Down` is the Frontier hookup, `Dir::Up` is the city-district hookup, both inserted at runtime by their own `extend_*`). The hub (`TUTORIAL_BASE`) is safe and spokes to: the Training Yard (`+1`, `safe: false` so its dummy is fightable), the Gathering Glade (`+2`), the Hall of Callings (`+4`), all safe, plus the Tinker's Hall (`+3`, `Dir::Down` from the hub) also safe.
- **What each room teaches.** The Training Yard homes one `MobSpawn` (`id 40000`, "a straw training dummy", `damage: 1`, `max_hp: 60`, 15s respawn) - tuned so it can never meaningfully hurt a fresh level-1 character, just long enough to practice engage/abilities/flee. The Gathering Glade has one tier-0 `NODES` entry per `GatherSkill` (all five, `level_req: 1`) so `y` can be tried immediately. The Tinker's Hall has one `FeatureKind::CraftStation` per `CraftSkill` (all five), a scaled-down mirror of Market Row's real ones, so `u` opens the same panel newcomers will use for real later - crafting anything real still needs materials from the Glade first, which is itself part of the lesson. The Hall of Callings holds one lookable `FeatureKind::Plaque`, "the Tome of the Seventeen Callings" (`world::tome_feature`, a `OnceLock`-generated `&'static [Feature]` chained into `features_at` alongside `waystone_features`) - built from `Class::ALL`'s own `name`/`resource`/`trait_name`/`tagline`, **not** hand-written text, so it can never drift out of sync with what a class actually does when the roster changes.
- **Invariants respected.** Every safe room here (all but the Training Yard) has a `VILLAGERS` entry, same as `every_public_safe_room_has_a_villager` requires everywhere else. All five rooms are `pvp: false`. `REGIONS` has a "Wayfarer's Hollow" entry so the atlas and the room-count invariant test both account for it.

### Frontier, Reaches, Kaelmyr, and Archipelago loot

Moved to `PROGRESSION.md`: which catalog each land draws from, the id ranges, and the shared realm ladder (`items::realm_loot`, Frontier 1-20 / Reaches 21-40 / Kaelmyr 41-60) that every land's tier is picked on.

---

## 7. Progression, Combat, And Economy [VOLATILE]

### The shape of the game [VOLATILE]

**Moved to `PROGRESSION.md`.** The crowns and what they encode, the lands (levels, health, xp rate, gear tier, how each is reached), the gate spine, and the connection graph all live there, together with the measured power ladder. That file owns those numbers; this one keeps no copy, so there is nothing here to drift.

Read it for any task that touches where a land sits, what it drops, or what opens it. The rest of this section is combat: how a fight resolves once you are standing in one.

### Classes and scores

Playable classes (17; the first five are the class-select `1-5` quick-pick):
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
- Each of the five newcomers carries a full 1..=50 ability roster (ids 1700/1800/1900/2000/2100+) with a level-50 capstone and two archetype paths at `ARCHETYPE_LEVEL`. Progression reads as tiered: staged ability unlocks across the curve, the L10 archetype specialisation, and the shared five-level named milestones.

Progression:
- Level cap is `Class::MAX_LEVEL = 100`.
- `xp_for_level` keeps early levels quick, then adds a much steeper post-level-8 term so midgame and Frontier progress target roughly week-scale casual play instead of a 1-2 sitting clear; `level_for_xp` caps at `MAX_LEVEL`.
- `Class::stats_at(level)` computes HP/resource/attack/resource regen.
- Ability scores are rolled before class selection, persist after class choice, and grow by placed points: `stats::points_earned(level)` is one point per `POINT_EVERY_LEVELS` (4) levels, so 25 over the whole ladder; what is left to place is that less the saved `score_points_spent`, a pure function of level so a save can never drift and a character saved before points existed simply has them all to place (`a_character_saved_before_points_existed_has_them_all_to_place`). A point raises any score up to `SCORE_CAP` (20); `spend_score_point` keeps the point when the score is capped and says so.
- Every score feeds exactly one mechanic through its D&D modifier, all of it pure in `stats.rs` and applied at one hook each in `svc.rs`: STR `swing_pct` (+2%/mod on `swing()`), DEX `crit_pct` (+2%/mod chance a swing crits for double, or below 10 glances for half; `crit_outcome` rolled in both combat rounds), CON `hp_bonus` (mod × (4 + level/2) max HP), INT `spell_power_pct` (+2%/mod on `spell_power_of`, so every ability), WIS `regen_bonus` (+1/mod a tick, `PlayerState::regen`), CHA `price_pct` (3%/mod off `buy_price`, on top of `sell_price`) and `tame_pct` (3%/mod on `tame_chance`). Nothing touches `attack_rating` itself; the old flat `attack_bonus` is gone. An 18 is therefore worth ~8-12% on its axis, and a placed point on an even score moves nothing until the next one (the point screen says so). Pinned by `every_ability_score_moves_the_number_it_promises` and `every_score_moves_one_number_and_says_so`.
- `Class::primary_score` is now only the score a calling leans on for damage (the row the sheet glows): STR for every martial and hybrid, INT for every caster, DEX for Rogue/Ranger/Monk, WIS for Cleric/Druid. Nobody's primary is CHA any more.
- The creation screen prints the rule and the rolled reading of all six under the attribute rows (`ui::attribute_rule_lines`); the point screen (`ui::score_point_lines`, a gate like the archetype crossroads) shows each score's reading now, after the point, and the rule. Nothing about the scores is hidden from the player.
- The sheet and the class-select screen glow the primary score's row in the class accent. Both the label (`ui::primary_label`) and the accent (`ui::class_accent`, `ui::class_emblem`) are matched exhaustively over `Class`, so a new calling breaks the build instead of rendering with no attribute highlighted and a nameless "Adventurer" bust.

### Abilities and damage

- `AbilityEffect` variants: `Strike`, `DamageOverTime`, `Heal`, `HealOverTime`, `Empower`, `Ward`, `Stun`, `Finisher`.
- Every class has a level-1 ability and a level-50 capstone; the classic five carry 12 abilities, the original newer seven carry 10 (each gained a level-28 ability in the Reaches expansion), and the five newest (Beastlord/Skald/Runemaster/Valewalker/Spiritmaster) carry 10 each. Slots past the 1-9/0 hotbar cast from the Abilities panel.
- Offensive abilities require a target. Heals, buffs, and wards do not.
- Damage schools: Physical, Fire, Frost, Holy, Shadow, Poison, Arcane, Lightning.
- `DamageProfile` lets each mob deal one attack type, resist up to one incoming school, and be weak to up to one incoming school.
- Resist halves damage, weak adds 50 percent, and minimum damage is 1.
- Auto-attacks are physical and still pass through mob resistances.
- **The damage formula (2026-08-28).** One shared **attack rating** (`PlayerState::attack_rating`: class curve + gear + active empower + primary score; `attack()` is that with the archetype's `attack_pct`, the number the sheet calls "attack"). What the rating feeds is decided per calling by `Class::damage_weights()` (`classes.rs`, `DamageWeights { auto_pct, spell_pct }`, three closed shapes: **casters** Mage/Runemaster/Necromancer/Warlock/Spiritmaster/Cleric at 50/60, **hybrids** Druid/Paladin/Bard/Skald/Beastlord at 95/45, **martials** Warrior/Rogue/Ranger/Monk/Berserker/Valewalker at 100/35):
  - the Physical auto lands for the **swing** = `attack() * auto_pct / 100` (`PlayerState::swing`; a Mage swings at half, a Warrior in full);
  - every ability lands for `magnitude + spell_power * ability_coef_pct(effect) / 100` (`ability_power`), where **spell power** = `attack_rating() * spell_pct / 100` (`PlayerState::spell_power`) and the coefficient is per effect (`svc::ability_coef_pct`: Strike 100, Finisher 150, Stun 50, DoT 30 per tick, Heal 50, HoT 20 per tick, Ward 60, Empower 25). Class traits (+20% Mage/Runemaster, +25% Ranger vs wounded) and then the archetype apply once to the whole hit in `ability_damage`; an Empower is fed the rating *without* the running empower so a buff never compounds itself. The table magnitude is the flat floor an ability keeps at level 1.
  - a `Stun` never shortens one already on the foe: `mob_stuns`/`pvp_stuns` keep the longer of the two (`a_shorter_stun_does_not_cut_a_longer_one_short`).
  - Consequences, measured in the arena: gear and level now lift abilities as well as the swing, so a caster's output rides its schools (and the resist/weak board) instead of the same Physical swing everyone had; on the neutral dummy martials sit at 55-67% auto, casters at 67-75% abilities, and the best-to-worst calling spread in the same gear is under 1.6 at every ladder step (was 2.0). Contracts: `casters_lean_on_abilities_and_martials_on_the_auto`, `classes_kill_at_a_similar_pace_in_the_same_gear` (`arena_test.rs`), `abilities_scale_with_spell_power_and_the_auto_swings_by_calling` (`svc_test.rs`). The sheet shows attack, swing, and spell (`PlayerView.swing`/`spell_power`).
  - **Resource regen grows with level**: `stats_at` adds `l / REGEN_LEVELS_PER_POINT` (one point per four levels) on top of every class's base. The bases were tuned for level-1 costs and the roster's costs climb to ~49 by level 100; without this a summit caster cast once every five ticks and fell back on its swing.
- Every generated zone carries a themed resist/weak profile on its regulars: see the dedicated section below, **The world resist/weak pass**.

### The world resist/weak pass [STABLE]

Landed 2026-08-20. Every generated zone gives its regular mobs a themed
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
The twelve authored regulars that used to resist it were re-themed
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

**The martial lever** is the weapon-coat family (poisons + the four Alchemy
oils; mechanics in Crafting depth): flat, charge-limited school riders on the
Physical auto that ride `seed_mob_dot`'s baked-in resist/weak multiplier.
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
`OIL_PER_TICK` over `TIER_ATTACK_BAR` (both in `svc.rs`, both pinned to a live
character by `svc_test.rs`), converted through `AUTO_SHARE`. It lands near 14%
of output. **That figure is for a character with no
companion**: `AUTO_SHARE` has no pet term, so a player running a maxed tame sees
roughly half of it. See "Balance: where a character's damage actually comes
from" below. This is the one part of the pass that has already failed once: the
rider was a hardcoded `0.15` while the real coat was worth three to six times
that, and no assertion in the file could see it. A model that measures a
constant instead of the game is not a budget. The second half of that lesson is
still outstanding: the rider is now derived, but the *character* it is a share
of is not (see the Balance section below).

**Tests** (`world_test.rs`): `every_generated_zone_spawn_wears_its_zone_theme`
(regulars wear the theme, zone bosses wear its weak and no resist),
`no_regular_resists_physical_and_nothing_is_weak_to_physical`,
`every_boss_carries_a_weakness`,
`the_school_census_stays_inside_its_declared_bands`,
`the_world_pass_redistributes_grind_rates_but_never_rebalances_a_class`; in
`svc_test.rs`, `the_attack_bar_still_matches_a_real_character` and
`the_coat_curves_stay_inside_their_share_of_the_bar`. Re-theming a zone means
editing its table row and letting these judge the census and budget; they, not
prose, are the contract.

**Visibility**: the targeted foe's traits line (`rank · strikes with X · weak
to Y · shrugs off Z`) in both battle surfaces, plus the per-hit log tags.
Deliberately no pre-fight display: the first swing is the probe, and pre-fight
knowledge is reserved as the future Thundersmith Ledger's territory.

### Combat rules

- `engage` targets the first alive mob in the current room unless the room is safe. Engaging a boss opens with a one-line bark; a blow worth ≥25% of the foe's max HP logs as "crush into" instead of "strike"; a foe first dropping below 25% logs a one-time "staggers" line - flavor only, no mechanics.
- Movement and recall are blocked during combat; flee clears target and moves through the first available room exit, or only breaks combat if no exit exists. **Flee is never free**: the foe you turn your back on lands one more blow as you run (`strike_player` at its normal damage, skipped while it is stunned), and a blow that fells you ends the flight where you stand. Breaking off also takes you off that foe's `engaged_by` list, which starts its recovery clock (see below); it no longer sheds its wounds the instant you run, so ducking next door and coming straight back resumes the same fight.
- **Who counts as still fighting** (`MobInstance::engaged_by`, `mobs_in_a_fight`): drawing on a mob puts you on its list, and you stay there until you flee, fall, or leave its room. Fleeing and falling (`fall_out_of_every_fight`) take you off at once, so a fighter raised where they fell, by a healer's rite or a veteran charge, rises out of the fight and is not struck until they draw on the foe again. A foe with nothing to shed (never wounded, stunned, or festering) that nobody fights forgets its whole list on the next sweep instead of lying in wait for whoever walks back in. A mob is in a fight while anyone holds it as a target **or** anyone on its list is alive in its room - the second half is the load-bearing one, because the combat round only ever resolves the mob a player is *targeting*. So a foe you turned from **keeps swinging at you while you stand in its room** (a second strike pass in the tick, stun-checked and Brute-enraged like the round proper), and a mob in a fight does not roam. Without it, "still in the room" would have been a free place to stand untargeted, take nothing, and heal up.
- **Foe recovery** (`MOB_RESET_TICKS` = 10, ~20s, `recover_abandoned_mobs` in the tick): any living mob that is wounded, stunned, or festering with **nobody left fighting it** for ten consecutive ticks recovers in full (health, stuns, DoTs, and its `engaged_by` list), and everyone in its room is told. The clock only runs once every fighter has left: dying, disconnecting, fleeing, walking off. Switching your lock onto the add it just summoned, or onto a pack-mate it dragged in, is the same fight and costs the foe nothing - that hole let a solo boss heal to full in six seconds while you killed its own add. The grace is deliberately wider than the shared potion cooldown (5 ticks) by a single round trip, so a hit-and-run buys two gulps at most and the engage / stun / flee / repeat loop stays closed. Pinned by `fleeing_costs_a_parting_blow_and_the_foe_recovers`, `a_stunned_foe_cannot_strike_at_a_fleeing_back`, `a_wounded_foe_nobody_fights_recovers_after_a_few_ticks`, `a_foe_someone_else_still_fights_keeps_its_wounds_when_you_flee`, `a_boss_keeps_its_wounds_while_you_fight_the_add_it_summoned`, `a_foe_you_turned_from_keeps_swinging_while_you_stand_in_its_room`, `stepping_out_of_the_room_starts_the_recovery_clock`, `a_fighter_raised_where_they_fell_is_out_of_the_fight`, `a_full_health_foe_forgets_a_fighter_who_walked_off` (`svc_test.rs`) and the two arena exploit pins (`arena_test.rs`). The Rogue's opening strike still re-arms on every engage; against a foe that is always at full health that is harmless.
- Rogue opening strike doubles the first auto-attack after engaging.
- Mage offensive spell damage is boosted by `Arcane Mastery`.
- Cleric healing is amplified by `Light of the Dawn`.
- Ranger damage is boosted against wounded targets below half health.
- Warrior survives the first lethal blow of each life at 1 HP.
- Veteran accounts, checked on join by account age, can resurrect in place while charges remain; fountains refresh charges.
- **Combat companions.** A pet bought from a capital Stable (`buy_pet`, one at the heel at a time; a new purchase sends the old one home to the **kennel**, see below) rides on `PlayerState` and so is always in its owner's room. In the combat round it **bites the owner's target** after the owner's strike (crediting the kill to the owner) for `Pet::attack()` (species base + an eighth per loyalty level, at least 1) **plus `PET_COEF_PCT` (20%) of the owner's attack rating**, the same flat-floor-plus-share shape abilities have, so the companion multiplies the build instead of replacing it (it used to be a flat lump worth 35-50% of any character's output; now 12-33% on a band-appropriate beast, pinned by `a_companion_is_a_share_of_the_fight_not_the_fight` and the exact number by `a_companion_bites_off_its_owners_rating`); when the owner is struck, `wound_pet` splashes `PET_WOUND_PCT` of the blow onto it (alongside `wound_escort`), **but only on survivable hits**, since the death branch takes no `wound_*` (combat is over once you fall). A pet at 0 HP is **downed** and stops fighting until **fed** (`G` from anywhere, or `~` when no stray is being courted; both land in `feed_owned_pet`, costing `PET_FEED_COST`). **Loyalty is capped at `MEALS_PER_DAY` (4) meals a UTC day** (`PlayerState::pet_meals`, persisted, on the player so a new beast does not reset the count): a meal is `FEED_LOYALTY` + `Pet::mend`, and past the cap a hurt or downed pet is still mended for the fee with no loyalty, while a well pet with no meal to have is turned away free (`Meal::Sated`). A pet at `PET_MAX_LEVEL` has no meal to have either (`NoMeal::FullyGrown`, beside `MealsSpent`): loyalty past the cap raises nothing, so it is mended when hurt and otherwise never charged, pinned by `a_healthy_companion_at_the_level_cap_is_turned_away_free`. Every pet takes 36 meals to reach `PET_MAX_LEVEL`, so nine days. Pinned by `a_companion_grows_on_four_meals_a_day_and_is_only_mended_past_them`. The room panel promotes `G` into `You can` only for a hurt or downed pet (`mend`/`rouse companion`); otherwise `pet_feed_chips` puts `G feed 20g · 45% to Lv4 · 2/4 today` under the pet, dropping the `G feed` chip when `G` would be turned away (meals spent, or max level). Loyalty raises the pet's level (more HP/attack). Persisted by species key + loyalty.
- **The kennel.** `pets::Kennel` on `PlayerState` holds every companion the player owns but is not leading, at most one per species and never the species at the heel, so each owned species exists exactly once (`adopt_tamed`, `adopt_bought`, `call_out`, `admit`; pinned in `pets_test.rs`). Nothing is ever released any more: a purchase sends the old companion home, a tame lands there when the heel is taken, and the Stable refuses to sell a species you already own (`Bought::AlreadyOwned`, no charge). Kennelled pets keep their loyalty and their runtime hp/downed state (a downed pet stays downed at home, so the kennel is no free heal). The Stable panel (`p`) lists **Your kennel** first, strongest rung first, then **For sale** (owned species show `owned` instead of a price); `Enter` on a kennel row is `call_out_pet`, which swaps it with the pet at the heel, only at a Stable and never mid-fight. Pinned end to end by `a_new_companion_sends_the_old_one_to_the_kennel_and_it_comes_back`.
- **Death & resurrection.** A lethal blow with no Warrior death-save and no veteran charge leaves the player a **corpse where they fell** (`dead = true`, hp 0, target/shield/empower cleared, 20% carried gold lost, escort lost; banked gold protected). The corpse lingers (`respawn_at = now + CORPSE_LINGER_SECS`). The player chooses: **wait** for a resurrection, or **release** to the temple now (`release_to_temple`, `r`/Enter while dead). If neither happens by the deadline the tick auto-releases them. **Resurrection** is a rite of the holy/nature callings (`Class::can_resurrect` → Cleric/Paladin/Druid): a living caster in the same room spends `RESURRECT_COST` to raise the nearest corpse **in place** at `RESURRECT_HP_PCT` of max (`resurrect_nearest`, `g` key). The snapshot exposes `dead`, `can_resurrect`, `corpse_here`, and per-occupant `alive` so the UI shows the fallen overlay, a `(fallen)` roster tag, and the rez hint. The dead state is **transient** (not persisted; a reload returns the character alive at a safe room).
- `seed_world()` applies a balance scaler after every spawn is generated: `tune_spawn_balance` scales each spawn by its `Band` row (see "The crowns and the story they encode" in the shape section), then `tune_crowns` re-fields the fourteen crowns at their `CROWNS` numbers. The old single endgame row (Frontier/Reaches/Kaelmyr all ×2 hp ×1.9 dmg) is gone; each land has its own pair of rows calibrated against its crown.

### Items, shops, and rewards

- Equipment slots: Weapon, Head, Chest, Legs, Hands, Feet, Ring, Trinket.
- Item rarities: Common, Uncommon, Rare, Epic, Legendary.
- Item kinds: Equipment, Consumable, Valuable.
- Valuables, including Frontier relics, show a `valuable / sell Xg` stat line in inventory/shop UI so players know they are sell loot; generated Frontier relic descriptions also state that they have no combat use.
- Gear rows in the inventory and shop panels carry a **comparison line** vs. what's worn in that slot (`InvView::compare`/`ShopEntryView::compare`, built by `svc::compare_to_worn`): green upgrade / red downgrade / amber trade-off, or "new slot"; empty for the worn item and non-gear (`ui::compare_line`).
- Starter inventory is a Rusty Shortsword and two Minor Healing Draughts. Starting gold is 120.
- Shops are in Embergate: Ember Forge, Outfitter, Apothecary, and Curio Cart.
- **The shops scale with the character (the market tier).** The authored stock above used to be the whole shop, so gold bought nothing after about L37: at Kaelmyr depth the best item in town read `▼-90%` against worn gear, and the only sinks left were a one-off deed, furniture, and pets that stop at rung 35. Each gear shop now *also* stocks its own slots from the generated realm catalogs at `PlayerState::market_tier`, so no new item catalog exists: the Forge sells Weapon/Hands, the Outfitter Head/Chest/Legs/Feet, the Curio Cart Ring/Trinket (`Shop::market_slots`), drawn through `items::market_item_id`, which walks Frontier 1-20 → Reaches 21-40 → Kaelmyr 41-60 (`MARKET_TIER_MAX`). The market stops at the end of the **road**, not at the end of the gear: `archipelago_items` is a full eight-slot Legendary set at t=61-80 above it, and it stays unbuyable on purpose, because the Archipelago is off-road content where the best gear is earned. `market_title_cap` makes that structural anyway (it ladders on the three gate titles and the Archipelago grants none), and `market_tier_base` would index past Kaelmyr's catalog if the ceiling were lifted without teaching it the fourth realm.
- **The market tier is derived from two factors, the lower binding.** *Level* reads `expected_kit_tier`, interpolated between `world::CROWNS`' own kit rows (L40 authored → L55 Frontier-10 → L65 Reaches-10 → L75 Kaelmyr-10 → L80 Kaelmyr-15), less `MARKET_LAG_PCT` (20). *Gate titles* cap the band, so gold can never buy out of a continent the character has not opened: Archdemon's Bane → Frontier top, King's Bane → Reaches top, Yssgar's Bane → Kaelmyr top. The clamp to `MARKET_TIER_MAX` is applied **before** the lag, never after, or a level past the ladder's end creeps back up to Kaelmyr's deepest drop. Contract: `the_shops_deep_stock_always_trails_the_kit_the_level_expects` (a bought piece is weaker than its own land's drop in every slot, at every rung), plus `gold_cannot_buy_a_kit_out_of_a_continent_you_have_not_opened` and `a_market_piece_you_have_not_earned_cannot_be_bought`.
- The result is a shop that reads about **-9% to -18%** against the kit the crown ladder expects, at every level from 45 to 100: a floor under a straggler, never a replacement for the hunt. `MARKET_MARKUP_PCT` (200, i.e. 3x list) makes it the late game's repeating gold sink: a full eight-slot set runs 21,600g at L55 and 103,200g at L85+. It also fills Head/Legs/Hands/Feet, which the Archipelago's finds never drop.
- **`PlayerState::shop_offers` is the single authority** on what is on sale and what it costs; both the `ShopView` builder and `buy` read it, so a stale panel or a hand-sent item id cannot buy out of band, and the panel can never quote a price `buy` disagrees with.
- **Consumables are deliberately not on the market.** A deep-realm draught heals `120 + 20t`, which at the L80 market tier is a 1,000+ heal, unlimited and on tap in town, against the Phoenix Tonic/Wyrmfire Cordial players already finish the game on. The heal curve is what the whole combat tuning rests on, so the Apothecary stays authored (`Shop::market_slots` is empty for it) and sustain stays earned.
- Shop economy intentionally includes expensive late-game gold sinks: masterwork weapon/armor/head/hands, premium curio gear, and the repeatable Phoenix Tonic. The masterwork shop pieces are shop-stock, not boss drops, so gold remains useful after normal boss clears.
- Apothecary consumables are tuned as the pressure valve for harder combat: early draughts are affordable recovery, Elixir of Renewal covers mid/late mixed HP/resource recovery, and Phoenix Tonic is a repeatable expensive late-game recovery sink. **Every heal/restore consumable shares one cooldown** (`QUAFF_COOLDOWN_TICKS` = 5, `PlayerState::quaff_cd`, transient): a second gulp inside it is refused ("still queasy") and nothing is spent. Draughts used to be spammable mid-fight, bounded by gold alone. Pinned by `a_draught_needs_a_breath_between_gulps`.
- Authored boss loot tables include head and hand upgrades across tiers; living-dark bosses add controlled post-Archdemon unique gear, while their regular mobs mostly drop regional relics and sustain consumables.
- Bosses always drop one item from their loot table. Regular mobs have a modest chance if their table is non-empty.
- Mob kills grant XP, reduced gold, possible loot, and titles. Boss XP and Frontier quest XP/gold bounties are intentionally damped so boss chains do not skip too much of the level curve.
- Boss title format is `Bane of ...`; lesser foes grant a derived `...bane` title.
- Frontier boss kills complete their zone quest, award XP/gold, and grant `Champion of the <zone>`. The payout and the title's level key off `world::frontier_zone_level(zone)` (a straight line from the seals' L40 to the King's L55, the ends the generator is sloped between), never the level displayed over the boss's head: that reads by bite and moves with every retune, and a one-time payout must not (`a_zone_boss_bounty_pays_by_the_zones_target_level_not_the_number_over_its_head`).
- Defeating the authored final boss, the Archdemon Mal'gareth, pays 10,000 chips and grants the `LMG` profile-award badge. **The badge is once per account; the chips repeat** (migration 158) behind two gates at once: once per `mud_characters.id`, and at most one crown of that kind every 7 days per account. Both gates or neither: a refusal writes nothing and pays nothing.
- Defeating the final Frontier boss, the King Who Was Promised Nothing, pays 10,000 chips on the same two gates and grants the `LKN` profile-award badge. (It paid 20,000 until migration 144 flattened the four crowns to 10,000 each; claims already banked at the old rate were left alone.)
- Defeating the final Reaches boss, Yssgar, the Sundering Deep, pays 20,000 chips on the same two gates and grants the `LYS` profile-award badge. Defeating Kaelmyr's last boss, Kaethyr Ascendant, Who Sang the God Awake, does the same for `LKA` (the Unquenched Throne's Kaethyr the Unquenched carries no achievement; only the Ascendant form at the Sundering Wound does). Migration 144 flattened all four crowns to 10,000; migration 158 lifted the two deepest to 20,000, because the reroll farm the lockout exists to stop is the easy pair and the hard pair needs a leveled character every time. `BossAchievement.payout` is still an `Option` so a future badge-only crown stays expressible. Badge codes are named after the boss (Mal'Gareth, King/Nothing, YSsgar, KAethyr Ascendant), and chat author labels collapse to the highest crown (`LKA` > `LYS` > `LKN` > `LMG`).
- Every mob kill emits a Lateania activity win event (dashboard/quest tier only; excluded from the #lounge feed). Only the **four named realm crowns** — the ones `boss_achievement_for` recognizes (Archdemon Mal'gareth, the Frontier King, Yssgar the Sundering Deep, Kaethyr Ascendant) — publish a structured `BossSlain` event to #lounge; the ~9 regional/zone bosses (`MobSpawn.boss` without an achievement) fall too often and stay dashboard-only. `publish_kill_outcome` therefore gates the `BossSlain` on `outcome.achievement.is_some()`, not on the `boss` flag (the flag was dropped from `KillOutcome`). A player materializing in the world via `join` publishes `GameStarted`, which also ships to #lounge through `app/activity/lounge.rs`. Final-boss kills route through `ChipService::credit_run_cooldown_reward_template`; if a gate refuses, activity still records the defeat without the chip/badge detail. The character the crown is keyed on is resolved in `publish_kill_outcome` from the service's `live_slot` binding (falling back to `active_slot`, the same rule `publish` uses) rather than carried on `KillOutcome`: the world state has no slot, and the read happens one step after the tick that produced the kill.

### Balance: where a character's damage actually comes from [VOLATILE]

An audit of the 30-60 band, the levels most of the game is played at (the 50-100
half is 75% of the xp curve but end-game grind; see "Where the time goes"
above). **Status (2026-08-28): the headline finding, "ability magnitude never
scales", is fixed by the damage formula (see "Abilities and damage" above), and
the arena (`arena.rs`) now measures what this section modelled; its composition
table is the live version of the "Modelled" table below. The rest stands as
findings, not a contract.** Everything under "Measured from
the code" is read straight out of the engine and is safe to rely on. Everything
under "Modelled" comes from a fight simulator built to mirror the tick loop; the
ordering is robust but the exact percentages are not, and nothing here is
test-enforced yet. Treat this section as the shared starting point for balance
work, and promote a finding into a test before acting on it as truth.

#### Measured from the code

**Ability magnitude never scaled with anything (fixed 2026-08-28).** The old
`svc::spell_damage` was the whole of it: `magnitude`, then `+20%` for
Mage/Runemaster, then `+25%` for a Ranger against a wounded foe, then the
archetype's `attack_pct`. It never read `attack()`, gear, or level. The
auto-attack did. So gear and level lifted the auto and left the ability table
frozen, and the ability rosters in `abilities.rs` (151KB, 362 records, the
largest content file in the folder) controlled a shrinking share of a
character as the game went on. `ability_damage` now adds spell power to every
magnitude; see "Abilities and damage".

**Every class's auto-attack is Physical.** `svc.rs`'s combat round resolves it as
`profile.apply(player_atk, DamageType::Physical)` with no class branch. A Mage
swings Physical exactly like a Warrior. This is why weapon coats are not a
martial mechanic (see below).

**The class attack formula is the single biggest class-dependent lever**
(`classes::stats_at`). Four tiers, and the gap never closes:

| tier | formula | attack at L50 | classes |
|---|---|---|---|
| A | `7 + 2l` | 105 | Berserker |
| A | `6 + 2l` | 104 | Warrior, Rogue, Ranger, Monk, Valewalker |
| B | `5 + 2l` | 103 | Mage, Necromancer, Warlock, Runemaster, Spiritmaster |
| C | `6 + 1.5l` | 79 | Skald |
| C | `5 + 1.5l` | 78 | Cleric, Druid, Bard, Paladin, Beastlord |

C tier is a flat ~25% deficit on the term that carries most of the damage.

**A pet was class-agnostic and enormous (fixed 2026-08-28: the bite is now
the species growth plus 20% of the owner's attack rating, see the combat
companions bullet).** `Pet::attack()` was `base + base * (level - 1) / 4`, so
a level-10 companion hit at 3.25x its species base: 65 for the shop
Emberdrake, ~107 for a tame-49 beast, 182 for the Wildbound Worldserpent. Reaching `PET_MAX_LEVEL` costs 900 loyalty at
`FEED_LOYALTY` 25 and `PET_FEED_COST` 20, so **720 gold total**, trivially
affordable in this band. The pet does not scale with your class, your level, or
your gear: it scales with Animal Taming and 720 gold.

**The whole pet package is Physical and no coat can touch it.** Pet bite, Savage
Bite, Pounce, and Rend all pass `DamageType::Physical`, and the coat block sits
inside the *player's* auto section and spends a charge only on the player's own
swing. Against the 14 Physical-wall bosses (which include Aelunor's 12 zone
bosses at Lv53-69, squarely in this band) a pet build loses half its largest
damage term with zero counterplay.

**Weapon coats are everyone's lever, not the martial lever.** Nothing on the
`use_item` -> `oil_school_tier` -> `coat_weapon` path checks class. `weapon_coat`
is a single `Option`, so a new coat overwrites the old (wasting its charges):
**oil or poison, never both**. `DotSource::Coat` refreshes one wound rather than
stacking, so while charges last the coat is a flat `per_tick` every tick, plus
`POISON_DOT_TICKS` lingering ticks after the last swing.

**Coats out-cover the casters on the resist/weak board.** `OIL_SCHOOLS` is
Fire/Frost/Holy/Lightning; poison vials add Poison. Against the 16 `ZoneTheme`
weaknesses:

| kit | weak lanes covered |
|---|---|
| any class carrying a full oil kit plus poison vials | **12 / 16** |
| Mage, in-band damage schools (Fire/Frost/Lightning) | 9 / 16 |
| Runemaster, in-band (Arcane/Fire/Lightning) | 9 / 16 |
| any class with no coats | 0 / 16 |

Only the Arcane lanes (Storm, Resonant) and Shadow lanes (Profane, Fae) are out
of reach. A Rogue with a shopping bag exploits the world pass better than a Mage
does, because a caster's schools are welded into the 5-25% of output its ability
table supplies while a coat rides the 40-75% the auto supplies.

**A Cleric cannot afford to attack anywhere in 1-60.** Regen 6 against Smite
(9/tick), Holy Fire (6.7/tick), Hammer of Faith (9.3/tick). The first
sustainable offensive ability is **Divine Radiance at L41**, at 5.1 resource per
tick for 2.6 damage per tick. Judgment at L50 costs exactly the regen (6.0/tick).
This is a hole, not a trade-off: nothing is bought with the shortfall.

#### Modelled

Damage composition per mob at L55, on a 3-tick fight plus 3-tick walk cycle
(regen runs through both), with a 107-attack level-10 pet and a tier-5 oil:

| class | auto | abilities | pet | coat |
|---|---|---|---|---|
| Rogue | 47% | 17% | 30% | 6% |
| Warrior | 43% | 12% | 37% | 7% |
| Mage | 43% | 13% | 37% | 7% |
| Skald | 37% | 16% | 39% | 8% |
| Beastlord | 37% | 6% | 50% | 8% |
| Cleric | 42% | 5% | 44% | 9% |

The same characters with no pet and no coat sit near 70/30 auto to abilities,
which is where `AUTO_SHARE` was set. **Three of the four terms are identical for
every class.** Gear attack, the pet, and the coat total roughly 213 damage per
tick at L55 and do not care who is holding the weapon.

**The consequence is dilution, not balance.** Best-to-worst spread:

| | worst class as a share of best |
|---|---|
| no pet, no coat | Cleric at ~50% of Rogue |
| pet 107 plus tier-5 oil | Cleric at ~68% of Rogue |

The ordering does not change: same first, same last, same sequence throughout.
The gap closed because the class-dependent share of a character shrank, not
because the classes converged. A 46% spread between best and worst is still very
large, and the levers for fixing it now move a third of the number they used to.

**Traits split cleanly into ones that scale and ones that do not.** The
distinction is whether a trait multiplies a term that grows, or hands over a flat
amount that the shared terms outgrow:

- **Scale with the game.** `Pack Bond` multiplies the pet (+30%), which is why
  Beastlord moves from last without a companion to second with a strong one, and
  keeps climbing as tames improve. `Opportunist` doubles one auto per fight, so
  it is worth `1/fight_length`, huge on 3-tick trash and near nothing on a boss.
  `Hunter's Instinct` multiplies both autos and abilities for the back half of
  every fight. `Reaping Harvest` heals `3 + level/4` per landed auto, free, every
  tick, which is 50-100% of post-armor incoming damage in this band and the
  reason Valewalker chain-pulls with no downtime.
- **Do not.** `Battle Hymn` / `War-Chant` give more of a resource that buys
  abilities, and abilities are the smallest term; the two classes carrying it pay
  the C attack tier for it. `Light of the Dawn` amplifies healing on a class that
  cannot attack. The on-kill restores (Necromancer, Spiritmaster, Warlock) pay
  nothing inside a fight and nothing at all on a boss. `Unbreakable` is one
  saved death per life, not throughput.
- **Gated past the point of use (fixed 2026-08-28).** `Frenzy` was
  `(missing_pct - 50).clamp(0, 50)`: nothing above half health, full value only
  at death's door, so a Berserker who drinks under 40% collected roughly none
  of it. It now ramps from full health, `missing_pct / 2`, to the same +50% at
  death's door (PvE and PvP), and the frame went from `42 + 10l` to `44 + 11l`:
  it was the one prepared path that died to a crown (Yssgar, L65).

#### Where the world pass's own model diverges

`AUTO_SHARE = 0.75` splits output into auto plus abilities and **has no pet
term**. The routed grind-rate budget in `world_test.rs` is therefore certifying a
character with no companion. If a maxed tame is the normal case, the coat rider
lands near 14% of a petless character and near 7% of a real one, and every
declared band halves with it: the +-15% per-zone swing, the >=+5%
best-zone-and-coat meaningfulness floor, and the <=+18% routed ceiling all
describe a fraction of a character that few players are. The +5% floor in
particular falls under the noise of a single gear upgrade.

This is the same failure mode the `TIER_ATTACK_BAR` comment already warns about
(a rider certified at a constant while the real one was three to six times that).
The bar itself is derived from a live character and is fine; the split it feeds
is not. **The fix is a pet term in the model, not a re-tune.** Until then, do not
read the world-pass percentages as what a player experiences.

#### Small defects found while measuring

Each is real and read from the code, none is fixed:

- **`Pack Bond`'s cooldown reduction is a no-op on Savage Bite.**
  `base_cd - base_cd * BEASTLORD_PET_PCT / 100` on integers: a `cooldown` of 3
  gives `3 - 0 = 3`. The comment promises "at least one tick off, never below
  one". cd 4 -> 3, 6 -> 5 and 7 -> 5 all work; only cd 3, the pet's most frequent
  skill, silently does not.
- **`Pack Bond` does not make the companion "hardier" in the way the trait text
  says.** `Pet::max_hp` has no Beastlord branch; the toughness is delivered as a
  30% cut to the splash in `wound_pet`. Functionally close, but the sheet shows
  an identical pet HP pool and reads as the trait not working.
- **Skald is Bard plus 1 attack and 2 HP.** Same resource, same curve, same
  regen, and the trait is literally the same branch
  (`matches!(p.class, Some(Class::Bard) | Some(Class::Skald))`) with the same
  description string. Skald also carries the thinnest roster in the band, 9
  abilities by L45 against Rogue's 12, with nothing at all between L42 and L50.
- **Mage and Runemaster are the same class statistically.** Identical
  `stats_at`, identical primary score and resource, identical archetype shapes,
  and one shared `if` for both traits. The only real difference is unlock
  density: Runemaster has dead bands at 28-36 and 42-50 and no Empower between
  L16 and L75, where Mage fills all of it. Mage leads burst by 13-38% across the
  band.
- **Runemaster's fantasy is not implemented.** Its description promises a graven
  mark "left to smoulder and then detonate"; Detonation Rune is a plain
  `DamageOverTime`. `svc.rs`'s single `Class::Runemaster` mention is the shared
  spell-damage `if`, and there is no other Runemaster-aware code.
- **Four items are Mage-locked and the newer twelve classes have no equivalent.**
  Apprentice Staff and Runed Battlestaff are `Some(Class::Mage)`, so a Runemaster
  swings the Rusty Shortsword (attack 4) until the unrestricted Embergate
  Falchion. Early game only, no effect past ~L15, but a rough first hour.
- **Duplicate ability names inside one class.** Warrior has two "Earthshaker"
  (L28 Stun, L30 DoT), Bard has three "Crescendo", Mage two "Blizzard" and two
  "Meteor", Warlock three "Chaos Bolt". They will render as duplicates in the
  hotbar and the abilities panel.
- **Hotbar slots shift on level-up.** `use_ability` indexes
  `unlocked_for(class, level)` positionally, so every new unlock renumbers every
  slot after it.

#### Open, worth digging into

- How common is a maxed tame in practice? Everything above hinges on it. If it
  is rare the class table matters much more; if it is universal the pet is the
  build.
- Beastlord's archetype choice looks inverted: DPS applies `attack_pct` to only
  ~43% of its output, so Wildwarden (tank) costs it roughly 6% damage for +22%
  mitigation and +12% HP. It may be the only class near the top of the damage
  table that can take the tank path close to free.
- Nothing in the engine caps or diminishes the flat terms, so the compression
  should get worse as tames and coat tiers climb. Worth checking what the
  composition looks like at L100 with a Worldserpent and a tier-5 oil.
- The traits that survive dilution are the multiplicative ones. Re-shaping the
  flat traits (`Battle Hymn`, `Light of the Dawn`, the on-kill restores) to
  multiply a shared term instead would be the cheapest way to make class choice
  matter again without nerfing anything.

---

## 8. Persistence [STABLE]

### Character save

Character persistence uses `late_core::models::mud_character` / `mud_characters`.

Saved character schema version: `19`.

Durable fields:
- class key, XP, level, carried gold, banked gold, current HP;
- saved room, but hydration only restores it if the room still exists and is safe;
- visited rooms for minimap;
- inventory and equipped `(slot-key, item-id)` pairs;
- rolled ability scores;
- titles, title levels, active title index;
- completed Frontier quest indices;
- chosen archetype key (validated against the saved class on load);
- companion species key + accumulated loyalty (the pet reloads at full health; its level derives from loyalty);
- kennelled companions as (species key, loyalty) pairs (schema v20; `#[serde(default)]`; unknown keys and species already owned are dropped on load with a warning);
- owned housing plot (tier index) + placed furnishings as (room, key) pairs (re-registered into `plot_owner`/`house_furniture` on load);
- appearance/bio trait indices (`Vec<u8>`, clamped to valid options on load);
- gathering-skill xp as (skill key, total xp) pairs (unknown keys dropped on load);
- crafting-skill xp as (skill key, total xp) pairs;
- Animal Taming xp as a single `taming_xp` value (schema v14; `#[serde(default)]`, so pre-v14 saves start the trade untrained);
- starter-chain progress as `starter_stage` + `starter_kills` (schema v19; `#[serde(default)]`, and hydration marks the chain complete for pre-v19 saves at level ≥ 10).

Transient by design:
- current target;
- active effects, cooldowns, shields, buffs, stuns;
- player respawn timer;
- follow target;
- pending activity events.

Unclassed characters are not exported. Empty or unreadable blobs are treated as no save.

### Character slots

An account can keep up to `svc::CHARACTER_SLOTS` (5) saved characters, so trying another class never means wiping the one you already have. `mud_characters` is keyed by `(user_id, slot)`; every character predating this feature landed in slot 0 unchanged when the `slot` column was added, so nobody's save moved.

The world identity a session plays is always still the account's own `user_id` - only the DB row `join`/`leave`/autosave read and write changes with the slot. Everything downstream of join (combat, pvp, leaderboard, inventory, the whole `WorldState.players` map) is completely unaware slots exist.

**Two slot maps, and conflating them loses saves.** The world holds one player per account, so exactly one character is live at a time, and "which slot did the landing ask for" is a different question from "which slot is the character in the world from":

- `active_slot` is *intent*: set by `LateaniaService::select_slot` (called from the landing before `enter_lateania`), account-wide, changed by every Enter from any connection. It is read in exactly one place, the `join_task` that creates the world player.
- `live_slot` is *fact*: bound by that same `join_task` at the moment `join` creates the player and released where the player is removed (`leave_task`, `delete_character_task`). `prepare_persist` resolves the target slot from here and takes no slot argument at all, so no caller can name the wrong one; it returns `None` when nothing is live to save.

Reading `active_slot` at save time was a real data-loss bug: a second connection opening the landing and selecting another slot redirected the *live* character's autosave on top of the character saved there, destroying it (`a_second_session_picking_another_slot_cannot_overwrite_the_live_character` pins this). A second session for an account already in the world attaches to the live character and is told so in the log; its slot pick simply loses.

The landing (`screen.rs`) is a character-select list, not a single "Enter"/"d" pair: `j`/`k`/arrows move a slot cursor (`App::lateania_slot_cursor`), Enter calls `select_slot` then `enter_lateania`, and `d` resets whichever slot is highlighted (not necessarily the one currently live). The rows it shows (`SlotSummary`: occupied?, class, level) come from `LateaniaService::character_slots`, whose freshness rules are in §3. The Games-hub "launch immediately" shortcut for Lateania now only navigates to the screen (it can't skip character-select), and the hub's own quick-delete shortcut excludes Lateania entirely - only the landing's own per-slot delete is safe now that there's more than one character to lose.

Deleting a slot only touches the live in-memory player and kicks a session out (via the existing `reset_versions` "reset elsewhere" signal) when the deleted slot is that account's `live_slot`; a landing cursor merely *pointing* at a slot must never evict the character someone is mid-fight with, so neither the delete path nor the `reset_versions` filter in `publish` may consult `active_slot` while a character is live. Internally, `persist_versions`/`persist_locks`/`prepared_saves`/`character_resets`/`character_reset_versions` are all keyed by `(user_id, slot)` (a `CharKey`), not just `user_id` - this is load-bearing: without it, a fast slot switch could hydrate a join from a different slot's still-in-flight save.

### Shared world save

Shared world persistence uses `late_core::models::mud_world_state` / `mud_world_states` with key `lateania`.

Saved world schema version: `1`.

Durable fields:
- mob HP/alive state;
- mob respawn remaining seconds;
- mob stuns;
- mob damage-over-time stacks.

World autosave runs every 15 seconds when `world_dirty` is set. Character autosave runs every 60 seconds for present characters. `flush_all` best-effort persists present characters and dirty world state during graceful shutdown.

Important race guard: world load is skipped if `world_revision != 0`, so a late DB load cannot overwrite live mutations that happened after startup.

Character save schema v5 stores class, XP/level, carried/banked gold, HP, last safe room/visited map, inventory/equipment, scores, titles/title levels, active title, and completed Frontier quests. Unclassed players are not exported. On load, invalid/non-safe rooms fall back to start, resource is restored to full, and saved positive HP is clamped to current max. Shared-world schema v1 stores mob alive/HP/respawn timers plus mob stuns and DoT stacks.

---

## 9. Critical Invariants [STABLE]

- `WorldState` is authoritative. `State` and UI are cache/projection only.
- Service tasks are async and snapshots can lag; every server mutation must validate against current `WorldState`, not the UI's stale row selection.
- Do not save mid-fight player state. Characters reload combat-ready in safe rooms.
- Do not wipe shared world state during per-character reset.
- Do not create a fresh starter character if DB load fails; that risks overwriting an existing save later.
- **Keep the account/character distinction sharp.** The live world identity is always the account's `user_id`; the slot (which of up to `CHARACTER_SLOTS` saves is loaded) is a separate, in-memory-only `active_slot` selection made before join. Never key `WorldState.players` or anything downstream of it by slot - only the DB read/write path (`join_task`/`leave_task`/autosave/`delete_character_task`) and the internal `persist_versions`/`persist_locks`/`prepared_saves`/`character_resets`/`character_reset_versions` maps (all keyed by the `CharKey = (Uuid, i16)` pair) need to know slots exist.
- Keep class keys and item IDs stable once persisted.
- Keep generated Frontier ID ranges aligned: 20 zones, 20 item tiers, IDs `3000..3200`, Frontier rooms at `2000+`, Frontier mob IDs at `900000..950000`.
- Keep generated Reaches ID ranges aligned: 20 zones, 20 item tiers, IDs `3200..3400`, Reaches rooms at `10000+`, Reaches mob IDs at `950000..960000`. `tune_spawn_balance` classifies by these ranges (`band_of` → `Band::Reaches`, its own pair of rows).
- Keep generated Kaelmyr ID ranges aligned: 20 zones, 20 item tiers, IDs `3400..3600`, Kaelmyr rooms at `12000+`, Kaelmyr mob IDs at `960000..970000` (`Band::Kaelmyr`); the Archipelago's `970000..980000` is `Band::Archipelago`, whose row is now 1:1 (its generator authors the fielded numbers).
- **A crown is named in three places that must agree**: its spawn literal in `world.rs`, its `CROWNS` row (`tune_crowns` panics at seed if a row has no spawn), and the arena's `CROWN_TARGETS` (`arena_test.rs`). Renaming a boss means all three. When adding Kaelmyr zones, update `KAELMYR_ZONES_DATA`, `KAELMYR_TIERS`, `kaelmyr_loot`, board-quest zone tests, the room-count band, and the shape test together — and zone-name fields must NOT start with "The " (the builder prepends it). Kaelmyr also has a `REGIONS` atlas entry (range derived from the zone consts) — every continent must appear in `REGIONS` or the atlas silently omits it.
- **The measured power ladder lives in `PROGRESSION.md`.** `tier` is a generator input, not a power level, and is not comparable across bands (`tune_spawn_balance` applies a different hp/dmg/xp row per `Band`, so two lands at the same tier can be a factor of three apart). Every measured level, health and xp figure, plus the four rules for re-tuning one, is in that file. Do not copy any of it back here.

- Keep the Archipelago's ID ranges aligned: 20 islands, rooms at `20000+` (`archipelago::ARCH_BASE`/`ARCH_STRIDE`), mob IDs at `970000..980000` (`ARCH_SPAWN_ID_START`, `Band::Archipelago`, its own dedicated row rather than sharing Kaelmyr's), generated gear at `5000..5200` (`ARCHIPELAGO_ITEM_BASE`/`ARCHIPELAGO_TIERS`, `power_offset = 60`), and regional finds at `3800..3840` (`ARCHIPELAGO_FIND_BASE`). **There is no island `tier`, on purpose.** `extend_archipelago` authors hp, damage and xp per island directly (`isle_n`) and `Band::Archipelago`'s row is **1:1**, so what the generator says is what is fielded and it can be read against `CROWNS` without arithmetic. A tier is a generator input that a band row then rescales, and the Archipelago briefly carried `isle + 52` against a row multiplying damage by 19/10 and 21/10: that one-line change put every mob on all 20 islands at L100 while out-hitting the final crown threefold, because a tier moves hp, damage and xp together and the row hid how far. The port it came from was answering a real complaint (Kaelmyr out-xp'd and out-golded every island; gold derives from xp via `gold_for_kill`, so an xp regression silently regresses gold) but the tier was the wrong knob for it - see the xp bullet in the measured ladder above. `archipelago_boss_loot` weights the two find ids into the pool at increasing odds by quartile (islands `0..10` ×1, `10..15` ×2, `15..20` ×4), and the last three islands (`isle + 3 >= ISLAND_COUNT`) are `apex`: **+25% boss hp and xp, and no longer any damage bonus**. A longer, richer fight, not a bigger bite - a damage step there would put a cliff in the middle of the level ramp, since past L80 displayed level is a linear read of damage. **The Archipelago's bosses are not crowns** - they are ordinary named bosses like every zone boss elsewhere, not in `CROWNS`/`CROWN_TARGETS`; nothing here needs the three-places rule above.
- Keep the Thornveil Falls ID ranges aligned: 12 zones, rooms at `34000+` (12×8 = 96 per zone, `THORNVEIL_BASE`/`THORNVEIL_ZONE_STRIDE`), mob IDs at `1000000+` (`THORNVEIL_SPAWN_ID_START`, well clear of Aelunor's `1600000+` and Wildbound's `1500000+`), classified by `band_of` into its own `Band::Thornveil` - a distinct band, but tuned to the **identical row** as `Band::Kaelmyr` in `tune_spawn_balance`'s match (both the main hp/dmg/xp table and the regular-mob respawn-rate `endgame` check just below it - a new band added to one and not the other silently splits their treatment). No generated gear catalog - `thornveil_loot` borrows `reaches_loot` at **one distinct tier per zone (`8 + z`, so 8..19)**, never clamped: the previous `(10 + z).min(19)` collapsed zones 9, 10 and 11 onto one table so the last quarter of the continent paid no gear progression at all. `thornveil_notable_loot` adds Thornveil's own regional finds at `3900..3924` (`THORNVEIL_FIND_BASE`), built through the shared `items::realm_slot_stats` (never a hand-mirrored copy of that table: one lived here and had already drifted on the ring line). **Thornveil is maze-only** (`thornveil_zone_is_cavern` always `false`), unlike every other mixed continent, because `every_public_safe_room_has_a_villager` needs every zone entrance room id known exactly for the static `VILLAGERS` table, and a `carve_cavern` entrance cell is RNG-picked rather than fixed at cell 0 like a maze's - adding cavern zones later means deriving those entrance ids from an actual `seed_world()` run, not guessing them. Tier is `z + 30`, *intended* to overlap Kaelmyr's lower half. **It does not**: measured, Thornveil reads L58-69 trash and L64-68 boss against Kaelmyr's L63-78 and L69-80, so every Thornveil notable sits below Kaelmyr's shallowest and the land lands as Reaches-tier side country rather than a second late-game road. Its base coefficients are ~10-15% gentler than Kaelmyr's *and* its tier starts two lower, so it is gentler twice over. Raising it is an open decision; `thornveil_falls_is_reachable_gated_and_sits_below_kaelmyr` pins the current bands exactly so a retune has to re-bless them. **Not a crown**: its final boss is an ordinary named boss, not in `CROWNS`/`CROWN_TARGETS` - if it should become one, that needs real arena calibration (§10's battle arena section), not a guessed number. When changing Thornveil Falls, update `THORNVEIL_ZONES_DATA`, `is_thornveil_room`, the `REGIONS` atlas entry, the room-count band, and the shape/reachability tests together.
- Keep the Sunderlakes ID ranges aligned: 14 zones, rooms at `16000+` (11×8 = 88 per zone, `LAKES_BASE`/`LAKES_ZONE_STRIDE`), mob IDs at `980000+` (`LAKES_SPAWN_ID_START`, a fresh band above Kaelmyr's `960000+` and the Archipelago's `970000+`), and the 40 fish items at `4600..4700` (`FISH_BASE`/`FISH_COUNT`, clear of materials `4000..4100`, crafted `4200..4500`, generated loot `3000..3600`). The Sunderlakes have no generated gear catalog — loot is fish. When changing the Sunderlakes, update `LAKES_ZONES_DATA`, `is_lakes_room`, the `REGIONS` atlas entry, the room-count band, the shape test, and the fish-node test together.
- Keep the Broceliande ID ranges aligned: 20 zones, rooms at `22000+` (11×9 = 99 per zone, `BROCELIANDE_BASE`/`BROCELIANDE_ZONE_STRIDE`, both public so `taming.rs` can place beasts), mob IDs at `990000+` (`BROCELIANDE_SPAWN_ID_START`, a fresh band above the Sunderlakes' `980000+`). Broceliande has no generated gear catalog — `broceliande_loot` borrows the Frontier tiers, which resolve through `item()`. **Its mob band `990000..` is deliberately cut out of the `kaelmyr` classification in `tune_spawn_balance`** so the Greenwood keeps gentle overworld multipliers, not the endgame ones (the lakes/archipelago at `970000..990000` still ride the endgame band with tiny base stats). When changing Broceliande, update `BROCELIANDE_ZONES_DATA`, `is_broceliande_room`, the `REGIONS` atlas entry, the room-count band + region sum, the shape/reachability tests, and — since the beasts are placed by zone — the `taming::wild_beasts` mapping together.
- Keep the Aelunor ID ranges aligned: 12 zones, rooms at `25000+` (9×8 = 72 per zone, `AELUNOR_BASE`/`AELUNOR_ZONE_STRIDE`, both public), mob IDs at `1600000+` (`AELUNOR_SPAWN_ID_START`, a fresh band above Wildbound's `1500000+`). Aelunor has no generated gear catalog — `aelunor_loot`/`aelunor_notable_loot` borrow the Frontier tiers exactly like `broceliande_loot`. **Its mob band `1600000+` falls outside every named endgame band in `tune_spawn_balance`**, so it keeps gentle overworld multipliers by default — do not raise `AELUNOR_SPAWN_ID_START` above `1700000` without checking it still clears every band there. **Any test or code that filters "Wildbound mobs" by `id >= WILDBOUND_SPAWN_ID_START` must also bound it above by `AELUNOR_SPAWN_ID_START`** (an unbounded `>=` used to silently sweep Aelunor's zone bosses in as "Wildbound apex bosses" too — `wildbound_template_pool_is_three_hundred_mobs_plus_three_apex_bosses` caught it). When changing Aelunor, update `AELUNOR_ZONES_DATA`, `is_aelunor_room`, the `REGIONS` atlas entry, the room-count band + region sum, the shape/reachability tests, and — since the beasts are placed by zone via `aelunor_entrances()` — the `taming::wild_beasts` mapping together. Silvael (`SILVAEL_BASE = 26_000`, fixed 8 rooms, `SILVAEL_ROOM_COUNT`) sits just past Aelunor's own range and re-derives its splice point at build time rather than hardcoding a direction or a room id — see the `extend_silvael` doc comment and the gotcha in §11 about excluding `is_aelunor_room` from that search.
- Keep the sixty tameable Broceliande beasts (`taming::TAMEABLE`) and their `wt_*`/`wb_*` keys stable once persisted, and the Stable's `pets::PET_SPECIES` keys too (a saved pet reloads by key); `tame_level` must stay non-decreasing across the list (small→large, easy→hard). The five Aelunor beasts (`taming::AELUNOR_TAMEABLE`, keys `ae_*`) are a **second, separate pool** appended after it — `WildBeast.species` indexes the combined `TAMEABLE` then `AELUNOR_TAMEABLE` sequence, and must always be resolved via `taming::beast_species(index)`, never by indexing `TAMEABLE` directly (that panics/mis-resolves once an Aelunor index is in play). `pet_species_by_key` must keep searching `PET_SPECIES`, `TAMEABLE`, **and** `AELUNOR_TAMEABLE`, or saved tamed pets from either wild pool won't reload.
- **Every companion is on one ladder** (`PetSpecies::rung`): a wild beast at its `tame_level`, a Stable pet at the `rung` its `PetSource::Stable` is priced to match. **No companion may be Pareto-dominated by one at the same or a lower rung** (`no_companion_is_out_classed_by_an_easier_one`, walking the Stable, `TAMEABLE`, and `AELUNOR_TAMEABLE` together): pets on a rung may trade `base_attack` for `base_hp` (a hitter vs. a wall is a real choice), but none may lose on *both* axes, or it is dead content nobody picks. That covers the Stable in both directions: a bought pet never loses to an easier wild beast and never beats a harder one. The classic fifty climb to taming 54 (the old ten-way tie at 50 left eight dead beasts) and the ten Wildbound beasts (`wb_*`, tame 55-100) start above that peak. The Stable (`pets::PET_SPECIES`, identical at all four capitals) climbs rung 4 to 35 at 120 to 8000 gold (`the_stable_climbs_to_mid_game_and_leaves_the_top_to_taming`); the top half of the ladder stays wild. Keep every Stable `base_attack` under 24: `Pet::attack` grows `base/8` per loyalty level, so 24 jumps the step, and the arena's `a_companion_is_a_share_of_the_fight_not_the_fight` measures the Stable's hardest biter at L32 and L55.
- There are no mounts. Riding (`G`, multi-room strides) was removed: with no movement cooldown it only saved keystrokes. The `wb_*` beasts are ordinary summit companions.
- Keep the Wildbound Waste ID ranges aligned: 3 biomes, rooms at `30000+` (`WILDBOUND_BASE`/`WILDBOUND_BIOME_STRIDE = 700` per biome: 4 town rooms + a field carve), mob IDs at `1500000+` (`WILDBOUND_SPAWN_ID_START`, well clear of every other band and of `SUMMON_ID_START = 990000000`). A `Room` must never be `safe && pvp` together. Wildbound falls into `tune_spawn_balance`'s default "gentle overworld" bucket (its id range matches none of the frontier/reaches/kaelmyr bands) - its own authored stats are calibrated for that, not for endgame multipliers. When changing it, update `WILDBOUND_BIOMES`, the `REGIONS` atlas entry, the room-count band + region sum, the shape/reachability/mob-pool tests, and the `CONTINENT_WAYSTONES` entry together.
- **Never bind a Lateania key on `?`.** late.sh reserves it globally across every door game for a cross-door help overlay (`app::input::door_games_allows_global_help`), checked *before* `Screen::Lateania` ever routes a byte into this door's own `handle_key` - a `b'?'` arm in `input.rs` compiles fine and is simply never reached. The Leaderboard originally shipped on `?` for exactly this reason before being moved to `!`; before adding a new key, grep the app-level `late-ssh/src/app/input.rs` for the byte too, not just this module's own `input.rs`.
- **Esc must always reach `input::handle_key` first**, never be special-cased earlier in `screen.rs` **or in the app-level `app::input::dispatch_escape`**. There are two interceptions to keep removed, not one: `dispatch_escape` runs *before* screen dispatch, so while `lateania_state` is live it must forward the byte to the door (`screen::GAME.handle_key`) and only route on to the Games hub if the door actually left. Fixing only the `screen.rs` half left Esc still quitting on a single press through this path, which is exactly how it shipped once (`one_escape_never_drops_you_out_of_an_active_lateania_world`, in `app/input_test.rs`). That function is what (a) cancels an in-progress chat compose on Esc instead of leaving (`chat_active()` is checked before the leave branch) and (b) gates an actual leave behind a confirming second Esc within `State::LEAVE_CONFIRM_SECS` (`arm_leave_confirm`/`confirm_leave`, shown in the title bar via `leave_confirm_pending`). `screen::handle_active_lateania_key` used to intercept `byte == 0x1B` and call `app.leave_lateania()` unconditionally before either of those could run - that bug is what let a single stray Esc (including one meant to cancel chat) instantly log a player out.
- Keep Wayfarer's Hollow at `TUTORIAL_BASE = 40000` (5 fixed rooms, exact count not a band) and `join()` pointed at `tutorial_start_room()`, never at `World::start_room` directly - the latter must stay Embergate's square (1) since map anchoring/`recall`/the temple all key off it. Before claiming a "free" direction off an existing room for new content, verify it in the **final** built world (`seed_world()`), not just the source literal - other `extend_*` functions dynamically claim directions off shared hubs like Embergate's square at runtime (Frontier takes `Down`, city districts take `Up`) and will silently overwrite a plain `.insert()` made earlier. When changing the Hollow, update the `REGIONS` atlas entry, the room-count band + region sum, and the shape/reachability/content tests together.
- When adding rooms, keep every exit target real, every room reachable from start, and every mob home valid.
- When adding boss or mob loot, every item ID must resolve through `item(id)`.
- When adding Frontier zones, update `FRONTIER_ZONES_DATA`, `FRONTIER_TIERS`, loot generation, quest mapping tests, and room-count expectations together.
- `seed_world()` leaks generated strings to `'static`; this is acceptable for one process lifetime and current tests, but avoid adding per-tick/per-request leaks.
- Active Lateania captures ordinary keys. Parent/global shortcuts must remain governed by the app-level dispatch code and root context.
- The `door` folder is a grouping folder. Keep Lateania-specific behavior in this context instead of creating a separate `door/CONTEXT.md`.
- Shared door-game host contracts live in sibling `door/game.rs`. Keep that interface minimal; do not push Lateania-specific state into the shared trait.
- **Every single-letter key (a-z, A-Z) and every sensible standalone symbol (`!;:~'/[]<>,.` and space) is already bound at the top-level `input.rs` match.** Confirmed by grepping every `b'...'` literal in the file - there is no free key left for a new dedicated world-action or panel toggle. A new interactive panel has to piggyback on an existing entry point (`Panel::Board` opens via the Examine list instead of its own key) rather than claim a new binding; check with a grep before assuming one is free.
- **A panel reachable only via Examine (not its own key) still needs the same three wiring points every other list panel does**: `state.rs`'s `list_len()` match, the `in_list` `matches!` block in `input.rs` (both copies - `handle_key` and `handle_arrow` each have their own), and `activate_selection`'s dispatch. Missing any one of these compiles fine and silently breaks cursor movement or number-key selection for just that panel.

---

## 10. Tests And Verification [STABLE]

Root policy applies: agents should not run `cargo test`, `cargo nextest`, or `cargo clippy`; leave blocking verification to the human owner. If a change needs verification, mention the focused command in handoff.

Inline pure tests currently cover:
- `world.rs`: exit validity, reachability, room count, overworld count, room description length, mob home validity, mob ID uniqueness, loot references, boss quest mapping, capital features, wildlife, minimap behavior, early Frontier regular difficulty.
- `svc.rs`: join/class stats, saved level reconciliation from XP, recall, following, stale follow targets, wildlife hunting and boons, unclassed/progression gating, buying/equipping, Rogue opening strike, Warrior death-save, title uniqueness, veteran resurrection, fountain restoration, ability score derived stats.
- `abilities.rs`: unique ability IDs, level-one abilities, capstones, monotonic unlocks.
- `classes.rs`: level cap, XP curve, XP/level round trip, HP growth.
- `items.rs`: authored item ID uniqueness, valid shop stock, slot reporting, nonzero sell price.
- `persist.rs`: character and world JSON round trips, empty blob as no-save, missing-field defaults.
- `damage.rs`, `stats.rs`: resistance math, minimum damage, D&D modifiers/roll ranges/defaults.
- Pure landing/input helpers can be unit-tested inline in `screen.rs` if any are extracted.
- DB/service coverage for Lateania goes in adjacent `_test.rs` files beside the module they exercise, using `crate::test_helpers::new_test_db`.

Lateania unit tests also lock broader gameplay invariants: world size/reachability, shop/item validity and gold sinks, Frontier gates/warnings, follow chains, wildlife hunting/boons, death/gold/veteran resurrection, the dead/corpse state (lingering corpse not an instant temple trip, release-to-temple, healer resurrection in place vs. an incapable class), combat companions (buying costs gold/refuses when unaffordable, the pet bites the owner's target, is downed by a barrage, and is revived/strengthened by feeding; every capital has a stable), player housing (claiming a deed, one-home-per-name, furnishing only a home you own while visitors cannot, the 50+-piece catalogue and non-overlapping plots), boss achievement mapping, saved-character level reconciliation, and persistence JSON round trips.

### The battle arena (`arena.rs`, `arena_test.rs`) [STABLE]

A test-only harness, declared as a child of `svc` (`#[cfg(test)] #[path = "arena.rs"] mod arena;`, beside `svc_test`) so it reaches the private combat entry points. It drives the **real engine** (`tick`, `engage_mob`, `use_ability`, `use_item`, `flee`) with a scripted character against a real spawn in its real home room and reports who survives and where every point of damage came from (auto / instant abilities / ability DoTs / coat / pet, pinned to the foe's health by `every_point_of_damage_is_accounted_for`). A `Recipe` states everything that moves a fight (class, level, archetype, `Gear` preset, `Companion`, `Coat`, potion count, `Policy`, and the ability-score `Build`); the arena pins what must not (clock at a clear day via `ARENA_CLOCK`, one fresh character per fight, the foe at full). **Builds** (`arena::Build`): `Neutral` (flat 10s, what every yardstick and crown contract runs), `Peak(Score)` (18 in one score), `Focused` (20 primary + 20 CON), `Blessed` (all 18), `Cursed` (all 3), `GlassCannon` (STR/DEX/INT 20, rest 3), `Tortoise` (CON/WIS 20, rest 3), `Merchant` (CHA 20, rest 3). The build contracts run in the suite on `BUILD_STEPS` (Warrior/Rogue/Mage/Cleric at L55 Frontier-10): `a_peak_score_moves_its_own_axis_and_nothing_else` (CHA changes no fight, CON changes the pool not the pace, STR/INT/WIS peaks land in their bands; Wisdom reads on `REGEN_TICKS` = 100 because regen only tells once the pool drains, a 20-tick window shows a Cleric's WIS as +0%), `a_crit_build_lands_its_share_over_a_long_window` (`CRIT_TICKS` = 150, a zero-crit window is then impossible), `the_whole_roll_is_a_share_of_the_fight_not_the_fight` (`BLESSED_GAIN` 5..40%, `CURSED_LOSS` -30..-3%), and `the_strange_builds_trade_what_they_say_they_trade`. The yardstick behind them is `arena_build_table` (`#[ignore]`, `BUILD_TICKS` = 200, ~2 minutes) and the same table is a section of part 2 of the report. Policies: `Honest` (rotate the roster by value, drink under 40%), `HitAndRun`, `StunAndFlee` (the exploit shapes, kept so the fix stays pinned). **`make arena` runs the whole battle checker** (every contract, both report parts, serially); it is deliberately not part of the suite: the crown and doorstep contracts and the reports are `#[ignore]` (minutes of real fights), only the sub-second ones (accounting, the exploit pins, the share and pace contracts) run with `cargo nextest`. Run it when combat, classes, gear, or bosses change. Balance questions get answered here, not modelled: `make test-llm ARGS="-p late-ssh --run-ignored all -j1 -E 'test(arena_report)'"` writes `late-ssh/target/lateania-arena.md` (part 1: every Long Road crown against every class at eight level/kit steps, bare and with a maxed tame + oil; ~3 minutes, so run serially with `-j1` or it trips nextest's per-test budget) and `lateania-arena-extra.md` (part 2, `arena_report_extra`: a damage-composition table, the **dps yardstick** (`Arena::measure`: 20 honest ticks on the neutral straw dummy with its pool raised, per class and ladder step, the one number that compares callings independent of shape), the exploit table, and every boss as the engine fields it. The fast tuning loop is `make test-llm ARGS="-p late-ssh --run-ignored all --no-capture -E 'test(arena_dps_table)'"` (the yardstick alone, seconds). Promote a report finding into an asserting arena test before acting on it as truth; the standing contracts are the two exploit pins, `casters_lean_on_abilities_and_martials_on_the_auto`, `classes_kill_at_a_similar_pace_in_the_same_gear` (`DPS_SPREAD_MAX`), `every_crown_falls_to_a_prepared_character_and_not_to_a_walk_in` (`CROWN_TARGETS`), and `the_trash_on_a_crowns_doorstep_is_in_band`. The yardsticks behind them (`arena_dps_table`, `arena_crown_yardstick`, `arena_doorstep_yardstick`) are `#[ignore]` prints for the tuning loop.

Expected focused command for human verification after Lateania changes:

```bash
cargo test -p late-ssh lateania
```

Put DB/service orchestration tests that cannot stay pure in adjacent `_test.rs` files beside the module they exercise; everything else stays inline and pure.

---

## 11. Known Gotchas And Future Work [VOLATILE]

### Next passes, ranked (written 2026-08-28, after the ability-score pass)

The state of play: the crown ladder, the damage formula, the pet share, the stun and bounty fixes, and the ability scores (six hooks, a point every four levels, the creation and point screens stating every number) are done and measured; the arena pins all of it. What is left, in the order it would change the game for a player, with the numbers that justify the rank:

1. **The roster cut.** `abilities.rs` is one roster wearing seventeen skins: slots 55-100 are byte-identical across every class (L100 is always a Finisher, cost 49, cd 8, mag 210, dur 2), the 1-50 half is nearly as templated, every class has every effect (Warrior heals, Mage wards, Rogue wards, Runemaster heals), and names duplicate inside a class (Warrior 2× Earthshaker, Bard 3× Crescendo, Warlock 3× Chaos Bolt, Mage 2× Blizzard and 2× Meteor, Cleric 2× Sanctuary, Druid 2× Thorn Lash and 2× Nature's Wrath, Paladin 2× Lay on Hands and 2× Hammer of Justice, Berserker 2× Rampage, Warlock 2× Curse of Agony). Worse since the spell-power fix: damage is `magnitude + spell_power × coef` with the coef per effect, so a later Strike only adds a bigger flat floor while its cost and cooldown climb, and the L1 button wins: Warrior L100 Cleave 314/tick for 10 rage vs Titan Cleave (L80) 218/tick for 13.5 rage; Mage Firebolt 530/tick for 8 mana vs Starfall Lance (L75) 320/tick for 13.5. Twenty of twenty-three slots are dead, against a 10-key hotbar that renumbers on every unlock (`use_ability` indexes `unlocked_for` positionally). The shape to aim for: ~8 abilities per class, each a distinct role (opener, spender, DoT, defensive, utility, big cooldown, capstone), scaling from spell power rather than from a renamed copy, later levels unlocking modifiers to existing buttons, and identity by omission (the Warrior has no heal, the Mage no ward, the Rogue two stuns and the Cleric none). Draft the per-class table for review before touching the data; `casters_lean_on_abilities_and_martials_on_the_auto`, `classes_kill_at_a_similar_pace_in_the_same_gear`, and the crown contract are the nets.
2. **Pin the builds against the crowns.** `every_crown_falls_to_a_prepared_character_and_not_to_a_walk_in` runs the `Neutral` build only. Two `#[ignore]` arena lines close it: a `Cursed` prepared character must still take every crown (a bad roll is a handicap, never a wall) and a `Blessed` walk-in must still lose (a good roll is never a skip). Half an hour, and it is the question a player who rolled 3s will ask.
3. **The authored core cannot level anyone, and the side countries have no quests.** See the two bullets below (31 spawns, 6,276 xp, about L11, against an Archdemon pitched at a prepared L35; no quest content for Lv15-30, Lv35-52, or past Lv78; none in any of the five side countries). Content, not balance, and the emptiest part of a new player's road.
4. **Gear is the majority of attack from the first town.** At L10 the class curve is 24 of a 122 rating and shop gear 98; at L100 it is 204 of 857. The class term is never the majority. Decide whether that is the intended gear game or whether `stats_at` should carry more; it interacts with (1), since the roster's flat magnitudes were the class's old lever.
5. **Small ones.** CHA reaches the item shop only, not stables, deeds, or furniture. The Druid's two paths have identical bare dps. The character sheet shows the six scores but not their effect lines (only the creation and point screens do).

- **(Resolved)** Off-screen POI border arrows used to point in a meaningless direction once the POI was in another reserved block, and were unfiltered. Fixed: `worldmap::poi_arrows` now drops any POI further than `PAN_LIMIT` from the player before projecting it (see §5.1) - the exact fix this entry used to describe as future work.
- A splice that discovers "the room that links to X" by scanning `room.exits` for a matching target must exclude any room that is itself inside the region X belongs to, or it can match one of X's own in-region neighbours instead of the real external anchor - `HashMap` iteration order then makes the match (and everything hung off it) silently nondeterministic between boots. `extend_silvael`'s anchor search does this (`is_aelunor_room` excluded) after `worldmap_test::world_coords_is_cached_and_complete` caught the case where it didn't.
- Adding a region to `REGIONS` also means placing it on the land map: a `Place` (or `Keep`) and a `Road` per real link, in `ui.rs`. `the_atlas_draws_every_road_in_the_world_and_invents_none` fails until you do, on purpose - the picture is authored, so a new country has to be drawn in rather than laid out for you (see §5.2).
- **Do not cite line numbers in this file.** They rot on the next edit and there is nothing to catch it: of the eight `file.rs:NNNN` references the gate-spine section once carried, seven were already pointing at unrelated lines before anyone noticed, and a comment-only banner pass moved the last correct one. Name the symbol instead - `can_cross_progression_gate`, `extend_kaelmyr` - which greps and survives.
- Some comments in `world.rs` may lag current content scale. Trust current tests/data: ~2600 rooms across base/overworld/Frontier, the three living-world regions, housing, city districts, and the ~900-room Sundered Reaches (see the room-count test's per-region ranges).
- `follow_task` still exists as an old toggle service command, but current input opens the Follow panel and uses `follow_to_task` / `stop_follow_task`.
- `say_task` exists, but active Lateania has no typed command prompt yet.
- Inventory snapshots include equipped items after pack items. Equip/use/sell mutations usually require the item to still be in `inventory`, so equipped-row activation is often a no-op.
- Inventory rows wrap in the side panel and equipped rows include their worn slot, e.g. `[worn weapon]` or `[worn chest]`.
- `view.occupants` includes other players in the room regardless of class; service follow selection only allows classed targets in the same room.
- Boon perks apply on room entry and can spam log lines if movement loops through boon rooms.
- Hunted game cooldowns are not persisted across process restart.
- **Ability scores (fixed 2026-08-28).** Before this date four of the six scores were display only and the primary score was a flat +4 at most, ~3% at L10 and 0.5% at L100 against gear; the roll decided nothing and nothing after the roll could change a score. Every score now has one hook and a point lands every four levels, see "Classes and progression"; the arena measures the builds (§10). What the first build table said (L55 Frontier-10, 200 ticks, dps vs neutral): peak STR +5/+4/+2/+3% (Warrior/Rogue/Mage/Cleric), peak DEX +5/+4/+2/+4%, peak INT +1/+2/+4/+4%, **peak WIS +5/+4/+13/+8%**, peak CON +9..11% hp, blessed +17..24%, cursed -13..-17%, glass cannon +14/+11/+4/+9% at -10..12% hp. Two things to weigh: **Wisdom is the best damage score for a caster** (a Mage's regen 7 → 11 is +57% resource, worth three times its INT), and INT is the weakest of the damage scores because it lifts only the spell-power half of an ability, not its table magnitude. Not retuned; a decision for the next pass (INT at 3%/mod, or WIS at 1 per 2 modifier points, or leave it as the caster's honest choice between burst and stamina).
- World content is authored as Rust data. A future data-file loader should preserve the existing `World`, `Room`, `MobSpawn`, `Feature`, and `CritterSpawn` shapes.
- **The authored core cannot level a player through itself.** Its 31 spawns hold 6,276 xp in total (about Lv11) while the Archdemon at its end is pitched at a prepared L35 (`CROWNS`), so players are pushed into the ungated side countries to level and return over-levelled. The consequence is that the run up to the Obsidian Throne plays as trivial even though its trash reads by bite a few levels under its crown, same as every other land. Fixing this means raising the core's xp budget (or its late trash), not re-tuning the boss: a crown's row is derived from the prepared median, never authored by feel. See §7 for the whole shape.
- **No quest content bridges Lv15-30, Lv35-52, or anything past Lv78**, and none of the five side countries has a single quest, despite Aelunor and Broceliande being the de facto route from the authored core to the Archdemon. Boards exist only in the three capitals and at the Kaelmyr ash-cairn. This is the emptiest part of the game for a new player and the most likely place to lose them.
