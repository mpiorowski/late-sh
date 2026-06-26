# Lateania Game Context

## Metadata
- Scope: `late-ssh/src/app/door/lateania` plus Lateania screen lifecycle in `late-ssh/src/app/door`
- Domain: Lateania, the persistent D&D-style MUD inside late.sh
- Primary audience: LLM agents changing the Lateania game runtime, content, UI, combat, or persistence
- Last updated: 2026-06-17
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
- The world holds `1565` rooms: 198 base/extension, 100 overworld, 1000 Frontier, plus three living-world regions — the 96-room Sunken Catacombs, the 96-room Thornwood Hollows, and the (CA-sized, ~75-room) Drowned Caverns. The room-count test checks each region range rather than one magic literal.
- Frontier has 20 zones, each 10 by 5 rooms, starting at room `2000`.
- Three deterministic living-world regions (fixed-seed `MazeRng`, identical every boot), each hung off a capital via a free direction:
  - **Sunken Catacombs** (rooms `5000+`, off `TASMANIA_SQUARE`) — braided maze (`carve_maze` + `extend_catacombs`); undead.
  - **Thornwood Hollows** (rooms `5200+`, off `MELVANALA_SQUARE`) — braided maze (`carve_maze` + `extend_thornwood`); beasts/fae.
  - **Drowned Caverns** (rooms `5400+`, off `MATLATESH_SQUARE`) — organic cellular-automata cave (`carve_cavern` + `extend_caverns`), NOT a maze: noise smoothed into chambers, then only the largest connected pocket is kept (so no unreachable rooms); rooms are sparse within the cell field. Aberrations.
- The living-world regions are a hard post-Archdemon arc: their capital entrances require `Bane of the Archdemon Mal'gareth`, their regular mobs are capped below local boss damage, and their boss titles act as the three living-dark seals for Frontier access.

---

## 2. Module Map [STABLE]

| File | Responsibility |
|---|---|
| `../game.rs` | Minimal host-facing door-game contract: id/title/description, render/input/leave hooks, optional activity mapping, and generic outcome events. |
| `mod.rs` | Module declarations and Lateania credits. Keep declaration-only. |
| `screen.rs` | Top-level Lateania screen shell and `DoorGame` implementation: landing page, launch/reset/leave input, terminal banner art, and active-world render delegation. |
| `state.rs` | Per-session client wrapper: snapshot receiver, local `Panel`, cursor, join retry, action delegation. Never mutate game truth here. |
| `input.rs` | Active-world key routing after launch. App-level launch/reset/leave handling belongs in `screen.rs`. |
| `ui.rs` | Ratatui rendering for class select, log, compact mode, side panels, minimap, hints. The Character panel expands to a full-width dashboard (accent-tinted class portrait, dot-rated ability scores, vitals/XP meters) when the area is at least 72x18, else falls back to the narrow side panel; Foes/Adventurers/Follow render as aligned roster rows with HP meters. Lock-free, snapshot-only. |
| `svc.rs` | Authoritative runtime: service tasks, `WorldState`, player/mob state, combat, movement, following, shops, persistence, snapshots, activity events. |
| `world.rs` | Immutable world data and generation: rooms, exits, mobs, features, wildlife, minimap, overworld, Frontier. |
| `classes.rs` | Five playable classes, resources, passive traits, level 1-50 stat curves, XP curve. |
| `abilities.rs` | Ability roster and unlock helpers. Effects are data, resolved in `svc.rs`. |
| `items.rs` | Item catalog, equipment slots, consumables, valuables, shops, generated Frontier loot. |
| `damage.rs` | Damage schools, mob resistance/weakness profiles, damage multiplier math. |
| `stats.rs` | D&D-style ability scores, 4d6-drop-lowest rolls, modifiers, HP/attack bonuses. |
| `persist.rs` | JSON schemas for durable character saves and shared world saves. Versioned (`SCHEMA_VERSION`); new fields use `#[serde(default)]` so old saves load (e.g. `board_progress`/`board_done` for quests). |

### Board quests [VOLATILE]

