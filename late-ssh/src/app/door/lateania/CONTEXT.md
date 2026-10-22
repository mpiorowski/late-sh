# Lateania Game Context

## Metadata
- Scope: `late-ssh/src/app/door/lateania` plus Lateania screen lifecycle in `late-ssh/src/app/door`
- Domain: Lateania, the persistent D&D-style MUD inside late.sh
- Primary audience: LLM agents changing the Lateania game runtime, content, UI, combat, or persistence
- Status: Active
- Parent context: `../CONTEXT.md` (the door family)
- Stability note: Sections marked `[STABLE]` should change rarely. Sections marked `[VOLATILE]` are expected to change when gameplay/content changes.

---

## 0. Context Maintenance Protocol [STABLE]

Read this file after `../CONTEXT.md` whenever a task touches Lateania's landing page, launch/leave behavior, reset prompt, active-world input capture, game runtime, content, UI, combat, or persistence.

**Companion files in this directory:**

- **`PROGRESSION.md`** - the world map: every land's levels, health, xp rate and gear tier, the fourteen crowns, the gate spine, the connection graph, and the measured power ladder to read before re-tuning any of it, plus the balance audit (where a character's damage comes from, trait scaling, known defects). **It owns those numbers and this file keeps no copy**, so a land's figures are changed there and nowhere else. Read it for any task touching where a land sits, what it drops, or what opens it.
- `WORLD.md` - every land and how it is carved, the room graph, features, wildlife, gathering, crafting, taming, the Wildbound Waste, the tutorial, the quest lines, and the world-content invariants (ID ranges). Read it before adding rooms, mobs, items or beasts.
- `COMBAT.md` - classes and ability scores, abilities and the damage formula, the world resist/weak pass, combat rules, companions, items, shops and rewards.
- `UI.md` - class selection, the active game keys, every panel and layout, the appearance builder and portrait.
- `WORLDMAP.md` - the overhead field and the land map: chrome shedding, geometry and its safeguards, stairs and POI arrows, the authored atlas.
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
- `Screen::Lateania` has no top-level number key. It is reached by selecting the Lateania card in the Games hub (page `3`) and pressing `Enter`, which opens the Lateania landing: the account's character slots (§8). `Enter` on a slot joins the live world with that character; `d` deletes it behind a confirm.
- One shared `LateaniaService` owns authoritative `WorldState` behind a Tokio mutex.
- Each connected session owns a lightweight `state::State` with a cached `MudSnapshot`, local side-panel state, and a list cursor.
- Commands are fire-and-forget service tasks. The UI renders snapshots and may briefly show old state.
- The world ticks every 2 seconds for combat rounds, effects, cooldowns, mob/player respawns, idle drops, and activity feed kill events.
- Character state and shared world state persist separately.

Current game scale:
- `seed_world()` starts at Embergate room `1`.
- The world holds ~11110 rooms across the capitals, the overworld, the Frontier, the three living-world regions, and the continents (the Sundered Reaches, Kaelmyr, the Sunderlakes, Broceliande, Aelunor and its city Silvael, the Shattered Archipelago, Thornveil Falls, the Wildbound Waste). `WORLD.md` describes each land: its room and mob id ranges, how it is carved, and what it is for.

---

## 2. Module Map [STABLE]

