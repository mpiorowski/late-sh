use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use anyhow::Result;
use chrono::{DateTime, NaiveDate, Utc};
use late_core::models::{
    aquarium_care::{self as care_rules, CARE_DAYS, FRY_DAYS, MURKY_AFTER_DAYS},
    aquarium_shield::AquariumShield,
};
use rand::{Rng, rngs::ThreadRng};
use ratatui::{layout::Rect, style::Color};

use super::{
    config::{AppConfig, Mode},
    creature::{
        ActivityState, CreatureDef, Entity, FRY_CREATURE, PoseIntent, Territory, Variant,
        tallest_variant_height,
    },
    world::{ReefWorld, WorldBounds, load_world_layer},
};

const FEED_EFFECT_DURATION: Duration = Duration::from_secs(8);
const HUNGRY_MOTION_DIVISOR: u64 = 4;

/// The owner's care of the tank, the bonsai's model with stakes. One free
/// feeding a day; hunger is derived, never stored: not fed today (UTC) is
/// hungry, and hungry fish sink to the floor (`nudge_hungry_entity_down`).
/// Fourteen straight fed days hatch a fry, fourteen unfed days starve a fish
/// (settled by the service at login), seven unfed days turn the water
/// murky, and the shield's auto feeder takes days off both clocks. Lives
/// beside the simulation rather than inside it because the sim also draws
/// other people's tanks (the profile modal), which carry no care of yours.
pub(crate) struct AquariumCare {
    pub(crate) last_fed: Option<DateTime<Utc>>,
    /// Straight fed UTC days, counting the last meal.
    pub(crate) streak: i32,
    /// Every shield window the owner bought, live or lapsed: a lapsed one
    /// still excuses the days it covered.
    pub(crate) shields: Vec<AquariumShield>,
    pub(crate) fry: Option<Fry>,
}

/// A hatchling: drawn as the small sprite for its first `FRY_DAYS`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Fry {
    pub(crate) creature: String,
    pub(crate) born: NaiveDate,
}

/// Outcome of a feed press. The day's meal is free; the only refusal is a
/// tank that has already eaten.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CareOutcome {
    Fed,
    AlreadyFedToday,
}

/// What the Zen tile's fourteen boxes show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CareBar {
    /// Fed today: this many boxes green, the way to the next fry (a full bar
    /// on the day one hatches, one box the day after).
    Streak(u32),
    /// Not fed: this many boxes red, the way to the next death. Saturates:
    /// a tank past its first loss stays full red until somebody feeds it.
    Dry(u32),
    /// The shield's auto feeder covers today: nothing counts either way.
    Minded,
}

impl AquariumCare {
    pub(crate) fn new(
        row: Option<late_core::models::aquarium_care::AquariumCare>,
        shields: Vec<AquariumShield>,
    ) -> Self {
        match row {
            Some(row) => Self {
                last_fed: Some(row.last_fed),
                streak: row.streak,
                shields,
                fry: match (row.fry_creature, row.fry_born) {
                    (Some(creature), Some(born)) => Some(Fry { creature, born }),
                    (Some(_), None) | (None, Some(_)) | (None, None) => None,
                },
            },
            None => Self {
                last_fed: None,
                streak: 0,
                shields,
                fry: None,
            },
        }
    }

    /// Whether the shield's auto feeder covers today.
    pub(crate) fn minded_on(&self, today: NaiveDate) -> bool {
        self.shields.iter().any(|shield| shield.covers_day(today))
    }

    /// Whether the fish go hungry right now: nobody fed them today and no
    /// auto feeder is minding the tank.
    pub(crate) fn hungry(&self) -> bool {
        self.hungry_on(Utc::now().date_naive())
    }

    pub(crate) fn hungry_on(&self, today: NaiveDate) -> bool {
        !fed_on(self.last_fed, today) && !self.minded_on(today)
    }

    /// Unfed days on the clock since the last meal, shield days excused.
    pub(crate) fn dry_days_on(&self, today: NaiveDate) -> u32 {
        match self.last_fed {
            Some(last) => care_rules::dry_days(last.date_naive(), today, &self.shields),
            None => 0,
        }
    }

    /// Whether the water has gone murky: a week or more unfed.
    pub(crate) fn murky(&self) -> bool {
        self.murky_on(Utc::now().date_naive())
    }

    pub(crate) fn murky_on(&self, today: NaiveDate) -> bool {
        self.dry_days_on(today) >= MURKY_AFTER_DAYS
    }

