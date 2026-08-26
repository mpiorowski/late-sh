/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * This Source Code Form is "Incompatible With Secondary Licenses", as
 * defined by the Mozilla Public License, v. 2.0.
 *
 * Derived from A Dark Room by Michael Townsend / Doublespeak Games
 * (https://github.com/doublespeakgames/adarkroom), MPL-2.0. The rules below
 * are transcribed from `script/room.js`, `script/outside.js` and
 * `script/state_manager.js`. See LICENSING.md and NOTICE. */

//! The persistent game and the rules that act on it.
//!
//! Upstream keeps one nested JSON blob behind a `StateManager` and mutates it
//! from every module. This is the same blob with a closed shape: unknown
//! resources, buildings and jobs cannot be represented, and every clock is an
//! explicit countdown rather than a live `setTimeout` handle, because the port
//! settles time forward on demand instead of ticking (see `sim`).

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::data::{self, Blueprint, Building, Fabricable, Fire, Job, Perk, Resource, Temperature};
use super::pace::Pace;
use super::world_data::{self, Tile};

/// How far along the stranger's arc is. Upstream tracks this as an integer
/// `game.builder.level` from -1 to 4.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Builder {
    /// Nobody has seen the light yet.
    #[default]
    Unseen,
    /// The light is out in the dark; she is on her way.
    Approaching,
    /// Collapsed in the corner.
    Collapsed,
    /// Shivering, mumbling.
    Shivering,
    /// Breathing calmly, asleep.
    Sleeping,
    /// On her feet, building things, feeding the fire.
    Helping,
}

/// Which panel the player is looking at. The stranger only gets up when the
/// player is actually in the room, mirroring upstream's `onArrival` hook.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum View {
    #[default]
    Room,
    Outside,
    Path,
    World,
    Fabricator,
    Ship,
}

/// How far along the thief's arc is (upstream's `game.thieves`, 1 and 2).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Thieves {
    /// Nobody is skimming yet.
    #[default]
    None,
    /// Supplies are going missing every income tick.
    Active,
    /// Hanged or spared; the skim has stopped for good.
    Dealt,
}

/// A payoff owed at a wall-clock moment: the Mysterious Wanderer's return.
/// Persisted, because the whole point is that it lands later.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PendingReward {
    pub resource: Resource,
    pub amount: i64,
    /// Unix seconds this comes due.
    pub due: i64,
    pub message: String,
}

/// The committed map: the terrain, what has been seen, and which landmarks
/// have been used up. Rows are strings so a save stays readable and small.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct WorldMap {
    /// One string per row, `RADIUS * 2 + 1` glyphs wide.
    pub tiles: Vec<String>,
    /// `0`/`1` per square: whether the wanderer has ever seen it.
    pub mask: Vec<String>,
    /// `0`/`1` per square: whether its setpiece has been played out.
    pub visited: Vec<String>,
}

/// One of the ravaged battleship's three decks. Upstream keeps a free-form
/// flag per deck on `World.state`; a closed set means a save can never claim a
/// deck the ship does not have.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Deck {
    Engineering,
    Medical,
    Martial,
}

impl Deck {
    pub const ALL: [Deck; 3] = [Deck::Engineering, Deck::Medical, Deck::Martial];

    /// The event key of the wing this deck's elevator opens on. `const` so the
    /// antechamber's buttons can be built from it rather than repeating it.
    pub const fn scene(self) -> &'static str {
        match self {
            Deck::Engineering => "executioner-engineering",
            Deck::Medical => "executioner-medical",
            Deck::Martial => "executioner-martial",
        }
    }

    /// What its button on the elevator bank says.
    pub const fn label(self) -> &'static str {
        match self {
            Deck::Engineering => "engineering",
            Deck::Medical => "medical",
            Deck::Martial => "martial",
        }
    }
}

/// How far into the ravaged battleship the wanderer has got. The three decks
/// can be taken in any order, so this is honestly a set rather than a ladder;
/// what stays closed is *which* decks exist.
///
/// Held on the trip as well as on the save, exactly like the map: upstream
/// tracks it on `World.state`, so progress made on a trip that ends in death
/// is lost with everything else, and only a safe return commits it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Battleship {
    /// The entrance turret is down and the strange device is taken. Upstream's
    /// `World.state.executioner`, which is also what opens the fabricator.
    pub entered: bool,
    pub decks: BTreeSet<Deck>,
}

