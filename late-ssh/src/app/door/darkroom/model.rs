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

use super::data::{self, Building, Fire, Job, Resource, Temperature};
use super::pace::Pace;

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

    // ---- pacing (ours, not upstream) ----
    pub pace: Pace,
    /// Unix seconds the sim last settled to. Zero on a fresh save.
    pub last_settled: i64,
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
}

/// What a gathering trip did.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GatherOutcome {
    Gathered(i64),
    OnCooldown,
}

impl Game {
    /// A brand new save: a dark room, a dead fire, and nothing else.
    pub fn new() -> Self {
        Self {
            temp_timer: data::ROOM_WARM_DELAY,
            fire_timer: data::FIRE_COOL_DELAY,
            income_timer: super::pace::slowed(data::INCOME_DELAY),
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
            if self.seen_buildings.contains(&building) {
                continue;
            }
            let standing = self.building_count(building) > 0;
            if standing || self.materials_in_reach(building) {
                self.seen_buildings.insert(building);
                if !standing {
                    newly.push(building);
                }
            }
        }
        newly
    }

    fn materials_in_reach(&self, building: Building) -> bool {
        let cost = building.cost(self.building_count(building));
        for (resource, amount) in &cost {
            let enough = match resource {
                Resource::Wood => self.store(Resource::Wood) >= amount / 2,
                other => self.has_seen(*other) && self.store(*other) > 0,
            };
            if !enough {
                return false;
            }
        }
        true
    }

    /// Put one up. Refuses whole: nothing is spent unless everything is there.
    pub fn build(&mut self, building: Building) -> BuildOutcome {
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
        self.buildings.insert(building, built + 1);
        for job in building.unlocks_jobs() {
            self.seen_jobs.insert(*job);
            self.workers.entry(*job).or_insert(0);
        }
        BuildOutcome::Built(building)
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