    pub(crate) fn bar(&self) -> CareBar {
        self.bar_on(Utc::now().date_naive())
    }

    pub(crate) fn bar_on(&self, today: NaiveDate) -> CareBar {
        if fed_on(self.last_fed, today) {
            let streak = self.streak.max(0) as u32;
            let shown = if streak == 0 {
                0
            } else {
                (streak - 1) % CARE_DAYS + 1
            };
            return CareBar::Streak(shown);
        }
        if self.minded_on(today) {
            return CareBar::Minded;
        }
        CareBar::Dry(self.dry_days_on(today).min(CARE_DAYS))
    }

    /// The fry still small enough to draw as the hatchling sprite, by
    /// creature name.
    pub(crate) fn fry_visible(&self) -> Option<&str> {
        self.fry_visible_on(Utc::now().date_naive())
    }

    pub(crate) fn fry_visible_on(&self, today: NaiveDate) -> Option<&str> {
        let fry = self.fry.as_ref()?;
        let grown = fry
            .born
            .checked_add_days(chrono::Days::new(FRY_DAYS as u64))
            .is_none_or(|grown_on| today >= grown_on);
        (!grown).then_some(fry.creature.as_str())
    }

    /// The day's meal. The service pays the chips and hatches any fry behind
    /// DB gates; the caller only learns whether this press was the one that
    /// fed the tank. The streak follows the same rule as the row: it
    /// continues when the last meal was yesterday or only shielded days
    /// ago, and restarts otherwise.
    pub(crate) fn feed(&mut self) -> CareOutcome {
        let now = Utc::now();
        let today = now.date_naive();
        if fed_on(self.last_fed, today) {
            return CareOutcome::AlreadyFedToday;
        }
        let continues_from = care_rules::streak_continues_from(today, &self.shields);
        let continues = self
            .last_fed
            .is_some_and(|last| last.date_naive() >= continues_from);
        self.streak = if continues { self.streak + 1 } else { 1 };
        self.last_fed = Some(now);
        CareOutcome::Fed
    }

    /// A fry hatched into the water (the service's event came back).
    pub(crate) fn set_fry(&mut self, creature: String, born: NaiveDate) {
        self.fry = Some(Fry { creature, born });
    }

    /// A live shield from the shop snapshot joins what we hold: a rebuy
    /// extends the live window in place (same start, later end), a fresh
    /// purchase after a lapse is a new one. `None` (no live shield) keeps
    /// every lapsed window, which the starvation count still needs.
    pub(crate) fn refresh_shield(&mut self, live: Option<AquariumShield>) {
        if let Some(live) = live {
            self.shields
                .retain(|shield| shield.starts_at != live.starts_at);
            self.shields.insert(0, live);
        }
    }
}

fn fed_on(last: Option<DateTime<Utc>>, today: NaiveDate) -> bool {
    last.is_some_and(|time| time.date_naive() == today)
}

pub(crate) struct AquariumState {
    pub(crate) definitions: Vec<CreatureDef>,
    pub(crate) entities: Vec<Entity>,
    pub(crate) tick: u64,
    pub(crate) show_background: bool,
    pub(crate) show_creature_names: bool,
    pub(crate) mode: RuntimeMode,
    feed_started_at: Option<Instant>,
    hungry: bool,
    murky: bool,
}

pub(crate) enum RuntimeMode {
    Tank(TankState),
    Reef(ReefState),
}

pub(crate) struct TankState {
    pub width: u16,
    pub height: u16,
}

pub(crate) struct ReefState {
    pub world: ReefWorld,
    pub respawn_delay: Duration,
    pub last_area: Rect,
    pub min_height: u16,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct WaterBand {
    pub top: i32,
    pub bottom: i32,
}

impl WaterBand {
    pub(crate) fn for_reef(world: &ReefWorld, terminal_height: u16) -> Self {
        Self {
            top: world.surface.height as i32,
            bottom: terminal_height.saturating_sub(world.floor.height) as i32,
        }
    }

    pub(crate) fn random_y_for(&self, variant: &Variant, rng: &mut ThreadRng) -> Option<i32> {
        let max_y = self.bottom - variant.height as i32;
        if max_y < self.top {
            None
        } else {
            Some(rng.gen_range(self.top..=max_y))
        }
    }

    pub(crate) fn clamp_y_for(&self, y: i32, variant: &Variant) -> Option<i32> {
        let max_y = self.bottom - variant.height as i32;
        if max_y < self.top {
            None
        } else {
            Some(y.clamp(self.top, max_y))
        }
    }

