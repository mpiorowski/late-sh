//! Traffic game state and physics.
//!
//! Geometry, lane configuration, and speed ranges are no longer hard-coded —
//! they're read live from the active [`Track`] and current [`Stage`] (see
//! `track.rs` and `tracks/`).  What remains in [`Config`] is only the visual
//! frame (terminal rows, lane width in chars, FPS) and the global player
//! input feel (accel/decel/lane-transition speed).

use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

use rand::{Rng, random};

use super::track::{ObstacleEffect, ObstacleStyle, Stage, Track};
use super::tracks::{ALL_TRACKS, DEFAULT_TRACK};

// ─── Static visual-frame configuration ──────────────────────────────────────

pub struct Config;

impl Config {
    /// Reference viewport height in rows. The road is drawn into at most this
    /// many rows (centred when the terminal is taller) and scales down to the
    /// terminal on shorter screens. The world simulation — spawn horizon, AI,
    /// minimap range — is defined against this reference so difficulty is
    /// identical regardless of the player's terminal size; only how much road is
    /// on screen changes. See `ui::road_anchor_row` for the runtime anchor.
    pub const VISIBLE_ROWS: u16 = 50;
    /// Fewest road rows the game will render into. Below this the picker/HUD
    /// shows a "terminal too small" notice instead of the road.
    pub const MIN_ROAD_ROWS: u16 = 20;
    /// Width of a single lane in character columns.
    pub const LANE_WIDTH: u16 = 5;
    /// Height of any car (player and AI) in terminal rows. Drives collision
    /// boxes, minimap scale, and player rear-position calculations.
    pub const CAR_HEIGHT_ROWS: u16 = 3;
    /// Rows between the bottom of the player car and the bottom of the viewport.
    pub const PLAYER_BOTTOM_MARGIN: u16 = 4;
    /// Reference screen row of the top edge of the player car at the full
    /// [`Self::VISIBLE_ROWS`] height (0 = top of viewport). Physics horizons
    /// (`VISIBLE_AHEAD_M`, `PRE_STAGE_M`) are anchored here; rendering uses a
    /// runtime anchor derived from the actual road height so the car keeps the
    /// same bottom margin on any terminal.
    pub const PLAYER_TOP_ROW: u16 =
        Self::VISIBLE_ROWS - Self::CAR_HEIGHT_ROWS - Self::PLAYER_BOTTOM_MARGIN;
    /// World-space scale: metres represented by one terminal row.
    /// Every metre ↔ row conversion multiplies or divides by this.
    pub const METERS_PER_ROW: f32 = 3.0;
    /// Metres of road visible ahead of the player (top of car to top of screen).
    pub const VISIBLE_AHEAD_M: f32 = Self::PLAYER_TOP_ROW as f32 * Self::METERS_PER_ROW;
    /// Metres covered by the minimap. Also used as the obstacle seeding horizon
    /// so the minimap always shows what has already been populated.
    pub const MINIMAP_RANGE_M: f32 = 2.0 * Self::VISIBLE_ROWS as f32 * Self::METERS_PER_ROW;
    /// Minimum front-to-back gap enforced between any two AI cars in the same
    /// lane during placement.
    pub const AI_MIN_SEPARATION_M: f32 = 32.0;
    /// Distance behind the player after which AI cars and obstacles are removed.
    pub const AI_DESPAWN_BEHIND_M: f32 = 200.0;
    /// Follow distance in rows: converted to metres each tick to determine when
    /// an AI car slows to match the speed of the one directly ahead.
    pub const AI_FOLLOW_GAP_ROWS: u16 = 5;
    /// Outer edge of the managed traffic zone. New cars are seeded anywhere
    /// between `MINIMAP_RANGE_M` and here so they scroll naturally into view.
    pub const SPAWN_AHEAD_M: f32 = Self::MINIMAP_RANGE_M * 1.5;
    /// Safe gap left clear in front of the player during initial fill so the
    /// road immediately ahead is obstacle-free on race start.
    pub const INITIAL_SKIP_M: f32 = Self::AI_MIN_SEPARATION_M * 2.5;
    /// Negative offset applied to `player_pos_m` at race start so the player
    /// spawns just before the first stage separator and it scrolls in naturally.
    pub const PRE_STAGE_M: f32 = Self::VISIBLE_AHEAD_M + Self::METERS_PER_ROW * 2.0;
    /// Half-width of the obstacle exclusion zone around every stage boundary.
    /// No obstacles are placed within this distance of a separator in either direction.
    pub const STAGE_CLEAR_M: f32 = 12.0;

    /// Player speed at race start and after every restart.
    pub const PLAYER_START_SPEED_KMH: f32 = 50.0;
    /// Speed gained per second while holding accelerate.
    pub const ACCEL_KMH_PER_S: f32 = 88.0;
    /// Speed lost per second while braking; doubled when using the handbrake.
    pub const DECEL_KMH_PER_S: f32 = 128.0;
    /// Rate at which speed eases back into the new lane's bounds after a lane
    /// change, preventing an instant hard clamp.
    pub const SPEED_CLAMP_PER_S: f32 = 80.0;
    /// Fixed physics timestep (15 Hz). All per-frame increments multiply by this.
    pub const TICK_DT: f32 = 1.0 / 15.0;
    /// Milliseconds a held-key input stays active before auto-releasing.
    pub const INPUT_HOLD_MS: u64 = 150;
    /// Duration of the on/off recovery flash after dismissing a crash popup. Also provides temporary immunity.
    pub const CRASH_FLASH_MS: u64 = 1500;
    /// Visual lane-change speed in display-lane-units per second. Controls
    /// how fast the player car glides to the target lane on screen.
    pub const LANE_TRANSITION_PER_S: f32 = 7.0;

