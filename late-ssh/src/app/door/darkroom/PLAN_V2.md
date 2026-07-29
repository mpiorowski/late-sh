# A Dark Room v2: the full game

Execution plan for finishing the port: everything from the trading post's buy
menu through the wasteland, combat, the ship, and the ending. Written for an
LLM executor; reviewed phase by phase by another agent. Read `CONTEXT.md`
first: everything it says about v1 still holds, and this plan only describes
what changes.

## Ground rules for the executor

- **Source of truth is the local clone** at `upstream-adarkroom/` (repo root,
  gitignored). Transcribe balance tables, timings and prose from the files
  named in each phase. Never from memory. If the clone is missing:
  `git clone --depth 1 https://github.com/doublespeakgames/adarkroom upstream-adarkroom`.
- **Licensing is file-level and non-negotiable.** Any file carrying
  upstream-derived tables, rules, or prose gets the MPL-2.0 header block
  (copy it verbatim from the top of `data.rs`, adjusting the "transcribed
  from" line). Files that are purely our own work (rendering, session state
  plumbing, persistence envelope) stay FSL-1.1-MIT (no header, repo default).
  When in doubt, MPL. Update `NOTICE` and `LICENSING.md` whenever the MPL
  file list changes.
- Upstream prose is transcribed **verbatim**, including its punctuation. The
  repo style rule against em dashes applies to our own code, comments and UI
  chrome, not to upstream's sentences.
- Tests live beside the file (`foo_test.rs` + `#[cfg(test)] mod foo_test;`),
  are written together with the code, and run through the capped runner:
  `ARGS="darkroom" make test-llm` (the env var goes *before* `make`, never
  as a `make` argument, or jemalloc's build dies).
- Never `git commit`. Leave everything in the working tree.
- No `#[allow(...)]` to silence lints; fix the structural cause.
- Closed enums with exhaustive matching everywhere. A new variant must break
  the build. No catch-all arms on enums we control.
- Keep the module flat (files directly in `darkroom/`), consistent with v1.
- At the end of every phase: stop. The reviewer reads the diff before the
  next phase starts.

## Decisions already made (do not relitigate)

1. **Scope: the classic game.** Trading post buy menu, workshop tier,
   mines and their jobs, the path, the world, combat, random events,
   setpieces, the ship, the space minigame, the ending. **Cut:** the
   Executioner battleship, the fabricator, prestige, scoring
   (`prestige.js`, `scoring.js`, `fabricator.js`, `events/executioner.js`,
   `events/marketing.js`, the `cache` landmark, and every
   fabricator/executioner item: hypo, stim, kinetic armour, plasma rifle,
   energy blade, disruptor, glowstone, cargo drone, fluid recycler).
   `laser rifle` and `energy cell` stay (classic setpiece loot).
2. **Ending: completed save.** Winning space shows the ending text, sets a
   persisted `completed` flag, and returns to the hub. The landing card shows
   the save is finished; the existing delete flow is how you replay. No
   auto-reset, no prestige.
3. **Space is a faithful real-time port** on the app's hot tick (15fps; the
   arcade already runs snake/tetris this way).
4. **Pacing constants stay as they are.** `SLOWDOWN` (5x), the 3h daily cap
   and the 45m floor pace the *village* only.
5. **Expeditions are live and outside the cap.** World moves run on
   keypresses and wall clock, consume no daily allowance, and are never
   slowed. Village credit keeps accruing while the player is out (it is
   connected time; the existing settle handles this with no new code).
6. **Expeditions persist and resume.** The in-flight expedition (position,
   hp, water, outfit, uncommitted map, combat snapshot) is saved on every
   move. Disconnect or Esc parks the trip; re-entering the door resumes it.
   Death still discards it. This deliberately softens upstream (a closed
   browser tab loses the trip) because SSH connections drop through no fault
   of the player. Supplies burn per move, not per second, so parking costs
   nothing and there is no balance to protect.

## The three clocks (extend CONTEXT.md's two)

| Clock | Applies to | Slowed? | Capped? |
|---|---|---|---|
| Village time | fire, temperature, builder, income, arrivals | 5x | 3h/day + floor |
| Wall-clock cooldowns | stoke/gather/traps buttons, embark cooldown, event scheduling, delayed event rewards | no | no |
| Live play | world moves (per keypress), combat, space | no | no |

Nothing in v2 touches `pace.rs`. The one new pacing-adjacent fact: while an
expedition is parked (player disconnected mid-trip), *nothing* about the
expedition changes; the village keeps its usual rules.

## Data model changes (phase 1 lays these down)

