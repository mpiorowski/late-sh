//! Pure game logic for asterion - data and rules with no I/O, UI, or SSH.

mod alarm_level;
mod direction;
mod entity;
mod game;
mod hero;
mod maze;
mod minotaur;
mod power_up;
mod utils;
mod view;

pub use alarm_level::AlarmLevel;
pub use direction::Direction;
pub use entity::Entity;
pub use game::{Game, MAX_MAZE_ID, POWER_UPS_PER_ROOM};
pub use hero::{GameCommand, Hero};
pub use maze::Maze;
pub use minotaur::Minotaur;
pub use power_up::PowerUp;
pub use utils::{GameColors, PlayerId, MAX_USERNAME_LEN};
pub use view::View;

pub type Position = (usize, usize);

pub trait IntoDirection {
    fn into_direction(self, direction: &Direction) -> Self;
    fn distance(&self, other: Position) -> f64;
    fn distance_squared(&self, other: Position) -> usize;
}

impl IntoDirection for Position {
    fn into_direction(self, direction: &Direction) -> Self {
        let (x, y) = self;
        let (new_x, new_y) = match direction {
            Direction::North => (x, y.saturating_sub(1)),
            Direction::South => (x, y + 1),
            Direction::West => (x.saturating_sub(1), y),
            Direction::East => (x + 1, y),
            _ => (x, y),
        };

        (new_x, new_y)
    }

    fn distance(&self, other: Position) -> f64 {
        (self.distance_squared(other) as f64).sqrt()
    }

    fn distance_squared(&self, other: Position) -> usize {
        ((self.0 as isize - other.0 as isize).pow(2) + (self.1 as isize - other.1 as isize).pow(2))
            as usize
    }
}