    pub(crate) fn y_bounds_for(&self, variant: &Variant) -> Option<(i32, i32)> {
        let max_y = self.bottom - variant.height as i32;
        if max_y < self.top {
            None
        } else {
            Some((self.top, max_y))
        }
    }

    pub(crate) fn floor_y_for(&self, variant: &Variant) -> Option<i32> {
        let y = self.bottom - variant.height as i32;
        if y < self.top { None } else { Some(y) }
    }
}

impl AquariumState {
    pub(crate) fn default_for_area(launch_area: Rect) -> Result<Self> {
        Self::new(
            crate::app::hub::aquarium::config::default_config()?,
            crate::app::hub::aquarium::creature::load_default_creatures()?,
            launch_area,
        )
    }

    pub(crate) fn new(
        config: AppConfig,
        definitions: Vec<CreatureDef>,
        launch_area: Rect,
    ) -> Result<Self> {
        let initial_count_scale = match config.mode {
            Mode::Reef => config.reef.creatures.count_scale,
            Mode::Tank => 1.0,
        };
        let mode = match config.mode {
            Mode::Tank => RuntimeMode::Tank(TankState {
                width: config.tank.width,
                height: config.tank.height,
            }),
            Mode::Reef => {
                let surface = load_world_layer(&config.reef.horizontal.surface)?;
                let floor = load_world_layer(&config.reef.horizontal.floor)?;
                let min_height = surface
                    .height
                    .saturating_add(floor.height)
                    .saturating_add(tallest_variant_height(&definitions));
                let world = ReefWorld::new(surface, floor);

                RuntimeMode::Reef(ReefState {
                    world,
                    respawn_delay: Duration::from_millis(config.reef.creatures.respawn_delay_ms),
                    last_area: launch_area,
                    min_height,
                })
            }
        };

        let mut app = Self {
            definitions,
            entities: Vec::new(),
            tick: 0,
            show_background: false,
            show_creature_names: false,
            mode,
            feed_started_at: None,
            hungry: false,
            murky: false,
        };
        app.spawn_initial_entities(launch_area, initial_count_scale);
        Ok(app)
    }

    #[cfg(test)]
    #[allow(dead_code)]
    fn min_height(&self) -> Option<u16> {
        match &self.mode {
            RuntimeMode::Tank(_) => None,
            RuntimeMode::Reef(reef) => Some(reef.min_height),
        }
    }

    fn spawn_initial_entities(&mut self, launch_area: Rect, count_scale: f64) {
        let mut rng = rand::thread_rng();

        for def_index in 0..self.definitions.len() {
            let count = scaled_initial_count(self.definitions[def_index].count, count_scale).max(1);
            for copy_index in 0..count {
                let entity = match &self.mode {
                    RuntimeMode::Tank(tank) => {
                        spawn_tank_entity(&self.definitions, def_index, copy_index, tank, &mut rng)
                    }
                    RuntimeMode::Reef(reef) => spawn_reef_entity(
                        &self.definitions,
                        def_index,
                        copy_index,
                        &reef.world,
                        launch_area,
                        SpawnMode::Anywhere,
                        &mut rng,
                    ),
                };
                self.entities.push(entity);
            }
        }
    }

    /// Put the owner's fish in the water: `active_creatures` is the shop's
    /// `(creature, count)` list, `fry` the creature whose newest hatchling
    /// is still small. The fry takes one of its parent species' places and
    /// swims as the `FRY_CREATURE` sprite in a parent colour. Nothing is
    /// respawned when the population already matches.
    pub(crate) fn set_active_creatures(
        &mut self,
        active_creatures: &[(String, usize)],
        fry: Option<&str>,
    ) {
        let desired = self.desired_population(active_creatures, fry);
        if self.population_matches(&desired) {
            return;
        }

        let mut rng = rand::thread_rng();
        let mut entities = Vec::new();
        let mut copy_index = 0;
        for spawn in &desired {
            for _ in 0..spawn.count {
                let mut entity = match &self.mode {
                    RuntimeMode::Tank(tank) => spawn_tank_entity(
                        &self.definitions,
                        spawn.def_index,
                        copy_index,
                        tank,
                        &mut rng,
                    ),
                    RuntimeMode::Reef(reef) => spawn_reef_entity(
                        &self.definitions,
                        spawn.def_index,
                        copy_index,
                        &reef.world,
                        reef.last_area,
                        SpawnMode::Anywhere,
                        &mut rng,
                    ),
                };
                if let Some(parent) = spawn.colour_of {
                    entity.color = entity_color(&self.definitions, parent, copy_index, &mut rng);
                }
                entities.push(entity);
                copy_index += 1;
            }
        }
        self.entities = entities;
    }