    /// Maximum entries kept in the right-panel obstacle-effect log.
    pub const RECENT_EFFECTS_CAPACITY: usize = 5;

    /// Minimum terminal width (columns) required to render the game.
    pub const MIN_TERMINAL_WIDTH_FLOOR: u16 = 70;
    /// Minimum terminal height (rows) required to render the game.
    pub const MIN_TERMINAL_HEIGHT: u16 = Self::MIN_ROAD_ROWS;
}

// ─── Top-level state machine ─────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TrafficScreen {
    Picker,
    Racing,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TrafficDir {
    Same,
    Oncoming,
}

#[derive(Clone, Copy, Debug)]
pub enum PlayerInput {
    Accelerate,
    Brake,
    Handbrake,
    None,
}

#[derive(Clone, Copy, Debug)]
pub enum Phase {
    Playing,
    /// Survived a crash with lives remaining: frozen behind a popup until the
    /// player dismisses it (any key), then resumes into a brief recovery flash.
    Crashed,
    Finished {
        elapsed_s: f32,
        score: i64,
    },
    Dead,
}

#[derive(Clone, Debug)]
pub struct AiCar {
    pub pos_m: f32,
    pub speed_kmh: f32,
    pub cruise_kmh: f32,
    pub lane_idx: usize,
    pub direction: TrafficDir,
    pub height_rows: u8,
}

/// Obstacle deterministically placed along a lane; cleared on crossing.
#[derive(Clone, Debug)]
pub struct SpawnedObstacle {
    pub style: ObstacleStyle,
    pub effects: &'static [ObstacleEffect],
    pub crash: bool,
    pub pos_m: f32,
    pub lane_idx: usize,
    pub triggered: bool,
}

#[derive(Clone, Debug)]
pub struct RecentEffect {
    pub label: &'static str,
    pub at: Instant,
}

/// All runtime traffic state — drives both the picker and the race.
pub struct State {
    pub screen: TrafficScreen,
    pub picker_selected_idx: usize,
    pub best_scores: HashMap<&'static str, i64>,
    pub best_score: i64,

    /// Persistence handle + owner; `None` in unit tests.
    svc: Option<super::svc::TrafficService>,
    user_id: uuid::Uuid,

    pub active_track: Option<&'static Track>,
    pub current_stage_idx: usize,
    pub stage_traveled_m: f32,
    pub player_pos_m: f32,
    pub player_speed_kmh: f32,
    pub player_lane_idx: usize,
    pub player_lane_display: f32,
    pub input: PlayerInput,
    pub input_last_set: Option<Instant>,
    pub ai_cars: Vec<AiCar>,
    pub obstacles: Vec<SpawnedObstacle>,
    pub obstacle_seed_m: f32,
    pub scenery_seed: u64,
    pub elapsed_s: f32,
    pub score: i64,
    pub phase: Phase,
    pub lives: u8,
    pub is_paused: bool,

    pub recent_effects: VecDeque<RecentEffect>,

    pub gas_blocked_until: Option<Instant>,
    pub brake_blocked_until: Option<Instant>,
    pub wheel_blocked_until: Option<Instant>,

    /// While set and in the future, the player car blinks (post-crash recovery).
    pub flash_until: Option<Instant>,
}

impl State {
    pub fn new() -> Self {
        Self {
            screen: TrafficScreen::Picker,
            picker_selected_idx: 0,
            best_scores: HashMap::new(),
            best_score: 0,
            svc: None,
            user_id: uuid::Uuid::nil(),
            active_track: None,
            current_stage_idx: 0,
            stage_traveled_m: 0.0,
            player_pos_m: 0.0,
            player_speed_kmh: Config::PLAYER_START_SPEED_KMH,
            player_lane_idx: 0,
            player_lane_display: 0.0,
            input: PlayerInput::None,
            input_last_set: None,
            ai_cars: Vec::new(),
            obstacles: Vec::new(),
            obstacle_seed_m: 0.0,
            scenery_seed: 0,
            elapsed_s: 0.0,
            score: 0,
            phase: Phase::Playing,
            lives: 0,
            is_paused: false,
            recent_effects: VecDeque::with_capacity(Config::RECENT_EFFECTS_CAPACITY),
            gas_blocked_until: None,
            brake_blocked_until: None,
            wheel_blocked_until: None,
            flash_until: None,
        }
    }

