# Lateania Game Context

## Metadata
- Scope: `late-ssh/src/app/door/lateania` plus Lateania screen lifecycle in `late-ssh/src/app/door`
- Domain: Lateania, the persistent D&D-style MUD inside late.sh
- Primary audience: LLM agents changing the Lateania game runtime, content, UI, combat, or persistence
- Last updated: 2026-07-20 (landing is single-column now; the right-column frontier banner art/image panel was removed)
- Status: Active
- Parent context: `../../../../../CONTEXT.md`
- Stability note: Sections marked `[STABLE]` should change rarely. Sections marked `[VOLATILE]` are expected to change when gameplay/content changes.

---

## 0. Context Maintenance Protocol [STABLE]

Read this file after root `CONTEXT.md` whenever a task touches Lateania's landing page, launch/leave behavior, reset prompt, active-world input capture, game runtime, content, UI, combat, or persistence.

- Keep this file aligned with game behavior, keybindings, save shape, world/content invariants, and known gotchas.
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
- The world holds ~8700 rooms: 198 base/extension, 100 overworld, 1000 Frontier, three living-world regions (96-room Sunken Catacombs, 96-room Thornwood Hollows, CA-sized ~75-room Drowned Caverns), the **Hearthward Close** housing district (rooms `9000+`, `extend_housing`), **20 city-district rooms** (`3000+`, `extend_cities`), the **Sundered Reaches**, a *second ~900-room continent* (rooms `10000+`, `extend_reaches`, hung off Matlatesh), **Kaelmyr, the Ashen Reach**, a *third ~2000-room continent* (rooms `12000+`, `extend_kaelmyr`, hung off Yssgar's chamber in the Reaches), the **Sunderlakes**, a peaceful *~1200-room water country* (rooms `16000+`, `extend_lakes`, hung off the Melvanala high lake), **Broceliande, the Greenwood**, a *fourth ~2000-room continent* (rooms `22000+`, `extend_broceliande`, hung off the Verdant Highlands' Faerie Hollow), and the **Shattered Archipelago** (`archipelago.rs` + `extend_villages`/`extend_archipelago`): four safe **portal villages** (rooms `8000+`) and a *~900-room island region* (rooms `20000+`, 20 islands, each a maze/cavern with a named boss), reached only by **waystone portals** (`FeatureKind::Portal`, a runtime feature layer over the static `FEATURES`), not by walking (the reachability test follows `portal_destinations()`). **Each Reaches, Kaelmyr, Sunderlakes, Broceliande, and Archipelago zone is carved as a braided maze (`carve_maze`) or an organic cavern (`carve_cavern`), never a uniform grid** (`reaches_zone_is_cavern`/`kaelmyr_zone_is_cavern`/`lakes_zone_is_cavern`/`broceliande_zone_is_cavern` pick the cave-like ones); zones chain deepest-room→next-entrance, mobs are behaviour-driven by maze-role (dead-ends ambush, junctions swarm, corridors patrol/cast), and `frontier_desc`/`kaelmyr_desc`/`lakes_desc`/`broceliande_desc` supply paragraph prose. The room-count test checks each region range; `is_reaches_room`/`is_kaelmyr_room`/`is_lakes_room`/`is_broceliande_room` mirror `is_frontier_room`; shape tests assert every continent has dead-ends and varied branching (not square blocks).
- **The Sunderlakes** (rooms `16000+`, mob ids `980000+`, no generated gear catalog): a large, *peaceful*, mid-game-friendly water country of flooded caverns, reed labyrinths, island-dotted meres and drowned valleys, hung off the Melvanala high lake by a normal walk (lightly gated, effectively open). 14 zones (11×8 cell fields; 10 braided reed-mazes + 4 flooded caverns) of ~1200 rooms, chained lake-to-lake. The draw is **fishing**: **40 distinct fish species** (items `4600..4700`) are caught at Fishing-gated resource `NODES` spread across the maze zones (four fish per zone, gates rising with depth so the prized deep-water catches need a trained angler). Every zone landing is a safe haven; mobs are fewer and far weaker than the endgame (a zone notable + a scatter of lake-wildlife). Fish sell for a wide price spread (a few gold to hundreds); ~a third are edible `Consumable`s and a handful of legendaries grant a well-fed `HealOverTime` "special" (see `fish_well_fed`/`use_item`).
- **Broceliande, the Greenwood** (rooms `22000+`, mob ids `990000+`, no generated gear catalog — loot borrows the Frontier tiers via `broceliande_loot`): a vast, *moderate* (tougher than the lakes, well below Kaelmyr) Dark-Age-of-Camelot country of deep-green oakwoods and steaming jungles, druid circles and briar mazes, standing stones and faerie rings, moss-grown keeps and vine-choked ruins. 20 zones (11×9 cell fields; 14 braided briar-mazes + 6 organic fern-caverns/glades, `broceliande_zone_is_cavern`) of ~2000 rooms, chained forest-gate to forest-gate, hung off the Verdant Highlands' Faerie Hollow (room `688`) by a normal walk; every zone's forest gate is a safe haven. Its through-line is the enchanted-wood dream: from the woodward holt on the eaves, past the druid circles and sleeping keeps, down into the jungle heart and the World-Oak at the centre. **Excluded from the endgame `tune_spawn_balance` scaler** (mob ids `990000..` are cut out of the `kaelmyr` band there) so it keeps gentle overworld multipliers. It is the **home of the animal-taming trade**: the fifty tameable beasts (see the taming subsection) gather at the zone forest gates.
- **Kaelmyr, the Ashen Reach** (rooms `12000+`, mob ids `960000+`, loot ids `3400..3600`): a burnt continent torn loose from the seabed when Yssgar was slain and the seas drained into the wound he left. 20 zones (13×9 cell fields; 16 braided mazes + 4 organic calderas) of ~2048 rooms total, each with a named boss, chained west-to-east and down. Five peoples run through the prose and mob/boss names: the **Emberkin** (ash-shamans of the western calderas), the **Cinderbound** (shackled dead who labour the ash), the **Gloamwrights** (glass-and-obsidian artificers of the black deserts), the **Stormheld** (sky-clans of the storm-spires), and the **Hollow Choir** (the final drowned-god cult at the wound). The continent ends at the **Unquenched Throne**, ruled since the Sundering by **Kaethyr the Unquenched, the Ashen King**, and the deepest zone (**Sundering Wound**) holds his ascended form, **Kaethyr Ascendant, Who Sang the God Awake** (the fourth realm crown: `LKA` profile badge once per account, no chip payout). Gated behind the **Bane of Yssgar, the Sundering Deep** title (`KAELMYR_GATE_TITLE`) with the same transient two-step warning as the Reaches sea-gate. An ash-cairn board sits in the safe entry hub (Cinderfall Shore ash-gate, room `12000`) carrying board quests 17–22.
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
| `ui.rs` | Ratatui rendering for class select, log, compact mode, side panels, minimap, hints. The Character panel expands to a full-width dashboard (accent-tinted class portrait, dot-rated ability scores, vitals/XP meters) when the area is at least 72x18, else falls back to the narrow side panel; Foes/Adventurers/Follow render as aligned roster rows with HP meters. Lock-free, snapshot-only. |
| `svc.rs` | Authoritative runtime: service tasks, `WorldState`, player/mob state, combat, movement, following, shops, persistence, snapshots, activity events. |
| `world.rs` | Immutable world data and generation: rooms, exits, mobs, features, wildlife, minimap, overworld, Frontier. Also **resource nodes** (`NODES`/`ResourceNode`/`nodes_at`/`node_index`): trees, ore veins, fishing spots and herb/skinning patches keyed to rooms, modelled exactly like `WILDLIFE` (static data + a per-node service cooldown), each with a skill, tier, min-level gate, and derived yield item. |
| `classes.rs` | Seventeen playable classes (Warrior/Mage/Cleric/Rogue/Ranger/Druid/Necromancer/Bard/Monk/Paladin/Warlock/Berserker/**Beastlord/Skald/Runemaster/Valewalker/Spiritmaster**), resources (incl. Spirit/Souls/Tempo/Ki), passive traits, level 1-50 stat curves, XP curve. Adding a class means an arm in every `match self` here (name/primary_score/resource/tagline/description/trait_name/trait_desc/stats_at/as_key/from_key), an entry in `ALL`, an ability roster in `abilities.rs`, and (if the trait needs runtime behaviour) a hook in `svc.rs`: upkeep loop for regen (Druid/Paladin) and Tempo (Bard/**Skald** War-Chant); `kill_mob` for harvest (Necromancer/Warlock/**Spiritmaster** Spirit Siphon); `strike_player` for Monk mitigation; `spell_damage` for Mage/**Runemaster** overflow; the combat round for Berserker frenzy and **Valewalker** heal-on-hit; the pet-bite step + `fire_pet_skills`/`wound_pet` for **Beastlord** Pack Bond (empowers the taming/pets companion - stronger bite, tougher, faster auto-skills). **Every level grants something:** the curve grows each level (surfaced by `check_level_up`, which logs the concrete +HP/+attack/+resource gains per level), plus `level_milestone`/`milestone_hp_bonus` add a named milestone (Blooded…Ascended) with a permanent +HP every fifth level, a pure function of level, so no extra save state; `current_milestone(level)` shows on the character sheet. **Archetypes:** at `ARCHETYPE_LEVEL` each class offers two paths (the `ARCHETYPES` data table; `archetypes_for`/`archetype_by_key`), each carrying a `Role` (Tank/Healer/DPS) and four percent modifiers (`attack_pct`/`mitigation_pct`/`heal_pct`/`max_hp_pct`). The modifiers apply at existing combat hooks in `svc.rs` (DPS in `attack()`+`spell_damage`, Tank in `strike_player`, Healer in `heal_player`, max-HP in `max_hp()`); no engine changes; the chosen `&'static ArchetypeDef` is held on `PlayerState` and persisted by key. |
| `abilities.rs` | Ability roster and unlock helpers. Effects are data, resolved in `svc.rs`. |
| `housing.rs` | Player housing data + address arithmetic. `TIERS` (5 homes Hut→Tower: price/ground/upper rooms), the 50+-piece `FURNITURE` catalogue, `HOUSING_BASE`/`plot_base`/`plot_of_room`/`is_housing_room`. Homes are **static rooms** (generated in `world.rs::extend_housing` as Hearthward Close off Market Row); only **ownership** (`plot_owner`) and **furnishings** (`house_furniture`) are dynamic side-state on `svc.rs`, so movement/visiting/snapshot work unchanged and the homes are public shared-world plots. |
| `appearance.rs` | Character appearance/bio. `FIELDS` (Build/Hair/Eyes/Bearing/Origin/Mark/Manner, each with a menu of options) + `compose_bio`. The TUI has no free-text, so a player customises by cycling preset options (`e` opens the Appearance panel; `Enter`/`x` cycle a field). Stored as `[u8; N_FIELDS]` on `PlayerState`, persisted (new fields default cleanly for old saves), shown on the sheet and when profiling another adventurer (Follow panel). Also `portrait(class_key, sel) -> Vec<String>` (`PORTRAIT_ROWS`): a composed ASCII bust assembled from the player's own Build/Hair/Eyes/Bearing choices plus a class-flavoured headpiece (helm/hood/circlet/laurel/wild-band by class key) - pure glyph rows, coloured by `ui.rs::composed_portrait` with the class accent + per-feature tints. Shown on the character sheet (`sheet_identity`, replacing the old shared `class_portrait`), when profiling another adventurer (Follow panel, keyed by their snapshot `class_key`/`appearance_idx`), and live in the `e` appearance builder as a preview. Snapshot carries `class_key`/`appearance_idx` on `PlayerView` and `OccupantView` (lock-free/snapshot-only). |
| `archipelago.rs` | The **Shattered Archipelago** data + address arithmetic: the four portal `VILLAGES` (rooms `8000+`), the 20 `ISLANDS` theme table (rooms `20000+`), `island_entrance`/`village_room`/`is_archipelago_room`/`has_waystone`, and `portal_destinations()` (the fast-travel menu). Rooms are generated in `world.rs`; the portal teleport (`travel`) lives in `svc.rs`. |
| `pets.rs` | Combat companions. `PetSpecies` data table (`PET_SPECIES`, `pet_species_by_key`) of buyable beasts, and the live `Pet` (held on `PlayerState`, always co-located with its owner). Loyalty (earned by feeding) drives the level via a pure function; `max_hp`/`attack` scale with level. `PetSpecies` carries a `tame_level` (`0` = buyable Stable species; `>0` = the Animal Taming level a wild beast needs); `pet_species_by_key` searches **both** `PET_SPECIES` and `taming::TAMEABLE` so a saved pet of either kind reloads. The world wiring (buying/feeding/wounds, the bite each round, and the level-gated **pet auto-skills**) lives in `svc.rs`. Persisted by species key + loyalty (HP restored full on load). |
| `taming.rs` | The **Animal Taming** trade. `TAMEABLE` = fifty tameable `PetSpecies` of Broceliande, small→large with `tame_level` rising 1..50 (harder and harder); `wild_beasts()`/`beasts_at` place each at a zone forest gate (a real, safe room). `tame_chance(xp, beast)` drives the success roll (40% at the required level, +9%/surplus level, capped 95%, 0 if under-level); `tame_xp` scales the reward. **Pet auto-skills** live here too: `PET_SKILLS` (Savage Bite L3 / Rend L8 / Intimidating Roar L15 / Loyal Guard L22 / Killing Pounce L30) with `pet_skills_at(level)`; `PetSkillEffect` is resolved in `svc.rs::fire_pet_skills`. Only data + pure maths; the action/panel/combat wiring is in `svc.rs`/`state.rs`/`ui.rs`. |
| `items.rs` | Item catalog, equipment slots, consumables, valuables, shops, generated Frontier loot. Also the **raw-material catalog** (`materials()`/`material_id`/`MATERIAL_BASE = 4000`): 5 skills x 5 tiers of gathered materials (logs/ores/fish/herbs/hides), `Valuable` kind (immediately sellable), IDs `4000..4100` (skill index x 20 + tier). The **crafted-goods catalog** (`crafted()`/`CRAFTED_BASE = 4200` + the `*_id(tier)` helpers): intermediates (ingots/planks/leather) and finished goods (weapons/armor/potions/poisons/food), IDs `4200..4500`. And the **Sunderlakes fish catalog** (`fish()`/`FISH_BASE = 4600`/`FISH_COUNT = 40`): 40 species with a wide sell-price spread, ~a third `Consumable` (edible), the rest `Valuable`; `fish_well_fed(id)` gives the well-fed regen for the legendary "special" fish. All chained into `item()`. |
| `skills.rs` | **Gathering skills** (`GatherSkill`: Woodcutting/Mining/Fishing/Foraging/Skinning) and **crafting skills** (`CraftSkill`: Smithing/Woodworking/Leatherworking/Alchemy/Cooking), both on one 1-50 xp curve (`xp_for_skill_level`/`skill_level_for_xp`/`skill_progress`), independent of class level and steepening past level 10. Persisted per-player as (skill key, xp) for each set. |
| `crafting.rs` | **Recipes** (`Recipe`/`recipes()`/`recipe(i)`/`recipe_indices_for(skill)`): inputs -> output, gated by a `CraftSkill` + level. 50 recipes (10 per tier x 5 tiers) that **chain** (ore -> ingot -> weapon). Data only; `svc::craft` resolves and applies them. Built at runtime and cached (inputs are `Vec`, not a leaked slice). |
| `damage.rs` | Damage schools, mob resistance/weakness profiles, damage multiplier math. |
| `stats.rs` | D&D-style ability scores, 4d6-drop-lowest rolls, modifiers, HP/attack bonuses. |
| `persist.rs` | JSON schemas for durable character saves and shared world saves. Versioned (`SCHEMA_VERSION`); new fields use `#[serde(default)]` so old saves load (e.g. `board_progress`/`board_done` for quests). |

### Board quests [VOLATILE]

`BOARD_QUESTS` (in `svc.rs`) is a static table of bounties posted on a `FeatureKind::Board` in each capital square (Tasmania/Melvanala/Matlatesh) plus the **Kaelmyr ash-cairn board** at the Cinderfall Shore ash-gate (room `12000`, quests 17–22: reach the ash-gate, cull the cinder-dead/Emberkin/Choir, salvage shore relics, reach the Ashen King). Each has an `Objective`: `Bounty{name_contains,count}`, `Collect{item,count}`, `Reach{zone}`, or `Escort{npc,dest_zone}`, and a `Repeat` (`Once`/`Daily`/`Weekly`). Per-player state: `board_progress` (accepted counters), `board_done` (one-offs claimed), `quest_cooldowns` (id→Unix seconds when a repeatable was last claimed), all persisted; plus a transient `escort: Option<EscortState>` (not persisted).

Examining a board (`use_board`): claims a finished bounty if ready (one-offs → `board_done`, repeatables → `quest_cooldowns`, re-available after `DAY_SECS`/×7 via `board_quest_available`), else posts the next available quest. Counter progress ticks via `bump_quests` from the kill / loot / room-enter paths. **Escorts** spawn a transient escortee that travels with the player; it is wounded by chance when the player is struck (`wound_escort`) and lost immediately on player death; reaching `dest_zone` with it alive completes the quest (`check_escort_arrival`, in `describe_room_context`). The escortee and active board quests surface in the room panel / quest journal.

---

## 3. Screen Lifecycle And Input Capture [STABLE]

- Lateania is no longer a top-level tab. It is launched from the Games hub (`late-ssh/src/app/door/hub`, page `3`), a selector that renders the selected door game's full landing; Lateania's landing is drawn by the now-`pub` `screen::draw_landing`, a single-column layout (logo, stats, actions) matching the NetHack/DCSS style, used both by the hub and the standalone screen fallback. `Screen::Lateania` is a live-world-only screen reached by pressing `Enter` on the selected Lateania card; that one keypress both switches the screen and joins the world (no intermediate standalone landing).
- `d` while Lateania is selected in the hub opens a destructive confirmation prompt to delete the current user's saved Lateania character. `Enter`/`Y` confirms; `N`, `d`, or `Esc` cancels (handled in the hub input, not the standalone landing).
- Launching Lateania creates `lateania::state::State`, subscribes to the shared service snapshot, and joins the persistent world.
- Leaving the active Lateania world drops its per-session state. `State::Drop` sends the service leave event.
- Navigating away from the Lateania screen also drops active Lateania state.
- Lateania is not an Arcade game and should not use `App::is_playing_game`; the app tracks active state by whether `App::lateania_state` is present.

Input capture contract:
- The Lateania landing page behaves like the Arcade lobby: screen switching and global shortcuts remain available unless the landing page itself handles the key.
- Active Lateania captures ordinary key input, including number keys, `Tab`, `Shift+Tab`, `q`, and single-byte global shortcuts.
- Active Lateania still allows `Esc` to leave the active world; it now returns to the Games hub (page `3`), not a standalone landing page.
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

### Active game keys

- Movement: `w/a/s/d`, `h/l` for west/east, and arrow keys for cardinal directions; `<` or `,` for up; `>` or `.` for down.
- The Matlatesh sea-gate into the Sundered Reaches requires `Bane of the King Who Was Promised Nothing` and uses the same transient two-step warning as the Frontier descent.
- The ash-gate down from Yssgar's Reaches chamber into Kaelmyr requires `Bane of Yssgar, the Sundering Deep` (`is_kaelmyr_gateway`, `KAELMYR_GATE_TITLE`) and uses the same transient two-step warning. It is the deepest end-game gate in the game.
- The first dungeon descent from Whisperwood into Duskhollow requires `Bane of the Elder Treant`.
- Living-dark entrances from the three capitals require `Bane of the Archdemon Mal'gareth`.
- The Town Square Frontier descent requires `Bane of the Archdemon Mal'gareth`, `Bane of The Bonewright Lich`, `Bane of the Elder Dryad`, and `Bane of the Abyss-Thing`; after those title gates, it still uses a transient two-step warning: the first `>` logs that the Frontier is older, meaner country for seasoned adventurers, and the next `>` confirms descent. Service-backed non-movement actions clear the pending warning.
- Combat: `space`, `x`, or Enter attacks when not in a list panel; `z` flees.
- Abilities: `1-9` use unlocked ability slots unless a list panel is open; `0` uses slot 10. The Abilities panel is a list panel: Enter casts the highlighted ability, which is the only way to reach rosters deeper than ten (the classic classes' late slots).
- World actions: `y` works a resource node in the room (chop/mine/fish/forage/skin - the highest tier you qualify for); `u` opens the crafting panel where a craft station stands; `i` opens "the Ways" fast-travel menu when standing on a waystone portal (moved off `y`, which gather uses); `m` toggles the **World Atlas** (a whole-world exploration overview: `World::region_progress` scores each of the `REGIONS` for visited/total rooms + boss count and flags the region the player stands in (`RegionProgress.here`, a `◈ you are here` marker), rendered as meters + `◆N` loot markers by `ui.rs::atlas_panel`); `r` recalls to Embergate's Town Square when out of combat; `;` **retreats to the nearest safe haven** (`svc::retreat_to_haven`: a BFS over walkable exits to the closest `safe` room, refusing mid-combat and never expanding through a progression gate the player's titles wouldn't pass, via the silent `gate_blocks` twin of `can_cross_progression_gate`) - deep in a maze it reads as "back to this zone's gate"; `f` toggles the Follow panel; `g` casts the Resurrection rite on the nearest fallen adventurer in the room (Cleric/Paladin/Druid only); `p` opens the Stable (companion vendor) where one stands; `q` opens the **Animal Taming** panel where a tameable wild beast roams (Enter attempts the tame); `n` opens the housing ledger (at the clerk, or inside a home you own); `e` opens the appearance/bio builder. In the Inventory panel `A`/`C`/`J` batch-sell all loose gear / commons / non-upgrades (keeping potions and worn gear); inventory and shop rows show both a stat-delta line and a coloured `▲+N%` / `▼-N%` upgrade tag vs. what's worn.
- Local chat: `'` opens a **say** compose line (`state.chat_buffer`; input capture runs at the top of `handle_key`, before the Esc-leaves check, so Esc cancels compose). Enter sends via the existing world-local `say` (room occupants, `LogKind::Say`); backspace edits; the prompt renders on a reserved bottom row in `draw_page`. **Lateania chat is world-local and never reaches late.sh's global feed** (`say` only `log_to`s in-world players; it does not publish to activity/#lounge).
- While dead (a corpse): all normal keys are suppressed; only `r`/Enter (release to the temple) and `Esc` (leave) respond, until a resurrection or the auto-release deadline.
- Panels: `c` character, `v` abilities, `t` inventory, `b` shop where a merchant exists, `o` examine/look, `k` titles, `j` quest journal, `f` follow.
- List panels: `w/s` or up/down move cursor; `1-9` jump and activate; Enter activates. The view auto-scrolls to keep the highlighted row within a small scroll-off margin (top and bottom).
- Cursor-less text panels (character/quests): `[` / `]` scroll. Both scroll offsets share one interior-mutable `list_scroll` on `state::State`, clamped to content by the render pass and reset on panel change.
- Inventory panel: `x` sells the selected inventory row when a shop is present.
- Follow panel: Enter follows/stops the selected in-room adventurer; `x` stops following whoever is currently followed, including absent/separated targets.
- `Esc` leaves active Lateania and returns to the Games hub.

### Panels

`state::Panel` variants:
- `Room`: current room, vitals, exits, mobs, occupants, wildlife, features, minimap, hints.
- `Character`: class, trait, scores, stats, titles, resurrection charges.
- `Abilities`: unlocked abilities, cost/readiness/effect.
- `Inventory`: pack items plus equipped items as rows.
- `Shop`: merchant stock if `shop_at(room)` exists.
- `Examine`: room features; fountains can restore vitals.
- `Titles`: earned titles; selecting active title again clears it.
- `Quests`: read-only Frontier zone quest list.
- `Follow`: current occupants, follow target tag, stop-follow action.
- `Crafting`: recipes worked at the craft station(s) in the room; select and Enter to craft.
- `Taming`: the tameable wild beasts roaming the room (Broceliande), each with its required Animal Taming level and your odds; select and Enter to attempt a tame.

UI uses a two-column layout with compact fallback for terminals narrower than 50 columns or shorter than 9 rows. The left column splits current room context (`Now`) from newest-first action scrollback (`Recent`) with a visible divider; the `Now` region wraps the room description naturally and only truncates the whole context as a last resort to preserve recent-event space. Service room-description lines use `LogKind::Room` and are filtered out of `Recent` so movement does not bury combat, loot, chat, and system events. Arrivals use compact `LogKind::Travel` breadcrumbs so Recent still shows where the player has just been. Consecutive identical recent events are collapsed with an `xN` suffix so repeated blocked-movement warnings do not flood the split.
In the Room panel, the minimap is rendered in a separate bottom-aligned side-panel region, not appended to the room detail lines; keep it anchored so changing foes/features/hints does not make the map jump vertically.
Room-panel variable text rows (zone, exits, features, foes, occupants, wildlife) should use the side wrapping helpers in `ui.rs` so long labels wrap within the side column instead of clipping against the border.
Non-Room side panels are rendered through `side_paragraph`, which enables Ratatui wrapping for long quest, inventory, shop, title, and ability rows.

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

### Features

- `FEATURES` contains lookable room features.
- `FeatureKind::Fountain` restores HP/resource and refreshes veteran resurrection charges only when examined in a safe room.
- `FeatureKind::Bank` toggles deposit/withdraw of all carried gold at the Embergate banker's grille. Banked gold is safe from death loss but must be withdrawn before shopping.
- `FeatureKind::Stable` (one per capital) is the **companion vendor**: `p` opens the Stable panel where `Enter` buys the selected beast and `x` feeds/tends your current one. `room_has_stable` gates `buy_pet`/`feed_pet`. **Adding a feature shifts `features_at` indices; tests must find features by kind, not position** (a stale hardcoded index broke the bank test when the stable was added).
- `FeatureKind::Housing` (the clerk at Hearthward Close) is the **housing ledger**: `n` opens it. At the clerk it lists **deeds** (`buy_deed` claims a free plot of that tier; one home per name); inside a home you own it lists the **furniture catalogue** (`buy_furniture` places a piece in the current room, shown to everyone via the room description). Placed furnishings live in `house_furniture` keyed by room; ownership in `plot_owner` keyed by tier/plot index.
- **Interactable features stand out by colour** (`ui.rs::interactable_color` + `is_actionable_feature`): things you *act on* (fountain green; bank/board/stable/clerk gold + bold + a `◆` marker) pop like loot, while purely lookable scenery (plaque/vista) reads a softer cyan with a `·` marker.
- `FeatureKind::Portal` is a **waystone**: `i` opens the fast-travel menu (`world::waystone_destinations()` = `CONTINENT_WAYSTONES` - Embergate's square plus each continent's safe gate room (Sunderlakes landing, Broceliande forest gate, Reaches sea-gate, Kaelmyr's Cinderfall Shore) - followed by `archipelago::portal_destinations()`, the villages + island landings); `travel` teleports out of combat. **Gated continents keep their locks through the Ways:** each `CONTINENT_WAYSTONES` entry carries the walking gate's required title (a drift test in `svc.rs` pins them to the gate consts), the snapshot marks locked entries `sealed` (rendered dim), and `svc::travel` re-checks the title server-side. Portal features for the runtime rooms are synthesised in `waystone_features()` (a `OnceLock` layer over the static `FEATURES`), since those rooms are generated, not authored.
- Plaques and vistas are descriptive.
- Room descriptions intentionally mention only feature names; the detailed text is revealed by `o` / Examine.

### Wildlife

- `WILDLIFE` is separate from combat mobs.
- `CritterKind::Skittish` is ambient.
- `CritterKind::Game` can be hunted by attacking when no combat mob is present. Hunted game grants small XP and is hidden by a per-world 40-second cooldown keyed by global wildlife index.
- `CritterKind::Boon(Perk)` applies on room entry. Perks are `Embolden`, `Mend`, and `Quicken`.
- Wildlife appears in the Room panel; game critters show as huntable only while off cooldown.

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
- Crafted outputs are ordinary items, so they equip / are consumed / sell through the existing systems (weapons & armor equip, potions & food heal/restore, poisons are sellable valuables until the depth update makes them applyable).
- The **Trades** block shows all ten trades (gather then craft); the recipe `inputs` are summarised as `"3x Copper Ingot, 1x Oak Plank"`.

### Crafting depth [VOLATILE]

- **Applied poisons**: using a crafted poison (`items::poison_tier` routes it out of the normal consumable path in `use_item`) coats the weapon - `PlayerState::weapon_poison = Some((per_tick, charges))` (transient, `POISON_CHARGES = 5`). Each landed melee strike in the combat round seeds a `DamageType::Poison` DoT on the struck foe via the existing `seed_mob_dot` and spends a charge; `POISON_PER_TICK` scales the damage by poison tier.
- **Cooking buffs**: eating crafted food (`items::food_tier`) heals/restores as a normal consumable *and* pushes a `HealOverTime` self-effect (well-fed regen, `WELL_FED_TICKS`), reusing the ability HoT tick.
- **Masterwork sinks**: two Legendary smithing recipes (`items::masterwork_id`, level 45) consume a heap of top-tier intermediates (8-10 mithril ingots + ironbark planks / dire leather) for gear a clear step above the tiered craftables - the endgame material sink.
- None of this adds save state (`weapon_poison` is transient); no schema bump.

### Animal Taming [VOLATILE]

- **The trade.** `Animal Taming` is an eleventh trade (`skills::TamingSkill`), levelled 1..=50 on the same shared curve as gathering/crafting. Its xp is a single `taming_xp: i64` on `PlayerState` (there is only one taming trade), persisted (schema **v14**, `#[serde(default)]`), shown as the last row of the character-sheet **Trades** block.
- **The fifty beasts.** `taming::TAMEABLE` is fifty tameable `PetSpecies` ordered **small → large** (hare → hedgehog → … → wolf → direwolf → cave-bear → jungle-drake → … → treant/World-Oak scion). Each carries a `tame_level` (the required Animal Taming level) that **rises across the fifty** so taming gets harder and harder — the biggest beasts need level 50. Stats scale with size (a bigger beast is a stronger companion). `wild_beasts()` homes each beast at its zone's **forest gate** (the safe entrance room, always real, `beasts_at(room)`), spread across Broceliande's 20 zones by difficulty. **Keys are `wt_*` and persisted — never reorder/rename.**
- **The action + panel.** Standing where a tameable beast roams, `q` opens the **Taming panel** (`Panel::Taming`, `TamingView`/`TameEntryView`): it lists the beasts here with each one's required level and your live odds (or "needs Taming N" / "spooked"). `Enter` attempts the selected tame (`svc::tame`/`tame_task`). Success is a roll against `tame_chance` (40% at the required level, +9% per surplus level, capped 95%; refused outright when under-level). On **success** the beast becomes your active companion (replacing any current one, like `buy_pet`), using the same runtime `Pet` (fights/fed/persisted identically), and trains Animal Taming xp. On **failure** the beast bolts and is `spooked` for `TAME_COOLDOWN` (30s, per-player-per-beast in `tame_cooldowns`). Clear log feedback throughout ("eyes you warily…", "You've earned its trust!", "shies, then bolts into the briars"). The room panel shows a **Wild beasts** section (`◾`/odds + the `q` hint).
- **Pet auto-skills.** A companion unlocks abilities as it levels, firing **automatically** in the combat round on their own cooldowns (`taming::PET_SKILLS`, resolved by `svc::fire_pet_skills` in the pet-bite step; cooldowns keyed by `world_ticks` in `pet_skill_cd`, lock-free/snapshot-only): **Savage Bite** (L3, bonus damage), **Rend** (L8, a `seed_mob_dot` bleed), **Intimidating Roar** (L15, owner empower), **Loyal Guard** (L22, owner shield/splash mitigation), **Killing Pounce** (L30, heavy burst that can finish the foe → credits the owner). Magnitudes scale with the pet's attack. Unlocked skills surface in the room-panel pet line and the snapshot `PetView.skills`.

### Frontier and Reaches loot

- `items::FRONTIER_TIERS = 20`, one tier per Frontier zone; `items::REACHES_TIERS = 20`, one per Sundered Reaches zone; `items::KAELMYR_TIERS = 20`, one per Kaelmyr zone.
- Generated Frontier item IDs are `3000..3200`; generated Reaches IDs are `3200..3400`; generated Kaelmyr IDs are `3400..3600` (all 20 tiers times 10 slots, built by the shared `build_generated_items`).
- `item(id)` searches authored `ITEMS`, the generated Frontier catalog, the generated Reaches catalog, and the generated Kaelmyr catalog.
- Reaches spawns drop `reaches_loot(zone)`; the Reaches power curve continues the Frontier's (tier 0 lands just above Frontier tier 19). Kaelmyr spawns drop `kaelmyr_loot(zone)` with `power_offset = FRONTIER_TIERS + REACHES_TIERS`, so Kaelmyr tier 0 lands just above Reaches tier 19 — a real gear step past Yssgar.
- Frontier mob and boss loot tables use `frontier_loot(zone)`, which includes representative weapon, head, chest, hands, ring, draught, and relic entries for the zone tier.
- Frontier item generation now starts at post-living-dark power and climbs hard across all 20 tiers; regional boss loot is authored, meaningful post-Archdemon gear, while Frontier remains the best long-term gear path.
- Early Frontier regulars are tuned as endgame mobs: tests keep the first Frontier regular above the strongest living-dark boss damage while still below the first Frontier boss.

---

## 7. Progression, Combat, And Economy [VOLATILE]

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
- Level cap is `Class::MAX_LEVEL = 50`.
- `xp_for_level` keeps early levels quick, then adds a much steeper post-level-8 term so midgame and Frontier progress target roughly week-scale casual play instead of a 1-2 sitting clear; `level_for_xp` caps at 50.
- `Class::stats_at(level)` computes HP/resource/attack/resource regen.
- Ability scores are rolled before class selection and persist after class choice.
- Constitution adjusts max HP by level; class primary score adjusts attack.

### Abilities and damage

- `AbilityEffect` variants: `Strike`, `DamageOverTime`, `Heal`, `HealOverTime`, `Empower`, `Ward`, `Stun`, `Finisher`.
- Every class has a level-1 ability and a level-50 capstone; the classic five carry 12 abilities, the original newer seven carry 10 (each gained a level-28 ability in the Reaches expansion), and the five newest (Beastlord/Skald/Runemaster/Valewalker/Spiritmaster) carry 10 each. Slots past the 1-9/0 hotbar cast from the Abilities panel.
- Offensive abilities require a target. Heals, buffs, and wards do not.
- Damage schools: Physical, Fire, Frost, Holy, Shadow, Poison, Arcane, Lightning.
- `DamageProfile` lets each mob deal one attack type, resist up to one incoming school, and be weak to up to one incoming school.
- Resist halves damage, weak adds 50 percent, and minimum damage is 1.
- Auto-attacks are physical and still pass through mob resistances.

### Combat rules

- `engage` targets the first alive mob in the current room unless the room is safe.
- Movement and recall are blocked during combat; flee clears target and moves through the first available room exit, or only breaks combat if no exit exists.
- Rogue opening strike doubles the first auto-attack after engaging.
- Mage offensive spell damage is boosted by `Arcane Mastery`.
- Cleric healing is amplified by `Light of the Dawn`.
- Ranger damage is boosted against wounded targets below half health.
- Warrior survives the first lethal blow of each life at 1 HP.
- Veteran accounts, checked on join by account age, can resurrect in place while charges remain; fountains refresh charges.
- **Combat companions.** A pet bought from a capital Stable (`buy_pet`, one at a time; a new purchase releases the old) rides on `PlayerState` and so is always in its owner's room. In the combat round it **bites the owner's target** after the owner's strike (crediting the kill to the owner); when the owner is struck, `wound_pet` splashes `PET_WOUND_PCT` of the blow onto it (alongside `wound_escort`), **but only on survivable hits**, since the death branch takes no `wound_*` (combat is over once you fall). A pet at 0 HP is **downed** and stops fighting until **fed** (`feed_pet` at a Stable: revive + heal to full + `FEED_LOYALTY`, costing `PET_FEED_COST`). Loyalty raises the pet's level (more HP/attack). Persisted by species key + loyalty.
- **Death & resurrection.** A lethal blow with no Warrior death-save and no veteran charge leaves the player a **corpse where they fell** (`dead = true`, hp 0, target/shield/empower cleared, 20% carried gold lost, escort lost; banked gold protected). The corpse lingers (`respawn_at = now + CORPSE_LINGER_SECS`). The player chooses: **wait** for a resurrection, or **release** to the temple now (`release_to_temple`, `r`/Enter while dead). If neither happens by the deadline the tick auto-releases them. **Resurrection** is a rite of the holy/nature callings (`Class::can_resurrect` → Cleric/Paladin/Druid): a living caster in the same room spends `RESURRECT_COST` to raise the nearest corpse **in place** at `RESURRECT_HP_PCT` of max (`resurrect_nearest`, `g` key). The snapshot exposes `dead`, `can_resurrect`, `corpse_here`, and per-occupant `alive` so the UI shows the fallen overlay, a `(fallen)` roster tag, and the rez hint. The dead state is **transient** (not persisted; a reload returns the character alive at a safe room).
- `seed_world()` applies a balance scaler after all authored/overworld/Frontier/living-dark spawns are generated: authored regular mobs are modestly tougher with a small XP bump and faster respawns, authored bosses gain larger HP/damage bumps with lower XP, living-dark mobs/bosses become hard post-Archdemon progression, and Frontier mobs/bosses scale sharply above them while Frontier regulars remain rewarding enough to grind. The Sundered Reaches deliberately ride the same Frontier multipliers (their authored base stats sit on the same pre-scale curve): the Reaches enter just under the King Who Was Promised Nothing and climb well past him, ending at Yssgar, the strongest and best-rewarded fight in the game.

### Items, shops, and rewards

- Equipment slots: Weapon, Head, Chest, Legs, Hands, Feet, Ring, Trinket.
- Item rarities: Common, Uncommon, Rare, Epic, Legendary.
- Item kinds: Equipment, Consumable, Valuable.
- Valuables, including Frontier relics, show a `valuable / sell Xg` stat line in inventory/shop UI so players know they are sell loot; generated Frontier relic descriptions also state that they have no combat use.
- Gear rows in the inventory and shop panels carry a **comparison line** vs. what's worn in that slot (`InvView::compare`/`ShopEntryView::compare`, built by `svc::compare_to_worn`): green upgrade / red downgrade / amber trade-off, or "new slot"; empty for the worn item and non-gear (`ui::compare_line`).
- Starter inventory is a Rusty Shortsword and two Minor Healing Draughts. Starting gold is 120.
- Shops are in Embergate: Ember Forge, Outfitter, Apothecary, and Curio Cart.
- Shop economy intentionally includes expensive late-game gold sinks: masterwork weapon/armor/head/hands, premium curio gear, and the repeatable Phoenix Tonic. The masterwork shop pieces are shop-stock, not boss drops, so gold remains useful after normal boss clears.
- Apothecary consumables are tuned as the pressure valve for harder combat: early draughts are affordable recovery, Elixir of Renewal covers mid/late mixed HP/resource recovery, and Phoenix Tonic is a repeatable expensive late-game recovery sink.
- Authored boss loot tables include head and hand upgrades across tiers; living-dark bosses add controlled post-Archdemon unique gear, while their regular mobs mostly drop regional relics and sustain consumables.
- Bosses always drop one item from their loot table. Regular mobs have a modest chance if their table is non-empty.
- Mob kills grant XP, reduced gold, possible loot, and titles. Boss XP and Frontier quest XP/gold bounties are intentionally damped so boss chains do not skip too much of the level curve.
- Boss title format is `Bane of ...`; lesser foes grant a derived `...bane` title.
- Frontier boss kills complete their zone quest, award XP/gold, and grant `Champion of the <zone>`.
- Defeating the authored final boss, the Archdemon Mal'gareth, pays a once-per-account 10,000 chip lifetime payout and grants the `LMG` profile-award badge; repeat kills can still grant normal in-world rewards but not the chip payout again.
- Defeating the final Frontier boss, the King Who Was Promised Nothing, pays a once-per-account 20,000 chip lifetime payout and grants the `LKN` profile-award badge; repeat kills can still grant normal in-world rewards but not the chip payout again.
- Defeating the final Reaches boss, Yssgar, the Sundering Deep, grants the once-per-account `LYS` profile-award badge with **no chip payout** (`BossAchievement.payout: None`); the badge is the whole prize, keeping the chip economy flat. Defeating Kaelmyr's last boss, Kaethyr Ascendant, Who Sang the God Awake, likewise grants the once-per-account `LKA` badge, no chips (the Unquenched Throne's Kaethyr the Unquenched carries no achievement; only the Ascendant form at the Sundering Wound does). Badge codes are named after the boss (Mal'Gareth, King/Nothing, YSsgar, KAethyr Ascendant), and chat author labels collapse to the highest crown (`LKA` > `LYS` > `LKN` > `LMG`).
- Every mob kill emits a Lateania activity win event (dashboard/quest tier only; excluded from the #lounge feed). Only the **four named realm crowns** — the ones `boss_achievement_for` recognizes (Archdemon Mal'gareth, the Frontier King, Yssgar the Sundering Deep, Kaethyr Ascendant) — publish a structured `BossSlain` event to #lounge; the ~9 regional/zone bosses (`MobSpawn.boss` without an achievement) fall too often and stay dashboard-only. `publish_kill_outcome` therefore gates the `BossSlain` on `outcome.achievement.is_some()`, not on the `boss` flag (the flag was dropped from `KillOutcome`). A player materializing in the world via `join` publishes `GameStarted`, which also ships to #lounge through `app/activity/lounge.rs`. Final-boss kills route through lifetime reward templates; if the chip payout was already claimed, activity still records the defeat without the chip/badge detail.

---

## 8. Persistence [STABLE]

### Character save

Character persistence uses `late_core::models::mud_character` / `mud_characters`.

Saved character schema version: `14`.

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
- owned housing plot (tier index) + placed furnishings as (room, key) pairs (re-registered into `plot_owner`/`house_furniture` on load);
- appearance/bio trait indices (`Vec<u8>`, clamped to valid options on load);
- gathering-skill xp as (skill key, total xp) pairs (unknown keys dropped on load);
- crafting-skill xp as (skill key, total xp) pairs;
- Animal Taming xp as a single `taming_xp` value (schema v14; `#[serde(default)]`, so pre-v14 saves start the trade untrained).

Transient by design:
- current target;
- active effects, cooldowns, shields, buffs, stuns;
- player respawn timer;
- follow target;
- pending activity events.

Unclassed characters are not exported. Empty or unreadable blobs are treated as no save.

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
- Keep class keys and item IDs stable once persisted.
- Keep generated Frontier ID ranges aligned: 20 zones, 20 item tiers, IDs `3000..3200`, Frontier rooms at `2000+`, Frontier mob IDs at `900000..950000`.
- Keep generated Reaches ID ranges aligned: 20 zones, 20 item tiers, IDs `3200..3400`, Reaches rooms at `10000+`, Reaches mob IDs at `950000..960000`. `tune_spawn_balance` classifies by these ranges; the Reaches intentionally share the Frontier's endgame multipliers.
- Keep generated Kaelmyr ID ranges aligned: 20 zones, 20 item tiers, IDs `3400..3600`, Kaelmyr rooms at `12000+`, Kaelmyr mob IDs at `960000+`. `tune_spawn_balance` classifies Kaelmyr into the same endgame band as the Frontier/Reaches (its authored base stats simply sit a full continent higher on the curve). When adding Kaelmyr zones, update `KAELMYR_ZONES_DATA`, `KAELMYR_TIERS`, `kaelmyr_loot`, board-quest zone tests, the room-count band, and the shape test together — and zone-name fields must NOT start with "The " (the builder prepends it). Kaelmyr also has a `REGIONS` atlas entry (range derived from the zone consts) — every continent must appear in `REGIONS` or the atlas silently omits it.
- Keep the Sunderlakes ID ranges aligned: 14 zones, rooms at `16000+` (11×8 = 88 per zone, `LAKES_BASE`/`LAKES_ZONE_STRIDE`), mob IDs at `980000+` (`LAKES_SPAWN_ID_START`, a fresh band above Kaelmyr's `960000+` and the Archipelago's `970000+`), and the 40 fish items at `4600..4700` (`FISH_BASE`/`FISH_COUNT`, clear of materials `4000..4100`, crafted `4200..4500`, generated loot `3000..3600`). The Sunderlakes have no generated gear catalog — loot is fish. When changing the Sunderlakes, update `LAKES_ZONES_DATA`, `is_lakes_room`, the `REGIONS` atlas entry, the room-count band, the shape test, and the fish-node test together.
- Keep the Broceliande ID ranges aligned: 20 zones, rooms at `22000+` (11×9 = 99 per zone, `BROCELIANDE_BASE`/`BROCELIANDE_ZONE_STRIDE`, both public so `taming.rs` can place beasts), mob IDs at `990000+` (`BROCELIANDE_SPAWN_ID_START`, a fresh band above the Sunderlakes' `980000+`). Broceliande has no generated gear catalog — `broceliande_loot` borrows the Frontier tiers, which resolve through `item()`. **Its mob band `990000..` is deliberately cut out of the `kaelmyr` classification in `tune_spawn_balance`** so the Greenwood keeps gentle overworld multipliers, not the endgame ones (the lakes/archipelago at `970000..990000` still ride the endgame band with tiny base stats). When changing Broceliande, update `BROCELIANDE_ZONES_DATA`, `is_broceliande_room`, the `REGIONS` atlas entry, the room-count band + region sum, the shape/reachability tests, and — since the beasts are placed by zone — the `taming::wild_beasts` mapping together.
- Keep the fifty tameable beasts (`taming::TAMEABLE`) and their `wt_*` keys stable once persisted; `tame_level` must stay non-decreasing across the list (small→large, easy→hard). `pet_species_by_key` must keep searching both `PET_SPECIES` and `TAMEABLE`, or saved tamed pets won't reload.
- When adding rooms, keep every exit target real, every room reachable from start, and every mob home valid.
- When adding boss or mob loot, every item ID must resolve through `item(id)`.
- When adding Frontier zones, update `FRONTIER_ZONES_DATA`, `FRONTIER_TIERS`, loot generation, quest mapping tests, and room-count expectations together.
- `seed_world()` leaks generated strings to `'static`; this is acceptable for one process lifetime and current tests, but avoid adding per-tick/per-request leaks.
- Active Lateania captures ordinary keys. Parent/global shortcuts must remain governed by the app-level dispatch code and root context.
- The `door` folder is a grouping folder. Keep Lateania-specific behavior in this context instead of creating a separate `door/CONTEXT.md`.
- Shared door-game host contracts live in sibling `door/game.rs`. Keep that interface minimal; do not push Lateania-specific state into the shared trait.

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

Expected focused command for human verification after Lateania changes:

```bash
cargo test -p late-ssh lateania
```

Put DB/service orchestration tests that cannot stay pure in adjacent `_test.rs` files beside the module they exercise; everything else stays inline and pure.

---

## 11. Known Gotchas And Future Work [VOLATILE]

- Some comments in `world.rs` may lag current content scale. Trust current tests/data: ~2600 rooms across base/overworld/Frontier, the three living-world regions, housing, city districts, and the ~900-room Sundered Reaches (see the room-count test's per-region ranges).
- `follow_task` still exists as an old toggle service command, but current input opens the Follow panel and uses `follow_to_task` / `stop_follow_task`.
- `say_task` exists, but active Lateania has no typed command prompt yet.
- Inventory snapshots include equipped items after pack items. Equip/use/sell mutations usually require the item to still be in `inventory`, so equipped-row activation is often a no-op.
- Inventory rows wrap in the side panel and equipped rows include their worn slot, e.g. `[worn weapon]` or `[worn chest]`.
- `view.occupants` includes other players in the room regardless of class; service follow selection only allows classed targets in the same room.
- Boon perks apply on room entry and can spam log lines if movement loops through boon rooms.
- Hunted game cooldowns are not persisted across process restart.
- World content is authored as Rust data. A future data-file loader should preserve the existing `World`, `Room`, `MobSpawn`, `Feature`, and `CritterSpawn` shapes.