    /// The population as definition indices, the fry carved out of its
    /// parent's count. A fry whose parent is not swimming (sold off, or
    /// moved to inventory) is not drawn: there is nothing to be small next to.
    fn desired_population(
        &self,
        active_creatures: &[(String, usize)],
        fry: Option<&str>,
    ) -> Vec<PopulationSpawn> {
        let def_index = |name: &str| self.definitions.iter().position(|def| def.name == name);
        let fry_def = def_index(FRY_CREATURE);
        let mut spawns = Vec::new();
        for (name, count) in active_creatures {
            let Some(index) = def_index(name) else {
                continue;
            };
            let mut count = *count;
            if let (Some(fry_index), Some(parent)) = (fry_def, fry)
                && parent == name
                && count > 0
            {
                count -= 1;
                spawns.push(PopulationSpawn {
                    def_index: fry_index,
                    count: 1,
                    colour_of: Some(index),
                });
            }
            if count > 0 {
                spawns.push(PopulationSpawn {
                    def_index: index,
                    count,
                    colour_of: None,
                });
            }
        }
        spawns
    }

    fn population_matches(&self, desired: &[PopulationSpawn]) -> bool {
        let mut current: HashMap<usize, usize> = HashMap::new();
        for entity in &self.entities {
            *current.entry(entity.def).or_insert(0) += 1;
        }
        let mut wanted: HashMap<usize, usize> = HashMap::new();
        for spawn in desired {
            *wanted.entry(spawn.def_index).or_insert(0) += spawn.count;
        }
        current == wanted
    }

    /// Advance the simulation exactly one step: the aquarium keeps no clock
    /// of its own. The caller (App::tick on its half-rate edge) owns the
    /// cadence, so one call = one step = one visibly moved frame.
    pub(crate) fn tick(&mut self) {
        let now = Instant::now();
        if self
            .feed_started_at
            .is_some_and(|started| now.saturating_duration_since(started) >= FEED_EFFECT_DURATION)
        {
            self.feed_started_at = None;
        }
        self.tick = self.tick.wrapping_add(1);
        let mut rng = rand::thread_rng();
        match &mut self.mode {
            RuntimeMode::Tank(tank) => {
                let bounds = Rect::new(0, 0, tank.width - 2, tank.height - 2);
                for entity in &mut self.entities {
                    let def = &self.definitions[entity.def];
                    entity.maybe_rearrange_school(def, &mut rng);
                    let variant = def.best_variant(
                        entity.pose_dx_for(def),
                        entity.animation_tick_for(def, self.tick),
                        entity.phase,
                    );
                    // Tank motion never lets art touch the last water row
                    // (see `tick_bounded`), so the hungry rest row keeps the
                    // same -1.
                    let tank_rest_y =
                        (bounds.height.saturating_sub(variant.height).max(1) - 1) as i32;
                    if self.hungry {
                        nudge_hungry_entity_down(entity, tank_rest_y);
                        if !self.tick.is_multiple_of(HUNGRY_MOTION_DIVISOR) {
                            continue;
                        }
                    }
                    entity.tick_bounded(def, bounds, variant, self.tick, &mut rng);
                    if self.hungry {
                        nudge_hungry_entity_down(entity, tank_rest_y);
                    }
                }
            }
            RuntimeMode::Reef(reef) => tick_reef(
                &self.definitions,
                &mut self.entities,
                self.tick,
                self.hungry,
                reef,
                &mut rng,
            ),
        }
    }

    pub(crate) fn handle_resize(&mut self, width: u16, height: u16) {
        let mut rng = rand::thread_rng();
        if let RuntimeMode::Reef(reef) = &mut self.mode {
            reef.last_area = Rect::new(0, 0, width, height);
            rebind_creatures_to_reef(
                &self.definitions,
                &mut self.entities,
                &reef.world,
                reef.last_area,
                self.tick,
                &mut rng,
            );
        }
    }