All in `model.rs` / `data.rs` (MPL). Every new `Game` field carries a serde
default so v1 saves load unchanged. No schema_version bump needed.

- **`Resource` grows** to cover everything upstream keeps in `stores`:
  `Iron, Coal, Sulphur, Steel, Medicine, Bullets, EnergyCell, Bolas, Grenade,
  Bayonet, AlienAlloy, Compass, Torch, Waterskin, Cask, WaterTank, BoneSpear,
  Rucksack, Wagon, Convoy, LeatherArmour, IronArmour, SteelArmour, IronSword,
  SteelSword, Rifle, LaserRifle`. One enum, one stores map, exactly like
  upstream's single `stores` object: `has_seen` gating, trade costs, craft
  costs, and outfitting all work uniformly. Add a closed
  `ResourceKind { Good, Tool, Weapon, Upgrade }` classifier (from the `type`
  fields in `room.js` `Craftables`/`TradeGoods`) matched exhaustively; the
  path screen and loot UI key off it.
- **`Building` grows**: `Workshop, Steelworks, Armoury, IronMine, CoalMine,
  SulphurMine`. The mines are granted by the world (never offered by the
  builder): give `Building` a `builder_built() -> bool` and have
  `refresh_build_options` / the build rows skip non-builder buildings.
  `unlocks_jobs`: Workshop none, Steelworks → Steelworker, Armoury →
  Armourer, IronMine → IronMiner, CoalMine → CoalMiner, SulphurMine →
  SulphurMiner (upstream `Outside.checkWorker`).
- **`Job` grows**: `IronMiner, CoalMiner, SulphurMiner, Steelworker,
  Armourer`, yields from `outside.js` `_INCOME` (miners: cured meat -1 →
  ore 1; steelworker: iron -1, coal -1 → steel 1; armourer: steel -1,
  sulphur -1 → bullets 1). `sim::step_income` and the worker UI pick these
  up automatically via `Job::ALL`.