    /// Attach persistence and seed per-track bests loaded from the DB. Called
    /// once at session start. `track_scores` is keyed by `Track::name`.
    pub fn hydrate(
        &mut self,
        user_id: uuid::Uuid,
        svc: super::svc::TrafficService,
        track_scores: Vec<late_core::models::traffic::TrackScore>,
        high_score: Option<late_core::models::traffic::HighScore>,
    ) {
        self.user_id = user_id;
        self.svc = Some(svc);
        for ts in track_scores {
            if let Some(track) = ALL_TRACKS.iter().find(|t| t.name == ts.track_key) {
                self.best_scores.insert(track.name, ts.score as i64);
            }
        }
        self.best_score = match high_score {
            Some(hs) => hs.score as i64,
            None => self.best_scores.values().copied().sum(),
        };
    }

    // ─── Picker ──────────────────────────────────────────────────────────────

    pub fn picker_move(&mut self, delta: i32) {
        let len = ALL_TRACKS.len() as i32;
        if len == 0 {
            return;
        }
        let cur = self.picker_selected_idx as i32;
        self.picker_selected_idx = ((cur + delta).rem_euclid(len)) as usize;
    }

    pub fn start_selected_track(&mut self) {
        let track = ALL_TRACKS
            .get(self.picker_selected_idx)
            .copied()
            .unwrap_or(DEFAULT_TRACK);
        self.start_track(track);
    }

    pub fn start_track(&mut self, track: &'static Track) {
        self.active_track = Some(track);
        self.current_stage_idx = 0;
        self.stage_traveled_m = -Config::PRE_STAGE_M;
        self.player_pos_m = -Config::PRE_STAGE_M;
        self.player_speed_kmh = Config::PLAYER_START_SPEED_KMH;
        self.player_lane_idx = track.stages[0].road.lanes.player_start_idx();
        self.player_lane_display = self.player_lane_idx as f32;
        self.input = PlayerInput::None;
        self.input_last_set = None;
        self.ai_cars.clear();
        self.obstacles.clear();
        self.obstacle_seed_m = -Config::PRE_STAGE_M;
        self.scenery_seed = random();
        self.elapsed_s = 0.0;
        self.score = Track::SCORE_MAX;
        self.phase = Phase::Playing;
        self.lives = track.lives;
        self.is_paused = false;
        self.recent_effects.clear();
        self.gas_blocked_until = None;
        self.brake_blocked_until = None;
        self.wheel_blocked_until = None;
        self.flash_until = None;
        self.screen = TrafficScreen::Racing;
    }

    pub fn restart_current(&mut self) {
        if let Some(track) = self.active_track {
            self.start_track(track);
        }
    }

    pub fn return_to_picker(&mut self) {
        self.screen = TrafficScreen::Picker;
    }

    /// Live 0..=SCORE_MAX grade: extrapolate the current average pace to the
    /// full track and grade that projected completion time. Converges to the
    /// real finish score as the run completes.
    fn projected_grade(&self) -> i64 {
        let Some(track) = self.active_track else {
            return 0;
        };
        let covered = self.player_pos_m;
        if covered <= 0.0 || self.elapsed_s <= 0.0 {
            return Track::SCORE_MAX;
        }
        let total_m = track.total_distance_km() * 1000.0 * self.distance_scale();
        if total_m <= 0.0 {
            return 0;
        }
        let projected_time = self.elapsed_s * (total_m / covered);
        track.grade_time(projected_time)
    }

    /// Score banked when the player dies mid-run: the current pace grade scaled
    /// by the fraction of the track distance actually covered. Finishing implies
    /// `fraction == 1.0`, so this agrees with `grade_time` at the finish line;
    /// dying halfway at a perfect pace banks half. Rewards distance, not just
    /// pace, so a fast-but-brief run before a crash can't bank a full score.
    fn death_score(&self) -> i64 {
        let Some(track) = self.active_track else {
            return 0;
        };
        let total_m = track.total_distance_km() * 1000.0 * self.distance_scale();
        if total_m <= 0.0 {
            return 0;
        }
        let frac = (self.player_pos_m.max(0.0) / total_m).clamp(0.0, 1.0);
        ((self.projected_grade() as f32) * frac).round() as i64
    }

    // ─── Active track / stage accessors ──────────────────────────────────────

