//! Realm: multiplayer territory conquest. DB-durable games (see
//! `late-core/src/models/realm_game.rs`); on the house's campaign, orders
//! written by day resolve together at the night, when the day turns over at
//! the house hour (`svc::HOUSE_RESET_HOUR_UTC`), and on the standard ruleset
//! an action resolves the instant it is taken. See CONTEXT.md.

pub mod history_ui;
pub mod input;
pub mod join_ui;
pub mod landing_ui;
pub mod live;
pub mod log_ui;
pub mod map;
pub mod map_ui;
pub mod mapgen;
pub mod night;
pub mod orders_ui;
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
