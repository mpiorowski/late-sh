use crate::{entity::Entity, power_up::PowerUp, Direction, PlayerId, Position};
use std::{
    collections::{HashMap, HashSet},
    time::{Duration, Instant},
};
use strum_macros::Display;

#[derive(Debug)]
pub enum GameCommand {
    Move { direction: Direction },
    TurnClockwise,
    TurnCounterClockwise,
    CycleUiOptions,
}

#[derive(Debug, Clone, Copy, Display, PartialEq)]
pub enum HeroState {
    WaitingToStart,
    InMaze {
        instant: Instant,
    },
    Dead {
        duration: Duration,
        instant: Instant,
    },
    Victory {
        duration: Duration,
        instant: Instant,
    },
}
#[derive(Debug, Clone, Copy, Display, PartialEq)]
pub enum UiOptions {
    Dark,
    Light,
}

impl UiOptions {
    pub fn next(&self) -> Self {
        match self {
            Self::Dark => Self::Light,
            Self::Light => Self::Dark,
        }
    }
}

#[derive(Debug)]
pub struct Hero {
    id: PlayerId,
    name: String,
    pub state: HeroState,
    maze_id: usize,
    position: Position,
    direction: Direction,
    vision: usize,
    speed: u64,
    memory: u64,
    past_visible_positions: HashMap<usize, HashMap<Position, Instant>>,
    last_move_time: Instant,
    collected_power_ups: HashMap<usize, Vec<Position>>,
    ui_options: UiOptions,
}

impl Hero {
    pub const MAX_SPEED: u64 = 8;
    pub const MAX_VISION: usize = 8;
    pub const INITIAL_SPEED: u64 = 4;
    pub const INITIAL_VISION: usize = 1;
    pub const INITIAL_MEMORY: u64 = 0;
    pub fn new(id: PlayerId, name: String, position: Position) -> Self {
        let state = HeroState::WaitingToStart;
        Self {
            id,
            name,
            state,
            maze_id: 0,
            position,
            direction: Direction::East,
            vision: Self::INITIAL_VISION,
            speed: Self::INITIAL_SPEED,
            memory: Self::INITIAL_MEMORY,
            past_visible_positions: HashMap::new(),
            last_move_time: Instant::now(),
            collected_power_ups: HashMap::new(),
            ui_options: UiOptions::Dark,
        }
    }

    pub fn reset(&mut self, position: Position) {
        self.state = HeroState::WaitingToStart;
        self.maze_id = 0;
        self.position = position;
        self.direction = Direction::East;
        self.vision = Self::INITIAL_VISION;
        self.speed = Self::INITIAL_SPEED;
        self.memory = Self::INITIAL_MEMORY;
        self.past_visible_positions.clear();
        self.last_move_time = Instant::now();
        self.collected_power_ups.clear();
    }

    pub fn cycle_ui_options(&mut self) {
        self.ui_options = self.ui_options.next();
    }

    pub fn is_dead(&self) -> bool {
        matches!(self.state, HeroState::Dead { .. })
    }

    pub fn has_won(&self) -> Option<Duration> {
        match self.state {
            HeroState::Victory { duration, .. } => Some(duration),
            _ => None,
        }
    }

    pub fn memory(&self) -> u64 {
        self.memory
    }

    pub fn elapsed_duration_from_start(&self) -> Duration {
        match self.state {
            HeroState::WaitingToStart => Duration::from_millis(0),
            HeroState::InMaze { instant } => instant.elapsed(),
            HeroState::Dead { duration, .. } => duration,
            HeroState::Victory { duration, .. } => duration,
        }
    }

    pub fn can_move(&self) -> bool {
        if self.is_dead() {
            return false;
        }

        self.last_move_time.elapsed() >= self.movement_recovery_duration()
    }

    pub fn past_visibility_duration(&self) -> Duration {
        Duration::from_secs_f32(10.0 + 10.0 * self.memory as f32)
    }

    pub fn update_past_visible_positions(&mut self, visible_positions: HashSet<Position>) {
        let duration = self.past_visibility_duration();

        let past_visible_positions = self.past_visible_positions.entry(self.maze_id).or_default();

        for &position in visible_positions.iter() {
            past_visible_positions.insert(position, Instant::now());
        }
        past_visible_positions.retain(|_, instant| instant.elapsed() < duration);
    }

    pub fn past_visible_positions(&self) -> &HashMap<Position, Instant> {
        self.past_visible_positions.get(&self.maze_id).unwrap()
    }

    pub fn set_direction(&mut self, direction: Direction) {
        self.direction = direction;
    }

    pub fn set_position(&mut self, position: Position) {
        self.position = position;
        self.last_move_time = Instant::now();
    }

    pub fn set_maze_id(&mut self, maze_id: usize) {
        self.maze_id = maze_id;
    }

    pub fn decrease_vision(&mut self) {
        self.vision -= 1;
    }

    pub fn apply_random_power_up_at_position(&mut self, position: Position) {
        let mut available_power_ups = vec![];

        if self.speed < Self::MAX_SPEED {
            available_power_ups.push(PowerUp::Speed);
        }

        if self.vision < Self::MAX_VISION {
            available_power_ups.push(PowerUp::Vision);
        }

        available_power_ups.push(PowerUp::Memory);

        let idx = rand::random_range(0..available_power_ups.len());

        let power_up = available_power_ups[idx];
        match power_up {
            PowerUp::Speed => self.speed = (self.speed + 1).min(Self::MAX_SPEED),
            PowerUp::Vision => self.vision = (self.vision + 1).min(Self::MAX_VISION),
            PowerUp::Memory => self.memory += 1,
        }

        self.collected_power_ups
            .entry(self.maze_id)
            .and_modify(|e| e.push(position))
            .or_insert(vec![position]);
    }

    pub fn power_ups_collected_in_maze(&self, maze_id: usize) -> usize {
        self.collected_power_ups
            .get(&maze_id)
            .map(|v| v.len())
            .unwrap_or_default()
    }

    pub fn power_up_collected_at(&self, maze_id: usize, position: Position) -> bool {
        self.collected_power_ups
            .get(&maze_id)
            .map(|v| v.contains(&position))
            .unwrap_or_default()
    }
}

impl Entity for Hero {
    fn id(&self) -> PlayerId {
        self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn vision(&self) -> usize {
        self.vision
    }

    fn speed(&self) -> u64 {
        self.speed
    }

    fn position(&self) -> super::Position {
        self.position
    }

    fn direction(&self) -> Direction {
        self.direction
    }

    fn maze_id(&self) -> usize {
        self.maze_id
    }
}