    pub fn track(&self) -> Option<&'static Track> {
        self.active_track
    }

    pub fn current_stage(&self) -> Option<&'static Stage> {
        let track = self.active_track?;
        track.stages.get(self.current_stage_idx)
    }

    pub fn total_lanes(&self) -> usize {
        self.current_stage()
            .map(|s| s.road.lanes.total())
            .unwrap_or(0)
    }

    pub fn lanes_incoming(&self) -> usize {
        self.current_stage()
            .map(|s| s.road.lanes.incoming.len())
            .unwrap_or(0)
    }

    pub fn direction_of(&self, lane_idx: usize) -> TrafficDir {
        if lane_idx < self.lanes_incoming() {
            TrafficDir::Oncoming
        } else {
            TrafficDir::Same
        }
    }

    pub fn speed_scale(&self) -> f32 {
        self.active_track.map(|t| t.speed_scale).unwrap_or(1.0)
    }

    pub fn distance_scale(&self) -> f32 {
        self.active_track.map(|t| t.distance_scale).unwrap_or(1.0)
    }

    pub fn displayed_km_total(&self) -> f32 {
        self.player_pos_m.max(0.0) / self.distance_scale() / 1000.0
    }

    pub fn displayed_km_stage(&self) -> f32 {
        self.stage_traveled_m.max(0.0) / self.distance_scale() / 1000.0
    }

    pub fn track_total_km(&self) -> f32 {
        self.active_track
            .map(|t| t.total_distance_km())
            .unwrap_or(0.0)
    }

    pub fn current_stage_km(&self) -> f32 {
        self.current_stage().map(|s| s.distance_km).unwrap_or(0.0)
    }

    // ─── Player control ───────────────────────────────────────────────────────

    pub fn is_playing(&self) -> bool {
        matches!(self.phase, Phase::Playing) && self.screen == TrafficScreen::Racing
    }

    pub fn toggle_pause(&mut self) {
        if matches!(self.phase, Phase::Playing) {
            self.is_paused = !self.is_paused;
        }
    }

    pub fn move_left(&mut self) {
        if !self.is_playing() || self.is_paused || self.wheels_blocked() {
            return;
        }
        if self.player_lane_idx > 0 {
            self.player_lane_idx -= 1;
        }
    }

    pub fn move_right(&mut self) {
        if !self.is_playing() || self.is_paused || self.wheels_blocked() {
            return;
        }
        if self.player_lane_idx + 1 < self.total_lanes() {
            self.player_lane_idx += 1;
        }
    }

    fn wheels_blocked(&self) -> bool {
        self.wheel_blocked_until.is_some_and(|t| Instant::now() < t)
    }

    fn gas_blocked(&self) -> bool {
        self.gas_blocked_until.is_some_and(|t| Instant::now() < t)
    }

    fn brake_blocked(&self) -> bool {
        self.brake_blocked_until.is_some_and(|t| Instant::now() < t)
    }

    pub fn set_input(&mut self, input: PlayerInput) {
        let allowed = match input {
            PlayerInput::Accelerate => !self.gas_blocked(),
            PlayerInput::Brake | PlayerInput::Handbrake => !self.brake_blocked(),
            PlayerInput::None => true,
        };
        if !allowed {
            return;
        }
        self.input = input;
        self.input_last_set = Some(Instant::now());
    }

    // ─── Tick ─────────────────────────────────────────────────────────────────

    pub fn tick(&mut self) {
        if !self.is_playing() || self.is_paused {
            return;
        }
        let dt = Config::TICK_DT;

        if let Some(t) = self.input_last_set
            && t.elapsed() > Duration::from_millis(Config::INPUT_HOLD_MS)
        {
            self.input = PlayerInput::None;
            self.input_last_set = None;
        }

        let lane = self.current_lane_cfg();
        let (own_min, own_max, passive) = lane
            .map(|l| (l.own_min_speed, l.own_max_speed, l.passive_decel))
            .unwrap_or((0.0, 200.0, 0.0));

        match self.input {
            PlayerInput::Accelerate => {
                if self.player_speed_kmh < own_max {
                    self.player_speed_kmh =
                        (self.player_speed_kmh + Config::ACCEL_KMH_PER_S * dt).min(own_max);
                }
            }
            PlayerInput::Brake => {
                if self.player_speed_kmh > own_min {
                    self.player_speed_kmh =
                        (self.player_speed_kmh - Config::DECEL_KMH_PER_S * dt).max(own_min);
                }
            }
            PlayerInput::Handbrake => {
                if self.player_speed_kmh > own_min {
                    self.player_speed_kmh =
                        (self.player_speed_kmh - Config::DECEL_KMH_PER_S * 2.0 * dt).max(own_min);
                }
            }
            PlayerInput::None => {
                if passive > 0.0 && self.player_speed_kmh > own_min {
                    self.player_speed_kmh = (self.player_speed_kmh - passive * dt).max(own_min);
                }
            }
        }

        // Ease speed back into the new lane's bounds after a lane change.
        let step = Config::SPEED_CLAMP_PER_S * dt;
        if self.player_speed_kmh > own_max {
            self.player_speed_kmh = (self.player_speed_kmh - step).max(own_max);
        } else if self.player_speed_kmh < own_min {
            self.player_speed_kmh = (self.player_speed_kmh + step).min(own_min);
        }
        if self.player_speed_kmh < 0.0 {
            self.player_speed_kmh = 0.0;
        }

        let speed_scale = self.speed_scale();
        let player_step = self.player_speed_kmh / 3.6 * speed_scale * dt;
        self.player_pos_m += player_step;
        self.stage_traveled_m += player_step;
        self.elapsed_s += dt;
        self.score = self.projected_grade();

        let target = self.player_lane_idx as f32;
        let max_step = Config::LANE_TRANSITION_PER_S * dt;
        let diff = target - self.player_lane_display;
        if diff.abs() <= max_step {
            self.player_lane_display = target;
        } else {
            self.player_lane_display += diff.signum() * max_step;
        }

        self.update_ai(dt);
        self.manage_ai();
        self.spawn_obstacles_ahead();
        self.check_obstacle_crossings();

        // An obstacle crash may have ended play (Crashed/Dead); stop here so the
        // later collision check can't spend a second life in the same tick.
        if !matches!(self.phase, Phase::Playing) {
            return;
        }

        if let Some(stage) = self.current_stage() {
            let stage_distance_m = stage.distance_km * 1000.0 * self.distance_scale();
            if self.stage_traveled_m >= stage_distance_m {
                self.advance_stage();
            }
        }

        if !self.is_flashing() && self.check_collision() {
            self.handle_crash();
            return;
        }

        if let Some(track) = self.active_track {
            let total_m = track.total_distance_km() * 1000.0 * self.distance_scale();
            if self.player_pos_m >= total_m {
                let score = track.grade_time(self.elapsed_s);
                self.score = score;
                self.phase = Phase::Finished {
                    elapsed_s: self.elapsed_s,
                    score,
                };
                self.record_best();
            }
        }
    }

    fn current_lane_cfg(&self) -> Option<&'static super::track::Lane> {
        let stage = self.current_stage()?;
        stage.road.lanes.get(self.player_lane_idx)
    }

    /// Record the run's score as a per-track best. Finishing banks the full time
    /// grade; dying banks a partial score for the distance covered (see
    /// [`Self::death_score`]). The Traffic high score is the sum of all per-track
    /// bests.
    fn record_best(&mut self) {
        let Some(track) = self.active_track else {
            return;
        };
        // `self.score` is set by both end-of-run paths (finish and death) before
        // this is called. Ignore non-positive scores so a wipe never lowers a
        // stored best.
        let score = self.score;
        if score <= 0 {
            return;
        }
        let entry = self.best_scores.entry(track.name).or_insert(0);
        if score > *entry {
            *entry = score;
            if let Some(svc) = &self.svc {
                svc.submit_track_score_task(self.user_id, track.name.to_string(), score as i32);
            }
        }
        self.best_score = self.best_scores.values().copied().sum();
    }

    fn advance_stage(&mut self) {
        let Some(track) = self.active_track else {
            return;
        };
        let stage_m =
            track.stages[self.current_stage_idx].distance_km * 1000.0 * self.distance_scale();
        let overflow = (self.stage_traveled_m - stage_m).max(0.0);
        let next_idx = self.current_stage_idx + 1;
        if next_idx >= track.stages.len() {
            return;
        }

        let old_incoming = track.stages[self.current_stage_idx]
            .road
            .lanes
            .incoming
            .len();

        self.current_stage_idx = next_idx;
        self.stage_traveled_m = overflow;

        let new_incoming = track.stages[next_idx].road.lanes.incoming.len();
        let new_outgoing = track.stages[next_idx].road.lanes.outgoing.len();
        let new_total = new_incoming + new_outgoing;

        // Remap each AI car's lane_idx to the new stage's layout, preserving
        // direction (incoming stays incoming, outgoing stays outgoing). Cars on
        // a side that the new stage has no lanes for are dropped.
        self.ai_cars.retain_mut(|car| {
            if car.lane_idx < old_incoming {
                if new_incoming == 0 {
                    return false;
                }
                car.lane_idx = car.lane_idx.min(new_incoming - 1);
                car.direction = TrafficDir::Oncoming;
            } else {
                let outgoing_idx = car.lane_idx - old_incoming;
                if new_outgoing == 0 {
                    return false;
                }
                car.lane_idx = new_incoming + outgoing_idx.min(new_outgoing - 1);
                car.direction = TrafficDir::Same;
            }
            true
        });

        let new_outgoing_start = new_incoming;
        if self.player_lane_idx >= new_total {
            self.player_lane_idx = new_outgoing_start;
            self.player_lane_display = self.player_lane_idx as f32;
        }

        // Drop any car overlapping the player's position after the remap.
        // Use a meter-based clearance: player spans ~CAR_HEIGHT rows above
        // player_pos_m; clear a generous window so no instant collision fires.
        let player_lane = self.player_lane_idx;
        let player_pos = self.player_pos_m;
        let clear_half = Config::INITIAL_SKIP_M * 0.5;
        self.ai_cars.retain(|car| {
            car.lane_idx != player_lane || (car.pos_m - player_pos).abs() > clear_half
        });

        self.obstacles.clear();
        self.obstacle_seed_m = self.player_pos_m;

        self.push_effect("new stage");
    }

    // ─── AI ──────────────────────────────────────────────────────────────────

    fn update_ai(&mut self, dt: f32) {
        let follow_gap_m = Config::AI_FOLLOW_GAP_ROWS as f32 * Config::METERS_PER_ROW;
        let speed_scale = self.speed_scale();

        let snap: Vec<(f32, usize, TrafficDir, f32, f32)> = self
            .ai_cars
            .iter()
            .map(|c| {
                let half_m = c.height_rows as f32 * Config::METERS_PER_ROW * 0.5;
                (c.pos_m, c.lane_idx, c.direction, c.speed_kmh, half_m)
            })
            .collect();

        for (i, car) in self.ai_cars.iter_mut().enumerate() {
            let my_half = car.height_rows as f32 * Config::METERS_PER_ROW * 0.5;
            let my_front = match car.direction {
                TrafficDir::Same => car.pos_m + my_half,
                TrafficDir::Oncoming => car.pos_m - my_half,
            };

            let mut nearest: Option<(f32, f32)> = None;
            for (j, &(jpos, jlane, jdir, jspeed, jhalf)) in snap.iter().enumerate() {
                if i == j || jlane != car.lane_idx {
                    continue;
                }
                let j_back = match jdir {
                    TrafficDir::Same => jpos - jhalf,
                    TrafficDir::Oncoming => jpos + jhalf,
                };
                let gap = match car.direction {
                    TrafficDir::Same => j_back - my_front,
                    TrafficDir::Oncoming => my_front - j_back,
                };
                if gap <= 0.0 {
                    continue;
                }
                if nearest.is_none_or(|(g, _)| gap < g) {
                    nearest = Some((gap, jspeed));
                }
            }

            car.speed_kmh = match nearest {
                Some((gap, lspeed)) if gap < follow_gap_m && lspeed < car.cruise_kmh => lspeed,
                _ => car.cruise_kmh,
            };

            let step = car.speed_kmh / 3.6 * speed_scale * dt;
            match car.direction {
                TrafficDir::Same => car.pos_m += step,
                TrafficDir::Oncoming => car.pos_m -= step,
            }
        }
    }

    fn check_collision(&self) -> bool {
        // Overlap is decided in row units relative to the player, which sits at
        // rows `0..CAR_HEIGHT_ROWS`. Only the player↔car gap matters, so this is
        // independent of the render anchor (viewport height) — matching how the
        // renderer places both cars.
        let p_bot = Config::CAR_HEIGHT_ROWS as i32 - 1;
        for car in &self.ai_cars {
            if car.lane_idx != self.player_lane_idx {
                continue;
            }
            let center = ((self.player_pos_m - car.pos_m) / Config::METERS_PER_ROW) as i32;
            let h = car.height_rows as i32;
            let top = center - h / 2;
            let bot = top + h - 1;
            if top <= p_bot && bot >= 0 {
                return true;
            }
        }
        false
    }

    /// Handle a crash (AI collision or crash obstacle). Spends one life; if any
    /// remain the run continues in place with the surrounding cars in the
    /// player's lane cleared so no instant re-crash fires. At zero lives the
    /// run ends.
    fn handle_crash(&mut self) {
        self.lives = self.lives.saturating_sub(1);
        if self.lives == 0 {
            self.score = self.death_score();
            self.phase = Phase::Dead;
            self.record_best();
            return;
        }
        let player_lane = self.player_lane_idx;
        let player_pos = self.player_pos_m;
        let clear_half = Config::INITIAL_SKIP_M * 0.5;
        self.ai_cars.retain(|car| {
            car.lane_idx != player_lane || (car.pos_m - player_pos).abs() > clear_half
        });
        self.push_effect("crash! -1 life");
        self.phase = Phase::Crashed;
    }

    pub fn is_crashed(&self) -> bool {
        matches!(self.phase, Phase::Crashed) && self.screen == TrafficScreen::Racing
    }

    /// Dismiss the crash popup: resume play and start the recovery flash.
    pub fn resume_from_crash(&mut self) {
        if !matches!(self.phase, Phase::Crashed) {
            return;
        }
        self.phase = Phase::Playing;
        self.flash_until = Some(Instant::now() + Duration::from_millis(Config::CRASH_FLASH_MS));
        self.input = PlayerInput::None;
        self.input_last_set = None;
    }

    /// Post-crash recovery window: the car blinks and is immune to crashes.
    pub fn is_flashing(&self) -> bool {
        self.flash_until.is_some_and(|t| Instant::now() < t)
    }

    /// Whether the player car should be drawn this frame. Solid normally; during
    /// the recovery window it blinks on/off.
    pub fn player_visible(&self) -> bool {
        match self.flash_until {
            Some(t) => {
                let now = Instant::now();
                if now >= t {
                    return true;
                }
                let remaining_ms = (t - now).as_millis();
                (remaining_ms / 150).is_multiple_of(2)
            }
            None => true,
        }
    }

    fn manage_ai(&mut self) {
        let Some(stage) = self.current_stage() else {
            return;
        };
        let lanes = stage.road.lanes;
        let total_lanes = lanes.total();
        if total_lanes == 0 {
            return;
        }

        let player_pos = self.player_pos_m;

        // Despawn cars that fell too far behind or are in lanes that no longer exist.
        self.ai_cars.retain(|car| {
            car.pos_m > player_pos - Config::AI_DESPAWN_BEHIND_M && car.lane_idx < total_lanes
        });

        // Initial fill: on the very first tick of a race the car list is empty.
        // Populate the whole lookahead (skipping a safety gap near the player)
        // so the road isn't bare at start. Ongoing: spawn only beyond the
        // minimap so new cars scroll in naturally from ahead.
        let is_initial_fill = self.ai_cars.is_empty();
        let spawn_min = player_pos
            + if is_initial_fill {
                Config::INITIAL_SKIP_M
            } else {
                Config::MINIMAP_RANGE_M
            };
        let spawn_max = player_pos + Config::SPAWN_AHEAD_M;

        if spawn_max <= spawn_min {
            return;
        }

        let mut rng = rand::thread_rng();

        // Count current cars per lane.
        let mut per_lane = vec![0usize; total_lanes];
        for c in &self.ai_cars {
            per_lane[c.lane_idx] += 1;
        }

        for (lane_idx, &lane_count) in per_lane.iter().enumerate().take(total_lanes) {
            let lane = lanes.get(lane_idx).expect("idx in range");
            let target = density_to_count(lane.traffic_density);
            if lane_count >= target {
                continue;
            }

            let direction = self.direction_of(lane_idx);
            let needed = target - lane_count;

            // Initial fill: place as many as needed (up to MAX_ATTEMPTS tries).
            // Ongoing: place at most one car per manage_ai call to keep it smooth.
            let to_place = if is_initial_fill { needed } else { 1 };
            let max_attempts = to_place * 6;
            let mut placed = 0;

            for _ in 0..max_attempts {
                if placed >= to_place {
                    break;
                }
                let pos = spawn_min + rng.r#gen::<f32>() * (spawn_max - spawn_min);
                if !self.spawn_clear(pos, lane_idx, direction) {
                    continue;
                }
                let cruise = rng.gen_range(
                    lane.traffic_min_speed
                        ..=lane.traffic_max_speed.max(lane.traffic_min_speed + 1.0),
                );
                self.ai_cars.push(AiCar {
                    pos_m: pos,
                    speed_kmh: cruise,
                    cruise_kmh: cruise,
                    lane_idx,
                    direction,
                    height_rows: pick_car_height(lane.traffic_cars, &mut rng),
                });
                placed += 1;
            }
        }
    }

    fn spawn_clear(&self, pos: f32, lane_idx: usize, dir: TrafficDir) -> bool {
        self.ai_cars.iter().all(|o| {
            o.lane_idx != lane_idx
                || o.direction != dir
                || (o.pos_m - pos).abs() >= Config::AI_MIN_SEPARATION_M
        })
    }

    // ─── Obstacles ────────────────────────────────────────────────────────────

    fn spawn_obstacles_ahead(&mut self) {
        let Some(stage) = self.current_stage() else {
            return;
        };
        let lanes = stage.road.lanes;
        let target_max_m = self.player_pos_m + Config::MINIMAP_RANGE_M;

        const SLOT_M: f32 = 30.0;
        while self.obstacle_seed_m < target_max_m {
            let slot = (self.obstacle_seed_m / SLOT_M) as i64 ^ self.scenery_seed as i64;
            for lane_idx in 0..lanes.total() {
                let lane = match lanes.get(lane_idx) {
                    Some(l) => l,
                    None => continue,
                };
                for ob in lane.obstacles {
                    let h = hash3(slot, lane_idx as i64, ob.style.id);
                    let p = (h % 10_000) as f32 / 10_000.0;
                    if p < ob.frequency {
                        let pos = self.obstacle_seed_m + (h % SLOT_M as u64) as f32;
                        if self.is_near_stage_boundary(pos) {
                            continue;
                        }
                        self.obstacles.push(SpawnedObstacle {
                            style: ob.style,
                            effects: ob.effects,
                            crash: ob.has_crash(),
                            pos_m: pos,
                            lane_idx,
                            triggered: false,
                        });
                    }
                }
            }
            self.obstacle_seed_m += SLOT_M;
        }

        let cutoff = self.player_pos_m - Config::AI_DESPAWN_BEHIND_M;
        self.obstacles.retain(|o| o.pos_m > cutoff);
    }

    fn check_obstacle_crossings(&mut self) {
        let p_front = self.player_pos_m;
        let p_back = self.player_pos_m - Config::CAR_HEIGHT_ROWS as f32 * Config::METERS_PER_ROW;

        let mut triggered: Vec<(&'static str, &'static [ObstacleEffect])> = Vec::new();
        for obs in self.obstacles.iter_mut() {
            if obs.triggered || obs.lane_idx != self.player_lane_idx {
                continue;
            }
            if obs.pos_m >= p_back && obs.pos_m <= p_front {
                obs.triggered = true;
                triggered.push((obs.style.label, obs.effects));
            }
        }

        for (label, effects) in triggered {
            self.push_effect(label);
            for &eff in effects {
                self.apply_effect(eff);
            }
        }
    }

    fn apply_effect(&mut self, eff: ObstacleEffect) {
        match eff {
            ObstacleEffect::Crash => {
                if !self.is_flashing() {
                    self.handle_crash();
                }
            }
            ObstacleEffect::SpeedChange { affect } => {
                self.player_speed_kmh = (self.player_speed_kmh * (1.0 + affect)).max(0.0);
            }
            ObstacleEffect::BlockGas { cooldown_ms } => {
                self.gas_blocked_until =
                    Some(Instant::now() + Duration::from_millis(cooldown_ms as u64));
            }
            ObstacleEffect::BlockBrakes { cooldown_ms } => {
                self.brake_blocked_until =
                    Some(Instant::now() + Duration::from_millis(cooldown_ms as u64));
            }
            ObstacleEffect::BlockWheels { cooldown_ms } => {
                self.wheel_blocked_until =
                    Some(Instant::now() + Duration::from_millis(cooldown_ms as u64));
            }
        }
    }

    /// Push a labelled effect into the right-panel log, capping at capacity.
    fn push_effect(&mut self, label: &'static str) {
        self.recent_effects.push_front(RecentEffect {
            label,
            at: Instant::now(),
        });
        self.recent_effects
            .truncate(Config::RECENT_EFFECTS_CAPACITY);
    }

    // ─── Geometry helpers (used by ui.rs) ────────────────────────────────────

    /// Map a world position to a screen row, given the render anchor row (the
    /// row the player car's top occupies). The anchor is supplied by the
    /// renderer so the mapping tracks the actual viewport height; see
    /// `ui::road_anchor_row`.
    pub fn track_to_screen_row(&self, track_pos_m: f32, anchor_row: i32) -> i32 {
        let offset_m = self.player_pos_m - track_pos_m;
        anchor_row + (offset_m / Config::METERS_PER_ROW) as i32
    }

    pub fn progress_pct(&self) -> f32 {
        let Some(track) = self.active_track else {
            return 0.0;
        };
        let total_m = track.total_distance_km() * 1000.0 * self.distance_scale();
        if total_m == 0.0 {
            return 0.0;
        }
        (self.player_pos_m.max(0.0) / total_m * 100.0).clamp(0.0, 100.0)
    }

    pub fn elapsed_formatted(&self) -> String {
        let total = self.elapsed_s as u32;
        let mins = total / 60;
        let secs = total % 60;
        let tenth = ((self.elapsed_s - total as f32) * 10.0) as u32;
        format!("{}:{:02}.{}", mins, secs, tenth)
    }

    /// Absolute `player_pos_m` value at which stage `idx` begins.
    /// Stage 0 always starts at 0 (the pre-stage zone is before that).
    pub fn stage_start_pos_m(&self, idx: usize) -> f32 {
        let Some(track) = self.active_track else {
            return 0.0;
        };
        let scale = self.distance_scale();
        track.stages[..idx]
            .iter()
            .map(|s| s.distance_km * 1000.0 * scale)
            .sum()
    }

    fn is_near_stage_boundary(&self, pos: f32) -> bool {
        let Some(track) = self.active_track else {
            return false;
        };
        let scale = self.distance_scale();
        let mut boundary = 0.0f32;
        for stage in track.stages {
            if (pos - boundary).abs() < Config::STAGE_CLEAR_M {
                return true;
            }
            boundary += stage.distance_km * 1000.0 * scale;
        }
        (pos - boundary).abs() < Config::STAGE_CLEAR_M
    }
}

