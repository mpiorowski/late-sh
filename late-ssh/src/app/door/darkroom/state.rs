//! Per-session Dark Room state: the loaded game, which panel is open, the
//! cursor, the notification log, and whatever is happening right now (an
//! event modal, a fight, a trip, a flight).
//!
//! Time comes from two places and they are kept apart on purpose. Village
//! time is settled forward against the wall clock in `sim`, capped and slowed.
//! Live play (event cooldowns, the enemy's swing, the ascent) runs on the raw
//! delta between ticks, uncapped and unslowed. There is still no game loop:
//! `tick` is called by the app and is correct at any cadence.

use std::collections::VecDeque;

use chrono::{DateTime, Utc};
use tokio::sync::watch;
use uuid::Uuid;

use super::data::{self, Building, Craftable, Fire, Job, Resource, TradeGood};
use super::event::{self, Active, Ctx, Outcome};
use super::model::{
    BuildOutcome, BuyOutcome, CombatSnapshot, CraftOutcome, Game, GatherOutcome, LightFire,
    ShipState, StokeFire, View,
};
use super::pace;
use super::sim;
use super::space::{self, Flight, Space};
use super::svc::{DarkroomService, GameLoad};
use super::world::{self, Direction, Step};
use super::world_data::{self, Weapon};

/// How many notification lines the log keeps. Upstream fades them out of a
/// scrolling column; a fixed window is the terminal equivalent.
const LOG_CAP: usize = 200;

/// One actionable row in the current panel.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Row {
    LightFire,
    StokeFire,
    Build(Building),
    /// A workshop craftable. Holding the table entry itself means a row can
    /// only ever name something the table actually lists.
    Craft(&'static Craftable),
    /// A trading post good, same idea.
    Buy(&'static TradeGood),
    GatherWood,
    CheckTraps,
    Worker(Job),
    /// A row of the outfitting screen: how many to pack.
    Outfit(Resource),
    Embark,
    ReinforceHull,
    UpgradeEngine,
    LiftOff,
    /// A row of the event modal that is currently up.
    Event(event::Row),
    Leave,
}

/// Which group of room buttons a row belongs under, for the renderer's
/// legends. Upstream splits the room panel into build, craft and buy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Section {
    Build,
    Craft,
    Buy,
}

impl Section {
    pub fn legend(self) -> &'static str {
        match self {
            Section::Build => data::SECTION_BUILD,
            Section::Craft => data::SECTION_CRAFT,
            Section::Buy => data::SECTION_BUY,
        }
    }
}

/// What acting on a row asked the session to do.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Acted {
    /// Handled in place.
    Stay,
    /// The player wants out of the door.
    Leave,
}

pub struct State {
    svc: DarkroomService,
    user_id: Uuid,
    /// When this SSH session connected. Credit never reaches back past it, so
    /// logged-out time is worth nothing.
    session_start: DateTime<Utc>,
    load: watch::Receiver<GameLoad>,
    /// The authoritative game, once loaded.
    game: Option<Game>,
    pub view: View,
    pub cursor: usize,
    log: VecDeque<String>,
    /// Real seconds dropped on the floor because today's allowance is spent.
    /// Shown in the status line so a still village never looks like a bug.
    withheld_today: u32,
    /// The event modal, if one is up. Not persisted: a dropped session drops
    /// the modal, exactly as closing the tab did.
    pub event: Option<Active>,
    /// Seconds until the next event roll.
    event_timer: f64,
    /// The ascent, if the ship has left the ground.
    pub flight: Option<Space>,
    /// When `tick` last ran, for live play.
    last_tick: DateTime<Utc>,
}

impl State {
    pub fn new(svc: DarkroomService, user_id: Uuid, session_start: DateTime<Utc>) -> Self {
        let load = svc.load_game(user_id);
        let mut rng = rand::thread_rng();
        Self {
            svc,
            user_id,
            session_start,
            load,
            game: None,
            view: View::Room,
            cursor: 0,
            log: VecDeque::new(),
            withheld_today: 0,
            event: None,
            event_timer: event::next_event_delay(&mut rng),
            flight: None,
            last_tick: Utc::now(),
        }
    }

    /// The loaded game, or `None` while the DB round-trip is in flight.
    pub fn game(&self) -> Option<&Game> {
        self.game.as_ref()
    }

    /// Notification lines, newest last.
    pub fn log(&self) -> impl Iterator<Item = &str> {
        self.log.iter().map(String::as_str)
    }

    /// Village seconds left in today's allowance.
    pub fn credit_remaining(&self) -> u32 {
        self.game
            .as_ref()
            .map(|g| g.pace.remaining(Utc::now()))
            .unwrap_or(pace::DAILY_CREDIT_SECS)
    }

    /// Whether today's allowance ran out, so the UI can say so plainly.
    pub fn credit_exhausted(&self) -> bool {
        self.withheld_today > 0 && self.credit_remaining() == 0
    }