impl Battleship {
    /// Whether all three decks report clear, which is what unlocks the
    /// command deck's elevator.
    pub fn decks_clear(&self) -> bool {
        Deck::ALL.iter().all(|deck| self.decks.contains(deck))
    }
}

/// A fight in progress, kept only so a dropped session can pick it back up.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CombatSnapshot {
    /// The event and scene the fight belongs to, by table key.
    pub event: String,
    pub scene: String,
    pub enemy_hp: i64,
}

/// A trip in progress. Parked on every move, so a dropped connection costs
/// nothing: supplies burn per move, never per second.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Expedition {
    pub x: i32,
    pub y: i32,
    pub hp: i64,
    pub water: i64,
    pub food_move: u32,
    pub water_move: u32,
    pub fight_move: u32,
    pub starvation: bool,
    pub thirst: bool,
    pub danger: bool,
    /// What is in the pack right now.
    pub outfit: BTreeMap<Resource, i64>,
    /// The working copy of the map. Committed on a safe return, discarded on
    /// death, exactly as upstream keeps `World.state` apart from `game.world`.
    pub map: WorldMap,
    /// Outposts drunk dry this trip, as `x,y`.
    pub used_outposts: BTreeSet<String>,
    /// Mines whose setpiece was cleared this trip, granted by `go_home`.
    pub cleared: BTreeSet<Building>,
    /// Whether the crashed starship was found this trip.
    pub found_ship: bool,
    /// The working copy of the battleship's progress, started from the save on
    /// embark and committed on a safe return.
    pub battleship: Battleship,
    pub combat: Option<CombatSnapshot>,
}

/// The grid is `RADIUS * 2 + 1` square; rows are indexed by `y`, characters
/// within a row by `x`, so a saved map reads the way it is drawn.
pub const GRID: i32 = world_data::RADIUS * 2 + 1;

impl WorldMap {
    /// An unwritten map: all barrens, nothing seen, nothing visited.
    pub fn blank() -> Self {
        let width = GRID as usize;
        Self {
            tiles: vec![Tile::Barrens.glyph().to_string().repeat(width); width],
            mask: vec!["0".repeat(width); width],
            visited: vec!["0".repeat(width); width],
        }
    }

    fn char_at(rows: &[String], x: i32, y: i32) -> Option<char> {
        if x < 0 || y < 0 || x >= GRID || y >= GRID {
            return None;
        }
        rows.get(y as usize)
            .and_then(|row| row.chars().nth(x as usize))
    }

    fn put(rows: &mut [String], x: i32, y: i32, value: char) {
        if x < 0 || y < 0 || x >= GRID || y >= GRID {
            return;
        }
        let Some(row) = rows.get_mut(y as usize) else {
            return;
        };
        let mut chars: Vec<char> = row.chars().collect();
        if (x as usize) < chars.len() {
            chars[x as usize] = value;
            *row = chars.into_iter().collect();
        }
    }

    /// The tile at a square. Anything off the edge reads as barrens.
    pub fn tile(&self, x: i32, y: i32) -> Tile {
        match Self::char_at(&self.tiles, x, y) {
            Some(glyph) => Tile::from_glyph(glyph),
            None => Tile::Barrens,
        }
    }

    pub fn set_tile(&mut self, x: i32, y: i32, tile: Tile) {
        Self::put(&mut self.tiles, x, y, tile.glyph());
    }

    /// Whether the square has ever been in the lantern light.
    pub fn seen(&self, x: i32, y: i32) -> bool {
        Self::char_at(&self.mask, x, y) == Some('1')
    }

    pub fn set_seen(&mut self, x: i32, y: i32) {
        Self::put(&mut self.mask, x, y, '1');
    }

    /// Whether the square's setpiece has already been played out.
    pub fn visited(&self, x: i32, y: i32) -> bool {
        Self::char_at(&self.visited, x, y) == Some('1')
    }

    pub fn set_visited(&mut self, x: i32, y: i32) {
        Self::put(&mut self.visited, x, y, '1');
    }

    /// Whether every square has been seen, which retires the scout's map.
    pub fn all_seen(&self) -> bool {
        self.mask.iter().all(|row| !row.contains('0'))
    }
}

impl Expedition {
    pub fn carrying(&self, item: Resource) -> i64 {
        self.outfit.get(&item).copied().unwrap_or(0)
    }

    pub fn add(&mut self, item: Resource, amount: i64) {
        let next = (self.carrying(item) + amount).max(0);
        self.outfit.insert(item, next);
    }