| File | Responsibility |
|---|---|
| `../game.rs` | Minimal host-facing door-game contract: id/title/description, render/input/leave hooks, optional activity mapping, and generic outcome events. |
| `mod.rs` | Module declarations and Lateania credits. Keep declaration-only. |
| `screen.rs` | Top-level Lateania screen shell and `DoorGame` implementation: landing page, launch/reset/leave input, and active-world render delegation. |
| `state.rs` | Per-session client wrapper: snapshot receiver, local `Panel`, cursor, join retry, action delegation. Never mutate game truth here. |
| `input.rs` | Active-world key routing after launch. App-level launch/reset/leave handling belongs in `screen.rs`. |
| `ui.rs` | Ratatui rendering for class select, log, compact mode, the live field (full-width on narrow terminals), side panels, hints. The Character panel expands to a full-width dashboard (accent-tinted class portrait, dot-rated ability scores, vitals/XP meters) when the area is at least 72x18, else falls back to the narrow side panel. **That dashboard has no scroll of its own** (`[`/`]` only reach `draw_side`), so every column has to fit: keep unbounded lists (titles) last and capped, and Experience above them. Foes/Adventurers/Follow render as aligned roster rows with HP meters. Lock-free, snapshot-only. |
| `worldmap.rs` | The overhead world map's derived coordinate field, streaming viewport, fog of war, POI index, and camera. Pure and process-global; see `WORLDMAP.md` for its invariants. |
| `svc.rs` | Authoritative runtime: service tasks, `WorldState`, player/mob state, combat, movement, following, shops, persistence, snapshots, activity events. |
| `world.rs` | Immutable world data and generation: rooms, exits, mobs, features, wildlife, overworld, Frontier. Also **resource nodes** (`NODES`/`ResourceNode`/`nodes_at`/`node_index`): trees, ore veins, fishing spots and herb/skinning patches keyed to rooms, modelled exactly like `WILDLIFE` (static data + a per-node service cooldown), each with a skill, tier, min-level gate, and derived yield item. |
| `classes.rs` | The seventeen playable classes: resources, passive traits, per-level stat curves, the XP curve, level milestones, and the `ARCHETYPES` table. What adding a class touches is in `COMBAT.md`, Classes and scores. |
| `abilities.rs` | Ability roster and unlock helpers. Effects are data, resolved in `svc.rs`. |
| `housing.rs` | Player housing data + address arithmetic. `TIERS` (5 homes Hut→Tower: price/ground/upper rooms), the 50+-piece `FURNITURE` catalogue, `HOUSING_BASE`/`plot_base`/`plot_of_room`/`is_housing_room`. Homes are **static rooms** (generated in `world.rs::extend_housing` as Hearthward Close off Market Row); only **ownership** (`plot_owner`) and **furnishings** (`house_furniture`) are dynamic side-state on `svc.rs`, so movement/visiting/snapshot work unchanged and the homes are public shared-world plots. |
| `appearance.rs` | Character appearance/bio. `FIELDS` (Build/Hair/Eyes/Bearing/Origin/Mark/Manner, each with a menu of options) + `compose_bio`. The TUI has no free-text, so a player customises by cycling preset options (`e` opens the Appearance panel; `Enter`/`x` cycle a field). Stored as `[u8; N_FIELDS]` on `PlayerState`, persisted (missing fields default cleanly on load), shown on the sheet and when profiling another adventurer (Follow panel). The composed portrait is in `UI.md`, Appearance and portrait. |
| `archipelago.rs` | The **Shattered Archipelago** data + address arithmetic: the four portal `VILLAGES` (rooms `8000+`), the 20 `ISLANDS` theme table (rooms `20000+`), `island_entrance`/`village_room`/`is_archipelago_room`/`has_waystone`, and `portal_destinations()` (the fast-travel menu). Rooms are generated in `world.rs`; the portal teleport (`travel`) lives in `svc.rs`. |
| `pets.rs` | Combat companions. `PetSpecies` data table (`PET_SPECIES`, `pet_species_by_key`) of buyable beasts, and the live `Pet` (held on `PlayerState`, always co-located with its owner). Loyalty (earned by feeding) drives the level via a pure function; `max_hp`/`attack` scale with level. The rest is in `COMBAT.md`, Combat companions. |
| `taming.rs` | The **Animal Taming** trade: `TAMEABLE` (the sixty Broceliande and Wildbound beasts, `tame_level` 1..100) and `AELUNOR_TAMEABLE` (five more), `wild_beasts()`/`beasts_at(room)`, `tame_chance`/`tame_xp`, and the pet auto-skill ladders (`PET_SKILLS`, `pet_skills_at`). `WildBeast.species` indexes the **combined** pool (`TAMEABLE` then `AELUNOR_TAMEABLE`), so resolve it through `beast_species(index)`, never by indexing `TAMEABLE`. Data and pure maths only; the action, panel and combat wiring is in `svc.rs`/`state.rs`/`ui.rs`. See `WORLD.md`, Animal Taming. |
| `items.rs` | Item catalog, equipment slots, consumables, valuables, shops, generated Frontier loot. Also the raw-material, crafted-goods and fish catalogs, all chained into `item()`; see `WORLD.md`, Crafting. |
| `skills.rs` | **Gathering skills** (`GatherSkill`: Woodcutting/Mining/Fishing/Foraging/Skinning) and **crafting skills** (`CraftSkill`: Smithing/Woodworking/Leatherworking/Alchemy/Cooking), both on one 1-50 xp curve (`xp_for_skill_level`/`skill_level_for_xp`/`skill_progress`), independent of class level and steepening past level 10. Persisted per-player as (skill key, xp) for each set. |
| `crafting.rs` | **Recipes** (`Recipe`/`recipes()`/`recipe(i)`/`recipe_indices_for(skill)`): inputs -> output, gated by a `CraftSkill` + level. 50 recipes (10 per tier x 5 tiers) that **chain** (ore -> ingot -> weapon). Data only; `svc::craft` resolves and applies them. Built at runtime and cached (inputs are `Vec`, not a leaked slice). |
| `damage.rs` | Damage schools, mob resistance/weakness profiles, damage multiplier math. Also `ZoneTheme`: the closed 16-variant theme vocabulary of the world resist/weak pass, each an exhaustive const mapping to `(resist, weak)` (see `COMBAT.md`, "The world resist/weak pass"). |
| `stats.rs` | D&D-style ability scores: 4d6-drop-lowest rolls, modifiers, the six pure hooks (`swing_pct`, `crit_pct` + `crit_outcome`, `hp_bonus`, `spell_power_pct`, `regen_bonus`, `price_pct`/`tame_pct`), the wording that explains them (`Score::rule`, `AbilityScores::effect`), and the point economy (`points_earned`, `raise`, `SCORE_CAP`, `ScoreOfferView`). |
| `persist.rs` | JSON schemas for durable character saves and shared world saves. Versioned (`SCHEMA_VERSION`); new fields use `#[serde(default)]` so old saves load (e.g. `board_progress`/`board_done` for quests). |