impl Default for State {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Helpers ────────────────────────────────────────────────────────────────

fn density_to_count(density: f32) -> usize {
    let d = density.clamp(0.0, 1.0);
    (d * Config::SPAWN_AHEAD_M / Config::AI_MIN_SEPARATION_M).round() as usize
}

fn pick_car_height(cars: &[super::track::Car], rng: &mut impl Rng) -> u8 {
    if cars.is_empty() {
        return 3;
    }
    let total: f32 = cars.iter().map(|c| c.incidence).sum();
    if total <= 0.0 {
        return cars[0].height;
    }
    let mut r: f32 = rng.r#gen::<f32>() * total;
    for c in cars {
        if r < c.incidence {
            return c.height;
        }
        r -= c.incidence;
    }
    cars[cars.len() - 1].height
}

/// Cheap deterministic 3-input hash for obstacle placement seeding.
/// Exported to `ui.rs` for scenery placement (avoids duplicate).
pub(super) fn hash3(a: i64, b: i64, c: i64) -> u64 {
    let mut x = (a as u64).wrapping_mul(0x9E3779B97F4A7C15);
    x ^= (b as u64).wrapping_mul(0xC2B2AE3D27D4EB4F);
    x = x.rotate_left(31);
    x ^= (c as u64).wrapping_mul(0x165667B19E3779F9);
    x.wrapping_mul(0x94D049BB133111EB)
}

// ─── Unit tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::arcade::traffic::tracks::DEFAULT_TRACK;