    /// Manhattan distance from the village, which is what the wasteland's
    /// danger and encounter tiers key off.
    pub fn distance(&self) -> i32 {
        (self.x - world_data::VILLAGE_POS.0).abs() + (self.y - world_data::VILLAGE_POS.1).abs()
    }

    /// What the pack is carrying, by weight.
    pub fn load(&self) -> f64 {
        self.outfit
            .iter()
            .map(|(item, count)| *count as f64 * world_data::weight(*item))
            .sum()
    }
}

/// The ship, once the crashed starship has been found.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ShipState {
    pub hull: i64,
    pub thrusters: i64,
    /// Whether the ship's one arrival line has been printed (upstream's
    /// persisted `spaceShip.seenShip`).
    pub seen_ship: bool,
    /// Whether the "ready to leave?" warning has been shown.
    pub seen_warning: bool,
    /// Seconds left before liftoff is possible again.
    pub liftoff_cooldown: u32,
}

impl Default for ShipState {
    fn default() -> Self {
        Self {
            hull: 0,
            thrusters: 1,
            seen_ship: false,
            seen_warning: false,
            liftoff_cooldown: 0,
        }
    }
}

/// The whole save. Every field carries a serde default so an older blob always
/// deserializes; see `persist` for the envelope.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Game {
    // ---- what the player holds ----
    pub stores: BTreeMap<Resource, i64>,
    /// Fractional income not yet worth a whole unit. Upstream pays whole
    /// numbers on a fast clock; slowing income down means a hunter earns half
    /// a fur at a time, so the remainder has to live somewhere.
    pub carry: BTreeMap<Resource, f64>,
    pub buildings: BTreeMap<Building, u32>,
    pub workers: BTreeMap<Job, u32>,
    pub population: u32,

    // ---- what the player has been shown ----
    /// Build options that have appeared. Upstream latches these in
    /// `Room.buttons` so an option never disappears once offered, even after
    /// the materials are spent.
    pub seen_buildings: BTreeSet<Building>,
    /// Jobs whose building has been raised.
    pub seen_jobs: BTreeSet<Job>,
    /// Craft rows that have appeared, latched the same way (upstream's
    /// `Room.buttons` again, keyed by the item the row makes).
    pub seen_crafts: BTreeSet<Resource>,
    /// Trade rows that have appeared.
    pub seen_trades: BTreeSet<Resource>,
    pub forest_unlocked: bool,
    /// Whether the player has stepped outside yet (upstream's `seenForest`),
    /// which is what gates the one-time sky-is-grey line.
    pub seen_forest: bool,

    // ---- the room ----
    pub fire: Fire,
    pub temperature: Temperature,
    pub builder: Builder,

    // ---- clocks, in seconds until the next step ----
    pub fire_timer: u32,
    pub temp_timer: u32,
    pub builder_timer: u32,
    /// Set when the stranger collapses; the wood runs out when it fires.
    pub need_wood_timer: Option<u32>,
    pub income_timer: u32,
    pub pop_timer: u32,
    pub stoke_cooldown: u32,
    pub gather_cooldown: u32,
    pub traps_cooldown: u32,

    // ---- the wanderer ----
    pub perks: BTreeSet<Perk>,
    /// How many punches have been thrown, which is how fists get better.
    pub punches: u32,
    /// What is packed for the next trip. Upstream keeps this apart from the
    /// store room and deducts it again on every embark.
    pub outfit: BTreeMap<Resource, i64>,
    /// How many times hunger has been ignored, and thirst. Ten of either
    /// teaches a perk (upstream `character.starved`/`dehydrated`).
    pub starved: u32,
    pub dehydrated: u32,

    // ---- the wider world ----
    pub thieves: Thieves,
    /// What the thieves have taken, handed back if the thief is hanged.
    pub stolen: BTreeMap<Resource, i64>,
    /// Payoffs owed at a wall-clock time.
    pub pending_rewards: Vec<PendingReward>,
    /// Set by buying the compass: the dusty path opens.
    pub path_unlocked: bool,
    /// The committed map, generated the moment the path opens.
    pub world: Option<WorldMap>,
    /// A trip in progress, parked between sessions.
    pub expedition: Option<Expedition>,
    pub ship: Option<ShipState>,
    /// How far into the ravaged battleship this save has got.
    pub battleship: Battleship,
    /// Whether the strange device has come home and the fabricator stands.
    pub fabricator: bool,
    /// Whether the fabricator has said its one arrival line.
    pub seen_fabricator: bool,
    /// Blueprints redeemed out of the pack, which is what the fabricator's
    /// gated recipes read.
    pub blueprints: BTreeSet<Blueprint>,
    /// Whether the account had already finished the game when this save was
    /// created. It is the only thing a run inherits from the ones before it,
    /// and all it does is put the ravaged battleship on the map.
    pub veteran: bool,
    /// Whether a ruined city has been cleared, which is what brings the
    /// military down on the village.
    pub city_cleared: bool,
    /// Seconds before another expedition may set out (death cooldown).
    pub embark_cooldown: u32,
    /// Whether the whole map has been uncovered, which retires the scout's
    /// map offer.
    pub seen_all_map: bool,
    // ---- pacing (ours, not upstream) ----
    pub pace: Pace,
    /// Unix seconds the sim last settled to. Zero on a fresh save.
    pub last_settled: i64,
    /// This run's identity, which is what the ending's chip payout is keyed
    /// on (SHOP.md Phase 6: every run that gets out pays, and the run is the
    /// gate). The ending wipes the save, so the next run gets a new one. A
    /// blob written before this field existed deserializes with a fresh id
    /// rather than a nil one, which is why the default is `now_v7` and not
    /// `Default`.
    #[serde(default = "Uuid::now_v7")]
    pub run_id: Uuid,
}