    pub(crate) fn feed(&mut self) {
        self.feed_started_at = Some(Instant::now());
        self.hungry = false;
        for entity in self.entities.iter_mut().filter(|entity| entity.is_active()) {
            let Some(def) = self.definitions.get(entity.def) else {
                continue;
            };
            if def.is_sessile() {
                continue;
            }
            entity.activity = ActivityState::Active;
            entity.activity_ticks = entity.activity_ticks.max(18);
            entity.school_rearrangements = entity.school_rearrangements.wrapping_add(1);
            if entity.dx == 0 {
                entity.resume_lateral_motion();
            }
        }
    }

    pub(crate) fn feed_effect_tick(&self) -> Option<u64> {
        let started = self.feed_started_at?;
        let elapsed = Instant::now().saturating_duration_since(started);
        (elapsed < FEED_EFFECT_DURATION).then_some(elapsed.as_millis() as u64 / 120)
    }

    pub(crate) fn set_hungry(&mut self, hungry: bool) {
        self.hungry = hungry;
    }

    /// Murky water (a week unfed): the renderer dims the fish and floats
    /// algae through the water. The sim itself does not change.
    pub(crate) fn set_murky(&mut self, murky: bool) {
        self.murky = murky;
    }

    pub(crate) fn is_murky(&self) -> bool {
        self.murky
    }
}

/// One line of the population to spawn: `count` of `def_index`, coloured
/// like `colour_of` when set (the fry wears its parent's colour).
struct PopulationSpawn {
    def_index: usize,
    count: usize,
    colour_of: Option<usize>,
}

fn scaled_initial_count(count: usize, scale: f64) -> usize {
    if scale <= 0.0 {
        0
    } else {
        ((count as f64) * scale).round().min(usize::MAX as f64) as usize
    }
}

/// Sink a hungry creature one row per step until it rests at `rest_y` (the
/// lowest row its art can occupy without touching the floor/tank edge).
fn nudge_hungry_entity_down(entity: &mut Entity, rest_y: i32) {
    let rest_y = rest_y.max(0);
    if entity.y < rest_y {
        entity.y += 1;
        entity.dy = 1;
    } else {
        entity.y = rest_y;
        entity.dy = 0;
        entity.activity = ActivityState::Idle;
    }
}

fn tick_reef(
    definitions: &[CreatureDef],
    entities: &mut [Entity],
    tick: u64,
    hungry: bool,
    reef: &mut ReefState,
    rng: &mut ThreadRng,
) {
    if reef.last_area.height < reef.min_height {
        return;
    }

    let now = Instant::now();
    let bounds = reef.world.visible_bounds(reef.last_area.width);
    let band = WaterBand::for_reef(&reef.world, reef.last_area.height);

    for (copy_index, entity) in entities.iter_mut().enumerate() {
        if let Some(respawn_at) = entity.respawn_at {
            if now < respawn_at {
                continue;
            }

            let replacement = spawn_reef_entity(
                definitions,
                entity.def,
                copy_index,
                &reef.world,
                reef.last_area,
                SpawnMode::Edge,
                rng,
            );
            entity.x = replacement.x;
            entity.y = replacement.y;
            entity.dx = replacement.dx;
            entity.dy = replacement.dy;
            entity.phase = replacement.phase;
            entity.pose_intent = replacement.pose_intent;
            entity.lateral_dx = replacement.lateral_dx;
            entity.depth_swim_ticks = replacement.depth_swim_ticks;
            entity.school_rearrangements = replacement.school_rearrangements;
            entity.activity = replacement.activity;
            entity.activity_ticks = replacement.activity_ticks;
            entity.idle_move_chance = replacement.idle_move_chance;
            entity.idle_turn_chance = replacement.idle_turn_chance;
            entity.territory = replacement.territory;
            entity.respawn_at = None;
            continue;
        }

        let def = &definitions[entity.def];
        entity.maybe_rearrange_school(def, rng);
        if def.is_floor_bound() {
            let variant = def.best_variant_for(0, PoseIntent::Lateral, 0, entity.phase);
            entity.dx = 0;
            entity.dy = 0;
            entity.activity = ActivityState::Idle;
            entity.pose_intent = PoseIntent::Lateral;
            entity.y = band.floor_y_for(variant).unwrap_or(band.top);
            continue;
        }

        let motion_variant = def.best_variant_for(
            entity.pose_dx_for(def),
            entity.pose_intent,
            entity.animation_tick_for(def, tick),
            entity.phase,
        );
        if hungry {
            let rest_y = band.floor_y_for(motion_variant).unwrap_or(band.top);
            nudge_hungry_entity_down(entity, rest_y);
            if !tick.is_multiple_of(HUNGRY_MOTION_DIVISOR) {
                continue;
            }
        }
        update_reef_motion(def, entity, &band, motion_variant, tick, hungry, rng);

        entity.x += entity.dx as i32;
        entity.y += entity.dy as i32;

        let variant = def.best_variant_for(
            entity.pose_dx_for(def),
            entity.pose_intent,
            entity.animation_tick_for(def, tick),
            entity.phase,
        );
        if let Some(clamped_y) = band.clamp_y_for(entity.y, variant)
            && clamped_y != entity.y
        {
            if def.four_way_swimmer && entity.depth_swim_ticks > 0 {
                entity.resume_lateral_motion();
            } else {
                entity.dy = if clamped_y <= band.top {
                    entity.dy.abs()
                } else {
                    -entity.dy.abs()
                };
            }
            entity.y = clamped_y;
        }
        if hungry {
            let rest_y = band.floor_y_for(variant).unwrap_or(band.top);
            nudge_hungry_entity_down(entity, rest_y);
        }

        if entity_exited(entity, variant, bounds) {
            entity.mark_exited(reef.respawn_delay, now);
        }
    }
}

fn update_reef_motion(
    def: &CreatureDef,
    entity: &mut Entity,
    band: &WaterBand,
    variant: &Variant,
    tick: u64,
    hungry: bool,
    rng: &mut ThreadRng,
) {
    let was_idle = entity.activity == ActivityState::Idle;
    entity.advance_activity(def, rng);
    if entity.activity == ActivityState::Idle {
        entity.update_idle_motion(tick, rng);
        return;
    }
    if was_idle && entity.dx == 0 {
        entity.resume_lateral_motion();
    }

    if def.four_way_swimmer {
        update_four_way_swim(entity, rng);
    } else if def.brownian && rng.gen_bool(0.25) {
        entity.dx = rng.gen_range(-1..=1);
        entity.dy = rng.gen_range(-1..=1);
    } else if def.uses_default_movement()
        && rng.gen_bool(super::creature::default_movement_transition_chance())
    {
        entity.toggle_vertical_motion(rng);
    }

    apply_depth_bias(def, entity, band, variant, hungry, rng);
    apply_territory_bias(def, entity, rng);
}

fn apply_depth_bias(
    def: &CreatureDef,
    entity: &mut Entity,
    band: &WaterBand,
    variant: &Variant,
    hungry: bool,
    rng: &mut ThreadRng,
) {
    if (!hungry && def.four_way_swimmer) || def.is_floor_bound() {
        return;
    }
    let Some((min_y, max_y)) = band.y_bounds_for(variant) else {
        return;
    };
    if min_y >= max_y {
        return;
    }

    let preferences = &def.preferences;
    let mut target = if hungry { 1.0 } else { preferences.depth };
    if !hungry {
        target += preferences.demersal * (1.0 - target) * 0.4;
        target -= preferences.reefer * target * 0.25;
        target = target.clamp(0.0, 1.0);
    }

    let target_y = min_y + ((max_y - min_y) as f64 * target).round() as i32;
    let distance = target_y - entity.y;
    if distance.abs() <= 1 {
        if rng.gen_bool((preferences.sedentary * 0.15).clamp(0.0, 0.5)) {
            entity.dy = 0;
        }
        return;
    }

    let preference_strength = if hungry {
        0.85
    } else {
        (preferences
            .demersal
            .max(preferences.reefer)
            .max((preferences.depth - 0.5).abs() * 2.0)
            * 0.35
            + 0.08)
            .clamp(0.0, 0.6)
    };
    if rng.gen_bool(preference_strength) {
        entity.dy = distance.signum() as i16;
    }
}

fn apply_territory_bias(def: &CreatureDef, entity: &mut Entity, rng: &mut ThreadRng) {
    let Some(territory) = entity.territory else {
        return;
    };
    let territorial = def.preferences.territorial;
    if territorial <= 0.0 {
        return;
    }

    if entity.x < territory.min_x {
        entity.dx = 1;
    } else if entity.x > territory.max_x {
        entity.dx = -1;
    } else if rng.gen_bool((territorial * 0.12).clamp(0.0, 0.75)) {
        if entity.dx > 0 && entity.x >= territory.max_x {
            entity.dx = -1;
        } else if entity.dx < 0 && entity.x <= territory.min_x {
            entity.dx = 1;
        }
    }

    if entity.y < territory.min_y {
        entity.dy = 1;
    } else if entity.y > territory.max_y {
        entity.dy = -1;
    }
}

fn update_four_way_swim(entity: &mut Entity, rng: &mut ThreadRng) {
    if entity.depth_swim_ticks > 0 {
        entity.depth_swim_ticks -= 1;
        entity.dx = 0;
        entity.dy = match entity.pose_intent {
            PoseIntent::FaceAway => -1,
            PoseIntent::Face => 1,
            PoseIntent::Lateral => 0,
        };

        if entity.depth_swim_ticks == 0 {
            entity.resume_lateral_motion();
        }
        return;
    }

    if entity.dx != 0 {
        entity.lateral_dx = entity.dx.signum();
    }
    if entity.lateral_dx == 0 {
        entity.lateral_dx = if rng.gen_bool(0.5) { -1 } else { 1 };
    }

    entity.dx = entity.lateral_dx;
    entity.dy = 0;
    entity.pose_intent = PoseIntent::Lateral;

    if rng.gen_bool(0.035) {
        let swim_towards = rng.gen_bool(0.25);
        entity.pose_intent = if swim_towards {
            PoseIntent::Face
        } else {
            PoseIntent::FaceAway
        };
        entity.depth_swim_ticks = rng.gen_range(6..=18);
        entity.dx = 0;
        entity.dy = if swim_towards { 1 } else { -1 };
    } else if rng.gen_bool(0.02) {
        entity.lateral_dx = -entity.lateral_dx;
        entity.dx = entity.lateral_dx;
    }
}

fn entity_exited(entity: &Entity, variant: &Variant, bounds: WorldBounds) -> bool {
    entity.x + variant.width as i32 <= bounds.start || entity.x >= bounds.end
}

fn rebind_creatures_to_reef(
    definitions: &[CreatureDef],
    entities: &mut [Entity],
    world: &ReefWorld,
    area: Rect,
    tick: u64,
    rng: &mut ThreadRng,
) {
    let band = WaterBand::for_reef(world, area.height);
    for entity in entities {
        if !entity.is_active() {
            continue;
        }

        let def = &definitions[entity.def];
        let variant = def.best_variant_for(
            entity.pose_dx_for(def),
            entity.pose_intent,
            entity.animation_tick_for(def, tick),
            entity.phase,
        );
        if def.is_floor_bound() {
            if let Some(y) = band.floor_y_for(variant) {
                entity.y = y;
            }
            continue;
        }

        if band.clamp_y_for(entity.y, variant) != Some(entity.y)
            && let Some(y) = band.random_y_for(variant, rng)
        {
            entity.y = y;
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum SpawnMode {
    Anywhere,
    Edge,
}

fn spawn_tank_entity(
    definitions: &[CreatureDef],
    def_index: usize,
    copy_index: usize,
    tank: &TankState,
    rng: &mut ThreadRng,
) -> Entity {
    let def = &definitions[def_index];
    let (dx, dy) = def.starting_velocity(rng);
    let (activity, activity_ticks) = def.initial_activity(rng);
    let variant = def.best_variant(dx, 0, def_index + copy_index);
    let max_x = tank
        .width
        .saturating_sub(2)
        .saturating_sub(variant.width)
        .max(1) as i32;
    let max_y = tank
        .height
        .saturating_sub(2)
        .saturating_sub(variant.height)
        .max(1) as i32;
    let x = rng.gen_range(0..=max_x);
    let y = rng.gen_range(0..=max_y);
    let territory = assign_territory(
        def,
        x,
        y,
        TerritoryBounds {
            min_x: 0,
            max_x,
            min_y: 0,
            max_y,
        },
        rng,
    );

    Entity {
        def: def_index,
        x,
        y,
        dx,
        dy,
        phase: rng.gen_range(0..8),
        color: entity_color(definitions, def_index, copy_index, rng),
        respawn_at: None,
        pose_intent: PoseIntent::Lateral,
        lateral_dx: dx,
        depth_swim_ticks: 0,
        school_rearrangements: 0,
        activity,
        activity_ticks,
        idle_move_chance: super::creature::DEFAULT_IDLE_MOVE_CHANCE,
        idle_turn_chance: super::creature::DEFAULT_IDLE_TURN_CHANCE,
        territory,
    }
}

fn spawn_reef_entity(
    definitions: &[CreatureDef],
    def_index: usize,
    copy_index: usize,
    world: &ReefWorld,
    area: Rect,
    mode: SpawnMode,
    rng: &mut ThreadRng,
) -> Entity {
    let def = &definitions[def_index];
    let (mut dx, dy) = def.starting_velocity(rng);
    if dx == 0 && !def.is_floor_bound() {
        dx = if rng.gen_bool(0.5) { -1 } else { 1 };
    }

    let variant = def.best_variant(dx, 0, def_index + copy_index);
    let bounds = world.visible_bounds(area.width);
    let max_x = bounds
        .end
        .saturating_sub(variant.width as i32)
        .max(bounds.start);
    let (x, dx) = match mode {
        SpawnMode::Anywhere => (rng.gen_range(bounds.start..=max_x), dx),
        SpawnMode::Edge => {
            if rng.gen_bool(0.5) {
                (bounds.start, dx.abs().max(1))
            } else {
                (max_x, -dx.abs().max(1))
            }
        }
    };

    let band = WaterBand::for_reef(world, area.height);
    let y = if def.is_floor_bound() {
        band.floor_y_for(variant).unwrap_or(band.top)
    } else {
        band.random_y_for(variant, rng).unwrap_or(band.top)
    };
    let (dx, dy) = if def.is_floor_bound() {
        (0, 0)
    } else {
        (dx, dy)
    };
    let (activity, activity_ticks) = def.initial_activity(rng);
    let (min_y, max_y) = band.y_bounds_for(variant).unwrap_or((band.top, band.top));
    let territory = if def.is_floor_bound() {
        assign_territory(
            def,
            x,
            y,
            TerritoryBounds {
                min_x: bounds.start,
                max_x,
                min_y,
                max_y,
            },
            rng,
        )
    } else {
        None
    };

    Entity {
        def: def_index,
        x,
        y,
        dx,
        dy,
        phase: rng.gen_range(0..8),
        color: entity_color(definitions, def_index, copy_index, rng),
        respawn_at: None,
        pose_intent: PoseIntent::Lateral,
        lateral_dx: dx,
        depth_swim_ticks: 0,
        school_rearrangements: 0,
        activity,
        activity_ticks,
        idle_move_chance: super::creature::DEFAULT_IDLE_MOVE_CHANCE,
        idle_turn_chance: super::creature::DEFAULT_IDLE_TURN_CHANCE,
        territory,
    }
}

fn assign_territory(
    def: &CreatureDef,
    x: i32,
    y: i32,
    bounds: TerritoryBounds,
    rng: &mut ThreadRng,
) -> Option<Territory> {
    let geometry = def.preferences.territory_geometry.as_ref()?;
    let (width, height) = geometry.sample_size(rng);
    Some(Territory {
        min_x: anchored_min(x, width.max(1) as i32, bounds.min_x, bounds.max_x),
        max_x: anchored_max(x, width.max(1) as i32, bounds.min_x, bounds.max_x),
        min_y: anchored_min(y, height.max(1) as i32, bounds.min_y, bounds.max_y),
        max_y: anchored_max(y, height.max(1) as i32, bounds.min_y, bounds.max_y),
    })
}

#[derive(Debug, Clone, Copy)]
struct TerritoryBounds {
    min_x: i32,
    max_x: i32,
    min_y: i32,
    max_y: i32,
}

fn anchored_min(center: i32, size: i32, min: i32, max: i32) -> i32 {
    if min >= max {
        return min;
    }
    let size = size.min(max - min + 1).max(1);
    let start = center - size / 2;
    start.clamp(min, max - size + 1)
}

fn anchored_max(center: i32, size: i32, min: i32, max: i32) -> i32 {
    let start = anchored_min(center, size, min, max);
    start + size.min(max.saturating_sub(min) + 1).max(1) - 1
}

fn entity_color(
    definitions: &[CreatureDef],
    def_index: usize,
    copy_index: usize,
    rng: &mut ThreadRng,
) -> Color {
    let def = &definitions[def_index];
    if !def.colors.is_empty() {
        return def.colors[rng.gen_range(0..def.colors.len())];
    }

    let colors = [
        Color::LightCyan,
        Color::LightBlue,
        Color::LightGreen,
        Color::LightYellow,
        Color::LightMagenta,
        Color::Cyan,
        Color::Green,
        Color::Yellow,
        Color::White,
    ];
    let name_hash = def
        .name
        .bytes()
        .fold(0usize, |hash, byte| hash.wrapping_add(byte as usize));

    colors[(def_index + copy_index + name_hash) % colors.len()]
}

#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;
