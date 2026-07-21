//! The house: fixed multiplayer tables living behind the Lobby (`Ctrl+Q`).
//! One table per `HouseTable` variant, no user creation, no DB rows. The
//! runtimes here (poker, blackjack, asterion, tron) are the survivors of the
//! Rooms-domain demolition.

pub mod asterion;
pub mod blackjack;
pub mod game_ui;
pub mod image_render;
pub mod input;
pub mod poker;
pub mod registry;
pub mod ssnake;
pub mod state;
pub mod tables;
pub mod tron;
pub mod types;
pub mod ui;

#[cfg(test)]
mod tables_test;