/// What lighting the fire did. `drew_builder` marks the one moment the room
/// first burns bright enough to be seen from outside.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LightFire {
    Lit { drew_builder: bool },
    NotEnoughWood,
    OnCooldown,
}

/// What stoking the fire did.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StokeFire {
    Stoked { drew_builder: bool },
    OutOfWood,
    OnCooldown,
}

/// What a build attempt did.
#[derive(Clone, Debug, PartialEq)]
pub enum BuildOutcome {
    Built(Building),
    AtMaximum(Building),
    /// Short of these amounts (the first missing resource upstream reports).
    Missing(Resource),
    /// The builder is not on her feet yet.
    NoBuilder,
    /// The room is Cold or worse; upstream's builder just shivers.
    TooCold,
    /// A mine: the wasteland grants it, the builder never offers it.
    NotOffered(Building),
}

/// What a craft attempt did. Upstream runs crafting through the same
/// `Room.build` the buildings use, so the cold gate applies here too.
#[derive(Clone, Debug, PartialEq)]
pub enum CraftOutcome {
    Crafted(Resource),
    AtMaximum(Resource),
    Missing(Resource),
    TooCold,
}

/// What a purchase did. No cold gate: the nomads do not care how the room is.
#[derive(Clone, Debug, PartialEq)]
pub enum BuyOutcome {
    Bought(Resource),
    AtMaximum(Resource),
    Missing(Resource),
}

/// What a gathering trip did.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GatherOutcome {
    Gathered(i64),
    OnCooldown,
}

impl Game {
    /// A brand new save: a dark room, a dead fire, and nothing else.
    ///
    /// `veteran` says whether this account has ever finished the game before,
    /// which is the one thing a fresh run inherits (the caller reads it off
    /// `darkroom_veterans`, because the save it replaced was deleted on the way
    /// out). It carries no stores, no perks and no pacing change: all it does
    /// is put the ravaged battleship on the map when the path opens.
    pub fn new(veteran: bool) -> Self {
        Self {
            temp_timer: data::ROOM_WARM_DELAY,
            fire_timer: data::FIRE_COOL_DELAY,
            income_timer: super::pace::slowed(data::INCOME_DELAY),
            veteran,
            run_id: Uuid::now_v7(),
            ..Self::default()
        }
    }

    // ---- stores ----

    pub fn store(&self, resource: Resource) -> i64 {
        self.stores.get(&resource).copied().unwrap_or(0)
    }

    pub fn set_store(&mut self, resource: Resource, amount: i64) {
        self.stores.insert(resource, amount.max(0));
    }

    pub fn add_store(&mut self, resource: Resource, amount: i64) {
        let next = (self.store(resource) + amount).max(0);
        self.stores.insert(resource, next);
    }

    /// Whether the player has ever held this resource. Upstream gates build
    /// options on having *seen* a material, which it approximates by the key
    /// existing in `stores` at all.
    pub fn has_seen(&self, resource: Resource) -> bool {
        self.stores.contains_key(&resource)
    }

    pub fn building_count(&self, building: Building) -> u32 {
        self.buildings.get(&building).copied().unwrap_or(0)
    }

