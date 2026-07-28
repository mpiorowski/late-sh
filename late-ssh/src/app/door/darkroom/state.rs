//! Per-session Dark Room state: the loaded game, which panel is open, the
//! cursor, and the notification log. Single-player, so this session owns the
//! authoritative game outright; nothing is shared and nothing is published.
//!
//! Time advances in [`State::tick`] and before every action, by settling the
//! game forward against the wall clock (see `sim`). There is no game loop.

use std::collections::VecDeque;

use chrono::{DateTime, Utc};
use tokio::sync::watch;
use uuid::Uuid;

use super::data::{self, Building, Fire, Job, Resource};
use super::model::{BuildOutcome, Game, GatherOutcome, LightFire, StokeFire, View};
use super::pace;
use super::sim;
use super::svc::{DarkroomService, GameLoad};

/// How many notification lines the log keeps. Upstream fades them out of a
/// scrolling column; a fixed window is the terminal equivalent.
const LOG_CAP: usize = 200;

/// One actionable row in the current panel.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Row {
    LightFire,
    StokeFire,
    Build(Building),
    GatherWood,
    CheckTraps,
    Worker(Job),
    Leave,
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
}

impl State {
    pub fn new(svc: DarkroomService, user_id: Uuid, session_start: DateTime<Utc>) -> Self {
        let load = svc.load_game(user_id);
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
            }
        }
        // Not `||`: the settle must run whether or not the load just landed.
        let advanced = self.settle();
        changed || advanced
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
        // A credited second may have moved a countdown the player is watching
        // even when nothing was worth announcing, so it counts as a change.
        had_messages || settled.credited > 0
    }

    fn push_log(&mut self, message: String) {
        if self.log.len() == LOG_CAP {
            self.log.pop_front();
        }
        self.log.push_back(message);
    }

    // ---- navigation ----

    /// The rows the current panel offers, in display order.
    pub fn rows(&self) -> Vec<Row> {
        let Some(game) = self.game.as_ref() else {
            return vec![Row::Leave];
        };
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
        }
        rows.push(Row::Leave);
        rows
    }

    /// The row under the cursor, clamped to the current list.
    pub fn selected(&self) -> Row {
        let rows = self.rows();
        rows.get(self.cursor.min(rows.len() - 1))
            .copied()
            .unwrap_or(Row::Leave)
    }

    pub fn move_cursor(&mut self, delta: i32) {
        let len = self.rows().len() as i32;
        let next = (self.cursor as i32 + delta).rem_euclid(len);
        self.cursor = next as usize;
    }

    /// Switch panels. The forest is only reachable once the wood runs out.
    pub fn toggle_view(&mut self) {
        let outside_open = self.game.as_ref().is_some_and(|game| game.forest_unlocked);
        self.view = match (self.view, outside_open) {
            (View::Room, true) => View::Outside,
            (View::Room, false) => View::Room,
            (View::Outside, _) => View::Room,
        };
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
    }

    // ---- actions ----

    /// Act on the selected row.
    pub fn select(&mut self) -> Acted {
        self.settle();
        let row = self.selected();
        match row {
            Row::LightFire => self.light_fire(),
            Row::StokeFire => self.stoke_fire(),
            Row::Build(building) => self.build(building),
            Row::GatherWood => self.gather_wood(),
            Row::CheckTraps => self.check_traps(),
            Row::Worker(job) => self.assign(job, 1),
            Row::Leave => return Acted::Leave,
        }
        Acted::Stay
    }

    /// Move `count` villagers onto the selected trade. Same as selecting a
    /// worker row, but safe to bind to an arrow key: it does nothing anywhere
    /// else, so right-arrow can never trip an action the player did not aim
    /// at. Upstream's up/upMany buttons move 1 and 10.
    pub fn assign_selected(&mut self, count: u32) {
        self.settle();
        if let Row::Worker(job) = self.selected() {
            self.assign(job, count as i32);
        }
    }

    /// Move `count` villagers off the selected trade (the inverse).
    pub fn unassign_selected(&mut self, count: u32) {
        self.settle();
        if let Row::Worker(job) = self.selected() {
            self.assign(job, -(count as i32));
        }
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
            BuildOutcome::Built(building) => building.build_msg().to_string(),
            BuildOutcome::AtMaximum(building) => building
                .max_msg()
                .unwrap_or("there's no room for another")
                .to_string(),
            BuildOutcome::Missing(resource) => format!("not enough {}", resource.label()),
            BuildOutcome::NoBuilder => "there's no one to build it".to_string(),
            BuildOutcome::TooCold => data::MSG_BUILDER_SHIVERS.to_string(),
        };
        self.push_log(message);
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
            Row::GatherWood => "gather wood".to_string(),
            Row::CheckTraps => "check traps".to_string(),
            Row::Worker(job) => {
                let count = game.map(|g| g.worker_count(job)).unwrap_or(0);
                format!("{} {count}", job.label())
            }
            Row::Leave => "leave".to_string(),
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
            Row::Build(_) | Row::Worker(_) | Row::Leave => 0,
        }
    }

    /// The cost of a build row, for the renderer's hint line.
    pub fn row_cost(&self, row: Row) -> Vec<(Resource, i64)> {
        let (Some(game), Row::Build(building)) = (self.game.as_ref(), row) else {
            return Vec::new();
        };
        building.cost(game.building_count(building))
    }
}