    #[test]
    fn picker_starts_with_zero_score() {
        let s = State::new();
        assert_eq!(s.screen, TrafficScreen::Picker);
        assert_eq!(s.best_score, 0);
    }

    #[test]
    fn start_track_moves_to_racing() {
        let mut s = State::new();
        s.start_track(DEFAULT_TRACK);
        assert_eq!(s.screen, TrafficScreen::Racing);
        assert_eq!(s.current_stage_idx, 0);
        assert_eq!(
            s.player_lane_idx,
            DEFAULT_TRACK.stages[0].road.lanes.player_start_idx()
        );
    }

    #[test]
    fn move_left_then_right_returns_to_origin() {
        let mut s = State::new();
        s.start_track(DEFAULT_TRACK);
        let start = s.player_lane_idx;
        s.move_left();
        assert!(s.player_lane_idx < start);
        s.move_right();
        assert_eq!(s.player_lane_idx, start);
    }

    #[test]
    fn cannot_drive_above_lane_max_for_long() {
        let mut s = State::new();
        s.start_track(DEFAULT_TRACK);
        let lane = s.current_lane_cfg().unwrap();
        s.player_speed_kmh = lane.own_max_speed + 50.0;
        for _ in 0..30 {
            s.tick();
        }
        let lane = s.current_lane_cfg().unwrap();
        assert!(s.player_speed_kmh <= lane.own_max_speed + 1.0);
    }
}
