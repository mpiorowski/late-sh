//! Legend of the Green Dragon: a native in-process door game, an open-source
//! remake of LORD. Single-player, turn-based, DB-persisted (one character per
//! user). Balance data is transcribed from the LoGD seed (see `data`).
//!
//! Module map (flat, like the other door domains):
//! - `data`    — canonical LoGD balance tables (weapons, creatures, masters, ...)
//! - `combat`    — the pure round resolver (`bell_rand`, crits, glancing hits)
//!   and the specialty buff engine
//! - `commentary` — the shared chat primitive's pure rules (rooms, post prep,
//!   allowances, line composition)
//! - `model`     — the persistent `Character` and the rules acting on it
//! - `specialty` — the twelve specialty combat skills (buff factories)
//! - `events`    — forest special events (the non-combat vignettes)
//! - `inn`       — the inn's bard song and romance ladder resolvers
//! - `tavern`    — the Crooked Wheel's gambling game logic
//! - `persist`   — JSON save/load envelope with a schema version
//! - `svc`       — DB-backed load/save service (cheap to clone)
//! - `state`     — per-session game state: mode machine, encounter, message log
//! - `ui`        — rendering for the live page and the Games-hub landing card
//! - `screen`    — the `DoorGame` impl, launcher/active input, and `leave`
pub mod combat;
pub mod commentary;
pub mod data;
pub mod events;
pub mod inn;
pub mod model;
pub mod persist;
pub mod screen;
pub mod specialty;
pub mod state;
pub mod svc;
pub mod tavern;
pub mod ui;

#[cfg(test)]
mod commentary_test;

#[cfg(test)]
mod data_test;

#[cfg(test)]
mod events_test;

#[cfg(test)]
mod inn_test;

#[cfg(test)]
mod persist_test;

#[cfg(test)]
mod tavern_test;
