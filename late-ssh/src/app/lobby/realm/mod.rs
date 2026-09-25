//! Realm: multiplayer territory conquest. DB-durable games (see
//! `late-core/src/models/realm_game.rs`); actions resolve the instant they
//! are taken, and each game refills its daily action points at its own
//! chosen hour. See CONTEXT.md.

pub mod history_ui;
pub mod input;
pub mod log_ui;
pub mod map;
pub mod map_ui;
pub mod mapgen;
pub mod modal_ui;
pub mod overview_ui;
pub mod resolver;
pub mod results_ui;
pub mod rulesets;
pub mod state;
pub mod svc;
pub mod targets_ui;
pub mod ui;

#[cfg(test)]
mod svc_test;