---

## 3. Screen Lifecycle And Input Capture [STABLE]

- Lateania is not a top-level tab. It is launched from the Games hub (`late-ssh/src/app/door/hub`, page `3`), a selector that renders the selected door game's card; `screen::draw_landing` is a single-column layout (logo, stats, character slots, actions) matching the NetHack/DCSS style, used both by the hub card and by `Screen::Lateania` while no world is joined. `Enter` on the Lateania card switches to `Screen::Lateania`, which shows that landing as a character-select list: `j`/`k` move the slot cursor, `Enter` joins the world with the highlighted slot (`select_slot` + `App::enter_lateania`).
- `d` on the landing opens a destructive confirmation prompt to delete the highlighted slot's character (refused while the slot list is still loading). `y`/`Enter` confirms; `n`, `d`, `q`, or `Esc` cancels (`screen::handle_delete_confirm_key`). The hub itself offers no delete for Lateania.
- The landing's character list lives in the service (`LateaniaService::character_slots` -> `SlotList`), because the render path cannot await a query. The database is read **once per account** (`fill_slots_task`, kicked from `set_screen` into the landing or the Games hub, which is also the retry if that read fails); a read never replaces a list that is already `Ready`, so a query landing late cannot walk a fresher row backwards. Every change after that read rides the write that made it: `persist` and `delete_character_task` write their own slot's row (`publish_slot`). On top of the stored rows, `character_slots` lays any staged-but-unwritten save (`prepared_saves`, always newer than the row on disk) and then the live character's row off the world snapshot, so a character created minutes before its first save is never drawn as an empty slot. `SlotList::Loading` is its own state: an unread list reads "loading" and refuses `d`, rather than inviting you to start a new character over one it has not heard about. `App::tick` compares the list on the two screens that draw it and pays a frame when it moved, so a change made by another task is on screen within one idle tick (500ms); the `App::repaint_signal` handed to `fill_slots_task` and to a delete only makes those two immediate.
- Launching Lateania creates `lateania::state::State`, subscribes to the shared service snapshot, and joins the persistent world.
- Leaving the active Lateania world drops its per-session state. `State::Drop` sends the service leave event.
- Navigating away from the Lateania screen also drops active Lateania state.
- Lateania is not an Arcade game and should not use `App::is_playing_game`; the app tracks active state by whether `App::lateania_state` is present.