    /// Drain the load channel and advance the world. Returns whether anything
    /// the player can see changed (the render loop's dirty contract).
    pub fn tick(&mut self) -> bool {
        let now = Utc::now();
        let delta = (now - self.last_tick).num_milliseconds().max(0) as f64 / 1000.0;
        self.last_tick = now;

        let mut changed = false;
        // Read the value, never `has_changed()`: the loader drops its sender as
        // soon as it has sent, which makes `has_changed()` return `Err` and
        // would strand the session on "loading" forever.
        if self.game.is_none() {
            // Bind the clone before touching `self` again: the watch guard
            // holds a borrow for as long as it is alive.
            let loaded = match self.load.borrow_and_update().clone() {
                GameLoad::Ready(game) => Some(game),
                GameLoad::Loading => None,
            };
            if let Some(game) = loaded {
                // The opening lines, exactly as upstream prints them on init.
                let (fire, room) = (game.fire.text(), game.temperature.text());
                self.game = Some(*game);
                changed = true;
                self.push_log(format!("the room is {room}"));
                self.push_log(format!("the fire is {fire}"));
                self.resume_expedition();
            }
        }
        // Not `||`: the settle must run whether or not the load just landed.
        let advanced = self.settle();
        let live = self.step_live(delta);
        changed || advanced || live
    }

    /// Advance the game to now. Called on every tick and before every action,
    /// so the world is always current when the player acts on it.
    fn settle(&mut self) -> bool {
        let Some(game) = self.game.as_mut() else {
            return false;
        };
        let mut rng = rand::thread_rng();
        let settled = sim::settle(game, self.view, Utc::now(), self.session_start, &mut rng);
        self.withheld_today = self.withheld_today.saturating_add(settled.withheld);
        let had_messages = !settled.messages.is_empty();
        for message in settled.messages {
            self.push_log(message);
        }
        let opened = self.open_path_if_bought();
        // A credited second may have moved a countdown the player is watching
        // even when nothing was worth announcing, so it counts as a change.
        had_messages || opened || settled.credited > 0
    }

    /// Live play: the enemy's swing, button cooldowns, the ascent, and the
    /// clock that decides when something wanders in.
    fn step_live(&mut self, delta: f64) -> bool {
        if self.game.is_none() || delta <= 0.0 {
            return false;
        }
        let mut changed = false;
        if self.event.is_some() {
            changed |= self.tick_event(delta);
        }
        if self.flight.is_some() {
            changed |= self.tick_flight(delta);
        }
        // Events only wander in while the player is in the village or on the
        // path; the wasteland has its own encounters.
        let in_village = matches!(self.view, View::Room | View::Outside);
        if self.event.is_none() && self.flight.is_none() && in_village {
            self.event_timer -= delta;
            if self.event_timer <= 0.0 {
                self.roll_event();
            }
        }
        changed
    }

    fn tick_event(&mut self, delta: f64) -> bool {
        let mut out = Vec::new();
        let outcome = {
            let (Some(game), Some(active)) = (self.game.as_mut(), self.event.as_mut()) else {
                return false;
            };
            let mut trip = game.expedition.take();
            let mut rng = rand::thread_rng();
            let outcome = {
                let mut ctx = Ctx {
                    game: &mut *game,
                    trip: trip.as_mut(),
                    view: self.view,
                    now: Utc::now(),
                };
                active.tick(delta, &mut ctx, &mut rng, &mut out)
            };
            game.expedition = trip;
            outcome
        };
        for message in out {
            self.push_log(message);
        }
        self.finish_event(outcome);
        true
    }

    fn tick_flight(&mut self, delta: f64) -> bool {
        let Some(flight) = self.flight.as_mut() else {
            return false;
        };
        let mut rng = rand::thread_rng();
        flight.tick(delta, &mut rng);
        match flight.outcome {
            None => true,
            Some(Flight::Crashed) => {
                self.flight = None;
                self.push_log(space::MSG_CRASH.to_string());
                if let Some(game) = self.game.as_mut()
                    && let Some(ship) = game.ship.as_mut()
                {
                    ship.liftoff_cooldown = space::LIFTOFF_COOLDOWN;
                }
                self.view = View::Ship;
                self.save();
                true
            }
            Some(Flight::Won) => {
                self.flight = None;
                for line in space::ENDING {
                    self.push_log(line.to_string());
                }
                if let Some(game) = self.game.as_mut() {
                    game.completed = true;
                }
                self.view = View::Ship;
                self.save();
                true
            }
        }
    }

    fn push_log(&mut self, message: String) {
        if self.log.len() == LOG_CAP {
            self.log.pop_front();
        }
        self.log.push_back(message);
    }

    // ---- the path opening ----

