use std::{
    collections::HashMap,
    fs,
    path::Path,
    str::FromStr,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, anyhow};
use kdl::{KdlDocument, KdlNode, KdlValue};
use rand::{Rng, rngs::ThreadRng};
use ratatui::{layout::Rect, style::Color};

use super::kdl_parse;

#[derive(Debug, Clone, Copy)]
struct EmbeddedKdl {
    path: &'static str,
    source: &'static str,
}

const DEFAULT_KINDOM_SOURCES: &[EmbeddedKdl] = &[
    EmbeddedKdl {
        path: "art/creatures/defaults/animal.kindom.kdl",
        source: include_str!("../../../../assets/aquarium/creatures/defaults/animal.kindom.kdl"),
    },
    EmbeddedKdl {
        path: "art/creatures/defaults/bacteria.kindom.kdl",
        source: include_str!("../../../../assets/aquarium/creatures/defaults/bacteria.kindom.kdl"),
    },
    EmbeddedKdl {
        path: "art/creatures/defaults/fungi.kindom.kdl",
        source: include_str!("../../../../assets/aquarium/creatures/defaults/fungi.kindom.kdl"),
    },
    EmbeddedKdl {
        path: "art/creatures/defaults/plant.kindom.kdl",
        source: include_str!("../../../../assets/aquarium/creatures/defaults/plant.kindom.kdl"),
    },
    EmbeddedKdl {
        path: "art/creatures/defaults/unalive.kindom.kdl",
        source: include_str!("../../../../assets/aquarium/creatures/defaults/unalive.kindom.kdl"),
    },
];

const DEFAULT_CREATURE_SOURCES: &[EmbeddedKdl] = &[
    EmbeddedKdl {
        path: "art/creatures/anchovy.kdl",
        source: include_str!("../../../../assets/aquarium/creatures/anchovy.kdl"),
    },
    EmbeddedKdl {
        path: "art/creatures/bee.kdl",
        source: include_str!("../../../../assets/aquarium/creatures/bee.kdl"),
    },
    EmbeddedKdl {
        path: "art/creatures/bertrand.kdl",
        source: include_str!("../../../../assets/aquarium/creatures/bertrand.kdl"),
    },
    EmbeddedKdl {
        path: "art/creatures/bigbert.kdl",
        source: include_str!("../../../../assets/aquarium/creatures/bigbert.kdl"),
    },
    EmbeddedKdl {
        path: "art/creatures/boxfish.kdl",
        source: include_str!("../../../../assets/aquarium/creatures/boxfish.kdl"),
    },
    EmbeddedKdl {
        path: "art/creatures/bumble.kdl",
        source: include_str!("../../../../assets/aquarium/creatures/bumble.kdl"),
    },
    EmbeddedKdl {
        path: "art/creatures/clownfish.kdl",
        source: include_str!("../../../../assets/aquarium/creatures/clownfish.kdl"),
    },
    EmbeddedKdl {
        path: "art/creatures/diamondfish.kdl",
        source: include_str!("../../../../assets/aquarium/creatures/diamondfish.kdl"),
    },
    EmbeddedKdl {
        path: "art/creatures/finnegan.kdl",
        source: include_str!("../../../../assets/aquarium/creatures/finnegan.kdl"),
    },
    EmbeddedKdl {
        path: "art/creatures/floata.kdl",
        source: include_str!("../../../../assets/aquarium/creatures/floata.kdl"),
    },
    EmbeddedKdl {
        path: "art/creatures/jellybean.kdl",
        source: include_str!("../../../../assets/aquarium/creatures/jellybean.kdl"),
    },
    EmbeddedKdl {
        path: "art/creatures/mj.kdl",
        source: include_str!("../../../../assets/aquarium/creatures/mj.kdl"),
    },
    EmbeddedKdl {
        path: "art/creatures/oldskool.kdl",
        source: include_str!("../../../../assets/aquarium/creatures/oldskool.kdl"),
    },
    EmbeddedKdl {
        path: "art/creatures/pufferfish.kdl",
        source: include_str!("../../../../assets/aquarium/creatures/pufferfish.kdl"),
    },
    EmbeddedKdl {
        path: "art/creatures/rugbert.kdl",
        source: include_str!("../../../../assets/aquarium/creatures/rugbert.kdl"),
    },
    EmbeddedKdl {
        path: "art/creatures/seahorse.kdl",
        source: include_str!("../../../../assets/aquarium/creatures/seahorse.kdl"),
    },
    EmbeddedKdl {
        path: "art/creatures/squeeb.kdl",
        source: include_str!("../../../../assets/aquarium/creatures/squeeb.kdl"),
    },
    EmbeddedKdl {
        path: "art/creatures/squigs.kdl",
        source: include_str!("../../../../assets/aquarium/creatures/squigs.kdl"),
    },
    EmbeddedKdl {
        path: "art/creatures/tiger.kdl",
        source: include_str!("../../../../assets/aquarium/creatures/tiger.kdl"),
    },
    EmbeddedKdl {
        path: "art/creatures/wigglewort.kdl",
        source: include_str!("../../../../assets/aquarium/creatures/wigglewort.kdl"),
    },
    EmbeddedKdl {
        path: "art/creatures/wingfish.kdl",
        source: include_str!("../../../../assets/aquarium/creatures/wingfish.kdl"),
    },
];