Input capture contract:
- The Lateania landing page behaves like the Arcade lobby: screen switching and global shortcuts remain available unless the landing page itself handles the key.
- Active Lateania captures ordinary key input, including number keys, `Tab`, `Shift+Tab`, `q`, and single-byte global shortcuts.
- Active Lateania still allows `Esc` to leave the active world, behind a confirming second press (Esc-Esc); leaving drops the session and shows the landing again.
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

In `UI.md`: class selection, the active game keys, every panel and its full-screen layout, the room rail, the narrow (phone) field, and the compact fallback. The map pages are in `WORLDMAP.md`.

## 6. World And Content [VOLATILE]

In `WORLD.md`: every land and how it is carved, the room graph, features, wildlife, gathering, crafting, animal taming, the Wildbound Waste, Wayfarer's Hollow, the board quests, the starter chain and the Long Road.

---

## 7. Progression, Combat, And Economy [VOLATILE]

- `PROGRESSION.md`: the crowns, the lands (levels, health, xp rate, gear tier, how each is reached), the gate spine, the connection graph, the measured power ladder, and the balance audit. It owns those numbers; no other file keeps a copy.
- `COMBAT.md`: classes and ability scores, abilities and the damage formula, the world resist/weak pass, combat rules, and items, shops and rewards.

## 8. Persistence [STABLE]

### Character save

Character persistence uses `late_core::models::mud_character` / `mud_characters`.

Saved character schema version: `21`.

Durable fields:
- class key, XP, level, carried gold, banked gold, current HP;
- saved room, but hydration only restores it if the room still exists and is safe;
- visited rooms, for the overhead map's fog of war;
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
- starter-chain progress as `starter_stage` + `starter_kills` (schema v19; `#[serde(default)]`, and hydration marks the chain complete for pre-v19 saves at level ≥ 10);
- ability-bar order as ability ids (`ability_order`; schema v21, `#[serde(default)]`, empty means the natural unlock order, resolved through `abilities::ordered_for`).

Transient by design:
- current target;
- active effects, cooldowns, shields, buffs, stuns;
- player respawn timer;
- follow target;
- pending activity events.

Unclassed characters are not exported. Empty or unreadable blobs are treated as no save.

### Character slots

An account can keep up to `svc::CHARACTER_SLOTS` (5) saved characters, so trying another class never means wiping the one you already have. `mud_characters` is keyed by `(user_id, slot)`; `slot` defaults to 0 (migration 139).

The world identity a session plays is always still the account's own `user_id` - only the DB row `join`/`leave`/autosave read and write changes with the slot. Everything downstream of join (combat, pvp, leaderboard, inventory, the whole `WorldState.players` map) is completely unaware slots exist.

**Two slot maps, and conflating them loses saves.** The world holds one player per account, so exactly one character is live at a time, and "which slot did the landing ask for" is a different question from "which slot is the character in the world from":

- `active_slot` is *intent*: set by `LateaniaService::select_slot` (called from the landing before `enter_lateania`), account-wide, changed by every Enter from any connection. It is read in exactly one place, the `join_task` that creates the world player.
- `live_slot` is *fact*: bound by that same `join_task` at the moment `join` creates the player and released where the player is removed (`leave_task`, `delete_character_task`). `prepare_persist` resolves the target slot from here and takes no slot argument at all, so no caller can name the wrong one; it returns `None` when nothing is live to save.