    /// Buying a compass, from anywhere, is what opens the path. Checked in
    /// one place so the trading post, the nomad and a lucky find all work.
    fn open_path_if_bought(&mut self) -> bool {
        let Some(game) = self.game.as_mut() else {
            return false;
        };
        if game.path_unlocked || game.store(Resource::Compass) == 0 {
            return false;
        }
        game.path_unlocked = true;
        if game.world.is_none() {
            let mut rng = rand::thread_rng();
            game.world = Some(world::generate(&mut rng));
        }
        let direction = game
            .world
            .as_ref()
            .map(world::compass_dir)
            .unwrap_or("nowhere");
        self.push_log(format!("the compass points {direction}"));
        self.save();
        true
    }

    // ---- events ----

    fn roll_event(&mut self) {
        let mut rng = rand::thread_rng();
        self.event_timer = event::next_event_delay(&mut rng);
        let Some(game) = self.game.as_mut() else {
            return;
        };
        let mut trip = game.expedition.take();
        let mut out = Vec::new();
        let started = {
            let mut ctx = Ctx {
                game: &mut *game,
                trip: trip.as_mut(),
                view: self.view,
                now: Utc::now(),
            };
            let look = ctx.look();
            let pick = event::pick(&super::scenes_village::POOL, &look, &mut rng).or_else(|| {
                event::pick(
                    std::slice::from_ref(&super::scenes_village::MILITARY_RAID),
                    &look,
                    &mut rng,
                )
            });
            pick.map(|chosen| Active::start(chosen, &mut ctx, &mut rng, &mut out))
        };
        game.expedition = trip;
        for message in out {
            self.push_log(message);
        }
        if let Some(active) = started {
            self.event = Some(active);
            self.cursor = 0;
        }
        self.save();
    }

    /// Start a named setpiece or encounter.
    fn start_event(&mut self, chosen: &'static event::Event) {
        let Some(game) = self.game.as_mut() else {
            return;
        };
        let mut trip = game.expedition.take();
        let mut out = Vec::new();
        let mut rng = rand::thread_rng();
        let active = {
            let mut ctx = Ctx {
                game: &mut *game,
                trip: trip.as_mut(),
                view: self.view,
                now: Utc::now(),
            };
            Active::start(chosen, &mut ctx, &mut rng, &mut out)
        };
        game.expedition = trip;
        for message in out {
            self.push_log(message);
        }
        self.event = Some(active);
        self.cursor = 0;
        self.park_combat();
        self.save();
    }

    /// Close the modal, or bury the wanderer.
    fn finish_event(&mut self, outcome: Outcome) {
        match outcome {
            Outcome::Continue => self.park_combat(),
            Outcome::Done => {
                self.event = None;
                self.cursor = 0;
                if let Some(game) = self.game.as_mut()
                    && let Some(trip) = game.expedition.as_mut()
                {
                    trip.combat = None;
                }
                self.save();
            }
            Outcome::Died => {
                self.event = None;
                self.cursor = 0;
                let mut out = Vec::new();
                if let Some(game) = self.game.as_mut() {
                    world::die(game, &mut out);
                }
                for message in out {
                    self.push_log(message);
                }
                self.view = View::Path;
                self.save();
            }
        }
    }

    /// Keep the save's copy of the fight in step, so a dropped session can
    /// pick it up where it left off.
    fn park_combat(&mut self) {
        let (Some(game), Some(active)) = (self.game.as_mut(), self.event.as_ref()) else {
            return;
        };
        let Some(trip) = game.expedition.as_mut() else {
            return;
        };
        trip.combat = active.fight().map(|fight| CombatSnapshot {
            event: active.event.key.to_string(),
            scene: active.scene.key.to_string(),
            enemy_hp: fight.enemy_hp,
        });
    }

    /// Pick a parked trip back up on load, fight and all.
    fn resume_expedition(&mut self) {
        let Some(game) = self.game.as_ref() else {
            return;
        };
        let Some(trip) = game.expedition.as_ref() else {
            return;
        };
        self.view = View::World;
        let Some(snapshot) = trip.combat.clone() else {
            return;
        };
        let found = super::scenes_encounters::by_key(&snapshot.event)
            .or_else(|| super::scenes_setpieces::by_key(&snapshot.event));
        match found.and_then(|chosen| {
            chosen
                .scene(&snapshot.scene)
                .map(|scene| Active::resume(chosen, scene, snapshot.enemy_hp))
        }) {
            Some(active) => self.event = Some(active),
            None => self.push_log("whatever was out there is gone".to_string()),
        }
    }

    // ---- navigation ----