    pub fn worker_count(&self, job: Job) -> u32 {
        self.workers.get(&job).copied().unwrap_or(0)
    }

    // ---- the village ----

    /// How many villagers the huts sleep.
    pub fn max_population(&self) -> u32 {
        self.building_count(Building::Hut) * data::HUT_ROOM
    }

    /// Everyone not assigned to a trade gathers wood, by definition.
    pub fn gatherers(&self) -> u32 {
        let assigned: u32 = self.workers.values().sum();
        self.population.saturating_sub(assigned)
    }

    /// Whether a hut stands, which is what turns the forest into a village.
    pub fn has_village(&self) -> bool {
        self.building_count(Building::Hut) > 0
    }

    // ---- the room ----

    /// Light a dead fire. Costs five wood, and refuses rather than half-doing
    /// it when the wood is short.
    pub fn light_fire(&mut self) -> LightFire {
        if self.stoke_cooldown > 0 {
            return LightFire::OnCooldown;
        }
        // The very first fire is free: upstream lets you light it with no
        // stores at all, which is how the game opens.
        if self.has_seen(Resource::Wood) {
            if self.store(Resource::Wood) < data::LIGHT_FIRE_COST {
                return LightFire::NotEnoughWood;
            }
            self.add_store(Resource::Wood, -data::LIGHT_FIRE_COST);
        }
        self.fire = Fire::Burning;
        self.stoke_cooldown = data::STOKE_COOLDOWN;
        LightFire::Lit {
            drew_builder: self.on_fire_change(),
        }
    }

    /// Feed a live fire one log.
    pub fn stoke_fire(&mut self) -> StokeFire {
        if self.stoke_cooldown > 0 {
            return StokeFire::OnCooldown;
        }
        if self.has_seen(Resource::Wood) {
            if self.store(Resource::Wood) == 0 {
                return StokeFire::OutOfWood;
            }
            self.add_store(Resource::Wood, -data::STOKE_FIRE_COST);
        }
        self.fire = self.fire.stoked();
        self.stoke_cooldown = data::STOKE_COOLDOWN;
        StokeFire::Stoked {
            drew_builder: self.on_fire_change(),
        }
    }

    /// Shared bookkeeping after the fire moves: the cool clock restarts, and a
    /// fire brighter than smoldering is what draws the stranger in. Returns
    /// whether *this* change is what put her on her way.
    fn on_fire_change(&mut self) -> bool {
        self.fire_timer = data::FIRE_COOL_DELAY;
        if self.fire.value() > Fire::Smoldering.value() && self.builder == Builder::Unseen {
            self.builder = Builder::Approaching;
            self.builder_timer = data::BUILDER_STATE_DELAY;
            return true;
        }
        false
    }