pub(crate) const IDLE_ACTION_INTERVAL: u64 = 4;
pub(crate) const DEFAULT_IDLE_MOVE_CHANCE: f64 = 0.30;
pub(crate) const DEFAULT_IDLE_TURN_CHANCE: f64 = 0.05;
pub(crate) const IDLE_CHANCE_STEP: f64 = 0.05;

#[derive(Debug, Clone)]
pub(crate) struct CreatureDef {
    pub name: String,
    pub kindom: Kindom,
    pub constraints: CreatureConstraints,
    pub preferences: CreaturePreferences,
    pub variants: Vec<Variant>,
    pub count: usize,
    pub four_way_swimmer: bool,
    pub spawn_location: SpawnLocation,
    h_velocity: Option<i16>,
    v_velocity: Option<i16>,
    pub brownian: bool,
    pub colors: Vec<Color>,
    default_movement: bool,
    school_rearrange_chance: Option<f64>,
}

impl CreatureDef {
    pub(crate) fn best_variant(&self, dx: i16, tick: u64, phase: usize) -> &Variant {
        self.best_variant_for(dx, PoseIntent::Lateral, tick, phase)
    }

    pub(crate) fn best_variant_for(
        &self,
        dx: i16,
        pose_intent: PoseIntent,
        tick: u64,
        phase: usize,
    ) -> &Variant {
        for wanted in self.pose_preferences(dx, pose_intent) {
            let matching = self
                .variants
                .iter()
                .filter(|variant| pose_matches(&variant.pose, wanted))
                .collect::<Vec<_>>();
            if !matching.is_empty() {
                return matching[(tick as usize / 3 + phase) % matching.len()];
            }
        }

        let face = self
            .variants
            .iter()
            .filter(|variant| variant.pose.starts_with("face"))
            .collect::<Vec<_>>();
        if !face.is_empty() {
            return face[(tick as usize / 3 + phase) % face.len()];
        }
        &self.variants[(tick as usize / 3 + phase) % self.variants.len()]
    }

    fn pose_preferences(&self, dx: i16, pose_intent: PoseIntent) -> &'static [&'static str] {
        match pose_intent {
            PoseIntent::Face => &["face"],
            PoseIntent::FaceAway => &["face-away"],
            PoseIntent::Lateral => {
                if dx < 0 {
                    if self.has_pose("left-drag") {
                        &["left-drag", "left"]
                    } else {
                        &["left"]
                    }
                } else if dx > 0 {
                    if self.has_pose("right-drag") {
                        &["right-drag", "right"]
                    } else {
                        &["right"]
                    }
                } else {
                    &["face"]
                }
            }
        }
    }

    pub(crate) fn has_motion_drag_poses(&self) -> bool {
        self.has_pose("left-drag") || self.has_pose("right-drag")
    }

    fn has_pose(&self, pose: &str) -> bool {
        has_pose(&self.variants, pose)
    }

    pub(crate) fn starting_velocity(&self, rng: &mut ThreadRng) -> (i16, i16) {
        let dx = self.h_velocity.unwrap_or_else(|| {
            let has_left = self
                .variants
                .iter()
                .any(|variant| variant.pose.starts_with("left"));
            let has_right = self
                .variants
                .iter()
                .any(|variant| variant.pose.starts_with("right"));

            match (has_left, has_right) {
                (true, true) => {
                    if rng.gen_bool(0.5) {
                        -1
                    } else {
                        1
                    }
                }
                (true, false) => -1,
                (false, true) => 1,
                (false, false) => {
                    if rng.gen_bool(0.5) {
                        -1
                    } else {
                        1
                    }
                }
            }
        });

        let dy = self.v_velocity.unwrap_or_else(|| {
            if self.brownian || rng.gen_bool(0.35) {
                rng.gen_range(-1..=1)
            } else {
                0
            }
        });

        (dx, dy)
    }

    pub(crate) fn uses_default_movement(&self) -> bool {
        self.default_movement
    }

    pub(crate) fn school_rearrange_chance(&self) -> Option<f64> {
        self.school_rearrange_chance
    }

    pub(crate) fn is_floor_bound(&self) -> bool {
        self.spawn_location == SpawnLocation::Floor
            || self
                .constraints
                .sessile
                .as_ref()
                .is_some_and(|sessile| sessile.to == "floor")
    }

    pub(crate) fn is_sessile(&self) -> bool {
        self.constraints.sessile.is_some()
    }

    pub(crate) fn initial_activity(&self, rng: &mut ThreadRng) -> (ActivityState, u16) {
        let idle_chance = (self.preferences.sedentary * 0.65
            + self.preferences.planktonic * 0.2
            + kindom_stillness(self.kindom) * 0.15
            - self.preferences.nektonic * 0.35)
            .clamp(0.05, 0.9);
        let state = if self.is_sessile() || rng.gen_bool(idle_chance) {
            ActivityState::Idle
        } else {
            ActivityState::Active
        };

        (state, self.activity_duration(state, rng))
    }

    pub(crate) fn next_activity(
        &self,
        current: ActivityState,
        rng: &mut ThreadRng,
    ) -> (ActivityState, u16) {
        let next = match current {
            ActivityState::Active => ActivityState::Idle,
            ActivityState::Idle => ActivityState::Active,
        };

        (next, self.activity_duration(next, rng))
    }

    fn activity_duration(&self, state: ActivityState, rng: &mut ThreadRng) -> u16 {
        let stillness = (self.preferences.sedentary
            + self.preferences.planktonic
            + kindom_stillness(self.kindom))
            / 3.0;
        let activity = self.preferences.nektonic;
        let (min, max) = match state {
            ActivityState::Active => {
                let min = lerp_u16(10, 26, activity);
                let max = lerp_u16(28, 92, (activity - stillness * 0.35).clamp(0.0, 1.0));
                (min, max.max(min))
            }
            ActivityState::Idle => {
                let min = lerp_u16(10, 44, stillness);
                let max = lerp_u16(24, 176, (stillness - activity * 0.25).clamp(0.0, 1.0));
                (min, max.max(min))
            }
        };

        rng.gen_range(min..=max)
    }
}