Never read `active_slot` at save time: a second connection opening the landing and selecting another slot would redirect the *live* character's autosave on top of the character saved there, destroying it (`a_second_session_picking_another_slot_cannot_overwrite_the_live_character` pins this). A second session for an account already in the world attaches to the live character and is told so in the log; its slot pick simply loses.

The landing (`screen.rs`) is a character-select list, not a single "Enter"/"d" pair: `j`/`k`/arrows move a slot cursor (`App::lateania_slot_cursor`), Enter calls `select_slot` then `enter_lateania`, and `d` resets whichever slot is highlighted (not necessarily the one currently live). The rows it shows (`SlotSummary`: occupied?, class, level) come from `LateaniaService::character_slots`, whose freshness rules are in §3. The Games-hub card only navigates to the landing (it cannot skip character select), and the hub's quick-delete excludes Lateania: the landing's per-slot delete is the only way to delete a character.

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
- **World-content invariants live in `WORLD.md`, Invariants**: every land's ID ranges (rooms, mobs, items), the tameable and Stable keys, the one companion ladder, no mounts, Wayfarer's Hollow, and the rules for adding rooms, loot and Frontier zones. Read them before adding content.
- **A crown is named in three places that must agree**: its spawn literal in `world.rs`, its `CROWNS` row (`tune_crowns` panics at seed if a row has no spawn), and the arena's `CROWN_TARGETS` (`arena_test.rs`). Renaming a boss means all three. When adding Kaelmyr zones, update `KAELMYR_ZONES_DATA`, `KAELMYR_TIERS`, `kaelmyr_loot`, board-quest zone tests, the room-count band, and the shape test together — and zone-name fields must NOT start with "The " (the builder prepends it). Kaelmyr also has a `REGIONS` atlas entry (range derived from the zone consts) — every continent must appear in `REGIONS` or the atlas silently omits it.
- **The measured power ladder lives in `PROGRESSION.md`.** `tier` is a generator input, not a power level, and is not comparable across bands (`tune_spawn_balance` applies a different hp/dmg/xp row per `Band`, so two lands at the same tier can be a factor of three apart). Every measured level, health and xp figure, plus the four rules for re-tuning one, is in that file. Do not copy any of it back here.

- **Never bind a Lateania key on `?`.** late.sh reserves it globally across every door game for a cross-door help overlay (`app::input::door_games_allows_global_help`), checked *before* `Screen::Lateania` ever routes a byte into this door's own `handle_key` - a `b'?'` arm in `input.rs` compiles fine and is simply never reached. Before adding a new key, grep the app-level `late-ssh/src/app/input.rs` for the byte too, not just this module's own `input.rs`.
- **Esc must always reach `input::handle_key` first**, never be special-cased earlier in `screen.rs` **or in the app-level `app::input::dispatch_escape`**. There are two interceptions to keep removed, not one: `dispatch_escape` runs *before* screen dispatch, so while `lateania_state` is live it must forward the byte to the door (`screen::GAME.handle_key`) and only route on to the Games hub if the door actually left. Fixing only the `screen.rs` half leaves Esc quitting on a single press through this path (`one_escape_never_drops_you_out_of_an_active_lateania_world`, in `app/input_test.rs`, pins both). That function is what (a) cancels an in-progress chat compose on Esc instead of leaving (`chat_active()` is checked before the leave branch) and (b) gates an actual leave behind a confirming second Esc within `State::LEAVE_CONFIRM_SECS` (`arm_leave_confirm`/`confirm_leave`, shown in the title bar via `leave_confirm_pending`). An earlier interception of `byte == 0x1B` that calls `app.leave_lateania()` skips both, so a single stray Esc (including one meant to cancel chat) logs a player out.
- `seed_world()` leaks generated strings to `'static`; this is acceptable for one process lifetime and current tests, but avoid adding per-tick/per-request leaks.
- Active Lateania captures ordinary keys. Parent/global shortcuts must remain governed by the app-level dispatch code and root context.
- The `door` folder holds the door family; what every door shares is `../CONTEXT.md`. Keep Lateania-specific behavior here, not there.
- Shared door-game host contracts live in sibling `door/game.rs`. Keep that interface minimal; do not push Lateania-specific state into the shared trait.
- **Every single-letter key (a-z, A-Z) and every sensible standalone symbol (`!;:~'/[]<>,.` and space) is already bound at the top-level `input.rs` match.** Confirmed by grepping every `b'...'` literal in the file - there is no free key left for a new dedicated world-action or panel toggle. A new interactive panel has to piggyback on an existing entry point (`Panel::Board` opens via the Examine list instead of its own key) rather than claim a new binding; check with a grep before assuming one is free.
- **A panel reachable only via Examine (not its own key) still needs the same three wiring points every other list panel does**: `state.rs`'s `list_len()` match, the `in_list` `matches!` block in `input.rs` (both copies - `handle_key` and `handle_arrow` each have their own), and `activate_selection`'s dispatch. Missing any one of these compiles fine and silently breaks cursor movement or number-key selection for just that panel.