    /// The rows the current panel offers, in display order.
    pub fn rows(&self) -> Vec<Row> {
        let Some(game) = self.game.as_ref() else {
            return vec![Row::Leave];
        };
        if let Some(active) = self.event.as_ref() {
            return active
                .rows(&look_at(game, self.view))
                .into_iter()
                .map(Row::Event)
                .collect();
        }
        let mut rows = Vec::new();
        match self.view {
            View::Room => {
                if game.fire == Fire::Dead {
                    rows.push(Row::LightFire);
                } else {
                    rows.push(Row::StokeFire);
                }
                for building in Building::ALL {
                    if game.build_available(building) {
                        rows.push(Row::Build(building));
                    }
                }
                for craftable in &data::CRAFTABLES {
                    if game.craft_available(craftable) {
                        rows.push(Row::Craft(craftable));
                    }
                }
                for good in &data::TRADE_GOODS {
                    if game.buy_available(good) {
                        rows.push(Row::Buy(good));
                    }
                }
            }
            View::Outside => {
                rows.push(Row::GatherWood);
                if game.building_count(Building::Trap) > 0 {
                    rows.push(Row::CheckTraps);
                }
                for job in Job::ALL {
                    if game.seen_jobs.contains(&job) {
                        rows.push(Row::Worker(job));
                    }
                }
            }
            View::Path => {
                for item in world_data::CARRYABLE {
                    if game.store(item) > 0 || game.outfit.get(&item).copied().unwrap_or(0) > 0 {
                        rows.push(Row::Outfit(item));
                    }
                }
                rows.push(Row::Embark);
            }
            View::World => return Vec::new(),
            View::Ship => {
                rows.push(Row::ReinforceHull);
                rows.push(Row::UpgradeEngine);
                rows.push(Row::LiftOff);
            }
        }
        rows.push(Row::Leave);
        rows
    }

    /// The row under the cursor, clamped to the current list.
    pub fn selected(&self) -> Row {
        let rows = self.rows();
        if rows.is_empty() {
            return Row::Leave;
        }
        rows.get(self.cursor.min(rows.len() - 1))
            .copied()
            .unwrap_or(Row::Leave)
    }

    pub fn move_cursor(&mut self, delta: i32) {
        let len = self.rows().len() as i32;
        if len == 0 {
            return;
        }
        let next = (self.cursor as i32 + delta).rem_euclid(len);
        self.cursor = next as usize;
    }

    /// Switch panels. The forest is only reachable once the wood runs out,
    /// the path once the compass is bought, the ship once it is found. The
    /// wasteland is not in the cycle: it is entered by embarking and left by
    /// coming home, dying, or parking the trip.
    pub fn toggle_view(&mut self) {
        if self.event.is_some() || self.flight.is_some() || self.view == View::World {
            return;
        }
        let Some(game) = self.game.as_ref() else {
            return;
        };
        let mut open = vec![View::Room];
        if game.forest_unlocked {
            open.push(View::Outside);
        }
        if game.path_unlocked {
            open.push(View::Path);
        }
        if game.ship.is_some() {
            open.push(View::Ship);
        }
        let at = open.iter().position(|view| *view == self.view).unwrap_or(0);
        self.view = open[(at + 1) % open.len()];
        self.cursor = 0;
        // The one-time line for stepping outside (upstream `onArrival`).
        let first_visit = self.view == View::Outside
            && self
                .game
                .as_mut()
                .is_some_and(|game| !std::mem::replace(&mut game.seen_forest, true));
        if first_visit {
            self.push_log(data::MSG_SEEN_FOREST.to_string());
            self.save();
        }
        // The ship says its one line the first time it is ever looked at
        // (upstream latches this on the save, not on the log).
        let first_look = self.view == View::Ship
            && self
                .game
                .as_mut()
                .and_then(|game| game.ship.as_mut())
                .is_some_and(|ship| !std::mem::replace(&mut ship.seen_ship, true));
        if first_look {
            self.push_log(space::MSG_SEEN_SHIP.to_string());
            self.save();
        }
    }

    // ---- actions ----

    /// Act on the selected row.
    pub fn select(&mut self) -> Acted {
        self.settle();
        // The wasteland has no rows: Enter out there must never fall through
        // to the Leave default and dump the player back in the hub.
        if self.rows().is_empty() {
            return Acted::Stay;
        }
        let row = self.selected();
        match row {
            Row::LightFire => self.light_fire(),
            Row::StokeFire => self.stoke_fire(),
            Row::Build(building) => self.build(building),
            Row::Craft(craftable) => self.craft(craftable),
            Row::Buy(good) => self.buy(good),
            Row::GatherWood => self.gather_wood(),
            Row::CheckTraps => self.check_traps(),
            Row::Worker(job) => self.assign(job, 1),
            Row::Outfit(item) => self.pack(item, 1),
            Row::Embark => self.embark(),
            Row::ReinforceHull => self.ship_spend(true),
            Row::UpgradeEngine => self.ship_spend(false),
            Row::LiftOff => self.lift_off(),
            Row::Event(row) => self.event_press(row),
            Row::Leave => return Acted::Leave,
        }
        Acted::Stay
    }

    /// Move `count` villagers onto the selected trade, or pack `count` more of
    /// the selected supply. Safe to bind to an arrow key: it does nothing
    /// anywhere else. Upstream's up/upMany buttons move 1 and 10.
    pub fn assign_selected(&mut self, count: u32) {
        self.settle();
        match self.selected() {
            Row::Worker(job) => self.assign(job, count as i32),
            Row::Outfit(item) => self.pack(item, count as i64),
            _ => {}
        }
    }