    /// The room title tracks the fire: it is only a *dark* room while the fire
    /// is out or nearly out.
    pub fn room_title(&self) -> &'static str {
        if self.fire.value() < Fire::Flickering.value() {
            data::TITLE
        } else {
            "A Firelit Room"
        }
    }

    /// The outside title tracks the hut count (upstream `Outside.setTitle`).
    pub fn outside_title(&self) -> &'static str {
        match self.building_count(Building::Hut) {
            0 => "A Silent Forest",
            1 => "A Lonely Hut",
            2..=4 => "A Tiny Village",
            5..=8 => "A Modest Village",
            9..=14 => "A Large Village",
            _ => "A Raucous Village",
        }
    }

    // ---- the forest ----

    /// Walk out and pick up what the forest floor offers.
    pub fn gather_wood(&mut self) -> GatherOutcome {
        if self.gather_cooldown > 0 {
            return GatherOutcome::OnCooldown;
        }
        let amount = if self.building_count(Building::Cart) > 0 {
            data::GATHER_WOOD_CART
        } else {
            data::GATHER_WOOD
        };
        self.add_store(Resource::Wood, amount);
        self.gather_cooldown = data::GATHER_DELAY;
        GatherOutcome::Gathered(amount)
    }

    /// How many trap rolls are due: one per trap, plus one per baited trap.
    pub fn trap_rolls(&self) -> u32 {
        let traps = self.building_count(Building::Trap);
        let bait = self.store(Resource::Bait).max(0) as u32;
        traps + bait.min(traps)
    }

    /// Standing traps split into `(bare, baited)`, the way upstream's village
    /// panel lists "trap" and "baited trap" as separate rows.
    pub fn trap_rows(&self) -> (u32, u32) {
        let traps = self.building_count(Building::Trap);
        let baited = (self.store(Resource::Bait).max(0) as u32).min(traps);
        (traps - baited, baited)
    }

    /// Bank what the traps caught and spend the bait that drew it in.
    pub fn collect_traps(&mut self, drops: &[(Resource, i64)]) {
        let traps = self.building_count(Building::Trap);
        let bait = self.store(Resource::Bait).max(0) as u32;
        self.add_store(Resource::Bait, -i64::from(bait.min(traps)));
        for (resource, amount) in drops {
            self.add_store(*resource, *amount);
        }
        self.traps_cooldown = data::TRAPS_DELAY;
    }

    // ---- building ----

    /// Whether a build option should be on screen. Upstream latches this: once
    /// offered, an option stays even when the materials are gone.
    pub fn build_available(&self, building: Building) -> bool {
        self.seen_buildings.contains(&building)
    }

    /// Re-check which options the builder can now offer, returning the ones
    /// that just became available so the caller can announce them. Upstream's
    /// rule: she is on her feet, one is already standing *or* half the wood is
    /// in hand and every other material has been seen.
    pub fn refresh_build_options(&mut self) -> Vec<Building> {
        if self.builder != Builder::Helping {
            return Vec::new();
        }
        let mut newly = Vec::new();
        for building in Building::ALL {
            if !building.builder_built() || self.seen_buildings.contains(&building) {
                continue;
            }
            let standing = self.building_count(building) > 0;
            if standing || self.materials_in_reach(&building.cost(self.building_count(building))) {
                self.seen_buildings.insert(building);
                if !standing {
                    newly.push(building);
                }
            }
        }
        newly
    }

    /// Latch the craft and buy rows that have come within reach. Neither
    /// announces anything: upstream hangs an `availableMsg` on its buildings
    /// only, so an item or a good just turns up in its column.
    ///
    /// Crafting wants the same half-the-wood rule the builder uses, plus a
    /// workshop. Buying wants a trading post and, per upstream `buyUnlocked`,
    /// that the good has been *seen* at least once, which is why the wasteland
    /// metals stay off the shelf until the wasteland has handed some over. The
    /// compass is the one exception, and the reason the path can be found at
    /// all.
    pub fn refresh_item_options(&mut self) {
        if self.builder == Builder::Helping {
            let workshop = self.building_count(Building::Workshop) > 0;
            for craftable in &data::CRAFTABLES {
                if self.seen_crafts.contains(&craftable.item) {
                    continue;
                }
                if craftable.item.kind().needs_workshop() && !workshop {
                    continue;
                }
                if self.materials_in_reach(craftable.cost) {
                    self.seen_crafts.insert(craftable.item);
                }
            }
        }
        if self.building_count(Building::TradingPost) > 0 {
            for good in &data::TRADE_GOODS {
                if good.good == Resource::Compass || self.has_seen(good.good) {
                    self.seen_trades.insert(good.good);
                }
            }
        }
    }

    /// Upstream's unlock test: at least half the wood, and at least one of
    /// everything else the recipe calls for.
    fn materials_in_reach(&self, cost: &[(Resource, i64)]) -> bool {
        for (resource, amount) in cost {
            let held = self.store(*resource);
            let enough = match resource {
                Resource::Wood => held as f64 >= *amount as f64 * 0.5 && held > 0,
                _ => held > 0,
            };
            if !enough {
                return false;
            }
        }
        true
    }

    /// Put one up. Refuses whole: nothing is spent unless everything is there.
    pub fn build(&mut self, building: Building) -> BuildOutcome {
        if !building.builder_built() {
            return BuildOutcome::NotOffered(building);
        }
        if self.builder != Builder::Helping {
            return BuildOutcome::NoBuilder;
        }
        // Upstream refuses outright while the room is Cold or worse, before it
        // even looks at costs: a shivering builder builds nothing.
        if self.temperature <= Temperature::Cold {
            return BuildOutcome::TooCold;
        }
        let built = self.building_count(building);
        if building.maximum().is_some_and(|max| built + 1 > max) {
            return BuildOutcome::AtMaximum(building);
        }
        let cost = building.cost(built);
        if let Some((resource, _)) = cost.iter().find(|(r, n)| self.store(*r) < *n) {
            return BuildOutcome::Missing(*resource);
        }
        for (resource, amount) in &cost {
            self.add_store(*resource, -amount);
        }
        self.raise(building);
        BuildOutcome::Built(building)
    }

    /// Stand a building up and open whatever trades come with it. Separate
    /// from `build` because the wasteland grants the mines outright, without
    /// paying anything for them.
    pub fn raise(&mut self, building: Building) {
        let built = self.building_count(building);
        self.buildings.insert(building, built + 1);
        for job in building.unlocks_jobs() {
            self.seen_jobs.insert(*job);
            self.workers.entry(*job).or_insert(0);
        }
    }

    // ---- the workshop and the trading post ----

    /// Whether a craft row should be on screen (latched, like the build rows).
    pub fn craft_available(&self, craftable: &data::Craftable) -> bool {
        self.seen_crafts.contains(&craftable.item)
    }

    /// Whether a buy row should be on screen.
    pub fn buy_available(&self, good: &data::TradeGood) -> bool {
        self.seen_trades.contains(&good.good)
    }

    /// Make one. Refuses whole, and refuses in a cold room: upstream runs
    /// craftables through the same handler as the buildings.
    pub fn craft(&mut self, craftable: &data::Craftable) -> CraftOutcome {
        if self.temperature <= Temperature::Cold {
            return CraftOutcome::TooCold;
        }
        let held = self.store(craftable.item);
        if craftable
            .maximum
            .is_some_and(|max| held + 1 > i64::from(max))
        {
            return CraftOutcome::AtMaximum(craftable.item);
        }
        if let Some((resource, _)) = craftable.cost.iter().find(|(r, n)| self.store(*r) < *n) {
            return CraftOutcome::Missing(*resource);
        }
        for (resource, amount) in craftable.cost {
            self.add_store(*resource, -amount);
        }
        self.add_store(craftable.item, 1);
        CraftOutcome::Crafted(craftable.item)
    }

    /// Whether a fabricate row should be on screen. Unlike the workshop, this
    /// does not latch on having once been affordable: upstream gates purely on
    /// the blueprint, and three recipes need none at all.
    pub fn fabricable_available(&self, fabricable: &Fabricable) -> bool {
        match fabricable.blueprint {
            Some(blueprint) => self.blueprints.contains(&blueprint),
            None => true,
        }
    }

    /// Fabricate one batch. Refuses whole, like everything else that spends
    /// stores. No cold gate: the fabricator is wanderer machinery humming in
    /// its own corner, not the builder working in a freezing room.
    pub fn fabricate(&mut self, fabricable: &Fabricable) -> CraftOutcome {
        let held = self.store(fabricable.item);
        if fabricable
            .maximum
            .is_some_and(|max| held + 1 > i64::from(max))
        {
            return CraftOutcome::AtMaximum(fabricable.item);
        }
        if let Some((resource, _)) = fabricable.cost.iter().find(|(r, n)| self.store(*r) < *n) {
            return CraftOutcome::Missing(*resource);
        }
        for (resource, amount) in fabricable.cost {
            self.add_store(*resource, -amount);
        }
        self.add_store(fabricable.item, fabricable.quantity);
        CraftOutcome::Crafted(fabricable.item)
    }

    /// Turn blueprint tokens carried home into recipes the fabricator knows,
    /// consuming the tokens (upstream `World.redeemBlueprints`). Returns
    /// whether anything was redeemed, which is what prints the line.
    pub fn redeem_blueprints(&mut self, outfit: &mut BTreeMap<Resource, i64>) -> bool {
        let mut redeemed = false;
        for blueprint in Blueprint::ALL {
            if outfit.remove(&blueprint.token()).is_some_and(|n| n > 0) {
                self.blueprints.insert(blueprint);
                redeemed = true;
            }
        }
        redeemed
    }

    /// Buy one. Refuses whole, same as everything else.
    pub fn buy(&mut self, good: &data::TradeGood) -> BuyOutcome {
        let held = self.store(good.good);
        if good.maximum.is_some_and(|max| held + 1 > i64::from(max)) {
            return BuyOutcome::AtMaximum(good.good);
        }
        if let Some((resource, _)) = good.cost.iter().find(|(r, n)| self.store(*r) < *n) {
            return BuyOutcome::Missing(*resource);
        }
        for (resource, amount) in good.cost {
            self.add_store(*resource, -amount);
        }
        self.add_store(good.good, 1);
        BuyOutcome::Bought(good.good)
    }

    // ---- the wanderer ----

    pub fn has_perk(&self, perk: Perk) -> bool {
        self.perks.contains(&perk)
    }

    /// Learn a perk. Returns whether it is new, so the caller can announce it.
    pub fn add_perk(&mut self, perk: Perk) -> bool {
        self.perks.insert(perk)
    }

    /// How much health the wanderer sets out with (upstream `getMaxHealth`).
    pub fn max_health(&self) -> i64 {
        let bonus = world_data::ARMOUR
            .iter()
            .find(|(item, _, _)| self.store(*item) > 0)
            .map(|(_, hp, _)| *hp)
            .unwrap_or(0);
        world_data::BASE_HEALTH + bonus
    }

    /// What the armour row on the path screen says.
    pub fn armour_label(&self) -> &'static str {
        world_data::ARMOUR
            .iter()
            .find(|(item, _, _)| self.store(*item) > 0)
            .map(|(_, _, label)| *label)
            .unwrap_or("none")
    }

    /// How much water the wanderer sets out with.
    pub fn max_water(&self) -> i64 {
        let bonus = world_data::WATERSKINS
            .iter()
            .find(|(item, _)| self.store(*item) > 0)
            .map(|(_, water)| *water)
            .unwrap_or(0);
        world_data::BASE_WATER + bonus
    }

    /// How much the pack holds.
    pub fn capacity(&self) -> f64 {
        let bonus = world_data::PACKS
            .iter()
            .find(|(item, _)| self.store(*item) > 0)
            .map(|(_, space)| *space)
            .unwrap_or(0.0);
        world_data::DEFAULT_BAG_SPACE + bonus
    }

    /// The odds of a swing landing.
    pub fn hit_chance(&self) -> f64 {
        if self.has_perk(Perk::Precise) {
            world_data::BASE_HIT_CHANCE + 0.1
        } else {
            world_data::BASE_HIT_CHANCE
        }
    }

    /// What a strip of cured meat is worth in a fight.
    pub fn meat_heal(&self) -> i64 {
        if self.has_perk(Perk::Gastronome) {
            world_data::MEAT_HEAL * 2
        } else {
            world_data::MEAT_HEAL
        }
    }

    /// Whether the wanderer can set out: cured meat actually *packed* (and
    /// still in the store room to take), and no death cooldown running.
    /// Upstream disables the embark button off `Path.outfit['cured meat']`,
    /// not off the store room.
    pub fn can_embark(&self) -> bool {
        let packed = self.outfit.get(&Resource::CuredMeat).copied().unwrap_or(0);
        self.embark_cooldown == 0 && packed.min(self.store(Resource::CuredMeat)) > 0
    }

    /// The tile the map holds at a square, or the barrens beyond its edge.
    pub fn world_tile(&self, x: i32, y: i32) -> Tile {
        self.world
            .as_ref()
            .map(|world| world.tile(x, y))
            .unwrap_or(Tile::Barrens)
    }

    // ---- workers ----

    /// Move villagers onto a trade, up to the number still gathering.
    pub fn assign_worker(&mut self, job: Job, count: u32) -> u32 {
        let moved = count.min(self.gatherers());
        if moved > 0 {
            *self.workers.entry(job).or_insert(0) += moved;
        }
        moved
    }

    /// Move villagers off a trade and back to gathering.
    pub fn unassign_worker(&mut self, job: Job, count: u32) -> u32 {
        let current = self.worker_count(job);
        let moved = count.min(current);
        if moved > 0 {
            self.workers.insert(job, current - moved);
        }
        moved
    }

    // ---- income ----

    /// Net movement per income tick for every resource, all sources summed:
    /// upstream's per-store income tooltip. Display only; the actual payout in
    /// `sim` runs source by source so a starved trade stalls alone.
    pub fn income_per_tick(&self) -> BTreeMap<Resource, f64> {
        let mut totals: BTreeMap<Resource, f64> = BTreeMap::new();
        if self.builder == Builder::Helping {
            let (resource, amount) = data::BUILDER_YIELD;
            *totals.entry(resource).or_insert(0.0) += amount;
        }
        let gatherers = f64::from(self.gatherers());
        if gatherers > 0.0 {
            let (resource, amount) = data::GATHERER_YIELD;
            *totals.entry(resource).or_insert(0.0) += amount * gatherers;
        }
        for job in Job::ALL {
            let workers = f64::from(self.worker_count(job));
            if workers == 0.0 {
                continue;
            }
            for (resource, amount) in job.yields() {
                *totals.entry(*resource).or_insert(0.0) += amount * workers;
            }
        }
        totals.retain(|_, amount| *amount != 0.0);
        totals
    }
}