`BOARD_QUESTS` (in `svc.rs`) is a static table of bounties posted on a `FeatureKind::Board` in each capital square (Tasmania/Melvanala/Matlatesh). Each has an `Objective` — `Bounty{name_contains,count}`, `Collect{item,count}`, `Reach{zone}`, or `Escort{npc,dest_zone}` — and a `Repeat` (`Once`/`Daily`/`Weekly`). Per-player state: `board_progress` (accepted counters), `board_done` (one-offs claimed), `quest_cooldowns` (id→Unix seconds when a repeatable was last claimed), all persisted; plus a transient `escort: Option<EscortState>` (not persisted).

Examining a board (`use_board`): claims a finished bounty if ready (one-offs → `board_done`, repeatables → `quest_cooldowns`, re-available after `DAY_SECS`/×7 via `board_quest_available`), else posts the next available quest. Counter progress ticks via `bump_quests` from the kill / loot / room-enter paths. **Escorts** spawn a transient escortee that travels with the player; it is wounded by chance when the player is struck (`wound_escort`) and lost immediately on player death; reaching `dest_zone` with it alive completes the quest (`check_escort_arrival`, in `describe_room_context`). The escortee and active board quests surface in the room panel / quest journal.

---

## 3. Screen Lifecycle And Input Capture [STABLE]

- Lateania is no longer a top-level tab. It is launched from the Games hub (`late-ssh/src/app/door/hub`, page `3`), a selector that renders the selected door game's full landing; Lateania's landing is drawn by the now-`pub` `screen::draw_landing` (the same two-column layout used by the standalone screen fallback). `Screen::Lateania` is a live-world-only screen reached by pressing `Enter` on the selected Lateania card; that one keypress both switches the screen and joins the world (no intermediate standalone landing).
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
- Class selection owns `1-5` after launch. Those keys must not switch top-level screens while Lateania is active.

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
- respawns downed players at `TEMPLE_ROOM = 4` after `PLAYER_RESPAWN_SECS = 8`;
- regenerates class resources and decrements buffs, shields, HoTs, stuns, and cooldowns;
- resolves one combat round for each engaged player, then per-mob behavior (`resolve_mob_behavior`): Caster bolts (storm-boosted), PackHunter gang-ups, Summoner adds, Brute enrage, Thief steal-and-flee, Skirmisher flee; all mob damage is scaled by `TimeOfDay::mob_damage_pct` (the dark hits harder) and Ambush reveals are fog-boosted;
- removes idle players after `PLAYER_IDLE_TIMEOUT_SECS = 10 * 60`, exporting their save;
- increments snapshot generation when dirty and drains kill outcomes for `ActivityGame::Mud`.

### Active sessions

- Active sessions are tracked per user and session UUID. Multiple sessions for the same user should not remove the player until all sessions leave.
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
- The first dungeon descent from Whisperwood into Duskhollow requires `Bane of the Elder Treant`.
- Living-dark entrances from the three capitals require `Bane of the Archdemon Mal'gareth`.
- The Town Square Frontier descent requires `Bane of the Archdemon Mal'gareth`, `Bane of The Bonewright Lich`, `Bane of the Elder Dryad`, and `Bane of the Abyss-Thing`; after those title gates, it still uses a transient two-step warning: the first `>` logs that the Frontier is older, meaner country for seasoned adventurers, and the next `>` confirms descent. Service-backed non-movement actions clear the pending warning.
- Combat: `space`, `x`, or Enter attacks when not in a list panel; `z` flees.
- Abilities: `1-9` use unlocked ability slots unless a list panel is open.
- World actions: `r` recalls to Embergate's Town Square when out of combat; `f` toggles the Follow panel.
- Panels: `c` character, `v` abilities, `t` inventory, `b` shop where a merchant exists, `o` examine/look, `k` titles, `j` quest journal, `f` follow.
- List panels: `w/s` or up/down move cursor; `1-9` jump and activate; Enter activates.
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