pub(crate) fn default_movement_transition_chance() -> f64 {
    1.0 - 0.5_f64.powf(1.0 / 200.0)
}

fn kindom_stillness(kindom: Kindom) -> f64 {
    match kindom {
        Kindom::Animal => 0.0,
        Kindom::Bacteria => 0.35,
        Kindom::Plant | Kindom::Fungi | Kindom::Unalive => 1.0,
    }
}

fn lerp_u16(min: u16, max: u16, t: f64) -> u16 {
    let t = t.clamp(0.0, 1.0);
    (min as f64 + (max.saturating_sub(min)) as f64 * t).round() as u16
}

#[derive(Debug, Clone)]
pub(crate) struct Variant {
    pub pose: String,
    pub art: Vec<String>,
    pub width: u16,
    pub height: u16,
    pub school: Option<School>,
}

#[derive(Debug, Clone)]
pub(crate) struct School {
    pub unit: String,
    pub units: Vec<SchoolUnit>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct SchoolUnit {
    pub x: u16,
    pub y: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PoseIntent {
    Lateral,
    Face,
    FaceAway,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SpawnLocation {
    Water,
    Floor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ActivityState {
    Active,
    Idle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub(crate) enum Kindom {
    #[default]
    Animal,
    Plant,
    Bacteria,
    Fungi,
    Unalive,
}

impl Kindom {
    fn as_str(self) -> &'static str {
        match self {
            Self::Animal => "animal",
            Self::Plant => "plant",
            Self::Bacteria => "bacteria",
            Self::Fungi => "fungi",
            Self::Unalive => "unalive",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct CreatureConstraints {
    pub sessile: Option<SessileConstraint>,
    pub walker: bool,
    pub obligate_airbreather: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SessileConstraint {
    pub attach: String,
    pub to: String,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CreaturePreferences {
    pub demersal: f64,
    pub depth: f64,
    pub reefer: f64,
    pub nektonic: f64,
    pub planktonic: f64,
    pub territorial: f64,
    pub territory_geometry: Option<TerritoryGeometry>,
    pub sociality: f64,
    pub schooler: f64,
    pub kinophile: f64,
    pub kinophobe: f64,
    pub sedentary: f64,
    pub flighty: f64,
}

impl Default for CreaturePreferences {
    fn default() -> Self {
        Self {
            demersal: 0.0,
            depth: 0.5,
            reefer: 0.0,
            nektonic: 0.0,
            planktonic: 0.0,
            territorial: 0.0,
            territory_geometry: None,
            sociality: 0.0,
            schooler: 0.0,
            kinophile: 0.5,
            kinophobe: 0.0,
            sedentary: 0.0,
            flighty: 0.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TerritoryGeometry {
    Rectangle(RectangleTerritoryGeometry),
}

impl TerritoryGeometry {
    pub(crate) fn sample_size(&self, rng: &mut ThreadRng) -> (u16, u16) {
        match self {
            Self::Rectangle(rectangle) => {
                (rectangle.width.sample(rng), rectangle.height.sample(rng))
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RectangleTerritoryGeometry {
    pub width: DimensionSpec,
    pub height: DimensionSpec,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DimensionSpec {
    Constant(u16),
    Uniform { min: u16, max: u16 },
}

impl DimensionSpec {
    fn sample(&self, rng: &mut ThreadRng) -> u16 {
        match *self {
            Self::Constant(value) => value,
            Self::Uniform { min, max } => rng.gen_range(min..=max),
        }
    }
}

impl Variant {
    fn from_kdl_node(pose: String, art: &str, unit: Option<&str>, unit_brownian: bool) -> Self {
        let art = art
            .trim_matches('\n')
            .lines()
            .map(|line| line.trim_end_matches('\r').to_string())
            .collect::<Vec<_>>();
        let width = art
            .iter()
            .map(|line| line.chars().count())
            .max()
            .unwrap_or_default()
            .min(u16::MAX as usize) as u16;
        let height = art.len().min(u16::MAX as usize) as u16;

        Self {
            pose,
            school: unit
                .filter(|unit| unit_brownian && !unit.is_empty())
                .map(|unit| School::from_art(unit, &art)),
            art,
            width,
            height,
        }
    }
}

impl School {
    fn from_art(unit: &str, art: &[String]) -> Self {
        let units = art
            .iter()
            .enumerate()
            .flat_map(|(row, line)| unit_positions(line, unit).map(move |column| (column, row)))
            .filter_map(|(column, row)| {
                let x = u16::try_from(column).ok()?;
                let y = u16::try_from(row).ok()?;
                Some(SchoolUnit { x, y })
            })
            .collect();

        Self {
            unit: unit.to_string(),
            units,
        }
    }
}

fn unit_positions<'a>(line: &'a str, unit: &'a str) -> impl Iterator<Item = usize> + 'a {
    line.match_indices(unit)
        .map(|(byte_index, _)| line[..byte_index].chars().count())
}

#[derive(Debug)]
pub(crate) struct Entity {
    pub def: usize,
    pub x: i32,
    pub y: i32,
    pub dx: i16,
    pub dy: i16,
    pub phase: usize,
    pub color: Color,
    pub respawn_at: Option<Instant>,
    pub pose_intent: PoseIntent,
    pub lateral_dx: i16,
    pub depth_swim_ticks: u8,
    pub school_rearrangements: u64,
    pub activity: ActivityState,
    pub activity_ticks: u16,
    pub idle_move_chance: f64,
    pub idle_turn_chance: f64,
    pub territory: Option<Territory>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Territory {
    pub min_x: i32,
    pub max_x: i32,
    pub min_y: i32,
    pub max_y: i32,
}

impl Entity {
    pub(crate) fn tick_bounded(
        &mut self,
        def: &CreatureDef,
        bounds: Rect,
        variant: &Variant,
        tick: u64,
        rng: &mut ThreadRng,
    ) {
        let was_idle = self.activity == ActivityState::Idle;
        self.advance_activity(def, rng);
        if self.activity == ActivityState::Idle {
            self.update_idle_motion(tick, rng);
            return;
        }
        if was_idle && self.dx == 0 {
            self.resume_lateral_motion();
        }

        if def.brownian && rng.gen_bool(0.25) {
            self.dx = rng.gen_range(-1..=1);
            self.dy = rng.gen_range(-1..=1);
        } else if def.uses_default_movement() && rng.gen_bool(default_movement_transition_chance())
        {
            self.toggle_vertical_motion(rng);
        }

        let max_x = (bounds.width.saturating_sub(variant.width).max(1) - 1) as i32;
        let max_y = (bounds.height.saturating_sub(variant.height).max(1) - 1) as i32;

        self.x += self.dx as i32;
        self.y += self.dy as i32;

        if self.x <= 0 {
            self.x = 0;
            self.dx = self.dx.abs().max(1);
        } else if self.x >= max_x {
            self.x = max_x;
            self.dx = -self.dx.abs().max(1);
        }

        if self.y <= 0 {
            self.y = 0;
            self.dy = self.dy.abs();
        } else if self.y >= max_y {
            self.y = max_y;
            self.dy = -self.dy.abs();
        }
    }

    pub(crate) fn is_active(&self) -> bool {
        self.respawn_at.is_none()
    }

    pub(crate) fn mark_exited(&mut self, delay: Duration, now: Instant) {
        self.respawn_at = Some(now + delay);
    }

    pub(crate) fn resume_lateral_motion(&mut self) {
        if self.lateral_dx == 0 {
            self.lateral_dx = if self.dx < 0 { -1 } else { 1 };
        }

        self.dx = self.lateral_dx;
        self.dy = 0;
        self.pose_intent = PoseIntent::Lateral;
        self.depth_swim_ticks = 0;
    }

    pub(crate) fn pose_dx(&self) -> i16 {
        if self.dx != 0 {
            self.dx
        } else if self.activity == ActivityState::Idle {
            self.facing_dx()
        } else {
            0
        }
    }

    pub(crate) fn pose_dx_for(&self, def: &CreatureDef) -> i16 {
        if self.dx != 0 {
            self.dx
        } else if self.activity == ActivityState::Idle && def.has_motion_drag_poses() {
            0
        } else {
            self.pose_dx()
        }
    }

    pub(crate) fn animation_tick(&self, tick: u64) -> u64 {
        if self.activity == ActivityState::Idle {
            0
        } else {
            tick
        }
    }

    pub(crate) fn animation_tick_for(&self, def: &CreatureDef, tick: u64) -> u64 {
        if self.activity == ActivityState::Idle && def.is_sessile() {
            tick
        } else {
            self.animation_tick(tick)
        }
    }

    pub(crate) fn update_idle_motion(&mut self, tick: u64, rng: &mut ThreadRng) {
        self.dy = 0;
        self.pose_intent = PoseIntent::Lateral;

        if self.lateral_dx == 0 {
            self.lateral_dx = if rng.gen_bool(0.5) { -1 } else { 1 };
        }

        if !tick.is_multiple_of(IDLE_ACTION_INTERVAL) {
            self.dx = 0;
            return;
        }

        if rng.gen_bool(self.idle_turn_chance.clamp(0.0, 1.0)) {
            self.lateral_dx = -self.facing_dx();
            self.dx = 0;
            self.reset_idle_chances();
        } else if rng.gen_bool(self.idle_move_chance.clamp(0.0, 1.0)) {
            self.dx = self.facing_dx();
            self.idle_move_chance = (self.idle_move_chance - IDLE_CHANCE_STEP).max(0.0);
            self.idle_turn_chance = (self.idle_turn_chance + IDLE_CHANCE_STEP).min(1.0);
        } else {
            self.dx = 0;
        }
    }

    fn facing_dx(&self) -> i16 {
        if self.lateral_dx < 0 { -1 } else { 1 }
    }

    fn reset_idle_chances(&mut self) {
        self.idle_move_chance = DEFAULT_IDLE_MOVE_CHANCE;
        self.idle_turn_chance = DEFAULT_IDLE_TURN_CHANCE;
    }

    pub(crate) fn toggle_vertical_motion(&mut self, rng: &mut ThreadRng) {
        self.dy = if self.dy == 0 {
            if rng.gen_bool(0.5) { -1 } else { 1 }
        } else {
            0
        };
    }

    pub(crate) fn maybe_rearrange_school(&mut self, def: &CreatureDef, rng: &mut ThreadRng) {
        if let Some(chance) = def.school_rearrange_chance()
            && rng.gen_bool(chance)
        {
            self.school_rearrangements = self.school_rearrangements.wrapping_add(1);
        }
    }

    pub(crate) fn advance_activity(&mut self, def: &CreatureDef, rng: &mut ThreadRng) {
        if def.is_sessile() {
            self.activity = ActivityState::Idle;
            self.activity_ticks = self.activity_ticks.max(1);
            return;
        }

        self.activity_ticks = self.activity_ticks.saturating_sub(1);
        if self.activity_ticks == 0 {
            let (activity, ticks) = def.next_activity(self.activity, rng);
            self.activity = activity;
            self.activity_ticks = ticks.max(1);
        }
    }
}

#[derive(Debug, Clone)]
struct CreatureTemplate {
    kindom: Kindom,
    constraints: CreatureConstraints,
    preferences: CreaturePreferences,
    count: Option<usize>,
    motion: Option<String>,
    unit_motion: Option<UnitMotionTemplate>,
    h_velocity: Option<i16>,
    v_velocity: Option<i16>,
    spawn_location: Option<SpawnLocation>,
    colors: Option<Vec<Color>>,
}

impl CreatureTemplate {
    fn new(kindom: Kindom) -> Self {
        Self {
            kindom,
            constraints: CreatureConstraints::default(),
            preferences: CreaturePreferences::default(),
            count: None,
            motion: None,
            unit_motion: None,
            h_velocity: None,
            v_velocity: None,
            spawn_location: None,
            colors: None,
        }
    }

    fn apply_doc(&mut self, doc: &KdlDocument, path: &Path) -> Result<()> {
        if let Some(kindom) = doc_kindom(doc, path)? {
            self.kindom = kindom;
        }
        if let Some(count) = doc_int_arg(doc, "count").and_then(|value| value.try_into().ok()) {
            self.count = Some(count);
        }
        if let Some(motion) = doc_string_arg(doc, "motion") {
            self.motion = Some(motion);
        }
        if let Some(unit_motion) = doc.get("unit-motion") {
            self.unit_motion = Some(parse_unit_motion(unit_motion)?);
        }
        if let Some(h_velocity) = doc_int_arg(doc, "h-velocity") {
            self.h_velocity = Some(clamp_velocity(h_velocity));
        }
        if let Some(v_velocity) = doc_int_arg(doc, "v-velocity") {
            self.v_velocity = Some(clamp_velocity(v_velocity));
        }
        if let Some(spawn_location) = doc_string_arg(doc, "spawn-location") {
            self.spawn_location = Some(parse_spawn_location(&spawn_location, path)?);
        }
        if doc.get("colors").is_some() {
            self.colors = Some(parse_colors(doc, path)?);
        }
        if let Some(constraints) = doc.get("constraints") {
            self.constraints = parse_constraints(constraints, path)?;
        }
        if let Some(preferences) = doc.get("preferences") {
            self.preferences.apply_node(preferences, path)?;
        }

        Ok(())
    }
}

#[derive(Debug, Clone)]
struct UnitMotionTemplate {
    brownian: bool,
    rearrange_chance: Option<f64>,
}

#[derive(Debug, Default)]
struct KindomDefaults {
    templates: HashMap<Kindom, CreatureTemplate>,
}

impl KindomDefaults {
    fn get(&self, kindom: Kindom) -> Option<&CreatureTemplate> {
        self.templates.get(&kindom)
    }
}

fn load_kindom_defaults(creature_dir: &Path) -> Result<KindomDefaults> {
    let defaults_dir = creature_dir.join("defaults");
    let entries = match fs::read_dir(&defaults_dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(KindomDefaults::default());
        }
        Err(error) => {
            return Err(error)
                .with_context(|| format!("reading kindom defaults {}", defaults_dir.display()));
        }
    };

    let mut defaults = KindomDefaults::default();
    for entry in entries {
        let path = entry?.path();
        if !is_kindom_default_file(&path) {
            continue;
        }

        let doc = load_kdl_document(&path)?;
        let kindom = default_file_kindom(&path)?;
        if let Some(doc_kindom) = doc_kindom(&doc, &path)?
            && doc_kindom != kindom
        {
            return Err(anyhow!(
                "{} declares kindom {}, but its filename is for kindom {}",
                path.display(),
                doc_kindom.as_str(),
                kindom.as_str()
            ));
        }

        let mut template = CreatureTemplate::new(kindom);
        template.apply_doc(&doc, &path)?;
        defaults.templates.insert(kindom, template);
    }

    Ok(defaults)
}

fn load_embedded_kindom_defaults() -> Result<KindomDefaults> {
    let mut defaults = KindomDefaults::default();
    for source in DEFAULT_KINDOM_SOURCES {
        let path = Path::new(source.path);
        let doc = kdl_parse::parse_document(path, source.source)?;
        let kindom = default_file_kindom(path)?;
        if let Some(doc_kindom) = doc_kindom(&doc, path)?
            && doc_kindom != kindom
        {
            return Err(anyhow!(
                "{} declares kindom {}, but its filename is for kindom {}",
                path.display(),
                doc_kindom.as_str(),
                kindom.as_str()
            ));
        }

        let mut template = CreatureTemplate::new(kindom);
        template.apply_doc(&doc, path)?;
        defaults.templates.insert(kindom, template);
    }

    Ok(defaults)
}

pub(crate) fn load_default_creatures() -> Result<Vec<CreatureDef>> {
    let defaults = load_embedded_kindom_defaults()?;
    let creatures = DEFAULT_CREATURE_SOURCES
        .iter()
        .map(|source| load_creature_from_source(source.path, source.source, &defaults))
        .collect::<Result<Vec<_>>>()?;

    if creatures.is_empty() {
        Err(anyhow!("no embedded aquarium creatures found"))
    } else {
        Ok(creatures)
    }
}

#[allow(dead_code)]
pub(crate) fn load_creatures(dir: &Path) -> Result<Vec<CreatureDef>> {
    let defaults = load_kindom_defaults(dir)?;
    let mut paths = fs::read_dir(dir)
        .with_context(|| format!("reading creature directory {}", dir.display()))?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    paths.sort();

    let creatures = paths
        .into_iter()
        .filter(|path| is_loadable_creature_file(path))
        .map(|path| load_creature_with_defaults(&path, &defaults))
        .collect::<Result<Vec<_>>>()?;

    if creatures.is_empty() {
        Err(anyhow!("no .kdl creatures found in {}", dir.display()))
    } else {
        Ok(creatures)
    }
}

fn load_creature_from_source(
    path: &'static str,
    source: &'static str,
    defaults: &KindomDefaults,
) -> Result<CreatureDef> {
    let path = Path::new(path);
    let doc = kdl_parse::parse_document(path, source)?;
    build_creature_from_doc(path, doc, defaults)
}

#[allow(dead_code)]
pub(crate) fn load_creature(path: &Path) -> Result<CreatureDef> {
    let defaults = path
        .parent()
        .map(load_kindom_defaults)
        .transpose()?
        .unwrap_or_default();

    load_creature_with_defaults(path, &defaults)
}

fn load_creature_with_defaults(path: &Path, defaults: &KindomDefaults) -> Result<CreatureDef> {
    let doc = load_kdl_document(path)?;
    build_creature_from_doc(path, doc, defaults)
}

fn build_creature_from_doc(
    path: &Path,
    doc: KdlDocument,
    defaults: &KindomDefaults,
) -> Result<CreatureDef> {
    let kindom = doc_kindom(&doc, path)?.unwrap_or_default();
    let mut template = defaults
        .get(kindom)
        .cloned()
        .unwrap_or_else(|| CreatureTemplate::new(kindom));
    template.apply_doc(&doc, path)?;

    let fallback_name = path
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or("creature")
        .to_string();

    let name = doc_string_arg(&doc, "name").unwrap_or(fallback_name);
    let brownian = template
        .motion
        .as_deref()
        .is_some_and(|motion| motion == "brownian");
    let unit_brownian = template
        .unit_motion
        .as_ref()
        .is_some_and(|motion| motion.brownian);
    let school_rearrange_chance = if unit_brownian {
        Some(
            template
                .unit_motion
                .as_ref()
                .and_then(|motion| motion.rearrange_chance)
                .unwrap_or(0.33),
        )
    } else {
        None
    };
    let h_velocity = template.h_velocity;
    let v_velocity = template.v_velocity;
    let spawn_location = template.spawn_location.unwrap_or(SpawnLocation::Water);
    let count = template.count.unwrap_or(1);
    let colors = template.colors.unwrap_or_default();
    let default_movement = template.motion.is_none()
        && template.h_velocity.is_none()
        && template.v_velocity.is_none()
        && spawn_location == SpawnLocation::Water;
    let variants = doc
        .nodes()
        .iter()
        .filter_map(|node| {
            let pose = node.name().value();
            if !is_pose_node(pose) {
                return None;
            }
            let art = node.get(0)?.as_string()?;
            let unit = node.get("unit").and_then(KdlValue::as_string);
            Some(Variant::from_kdl_node(
                pose.to_string(),
                art,
                unit,
                unit_brownian,
            ))
        })
        .collect::<Vec<_>>();

    if variants.is_empty() {
        return Err(anyhow!("{} has no drawable pose nodes", path.display()));
    }
    let four_way_swimmer = has_pose(&variants, "left")
        && has_pose(&variants, "right")
        && has_pose(&variants, "face")
        && has_pose(&variants, "face-away");

    Ok(CreatureDef {
        name,
        kindom: template.kindom,
        constraints: template.constraints,
        preferences: template.preferences,
        variants,
        count,
        four_way_swimmer,
        spawn_location,
        h_velocity,
        v_velocity,
        brownian,
        colors,
        default_movement,
        school_rearrange_chance,
    })
}

pub(crate) fn tallest_variant_height(definitions: &[CreatureDef]) -> u16 {
    definitions
        .iter()
        .flat_map(|definition| &definition.variants)
        .map(|variant| variant.height)
        .max()
        .unwrap_or_default()
}

fn is_pose_node(name: &str) -> bool {
    ["left", "right", "face", "away"]
        .iter()
        .any(|prefix| name.starts_with(prefix))
}

fn has_pose(variants: &[Variant], pose: &str) -> bool {
    variants
        .iter()
        .any(|variant| pose_matches(&variant.pose, pose))
}

fn pose_matches(pose: &str, wanted: &str) -> bool {
    if pose == wanted {
        return true;
    }

    pose.strip_prefix(wanted)
        .is_some_and(|suffix| !suffix.is_empty() && suffix.chars().all(|ch| ch.is_ascii_digit()))
}

#[allow(dead_code)]
fn is_loadable_creature_file(path: &Path) -> bool {
    path.extension().is_some_and(|extension| extension == "kdl")
        && !path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(".noload.kdl"))
}

fn is_kindom_default_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.ends_with(".kindom.kdl") && !name.ends_with(".noload.kdl"))
}

fn load_kdl_document(path: &Path) -> Result<KdlDocument> {
    let source = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    kdl_parse::parse_document(path, &source)
}

fn default_file_kindom(path: &Path) -> Result<Kindom> {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.strip_suffix(".kindom.kdl"))
        .ok_or_else(|| anyhow!("{} is not a kindom default file", path.display()))?;
    parse_kindom(name, path)
}

fn doc_kindom(doc: &KdlDocument, path: &Path) -> Result<Option<Kindom>> {
    doc_string_arg(doc, "kindom")
        .map(|kindom| parse_kindom(&kindom, path))
        .transpose()
}

fn parse_kindom(value: &str, path: &Path) -> Result<Kindom> {
    match value {
        "animal" => Ok(Kindom::Animal),
        "plant" => Ok(Kindom::Plant),
        "bacteria" => Ok(Kindom::Bacteria),
        "fungi" => Ok(Kindom::Fungi),
        "unalive" => Ok(Kindom::Unalive),
        other => Err(anyhow!(
            "{} has unsupported kindom {other:?}",
            path.display()
        )),
    }
}

fn doc_string_arg(doc: &KdlDocument, node_name: &str) -> Option<String> {
    doc.get(node_name)
        .and_then(|node| node.get(0))
        .and_then(KdlValue::as_string)
        .map(ToOwned::to_owned)
}

fn doc_int_arg(doc: &KdlDocument, node_name: &str) -> Option<i128> {
    doc.get(node_name)
        .and_then(|node| node.get(0))
        .and_then(KdlValue::as_integer)
}

fn parse_colors(doc: &KdlDocument, path: &Path) -> Result<Vec<Color>> {
    let Some(node) = doc.get("colors") else {
        return Ok(Vec::new());
    };

    node.entries()
        .iter()
        .filter(|entry| entry.name().is_none())
        .map(|entry| {
            let value = entry
                .value()
                .as_string()
                .ok_or_else(|| anyhow!("{} `colors` entries must be strings", path.display()))?;
            Color::from_str(value)
                .map_err(|_| anyhow!("{} has unsupported color {value:?}", path.display()))
        })
        .collect()
}

fn parse_spawn_location(value: &str, path: &Path) -> Result<SpawnLocation> {
    match value {
        "floor" => Ok(SpawnLocation::Floor),
        "water" => Ok(SpawnLocation::Water),
        other => Err(anyhow!(
            "{} has unsupported spawn-location {other:?}",
            path.display()
        )),
    }
}

fn parse_unit_motion(node: &KdlNode) -> Result<UnitMotionTemplate> {
    let brownian = node
        .get(0)
        .and_then(KdlValue::as_string)
        .is_some_and(|motion| motion == "brownian");
    let rearrange_chance = if brownian {
        optional_probability_prop(node, "rearrange-chance")?
    } else {
        None
    };

    Ok(UnitMotionTemplate {
        brownian,
        rearrange_chance,
    })
}

fn parse_constraints(node: &KdlNode, path: &Path) -> Result<CreatureConstraints> {
    let mut constraints = CreatureConstraints::default();
    let Some(children) = node.children() else {
        return Ok(constraints);
    };

    for child in children.nodes() {
        match child.name().value() {
            "sessile" => {
                constraints.sessile = Some(SessileConstraint {
                    attach: required_string_prop(child, "attach", path)?.to_string(),
                    to: required_string_prop(child, "to", path)?.to_string(),
                });
            }
            "walker" => constraints.walker = true,
            "airbreathing" | "obligate-airbreather" => {
                constraints.obligate_airbreather = true;
            }
            other => {
                return Err(anyhow!(
                    "{} has unsupported constraint {other:?}",
                    path.display()
                ));
            }
        }
    }

    Ok(constraints)
}

impl CreaturePreferences {
    fn apply_node(&mut self, node: &KdlNode, path: &Path) -> Result<()> {
        let Some(children) = node.children() else {
            return Ok(());
        };

        for child in children.nodes() {
            match child.name().value() {
                "demersal" => self.demersal = probability_arg(child, path)?,
                "depth" => self.depth = probability_arg(child, path)?,
                "reefer" => self.reefer = probability_arg(child, path)?,
                "nektonic" => self.nektonic = probability_arg(child, path)?,
                "planktonic" => self.planktonic = probability_arg(child, path)?,
                "territorial" => self.territorial = probability_arg(child, path)?,
                "territory-geometry" => {
                    self.territory_geometry = Some(parse_territory_geometry(child, path)?);
                }
                "sociality" => self.sociality = probability_arg(child, path)?,
                "schooler" => self.schooler = probability_arg(child, path)?,
                "kinophile" => self.kinophile = probability_arg(child, path)?,
                "kinophobe" => self.kinophobe = probability_arg(child, path)?,
                "sedentary" => self.sedentary = probability_arg(child, path)?,
                "flighty" => self.flighty = probability_arg(child, path)?,
                other => {
                    return Err(anyhow!(
                        "{} has unsupported preference {other:?}",
                        path.display()
                    ));
                }
            }
        }

        Ok(())
    }
}

fn parse_territory_geometry(node: &KdlNode, path: &Path) -> Result<TerritoryGeometry> {
    let children = node.children().ok_or_else(|| {
        anyhow!(
            "{} `territory-geometry` requires a geometry child",
            path.display()
        )
    })?;
    let mut geometry_nodes = children.nodes().iter();
    let geometry = geometry_nodes.next().ok_or_else(|| {
        anyhow!(
            "{} `territory-geometry` requires a geometry child",
            path.display()
        )
    })?;
    if geometry_nodes.next().is_some() {
        return Err(anyhow!(
            "{} `territory-geometry` supports exactly one geometry child",
            path.display()
        ));
    }

    match geometry.name().value() {
        "rectangle" => Ok(TerritoryGeometry::Rectangle(parse_rectangle_geometry(
            geometry, path,
        )?)),
        other => Err(anyhow!(
            "{} has unsupported territory geometry {other:?}",
            path.display()
        )),
    }
}

fn parse_rectangle_geometry(node: &KdlNode, path: &Path) -> Result<RectangleTerritoryGeometry> {
    let width = required_child(node, "width", path)?;
    let height = required_child(node, "height", path)?;

    Ok(RectangleTerritoryGeometry {
        width: parse_dimension_spec(width, path)?,
        height: parse_dimension_spec(height, path)?,
    })
}

fn parse_dimension_spec(node: &KdlNode, path: &Path) -> Result<DimensionSpec> {
    if let Some(value) = node.get(0) {
        let value = value.as_integer().ok_or_else(|| {
            anyhow!(
                "{} `{}` dimension argument must be an integer",
                path.display(),
                node.name().value()
            )
        })?;
        return Ok(DimensionSpec::Constant(checked_u16(
            value,
            node.name().value(),
            path,
        )?));
    }

    match required_string_prop(node, "distribution", path)? {
        "uniform" => {
            let min = required_u16_prop(node, "min", path)?;
            let max = required_u16_prop(node, "max", path)?;
            if min > max {
                return Err(anyhow!(
                    "{} `{}` min must be <= max",
                    path.display(),
                    node.name().value()
                ));
            }
            Ok(DimensionSpec::Uniform { min, max })
        }
        other => Err(anyhow!(
            "{} `{}` has unsupported distribution {other:?}",
            path.display(),
            node.name().value()
        )),
    }
}

fn required_child<'a>(node: &'a KdlNode, name: &str, path: &Path) -> Result<&'a KdlNode> {
    node.children()
        .and_then(|children| children.get(name))
        .ok_or_else(|| {
            anyhow!(
                "{} `{}` requires `{name}` child",
                path.display(),
                node.name().value()
            )
        })
}

fn probability_arg(node: &KdlNode, path: &Path) -> Result<f64> {
    let value = node
        .get(0)
        .and_then(|value| {
            value
                .as_float()
                .or_else(|| value.as_integer().map(|int| int as f64))
        })
        .ok_or_else(|| {
            anyhow!(
                "{} `{}` requires numeric argument",
                path.display(),
                node.name().value()
            )
        })?;

    if value.is_finite() && (0.0..=1.0).contains(&value) {
        Ok(value)
    } else {
        Err(anyhow!(
            "{} `{}` argument must be a finite number from 0.0 to 1.0",
            path.display(),
            node.name().value()
        ))
    }
}

fn required_string_prop<'a>(node: &'a KdlNode, name: &str, path: &Path) -> Result<&'a str> {
    node.get(name).and_then(KdlValue::as_string).ok_or_else(|| {
        anyhow!(
            "{} `{}` requires string property `{name}`",
            path.display(),
            node.name().value()
        )
    })
}

fn required_u16_prop(node: &KdlNode, name: &str, path: &Path) -> Result<u16> {
    let value = node
        .get(name)
        .and_then(KdlValue::as_integer)
        .ok_or_else(|| {
            anyhow!(
                "{} `{}` requires integer property `{name}`",
                path.display(),
                node.name().value()
            )
        })?;
    checked_u16(value, name, path)
}

fn checked_u16(value: i128, name: &str, path: &Path) -> Result<u16> {
    value.try_into().map_err(|_| {
        anyhow!(
            "{} `{name}` must be a non-negative integer no larger than {}",
            path.display(),
            u16::MAX
        )
    })
}

fn optional_probability_prop(node: &kdl::KdlNode, name: &str) -> Result<Option<f64>> {
    let Some(value) = node.get(name) else {
        return Ok(None);
    };
    let Some(value) = value
        .as_float()
        .or_else(|| value.as_integer().map(|int| int as f64))
    else {
        return Err(anyhow!(
            "`{}` property `{name}` must be a number from 0.0 to 1.0",
            node.name().value()
        ));
    };

    if !(0.0..=1.0).contains(&value) {
        return Err(anyhow!(
            "`{}` property `{name}` must be from 0.0 to 1.0, got {value}",
            node.name().value()
        ));
    }

    Ok(Some(value))
}

fn clamp_velocity(value: i128) -> i16 {
    value.clamp(-1, 1) as i16
}

#[cfg(test)]
#[path = "creature_test.rs"]
mod creature_test;