---

## 10. Tests And Verification [STABLE]

Every module's tests sit in its adjacent `_test.rs` file (`world_test.rs`, `svc_test.rs`, `abilities_test.rs`, `classes_test.rs`, `items_test.rs`, `persist_test.rs`, `damage_test.rs`, `stats_test.rs`, ...). Agents run them through the capped runner: `make test-llm ARGS="-p late-ssh --lib -E 'test(lateania)'"`.

Lateania unit tests also lock broader gameplay invariants: world size/reachability, shop/item validity and gold sinks, Frontier gates/warnings, follow chains, wildlife hunting/boons, death/gold/veteran resurrection, the dead/corpse state (lingering corpse not an instant temple trip, release-to-temple, healer resurrection in place vs. an incapable class), combat companions (buying costs gold/refuses when unaffordable, the pet bites the owner's target, is downed by a barrage, and is revived/strengthened by feeding; every capital has a stable), player housing (claiming a deed, one-home-per-name, furnishing only a home you own while visitors cannot, the 50+-piece catalogue and non-overlapping plots), boss achievement mapping, saved-character level reconciliation, and persistence JSON round trips.

### The battle arena (`arena.rs`, `arena_test.rs`) [STABLE]

A test-only harness, declared as a child of `svc` (`#[cfg(test)] #[path = "arena.rs"] mod arena;`, beside `svc_test`) so it reaches the private combat entry points. It drives the **real engine** (`tick`, `engage_mob`, `use_ability`, `use_item`, `flee`) with a scripted character against a real spawn in its real home room and reports who survives and where every point of damage came from (auto / instant abilities / ability DoTs / coat / pet, pinned to the foe's health by `every_point_of_damage_is_accounted_for`). A `Recipe` states everything that moves a fight (class, level, archetype, `Gear` preset, `Companion`, `Coat`, potion count, `Policy`, and the ability-score `Build`); the arena pins what must not (clock at a clear day via `ARENA_CLOCK`, one fresh character per fight, the foe at full). **Builds** (`arena::Build`): `Neutral` (flat 10s, what every yardstick and crown contract runs), `Peak(Score)` (18 in one score), `Focused` (20 primary + 20 CON), `Blessed` (all 18), `Cursed` (all 3), `GlassCannon` (STR/DEX/INT 20, rest 3), `Tortoise` (CON/WIS 20, rest 3), `Merchant` (CHA 20, rest 3). The build contracts run in the suite on `BUILD_STEPS` (Warrior/Rogue/Mage/Cleric at L55 Frontier-10): `a_peak_score_moves_its_own_axis_and_nothing_else` (CHA changes no fight, CON changes the pool not the pace, STR/INT/WIS peaks land in their bands; Wisdom reads on `REGEN_TICKS` = 100 because regen only tells once the pool drains, a 20-tick window shows a Cleric's WIS as +0%), `a_crit_build_lands_its_share_over_a_long_window` (`CRIT_TICKS` = 150, a zero-crit window is then impossible), `the_whole_roll_is_a_share_of_the_fight_not_the_fight` (`BLESSED_GAIN` 5..40%, `CURSED_LOSS` -30..-3%), and `the_strange_builds_trade_what_they_say_they_trade`. The yardstick behind them is `arena_build_table` (`#[ignore]`, `BUILD_TICKS` = 200, ~2 minutes) and the same table is a section of part 2 of the report. Policies: `Honest` (rotate the roster by value, drink under 40%), `HitAndRun`, `StunAndFlee` (the exploit shapes, kept so the fix stays pinned). **`make arena` runs the whole battle checker** (every contract, both report parts, serially); it is deliberately not part of the suite: the crown and doorstep contracts and the reports are `#[ignore]` (minutes of real fights), only the sub-second ones (accounting, the exploit pins, the share and pace contracts) run with `cargo nextest`. Run it when combat, classes, gear, or bosses change. Balance questions get answered here, not modelled: `make test-llm ARGS="-p late-ssh --lib --run-ignored all -j1 -E 'test(arena_report)'"` writes `late-ssh/target/lateania-arena.md` (part 1: every Long Road crown against every class at eight level/kit steps, bare and with a maxed tame + oil; ~3 minutes, so run serially with `-j1` or it trips nextest's per-test budget) and `lateania-arena-extra.md` (part 2, `arena_report_extra`: a damage-composition table, the **dps yardstick** (`Arena::measure`: 20 honest ticks on the neutral straw dummy with its pool raised, per class and ladder step, the one number that compares callings independent of shape), the exploit table, and every boss as the engine fields it. The fast tuning loop is `make test-llm ARGS="-p late-ssh --lib --run-ignored all --no-capture -E 'test(arena_dps_table)'"` (the yardstick alone, seconds). Promote a report finding into an asserting arena test before acting on it as truth; the standing contracts are the two exploit pins, `casters_lean_on_abilities_and_martials_on_the_auto`, `classes_kill_at_a_similar_pace_in_the_same_gear` (`DPS_SPREAD_MAX`), `every_crown_falls_to_a_prepared_character_and_not_to_a_walk_in` (`CROWN_TARGETS`), and `the_trash_on_a_crowns_doorstep_is_in_band`. The yardsticks behind them (`arena_dps_table`, `arena_crown_yardstick`, `arena_doorstep_yardstick`) are `#[ignore]` prints for the tuning loop.


---

## 11. Known Gotchas And Future Work [VOLATILE]

### Open work, ranked

The crown ladder, the damage formula, the pet share, and the ability scores are measured and pinned by the arena. What is left, in the order it would change the game for a player:

1. **The roster cut.** `abilities.rs` is one roster wearing seventeen skins: slots 55-100 are byte-identical across every class (L100 is always a Finisher, cost 49, cd 8, mag 210, dur 2), the 1-50 half is nearly as templated, every class has every effect (Warrior heals, Mage wards, Rogue wards, Runemaster heals), and names duplicate inside a class (`PROGRESSION.md`, "Known defects"). Damage is `magnitude + spell_power × coef` with the coef per effect, so a later Strike only adds a bigger flat floor while its cost and cooldown climb, and the L1 button wins: Warrior L100 Cleave 314/tick for 10 rage vs Titan Cleave (L80) 218/tick for 13.5 rage; Mage Firebolt 530/tick for 8 mana vs Starfall Lance (L75) 320/tick for 13.5. Twenty of twenty-three slots are dead, against a 10-key hotbar. The bar is player-reorderable (`x` in the Abilities panel; `ability_order` is persisted, and `use_ability` resolves slots through `ordered_for`), and newly unlocked abilities append to the end of the bar. The shape to aim for: ~8 abilities per class, each a distinct role (opener, spender, DoT, defensive, utility, big cooldown, capstone), scaling from spell power rather than from a renamed copy, later levels unlocking modifiers to existing buttons, and identity by omission (the Warrior has no heal, the Mage no ward, the Rogue two stuns and the Cleric none). Draft the per-class table for review before touching the data; `casters_lean_on_abilities_and_martials_on_the_auto`, `classes_kill_at_a_similar_pace_in_the_same_gear`, and the crown contract are the nets.
2. **Pin the builds against the crowns.** `every_crown_falls_to_a_prepared_character_and_not_to_a_walk_in` runs the `Neutral` build only. Two `#[ignore]` arena lines close it: a `Cursed` prepared character must still take every crown (a bad roll is a handicap, never a wall) and a `Blessed` walk-in must still lose (a good roll is never a skip).
3. **The authored core cannot level a player through itself, and the side countries have no quests.** The core's 31 spawns hold 6,276 xp in total (about Lv11) while the Archdemon at its end is pitched at a prepared L35 (`CROWNS`), so players level in the ungated side countries and return over-levelled, and the run up to the Obsidian Throne plays as trivial. The fix is raising the core's xp budget (or its late trash), not re-tuning the boss: a crown's row is derived from the prepared median, never authored by feel (`PROGRESSION.md`). No quest content bridges Lv15-30, Lv35-52, or anything past Lv78, and none of the five side countries has a quest, though Aelunor and Broceliande are the de facto route from the core to the Archdemon. Boards exist only in the three capitals and at the Kaelmyr ash-cairn.
4. **Gear is the majority of attack from the first town.** At L10 the class curve is 24 of a 122 rating and shop gear 98; at L100 it is 204 of 857. Decide whether that is the intended gear game or whether `stats_at` should carry more; it interacts with (1), since the roster's flat magnitudes are the other class lever.
5. **Score tuning.** Wisdom out-damages Intelligence for casters (`PROGRESSION.md`, "Ability scores, measured"); retune or keep as the caster's choice.
6. **Small ones.** CHA reaches the item shop only, not stables, deeds, or furniture. The Druid's two paths have identical bare dps. The character sheet shows the six scores but not their effect lines (only the creation and point screens do).

### Gotchas

- A splice that discovers "the room that links to X" by scanning `room.exits` for a matching target must exclude any room that is itself inside the region X belongs to, or it can match one of X's own in-region neighbours instead of the real external anchor, and `HashMap` iteration order makes the match (and everything hung off it) silently nondeterministic between boots. `extend_silvael`'s anchor search excludes `is_aelunor_room` for this reason; `worldmap_test::world_coords_is_cached_and_complete` pins it.
- Adding a region to `REGIONS` also means placing it on the land map: a `Place` (or `Keep`) and a `Road` per real link, in `ui.rs`. `the_atlas_draws_every_road_in_the_world_and_invents_none` fails until you do, on purpose: the picture is authored, so a new country has to be drawn in rather than laid out for you (see `WORLDMAP.md`).
- **Do not cite line numbers in this file.** They rot on the next edit and nothing catches it. Name the symbol instead (`can_cross_progression_gate`, `extend_kaelmyr`), which greps and survives.
- Some comments in `world.rs` may lag current content scale. Trust current tests/data: ~2600 rooms across base/overworld/Frontier, the three living-world regions, housing, city districts, and the ~900-room Sundered Reaches (see the room-count test's per-region ranges).
- `follow_task` is a toggle service command that input never calls: opens the Follow panel and uses `follow_to_task` / `stop_follow_task`.
- `say_task` exists, but active Lateania has no typed command prompt.
- Inventory snapshots include equipped items after pack items. Equip/use/sell mutations usually require the item to still be in `inventory`, so equipped-row activation is often a no-op.
- Inventory rows wrap in the side panel and equipped rows include their worn slot, e.g. `[worn weapon]` or `[worn chest]`.
- `view.occupants` includes other players in the room regardless of class; service follow selection only allows classed targets in the same room.
- Boon perks apply on room entry and can spam log lines if movement loops through boon rooms.
- Hunted game cooldowns are not persisted across process restart.
- World content is authored as Rust data. A future data-file loader should preserve the existing `World`, `Room`, `MobSpawn`, `Feature`, and `CritterSpawn` shapes.