    /// The inverse: off the trade, or out of the pack.
    pub fn unassign_selected(&mut self, count: u32) {
        self.settle();
        match self.selected() {
            Row::Worker(job) => self.assign(job, -(count as i32)),
            Row::Outfit(item) => self.pack(item, -(count as i64)),
            _ => {}
        }
    }

    fn event_press(&mut self, row: event::Row) {
        let mut out = Vec::new();
        let outcome = {
            let (Some(game), Some(active)) = (self.game.as_mut(), self.event.as_mut()) else {
                return;
            };
            let mut trip = game.expedition.take();
            let mut rng = rand::thread_rng();
            let outcome = {
                let mut ctx = Ctx {
                    game: &mut *game,
                    trip: trip.as_mut(),
                    view: self.view,
                    now: Utc::now(),
                };
                active.press(row, &mut ctx, &mut rng, &mut out)
            };
            game.expedition = trip;
            outcome
        };
        for message in out {
            self.push_log(message);
        }
        self.cursor = 0;
        self.finish_event(outcome);
        self.save();
    }

    fn light_fire(&mut self) {
        let Some(game) = self.game.as_mut() else {
            return;
        };
        match game.light_fire() {
            LightFire::Lit { drew_builder } => {
                let fire = game.fire.text();
                self.push_log(format!("the fire is {fire}"));
                self.announce_builder_seen(drew_builder);
                self.save();
            }
            LightFire::NotEnoughWood => self.push_log(data::MSG_NOT_ENOUGH_WOOD.to_string()),
            LightFire::OnCooldown => {}
        }
    }

    fn stoke_fire(&mut self) {
        let Some(game) = self.game.as_mut() else {
            return;
        };
        match game.stoke_fire() {
            StokeFire::Stoked { drew_builder } => {
                let fire = game.fire.text();
                self.push_log(format!("the fire is {fire}"));
                self.announce_builder_seen(drew_builder);
                self.save();
            }
            StokeFire::OutOfWood => self.push_log(data::MSG_WOOD_RUN_OUT.to_string()),
            StokeFire::OnCooldown => {}
        }
    }

    /// The one line that fires the moment the room is bright enough to be seen
    /// from outside. Upstream prints it inside `onFireChange`.
    fn announce_builder_seen(&mut self, drew_builder: bool) {
        if drew_builder {
            self.push_log(data::MSG_FIRE_SPILLS.to_string());
        }
    }

    fn build(&mut self, building: Building) {
        let Some(game) = self.game.as_mut() else {
            return;
        };
        let message = match game.build(building) {
            // Only a mine has no line, and a mine is never a build row.
            BuildOutcome::Built(building) => building
                .build_msg()
                .unwrap_or("the builder puts it up")
                .to_string(),
            BuildOutcome::AtMaximum(building) => building
                .max_msg()
                .unwrap_or("there's no room for another")
                .to_string(),
            BuildOutcome::Missing(resource) => format!("not enough {}", resource.label()),
            BuildOutcome::NoBuilder => "there's no one to build it".to_string(),
            BuildOutcome::TooCold => data::MSG_BUILDER_SHIVERS.to_string(),
            BuildOutcome::NotOffered(_) => {
                "that one comes from out there, not from here".to_string()
            }
        };
        self.push_log(message);
        self.save();
    }

    fn craft(&mut self, craftable: &'static Craftable) {
        let Some(game) = self.game.as_mut() else {
            return;
        };
        let message = match game.craft(craftable) {
            CraftOutcome::Crafted(_) => craftable.build_msg.to_string(),
            CraftOutcome::AtMaximum(item) => {
                format!("there's no need for another {}", item.label())
            }
            CraftOutcome::Missing(resource) => format!("not enough {}", resource.label()),
            CraftOutcome::TooCold => data::MSG_BUILDER_SHIVERS.to_string(),
        };
        self.push_log(message);
        self.save();
    }

    fn buy(&mut self, good: &'static TradeGood) {
        let Some(game) = self.game.as_mut() else {
            return;
        };
        // Upstream prints its (undefined) `buildMsg` here and so says nothing
        // at all; the terminal has no button flash to fall back on, so the
        // trade gets a line of our own.
        let message = match game.buy(good) {
            BuyOutcome::Bought(bought) => format!("bought {}", bought.label()),
            BuyOutcome::AtMaximum(bought) => {
                format!("there's no need for another {}", bought.label())
            }
            BuyOutcome::Missing(resource) => format!("not enough {}", resource.label()),
        };
        self.push_log(message);
        self.open_path_if_bought();
        self.save();
    }

