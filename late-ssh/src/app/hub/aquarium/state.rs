use std::time::{Duration, Instant};

use anyhow::Result;
use rand::{Rng, rngs::ThreadRng};
use ratatui::{layout::Rect, style::Color};

use super::{
    config::{AppConfig, Mode},
    creature::{
        ActivityState, CreatureDef, Entity, PoseIntent, Territory, Variant, tallest_variant_height,
    },
    world::{ReefWorld, WorldBounds, load_world_layer},
};

const SIMULATION_STEP: Duration = Duration::from_millis(220);

pub struct AquariumState {
    pub(crate) definitions: Vec<CreatureDef>,
    pub(crate) entities: Vec<Entity>,
    pub(crate) tick: u64,
    pub(crate) show_background: bool,
    pub(crate) show_creature_names: bool,
    pub(crate) mode: RuntimeMode,
    last_step_at: Instant,
}

pub enum RuntimeMode {
    Tank(TankState),
    Reef(ReefState),
}

pub struct TankState {
    pub width: u16,
    pub height: u16,
}

pub struct ReefState {
    pub world: ReefWorld,
    pub respawn_delay: Duration,
    pub last_area: Rect,
    pub min_height: u16,
}

#[derive(Debug, Clone, Copy)]
pub struct WaterBand {
    pub top: i32,
    pub bottom: i32,
}

impl WaterBand {
    pub fn for_reef(world: &ReefWorld, terminal_height: u16) -> Self {
        Self {
            top: world.surface.height as i32,
            bottom: terminal_height.saturating_sub(world.floor.height) as i32,
        }
    }

    pub fn random_y_for(&self, variant: &Variant, rng: &mut ThreadRng) -> Option<i32> {
        let max_y = self.bottom - variant.height as i32;
        if max_y < self.top {
            None
        } else {
            Some(rng.gen_range(self.top..=max_y))
        }
    }

    pub fn clamp_y_for(&self, y: i32, variant: &Variant) -> Option<i32> {
        let max_y = self.bottom - variant.height as i32;
        if max_y < self.top {
            None
        } else {
            Some(y.clamp(self.top, max_y))
        }
    }

    pub fn y_bounds_for(&self, variant: &Variant) -> Option<(i32, i32)> {
        let max_y = self.bottom - variant.height as i32;
        if max_y < self.top {
            None
        } else {
            Some((self.top, max_y))
        }
    }

    pub fn floor_y_for(&self, variant: &Variant) -> Option<i32> {
        let y = self.bottom - variant.height as i32;
        if y < self.top { None } else { Some(y) }
    }
}

impl AquariumState {
    pub fn default_for_area(launch_area: Rect) -> Result<Self> {
        Self::new(
            crate::app::hub::aquarium::config::default_config()?,
            crate::app::hub::aquarium::creature::load_default_creatures()?,
            launch_area,
        )
    }

    pub fn new(
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
                let world = ReefWorld::new(
                    surface,
                    floor,
                    launch_area.width,
                    config.reef.horizontal.offscreen_pages,
                );

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
            last_step_at: Instant::now(),
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

    pub fn set_active_creatures(&mut self, active_creatures: &[(String, usize)]) {
        let mut rng = rand::thread_rng();
        let mut entities = Vec::new();
        let mut copy_index = 0;
        for (name, count) in active_creatures {
            let Some(def_index) = self.definitions.iter().position(|def| def.name == *name) else {
                continue;
            };
            for _ in 0..*count {
                let entity = match &self.mode {
                    RuntimeMode::Tank(tank) => {
                        spawn_tank_entity(&self.definitions, def_index, copy_index, tank, &mut rng)
                    }
                    RuntimeMode::Reef(reef) => spawn_reef_entity(
                        &self.definitions,
                        def_index,
                        copy_index,
                        &reef.world,
                        reef.last_area,
                        SpawnMode::Anywhere,
                        &mut rng,
                    ),
                };
                entities.push(entity);
                copy_index += 1;
            }
        }
        self.entities = entities;
    }

    pub fn tick(&mut self) {
        let now = Instant::now();
        if now.saturating_duration_since(self.last_step_at) < SIMULATION_STEP {
            return;
        }

        self.last_step_at = now;
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
                    entity.tick_bounded(def, bounds, variant, self.tick, &mut rng);
                }
            }
            RuntimeMode::Reef(reef) => tick_reef(
                &self.definitions,
                &mut self.entities,
                self.tick,
                reef,
                &mut rng,
            ),
        }
    }

    pub fn handle_resize(&mut self, width: u16, height: u16) {
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
}

fn scaled_initial_count(count: usize, scale: f64) -> usize {
    if scale <= 0.0 {
        0
    } else {
        ((count as f64) * scale).round().min(usize::MAX as f64) as usize
    }
}

fn tick_reef(
    definitions: &[CreatureDef],
    entities: &mut [Entity],
    tick: u64,
    reef: &mut ReefState,
    rng: &mut ThreadRng,
) {
    if reef.last_area.height < reef.min_height {
        return;
    }

    let now = Instant::now();
    let bounds = reef.world.simulated_bounds(reef.last_area.width);
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
        update_reef_motion(def, entity, &band, motion_variant, tick, rng);

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

    apply_depth_bias(def, entity, band, variant, rng);
    apply_territory_bias(def, entity, rng);
}

fn apply_depth_bias(
    def: &CreatureDef,
    entity: &mut Entity,
    band: &WaterBand,
    variant: &Variant,
    rng: &mut ThreadRng,
) {
    if def.four_way_swimmer || def.is_floor_bound() {
        return;
    }
    let Some((min_y, max_y)) = band.y_bounds_for(variant) else {
        return;
    };
    if min_y >= max_y {
        return;
    }

    let preferences = &def.preferences;
    let mut target = preferences.depth;
    target += preferences.demersal * (1.0 - target) * 0.4;
    target -= preferences.reefer * target * 0.25;
    target = target.clamp(0.0, 1.0);

    let target_y = min_y + ((max_y - min_y) as f64 * target).round() as i32;
    let distance = target_y - entity.y;
    if distance.abs() <= 1 {
        if rng.gen_bool((preferences.sedentary * 0.15).clamp(0.0, 0.5)) {
            entity.dy = 0;
        }
        return;
    }

    let preference_strength = (preferences
        .demersal
        .max(preferences.reefer)
        .max((preferences.depth - 0.5).abs() * 2.0)
        * 0.35
        + 0.08)
        .clamp(0.0, 0.6);
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
    let bounds = world.simulated_bounds(area.width);
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
    let territory = assign_territory(
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