- **New `Game` fields** (introduced in the phase that uses them):
  - `perks: BTreeSet<Perk>` and `starved: u32` / `dehydrated: u32` counters.
  - `thieves: ThievesState` (closed enum: `None, Active, Dealt` per the
    thief event's state machine in `state_manager.js` / `events/global.js`).
  - `pending_rewards: Vec<PendingReward>`: wall-clock due timestamps for
    delayed event payoffs (the Mysterious Wanderer's returns), settled in
    `sim::settle` against real time.
  - `path_unlocked: bool` (set by buying the compass).
  - `world: Option<WorldMap>` (the committed map + mask, generated at
    compass purchase).
  - `expedition: Option<Expedition>` (the parked in-flight trip).
  - `ship: Option<ShipState>` (`hull`, `thrusters`, `seen_warning`).
  - `completed: bool`.
- **`Perk` closed enum** from `engine.js` `Engine.Perks`: `Boxer,
  MartialArtist, UnarmedMaster, Barbarian, SlowMetabolism, DesertRat,
  Evasive, Precise, Scout, Stealthy, Gastronome`, with `desc`/`notify` prose.

## Phases

Each phase compiles, passes `ARGS="darkroom" make test-llm`, and is playable
on its own. Stop after each for review.

---

### Phase 1: the economy opens (trading post + workshop tier)

The buy menu and the crafting tier need nothing from the wasteland to
*exist* (workshop costs wood/leather/scales, all obtainable in v1; iron,
coal, steel are purchasable with fur/scales/teeth), so this phase alone
un-bricks the current wall: leather and cured meat get sinks, the trading
post does something, and there is a reason to keep the village producing.

Upstream sources: `script/room.js` (`Craftables` lines ~12-357, `TradeGoods`
lines ~359-485, and the `build`/`buyThing`/`craft` + `updateBuildButtons` /
`craftUnlocked` / trade-visibility rules further down), `script/outside.js`
(`_INCOME`, `checkWorker`).

Work:

1. The `Resource`/`ResourceKind`/`Building`/`Job` extensions above, with
   costs, labels, `availableMsg`/`buildMsg` prose, and yields transcribed.
   Building costs: workshop 800 wood / 100 leather / 10 scales; steelworks
   1500 wood / 100 iron / 100 coal; armoury 3000 wood / 100 steel /
   50 sulphur.
2. **Trade goods** table + `Game::buy` (whole-refusal like `build`).
   Transcribe the *visibility* rule for each good from `room.js` exactly
   (which goods appear given what has been seen); latch offers the same way
   `seen_buildings` latches. The compass is in the table but is **deferred
   to phase 3** (its effect is opening the path; do not ship a dead special).
3. **Craftables** + `Game::craft` (whole-refusal): torch (unlimited),
   waterskin/cask/water tank, bone spear, rucksack/wagon/convoy,
   l/i/s armour, iron sword, steel sword, rifle. Transcribe
   `craftUnlocked`: workshop must stand, half-the-wood + seen-the-rest,
   latched. Rifle/armoury-tier costs pull sulphur/steel, which phase 1
   players can only buy; that is upstream's shape too.
4. UI: the Room panel gains **craft** and **buy** row sections (same cursor
   list, same cost hint line the build rows already use). Worker rows for
   the new jobs appear automatically once their buildings stand.
5. Tests (`model_test.rs`, `sim_test.rs`): buy/craft whole-refusal and
   latching; steelworker/armourer income chains stall correctly when inputs
   run dry; a v1 save blob (fixture JSON) still deserializes.

Out of scope for this phase: compass effect, mines (world-granted).

---

### Phase 2: the event engine, combat, and village events

Build the scene machine and combat on home turf, where death is not
possible, before the world depends on them.

Upstream sources: `script/events.js` (engine, combat, loot), 
`script/events/room.js`, `script/events/outside.js`, `script/events/global.js`
(village event pool), `script/events/encounters.js` (fight data for phase 3,
transcribed and tested now).

Work:

1. **`event.rs` (MPL)**: the engine. An `Event` is `{title, availability,
   scenes}`; a `Scene` is text + optional `notification`, `reward`, `onLoad`
   effects, and either buttons or combat. Buttons carry text, optional cost,
   optional cooldown, and a **weighted** `nextScene` table (upstream's
   `{'0.5': 'a', '1': 'end'}` roll). Replace upstream's JS closures with a
   closed `Effect` enum (`AddStore`, `AddPopulation`, `KillPopulation`,
   `AddBuilding`, `GrantPerk`, `SetThieves`, `ScheduleReward`, `EndEvent`,
   ... exactly the set the transcribed events need, no more) applied in one
   exhaustive match. Availability conditions likewise become a closed enum
   (`InRoom`, `InOutside`, `HasStore(..)`, `HutCountBetween(..)`,
   `PopulationOver(..)`, `WorldUnlocked`, ...), not predicates.
2. **Scheduling**: a wall-clock timer in `State`, uniform in upstream's
   `_EVENT_TIME_RANGE` `[3, 6]` minutes, ticked from `State::tick` (not from
   `settle`; events are session-live, not village time). An event fires only
   while the player is in the door and its availability holds; otherwise
   reroll per upstream's retry behavior. The active event is *not*
   persisted: a dropped session drops the modal, like upstream.
   Delayed rewards (`ScheduleReward`) *are* persisted via
   `Game::pending_rewards` and paid by `sim::settle` on wall clock.
3. **Combat** (inside `event.rs` or a sibling `combat.rs`, MPL): weapon
   table from `world.js` `Weapons` (damage, cooldown seconds, ammo costs),
   hit chance 0.8 (+0.1 precise), enemy attack timers, eat-meat button
   (heals `MEAT_HEAL` 8, x2 gastronome, cooldown 5s), use-meds (20, 7s),
   bolas stun, ranged ammo consumption, dodge (evasive), and the
   win/loot/leave flow: loot rows with counts, take/leave, bag-space rules
   apply only in the world (phase 3), at home loot goes to stores.
   Cooldowns are wall clock, driven by `State::tick`; rendering interpolates
   nothing (no animation port).
4. **Village events data** (`scenes_village.rs`, MPL): The Nomad, Noises
   (both), The Beggar, The Shady Builder, The Mysterious Wanderer (wood and
   fur variants, with their delayed returns), The Scout, The Master (grants
   its perks), The Sick Man, A Ruined Trap, Fire, Sickness, Plague, A Beast
   Attack, A Military Raid, The Thief (with the `thieves` state machine and
   the income-skimming it implies; transcribe how upstream applies the
   skim in `state_manager.js`). Availability transcribed exactly (Scout and
   Master require `WorldUnlocked`, so they stay dormant until phase 3).
5. **UI (`ui_event.rs`, FSL)**: a modal over the current panel: title bar,
   wrapped scene text, button rows with cost/cooldown hints, combat layout
   (player and enemy with hp counters, weapon rows), loot list. Input
   routing in `screen.rs`: while a modal is up, it owns the keys.
6. `Perk` enum + persistence lands here (Master, and the combat modifiers).
7. Tests: scene graph walking with seeded rng (weighted branches), cost
   gating, effect application (population, stores, thieves), delayed reward
   settling across a save/load, combat math (hit/damage/stun/ammo) with a
   seeded rng, loot take/leave.

---

### Phase 3: the path and the world

Upstream sources: `script/path.js` (outfitting, weights, capacity, embark),
`script/world.js` (everything), `script/events/encounters.js` (already
transcribed).

Work:

1. **Compass goes on sale** (phase 1 table + effect): buying it sets
   `path_unlocked`, generates `Game::world` if absent, logs upstream's
   "the compass points <dir>" line. Compass direction from `World.compassDir`
   against the ship's placement.
2. **`world_data.rs` (MPL)**: tile set and glyphs, terrain probabilities
   (forest .15 / field .35 / barrens .5), `STICKINESS` 0.5, landmark table
   (counts + min/max radii + scenes + labels, minus cache/executioner),
   `LIGHT_RADIUS` 2, `BASE_WATER` 10, `MOVES_PER_FOOD` 2, `MOVES_PER_WATER`
   1, `FIGHT_CHANCE` 0.2, `FIGHT_DELAY` 3, `BASE_HEALTH` 10,
   `BASE_HIT_CHANCE` 0.8, heals, `DEATH_COOLDOWN` 120, the `Weapons` table
   (if not already in phase 2), armour hp bonuses (l +5 / i +15 / s +35),
   water capacities (waterskin +10 / cask +20 / tank +50), bag capacities
   (base 10, rucksack +10 / wagon +30 / convoy +60), item weights, and the
   terrain-transition narration prose.
3. **`world.rs` (MPL)**: map generation (spiral `chooseTile` with
   stickiness, landmark placement respecting radii and `isTerrain`,
   village at center of the 61x61 grid), mask + `lightMap`/`uncoverMap`
   (scout doubles radius), movement with `narrateMove`, supply consumption
   (`useSupplies`: starvation/dehydration warnings, the 10-count perk
   grants, death), `checkDanger` (distance 8 without iron armour, 18
   without steel), `checkFight` (stealthy halves), `doSpace` dispatch,
   outposts (one use each, refill water, `drawRoad` on dungeon clear),
   `goHome` commit (merge world state, grant mine buildings, unlock ship,
   return outfit through `leaveItAtHome`), `die` (drop outfit, discard
   world changes, home, embark cooldown). Pure rules module: no I/O, no
   rendering, returns tagged outcomes.
4. **Persistence**: `WorldMap` as `Vec<String>` rows for map and mask;
   `Expedition` snapshot (position, hp, water, food/water move counters,
   outfit map, the *working copy* of the map/mask, used outposts, danger
   flag, fight-move counter, optional combat snapshot with enemy hp/ammo
   state). Saved fire-and-forget on every move through the existing
   per-user write gate. `goHome`/`die` clear it.
5. **Path view**: joins the Tab cycle (Room → Outside → Path → Ship as
   unlocked). Armour and water summary rows, one row per carryable
   (`ResourceKind::Tool | Weapon` plus the upstream extras: cured meat,
   bullets, energy cell, charm, medicine, grenade, bolas, bayonet, alien
   alloy), +/- 1 and 10 with weight math, free-space line, embark row
   (disabled without cured meat; cooldown after death). Embark deducts the
   outfit from stores and enters the world.
6. **World view (`ui_world.rs`, FSL)**: masked viewport centered on the
   player, compass/status line (water, hp, cured meat, distance), the
   shared log. Arrows/wasd move (each keypress = one move = supplies +
   fight check + landmark trigger). Esc parks the expedition and leaves the
   door (no confirm needed: nothing is lost). Stepping onto a landmark
   starts its setpiece (phase 4; until then, landmark tiles can no-op with
   their label logged, gated behind a `todo`-free stub scene that just
   describes and leaves: keep it honest and minimal).
7. Danger/starvation/thirst warnings and death flow wired to the log.
8. Tests: map gen invariants with seeded rng (landmark counts and radii,
   probabilities sum, village at center), supply burn and both death
   spirals (and the perk grants at 10), danger boundaries, fight cadence,
   outpost single-use + water refill, goHome commit (mines become
   buildings and unlock jobs; outfit returns minus `leaveItAtHome`), die
   discards, expedition snapshot round-trips through the save envelope,
   Esc-park-resume continuity.

---

### Phase 4: the setpieces

Upstream source: `script/events/setpieces.js` (~3,500 lines; this phase is
mostly careful transcription).

Work:

1. **`scenes_setpieces.rs` (MPL)** (split into `scenes_setpieces_a.rs` /
   `_b.rs` if it gets unwieldy; both MPL): outpost, iron mine, coal mine,
   sulphur mine, house, cave, town, city, ship (the crashed starship:
   grants alien alloy and unlocks the Ship tab via `goHome`), borehole,
   battlefield, swamp. Everything: entry costs (torch for caves and the
   town's deeper branches), embedded fights, weighted branches, loot
   tables, `clearDungeon` → outpost + road, the swamp's gastronome charm
   exchange, city's laser rifle / energy cell loot.
2. Bag space enforcement on world loot (take is bounded by free space;
   leave is always possible).
3. Mines cleared → `goHome` grants the mine building → miner jobs appear
   in the village → the full economy loop closes.
4. Tests: each landmark's happy path and its cost gating with seeded rng;
   dungeon clear converts the tile and draws a road; mine commit
   end-to-end (clear → home → building → job row → income with cured meat).

---

### Phase 5: the ship, space, and the ending

Upstream sources: `script/ship.js`, `script/space.js`.

Work:

1. **Ship view** (data in `data.rs` or `world_data.rs`, MPL): hull and
   engine rows, reinforce hull (1 alien alloy each), upgrade engine (1
   each), lift off (120s cooldown button, disabled at hull 0, one-time
   "Ready to Leave?" confirm event).
2. **`space.rs` (MPL)**: the ascent as a pure per-tick state machine
   stepped from the app's hot tick (15fps): ship position from held arrow
   keys at `SHIP_SPEED` (rescale upstream's px/33ms to cells/tick; keep
   relative speeds), asteroid spawns with upstream's glyph set and
   altitude-scaled frequency/speed (`getSpeed`), collisions cost hull,
   altitude climbs to 60km over 60s, layer titles (Troposphere /
   Stratosphere / Mesosphere / Thermosphere / Exosphere / Space), lose →
   crash notification, back to Ship with the liftoff cooldown; win → the
   ending. Esc aborts the flight (counts as a crash, no drama).
   Key handling must work with terminal key repeat (no key-up events over
   SSH): treat a held arrow as impulses, same approach as the arcade games.
3. **The ending**: upstream's ending sequence text, then `completed = true`,
   save, return to the hub. The landing card gains a completed state
   (small line, e.g. the score-less truth: the wanderer left). Playing a
   completed save re-enters the village as normal; the ship can lift off
   again (harmless, upstream allows it), but `completed` latches.
4. Update `screen.rs` `description()`, the hub landing card, `CONTEXT.md`
   (rewrite the scope section: v2 shipped, what was cut and why),
   `LICENSING.md`, `NOTICE`.
5. Tests: ship upgrade costs and gating; space stepping (collision, hull
   depletion, win altitude) with seeded rng; completed flag persists;
   landing card renders the completed state.

---

## Integration notes (all phases)

- `View` grows (`Room, Outside, Path, World, Ship`); Tab cycles the
  unlocked non-world views; `World` is entered only via embark and left
  only via goHome/death/Esc-park. The Sleeping → Helping builder gate keys
  off `View::Room` exactly as today.
- The event modal and combat live in `State` (session), not in `Game`,
  except the persisted `Expedition::combat` snapshot.
- `sim::settle` gains: pending-reward payouts (wall clock). Nothing else in
  the settle loop changes; expeditions and events never run inside it.
- Saves stay fire-and-forget through `svc.rs`'s per-user write gate; no new
  service code beyond what save shape changes require.
- The Esc arm in `app/input.rs` (`flush_pending_escape`) already routes to
  the door; in-door Esc semantics change per-view (modal closes if the
  scene allows leaving, world parks, space aborts, otherwise leave door).
  Do not add a new top-level arm.
- Rendering must not tick the sim; space/combat cooldowns advance in
  `State::tick` (which the app calls on its cadence), never in `ui_*.rs`.
- Keep `mod.rs` to module declarations only.

## Testing discipline

Func-with-test as you go, adjacent files, seeded rngs everywhere randomness
exists (`StdRng::seed_from_u64`), no sleeps, no full-suite runs (the
reviewer's `make check` covers that). Quality over coverage: the lists above
are the critical behaviors; do not mass-generate beyond them. Never weaken
an assertion to make it pass.

## Review protocol

After each phase the executor stops with the working tree dirty and reports:
what was transcribed from which upstream file, deviations (if any, with
reasons), test list, and anything suspicious left out of scope. The reviewer
checks the diff against upstream and this plan, with special attention to:
licensing placement, verbatim prose, closed-enum discipline, owner-scoped
persistence, and that nothing new runs on the render loop.