UI uses a two-column layout with compact fallback for terminals narrower than 50 columns or shorter than 9 rows. The left column splits current room context (`Now`) from newest-first action scrollback (`Recent`); service room-description lines use `LogKind::Room` and are filtered out of `Recent` so movement does not bury combat, loot, chat, and system events. Arrivals use compact `LogKind::Travel` breadcrumbs so Recent still shows where the player has just been.
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
- Plaques and vistas are descriptive.
- Room descriptions intentionally mention only feature names; the detailed text is revealed by `o` / Examine.

### Wildlife

- `WILDLIFE` is separate from combat mobs.
- `CritterKind::Skittish` is ambient.
- `CritterKind::Game` can be hunted by attacking when no combat mob is present. Hunted game grants small XP and is hidden by a per-world 40-second cooldown keyed by global wildlife index.
- `CritterKind::Boon(Perk)` applies on room entry. Perks are `Embolden`, `Mend`, and `Quicken`.
- Wildlife appears in the Room panel; game critters show as huntable only while off cooldown.

### Frontier loot

- `items::FRONTIER_TIERS = 20`, one tier per Frontier zone.
- Generated Frontier item IDs are `3000..3200`, 20 tiers times 10 slots.
- `item(id)` searches both authored `ITEMS` and generated Frontier catalog.
- Frontier mob and boss loot tables use `frontier_loot(zone)`, which includes representative weapon, head, chest, hands, ring, draught, and relic entries for the zone tier.
- Frontier item generation now starts at post-living-dark power and climbs hard across all 20 tiers; regional boss loot is authored, meaningful post-Archdemon gear, while Frontier remains the best long-term gear path.
- Early Frontier regulars are tuned as endgame mobs: tests keep the first Frontier regular above the strongest living-dark boss damage while still below the first Frontier boss.

---

## 7. Progression, Combat, And Economy [VOLATILE]

### Classes and scores

Playable classes:
- Warrior: Rage, `Unbreakable`, Strength primary.
- Mage: Mana, `Arcane Mastery`, Intelligence primary.
- Cleric: Mana, `Light of the Dawn`, Wisdom primary.
- Rogue: Energy, `Opportunist`, Dexterity primary.
- Ranger: Focus, `Hunter's Instinct`, Dexterity primary.

Progression:
- Level cap is `Class::MAX_LEVEL = 50`.
- `xp_for_level` keeps early levels quick, then adds a much steeper post-level-8 term so midgame and Frontier progress target roughly week-scale casual play instead of a 1-2 sitting clear; `level_for_xp` caps at 50.
- `Class::stats_at(level)` computes HP/resource/attack/resource regen.
- Ability scores are rolled before class selection and persist after class choice.
- Constitution adjusts max HP by level; class primary score adjusts attack.

### Abilities and damage

- `AbilityEffect` variants: `Strike`, `DamageOverTime`, `Heal`, `HealOverTime`, `Empower`, `Ward`, `Stun`, `Finisher`.
- Each class has 11 abilities including a level-1 ability and a level-50 capstone.
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
- Normal death clears target, removes 20% of carried gold, sets `respawn_at`, and later respawns the player at the temple. Banked gold is protected.
- `seed_world()` applies a balance scaler after all authored/overworld/Frontier/living-dark spawns are generated: authored regular mobs are modestly tougher with a small XP bump and faster respawns, authored bosses gain larger HP/damage bumps with lower XP, living-dark mobs/bosses become hard post-Archdemon progression, and Frontier mobs/bosses scale sharply above them while Frontier regulars remain rewarding enough to grind.

### Items, shops, and rewards

