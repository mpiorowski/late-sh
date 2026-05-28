//! sshattrick-core: pure game engine + asset rendering for sshattrick.

pub mod collision_detection;
pub mod constants;
pub mod engine;
pub mod game;
pub mod geom;
pub mod traits;
pub mod types;
pub mod utils;

pub use game::{Game, GameData, GameState};
pub use types::{AppResult, GameCommand, GameSide, Orientation, Palette};