    fn gather_wood(&mut self) {
        let Some(game) = self.game.as_mut() else {
            return;
        };
        match game.gather_wood() {
            GatherOutcome::Gathered(_) => {
                self.push_log(data::MSG_GATHER_WOOD.to_string());
                // Persist: a dropped connection must not lose gathered wood.
                self.save();
            }
            GatherOutcome::OnCooldown => {}
        }
    }

    fn check_traps(&mut self) {
        let Some(game) = self.game.as_mut() else {
            return;
        };
        if game.traps_cooldown > 0 {
            return;
        }
        let mut rng = rand::thread_rng();
        let drops = sim::roll_traps(game, &mut rng);
        game.collect_traps(&drops);
        if drops.is_empty() {
            self.push_log("the traps are empty".to_string());
        } else {
            self.push_log(sim::haul_message(&drops));
        }
        // Persist either way: bait was spent and the cooldown restarted.
        self.save();
    }

    fn assign(&mut self, job: Job, delta: i32) {
        let Some(game) = self.game.as_mut() else {
            return;
        };
        if delta > 0 {
            game.assign_worker(job, delta as u32);
        } else {
            game.unassign_worker(job, delta.unsigned_abs());
        }
        self.save();
    }

    // ---- the path ----

    /// Pack or unpack supplies, bounded by the store room and by weight.
    fn pack(&mut self, item: Resource, delta: i64) {
        let Some(game) = self.game.as_mut() else {
            return;
        };
        let packed = game.outfit.get(&item).copied().unwrap_or(0);
        let next = if delta > 0 {
            let free = game.capacity() - outfit_load(game);
            let by_weight = (free / world_data::weight(item)).floor() as i64;
            let by_store = game.store(item) - packed;
            packed + delta.min(by_weight).min(by_store).max(0)
        } else {
            (packed + delta).max(0)
        };
        game.outfit.insert(item, next);
        self.save();
    }

    fn embark(&mut self) {
        let Some(game) = self.game.as_mut() else {
            return;
        };
        if !game.can_embark() || game.world.is_none() {
            return;
        }
        let trip = world::embark(game);
        game.expedition = Some(trip);
        self.view = View::World;
        self.cursor = 0;
        self.save();
    }

    /// One step out in the wasteland. Every consequence of a move happens
    /// here, on the keypress, not on a clock.
    pub fn walk(&mut self, direction: Direction) {
        if self.event.is_some() || self.view != View::World {
            return;
        }
        self.settle();
        let mut out = Vec::new();
        let step = {
            let Some(game) = self.game.as_mut() else {
                return;
            };
            let Some(mut trip) = game.expedition.take() else {
                return;
            };
            let mut rng = rand::thread_rng();
            let step = world::step(game, &mut trip, direction, &mut rng, &mut out);
            game.expedition = Some(trip);
            step
        };
        for message in out {
            self.push_log(message);
        }
        match step {
            Step::Walked | Step::Blocked => {}
            Step::Home => self.go_home(),
            Step::Setpiece(scene) => {
                if let Some(chosen) = super::scenes_setpieces::by_key(scene) {
                    self.start_event(chosen);
                }
            }
            Step::Fight => self.start_encounter(),
            Step::Died => {
                let mut out = Vec::new();
                if let Some(game) = self.game.as_mut() {
                    world::die(game, &mut out);
                }
                for message in out {
                    self.push_log(message);
                }
                self.view = View::Path;
            }
        }
        // Fire and forget on every move: a dropped connection parks the trip
        // rather than losing it.
        self.save();
    }

    fn start_encounter(&mut self) {
        let Some(game) = self.game.as_mut() else {
            return;
        };
        let mut rng = rand::thread_rng();
        let chosen = event::pick(
            &super::scenes_encounters::ENCOUNTERS,
            &look_at(game, self.view),
            &mut rng,
        );
        if let Some(chosen) = chosen {
            self.start_event(chosen);
        }
    }

    fn go_home(&mut self) {
        let mut out = Vec::new();
        if let Some(game) = self.game.as_mut()
            && let Some(trip) = game.expedition.take()
        {
            world::go_home(game, trip, &mut out);
        }
        for message in out {
            self.push_log(message);
        }
        self.view = View::Path;
        self.cursor = 0;
        self.save();
    }

    /// Park the trip and step out of the door. Nothing is spent by walking
    /// away: supplies burn per move, so a parked trip costs nothing. A fight
    /// in progress is parked with it, so leaving the door is never a way to
    /// flee one: coming back resumes it, the same as a dropped connection.
    pub fn park(&mut self) {
        self.park_combat();
        self.event = None;
        self.save();
    }

    // ---- the ship ----

    fn ship_spend(&mut self, hull: bool) {
        let Some(game) = self.game.as_mut() else {
            return;
        };
        let cost = match hull {
            true => space::ALLOY_PER_HULL,
            false => space::ALLOY_PER_THRUSTER,
        };
        if game.store(Resource::AlienAlloy) < cost {
            self.push_log("not enough alien alloy".to_string());
            return;
        }
        game.add_store(Resource::AlienAlloy, -cost);
        let ship = game.ship.get_or_insert_with(ShipState::default);
        if hull {
            ship.hull += 1;
        } else {
            ship.thrusters += 1;
        }
        self.save();
    }