- Equipment slots: Weapon, Head, Chest, Legs, Hands, Feet, Ring, Trinket.
- Item rarities: Common, Uncommon, Rare, Epic, Legendary.
- Item kinds: Equipment, Consumable, Valuable.
- Valuables, including Frontier relics, show a `valuable / sell Xg` stat line in inventory/shop UI so players know they are sell loot; generated Frontier relic descriptions also state that they have no combat use.
- Starter inventory is a Rusty Shortsword and two Minor Healing Draughts. Starting gold is 120.
- Shops are in Embergate: Ember Forge, Outfitter, Apothecary, and Curio Cart.
- Shop economy intentionally includes expensive late-game gold sinks: masterwork weapon/armor/head/hands, premium curio gear, and the repeatable Phoenix Tonic. The masterwork shop pieces are shop-stock, not boss drops, so gold remains useful after normal boss clears.
- Apothecary consumables are tuned as the pressure valve for harder combat: early draughts are affordable recovery, Elixir of Renewal covers mid/late mixed HP/resource recovery, and Phoenix Tonic is a repeatable expensive late-game recovery sink.
- Authored boss loot tables include head and hand upgrades across tiers; living-dark bosses add controlled post-Archdemon unique gear, while their regular mobs mostly drop regional relics and sustain consumables.
- Bosses always drop one item from their loot table. Regular mobs have a modest chance if their table is non-empty.
- Mob kills grant XP, reduced gold, possible loot, and titles. Boss XP and Frontier quest XP/gold bounties are intentionally damped so boss chains do not skip too much of the level curve.
- Boss title format is `Bane of ...`; lesser foes grant a derived `...bane` title.
- Frontier boss kills complete their zone quest, award XP/gold, and grant `Champion of the <zone>`.
- Defeating the authored final boss, the Archdemon Mal'gareth, pays a once-per-account 10,000 chip lifetime payout and grants the `LAD` profile-award badge; repeat kills can still grant normal in-world rewards but not the chip payout again.
- Defeating the final Frontier boss, the King Who Was Promised Nothing, pays a once-per-account 20,000 chip lifetime payout and grants the `LFK` profile-award badge; repeat kills can still grant normal in-world rewards but not the chip payout again.
- Every mob kill emits a Lateania activity win event. Final-boss kills route through lifetime reward templates; if the chip payout was already claimed, activity still records the defeat without the chip/badge detail.

---

## 8. Persistence [STABLE]

### Character save

Character persistence uses `late_core::models::mud_character` / `mud_characters`.

Saved character schema version: `7`.

Durable fields:
- class key, XP, level, carried gold, banked gold, current HP;
- saved room, but hydration only restores it if the room still exists and is safe;
- visited rooms for minimap;
- inventory and equipped `(slot-key, item-id)` pairs;
- rolled ability scores;
- titles, title levels, active title index;
- completed Frontier quest indices.

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
- Keep generated Frontier ID ranges aligned: 20 zones, 20 item tiers, IDs `3000..3200`, Frontier rooms at `2000+`, Frontier mob IDs at `900000+`.
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
- DB/service coverage for Lateania belongs under `late-ssh/tests/door/` and must use shared testcontainers helpers.

Lateania unit tests also lock broader gameplay invariants: world size/reachability, shop/item validity and gold sinks, Frontier gates/warnings, follow chains, wildlife hunting/boons, death/gold/veteran resurrection, boss achievement mapping, saved-character level reconciliation, and persistence JSON round trips.

Expected focused command for human verification after Lateania changes:

```bash
cargo test -p late-ssh lateania
```

Use integration tests under `late-ssh/tests/door/` only for DB/service orchestration that cannot stay pure.

---

## 11. Known Gotchas And Future Work [VOLATILE]

- Some comments in `world.rs` may lag current content scale. Trust current tests/data: 1565 rooms, 20 Frontier zones, 1000 Frontier rooms, plus the three living-world regions.
- `follow_task` still exists as an old toggle service command, but current input opens the Follow panel and uses `follow_to_task` / `stop_follow_task`.
- `say_task` exists, but active Lateania has no typed command prompt yet.
- Inventory snapshots include equipped items after pack items. Equip/use/sell mutations usually require the item to still be in `inventory`, so equipped-row activation is often a no-op.
- Inventory rows wrap in the side panel and equipped rows include their worn slot, e.g. `[worn weapon]` or `[worn chest]`.
- `view.occupants` includes other players in the room regardless of class; service follow selection only allows classed targets in the same room.
- Boon perks apply on room entry and can spam log lines if movement loops through boon rooms.
- Hunted game cooldowns are not persisted across process restart.
- World content is authored as Rust data. A future data-file loader should preserve the existing `World`, `Room`, `MobSpawn`, `Feature`, and `CritterSpawn` shapes.