    fn lift_off(&mut self) {
        let ready = {
            let Some(ship) = self.game.as_mut().and_then(|game| game.ship.as_mut()) else {
                return;
            };
            if ship.hull <= 0 || ship.liftoff_cooldown > 0 {
                return;
            }
            // The first press is the one-way warning; the second one flies.
            if !ship.seen_warning {
                ship.seen_warning = true;
                None
            } else {
                Some((ship.hull, ship.thrusters))
            }
        };
        match ready {
            None => self.push_log(space::MSG_READY_TO_LEAVE.to_string()),
            Some((hull, thrusters)) => self.flight = Some(Space::new(hull, thrusters)),
        }
        self.save();
    }

    /// Steer the ship mid-ascent.
    pub fn steer(&mut self, dx: f64, dy: f64) {
        if let Some(flight) = self.flight.as_mut() {
            flight.nudge(dx, dy);
        }
    }

    /// Give up on the flight.
    pub fn abort_flight(&mut self) {
        if let Some(flight) = self.flight.as_mut() {
            flight.abort();
        }
    }

    // ---- persistence ----

    /// Persist the current game. Called after anything worth not losing.
    pub fn save(&self) {
        if let Some(game) = self.game.as_ref() {
            self.svc.save_game(self.user_id, game);
        }
    }

    /// Settle one last time and persist, so the credited clock stops here
    /// rather than at whenever the last action happened.
    pub fn save_on_leave(&mut self) {
        self.park();
        self.settle();
        self.save();
    }

    /// A label for the row, for the renderer.
    pub fn row_label(&self, row: Row) -> String {
        let game = self.game.as_ref();
        match row {
            Row::LightFire => "light fire".to_string(),
            Row::StokeFire => "stoke fire".to_string(),
            Row::Build(building) => building.label().to_string(),
            Row::Craft(craftable) => craftable.item.label().to_string(),
            Row::Buy(good) => good.good.label().to_string(),
            Row::GatherWood => "gather wood".to_string(),
            Row::CheckTraps => "check traps".to_string(),
            Row::Worker(job) => {
                let count = game.map(|g| g.worker_count(job)).unwrap_or(0);
                format!("{} {count}", job.label())
            }
            Row::Outfit(item) => {
                let packed = game.and_then(|g| g.outfit.get(&item).copied()).unwrap_or(0);
                let have = game.map(|g| g.store(item)).unwrap_or(0);
                format!("{} {packed}/{have}", item.label())
            }
            Row::Embark => "embark".to_string(),
            Row::ReinforceHull => "reinforce hull".to_string(),
            Row::UpgradeEngine => "upgrade engine".to_string(),
            Row::LiftOff => "lift off".to_string(),
            Row::Event(row) => self.event_row_label(row),
            Row::Leave => "leave".to_string(),
        }
    }

    fn event_row_label(&self, row: event::Row) -> String {
        let Some(active) = self.event.as_ref() else {
            return String::new();
        };
        match row {
            event::Row::Button(index) => active
                .scene
                .buttons
                .get(index)
                .map(|button| button.text.to_string())
                .unwrap_or_default(),
            event::Row::Attack(weapon) => match weapon.cost() {
                Some((ammo, _)) => format!("{} ({})", weapon.verb(), ammo.label()),
                None => weapon.verb().to_string(),
            },
            event::Row::Eat => "eat meat".to_string(),
            event::Row::Meds => "use meds".to_string(),
            event::Row::Take(index) => active
                .loot
                .get(index)
                .map(|loot| format!("{} [{}]", loot.item.label(), loot.left))
                .unwrap_or_default(),
            event::Row::TakeAll => "take everything".to_string(),
            event::Row::Leave => "leave".to_string(),
        }
    }

    /// Seconds until a row's cooldown expires, if it is on one.
    pub fn row_cooldown(&self, row: Row) -> u32 {
        let Some(game) = self.game.as_ref() else {
            return 0;
        };
        match row {
            Row::LightFire | Row::StokeFire => game.stoke_cooldown,
            Row::GatherWood => game.gather_cooldown,
            Row::CheckTraps => game.traps_cooldown,
            Row::Embark => game.embark_cooldown,
            Row::LiftOff => game.ship.as_ref().map(|s| s.liftoff_cooldown).unwrap_or(0),
            Row::Event(event::Row::Attack(weapon)) => self
                .event
                .as_ref()
                .and_then(|active| active.fight())
                .and_then(|fight| fight.weapon_cooldown.get(&weapon).copied())
                .unwrap_or(0.0)
                .ceil() as u32,
            Row::Event(event::Row::Eat) => self
                .event
                .as_ref()
                .and_then(|active| active.fight())
                .map(|fight| fight.eat_cooldown.ceil() as u32)
                .unwrap_or(0),
            Row::Event(event::Row::Meds) => self
                .event
                .as_ref()
                .and_then(|active| active.fight())
                .map(|fight| fight.meds_cooldown.ceil() as u32)
                .unwrap_or(0),
            Row::Build(_)
            | Row::Craft(_)
            | Row::Buy(_)
            | Row::Worker(_)
            | Row::Outfit(_)
            | Row::ReinforceHull
            | Row::UpgradeEngine
            | Row::Event(_)
            | Row::Leave => 0,
        }
    }

    /// The cost of a build, craft, buy or ship row, for the hint line.
    pub fn row_cost(&self, row: Row) -> Vec<(Resource, i64)> {
        let Some(game) = self.game.as_ref() else {
            return Vec::new();
        };
        match row {
            Row::Build(building) => building.cost(game.building_count(building)),
            Row::Craft(craftable) => craftable.cost.to_vec(),
            Row::Buy(good) => good.cost.to_vec(),
            Row::ReinforceHull => vec![(Resource::AlienAlloy, space::ALLOY_PER_HULL)],
            Row::UpgradeEngine => vec![(Resource::AlienAlloy, space::ALLOY_PER_THRUSTER)],
            Row::Event(event::Row::Button(index)) => self
                .event
                .as_ref()
                .and_then(|active| active.scene.buttons.get(index))
                .map(|button| {
                    button
                        .cost
                        .iter()
                        .filter_map(|cost| match cost {
                            event::Cost::Store(item, amount) => Some((*item, *amount)),
                            event::Cost::Water(_) | event::Cost::Hp(_) => None,
                        })
                        .collect()
                })
                .unwrap_or_default(),
            Row::LightFire
            | Row::StokeFire
            | Row::GatherWood
            | Row::CheckTraps
            | Row::Worker(_)
            | Row::Outfit(_)
            | Row::Embark
            | Row::LiftOff
            | Row::Event(_)
            | Row::Leave => Vec::new(),
        }
    }

    /// Whether the row has hit its ceiling. Upstream greys the button out and
    /// stops it responding; here it stays selectable and says so when pressed,
    /// because a terminal row that silently does nothing reads as broken.
    pub fn row_at_maximum(&self, row: Row) -> bool {
        let Some(game) = self.game.as_ref() else {
            return false;
        };
        match row {
            Row::Build(building) => building
                .maximum()
                .is_some_and(|max| game.building_count(building) >= max),
            Row::Craft(craftable) => craftable
                .maximum
                .is_some_and(|max| game.store(craftable.item) >= i64::from(max)),
            Row::Buy(good) => good
                .maximum
                .is_some_and(|max| game.store(good.good) >= i64::from(max)),
            Row::Embark => !game.can_embark(),
            Row::LiftOff => game.ship.as_ref().map(|s| s.hull <= 0).unwrap_or(true),
            Row::Event(row) => !self.event_row_ready(row),
            Row::LightFire
            | Row::StokeFire
            | Row::GatherWood
            | Row::CheckTraps
            | Row::Worker(_)
            | Row::Outfit(_)
            | Row::ReinforceHull
            | Row::UpgradeEngine
            | Row::Leave => false,
        }
    }

    fn event_row_ready(&self, row: event::Row) -> bool {
        let (Some(game), Some(active)) = (self.game.as_ref(), self.event.as_ref()) else {
            return false;
        };
        active.row_ready(row, &look_at(game, self.view))
    }

    /// The legend a row sits under, if it opens one of upstream's groups.
    pub fn row_section(&self, row: Row) -> Option<Section> {
        match row {
            Row::Build(_) => Some(Section::Build),
            Row::Craft(_) => Some(Section::Craft),
            Row::Buy(_) => Some(Section::Buy),
            Row::LightFire
            | Row::StokeFire
            | Row::GatherWood
            | Row::CheckTraps
            | Row::Worker(_)
            | Row::Outfit(_)
            | Row::Embark
            | Row::ReinforceHull
            | Row::UpgradeEngine
            | Row::LiftOff
            | Row::Event(_)
            | Row::Leave => None,
        }
    }

    /// The weapons the pack can swing, for the fight panel.
    pub fn fight_weapons(&self) -> Vec<Weapon> {
        match self.game.as_ref() {
            Some(game) => event::available_weapons(&look_at(game, self.view)),
            None => Vec::new(),
        }
    }
}

/// The read-only view of the save that the modal and the renderer share.
fn look_at(game: &Game, view: View) -> event::Look<'_> {
    event::Look {
        game,
        trip: game.expedition.as_ref(),
        view,
    }
}

/// What a packed outfit weighs.
fn outfit_load(game: &Game) -> f64 {
    game.outfit
        .iter()
        .map(|(item, count)| *count as f64 * world_data::weight(*item))
        .sum()
}
