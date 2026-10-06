//! A Dark Room: a native in-process door game, a terminal port of Michael
//! Townsend's minimalist incremental. Single-player, DB-persisted (one save per
//! user). It uses the Green Dragon integration pattern (native ratatui + a
//! service + a `DoorGame` impl), because upstream is a browser game with no
//! terminal to proxy.
//!
//! **Licensing.** Upstream is MPL-2.0 and this port reuses its balance data and
//! prose directly, which the MPL permits as long as the covered files stay
//! MPL-2.0. The rule for this directory: anything derived from upstream carries
//! the MPL header (`data`, `model`, `sim`), and everything that is ours stays
//! under the repo's FSL (`pace`, `persist`, `svc`, `state`, `ui`, `screen`).
//! MPL §3.3 is what lets the larger work ship under our own terms. See
//! LICENSING.md and NOTICE.
//!
//! **Pacing.** Upstream has no offline progress at all: it runs on wall clock
//! while the tab is open, so the arc takes a couple of sittings. This port
//! credits time while the SSH session is connected (anywhere on late.sh, not
//! just on this screen), runs the village at a fraction of upstream speed,
//! caps how much lands per day, and floors the first settle of each day once
//! the village stands, so the same arc spans weeks whether the player idles
//! or just checks in. The whole of that design lives in `pace`; nothing else
//! knows about it.
//!
//! Module map (flat, like the other door domains):
//! - `data`       — upstream balance tables, prose, and timing constants
//! - `pace`       — our pacing layer: credit accrual, daily cap, slowdown
//! - `model`      — the persistent `Game` and the rules acting on it
//! - `sim`        — the settle-forward village clock (no timers, no game loop)
//! - `event`      — the scene machine and the fight
//! - `scenes_*`   — the event pools: village, encounters, setpieces, and the
//!   ravaged battleship
//! - `world_data` — the wasteland's tables: tiles, landmarks, weapons, weights
//! - `world`      — map generation, walking, supplies, going home or not
//! - `space`      — the ascent, and the way off this rock
//! - `persist`    — JSON save/load envelope with a schema version
//! - `svc`        — DB-backed load/save service (cheap to clone)
//! - `state`      — per-session state: game, panel, cursor, log, what's live
//! - `ui`         — the live page and the Games-hub landing card
//! - `ui_event`   — the event modal and the fight panel
//! - `ui_world`   — the masked map and the ascent
//! - `screen`     — the `DoorGame` impl, launcher/active input, and `leave`
pub mod data;
pub mod event;
pub mod model;
pub mod pace;
pub mod persist;
pub mod scenes_encounters;
pub mod scenes_executioner;
pub mod scenes_setpieces;
pub mod scenes_village;
pub mod screen;
pub mod sim;
pub mod space;
pub mod state;
pub mod svc;
pub mod ui;
pub mod ui_event;
pub mod ui_world;
pub mod world;
pub mod world_data;

#[cfg(test)]
mod event_test;

#[cfg(test)]
mod model_test;

#[cfg(test)]
mod space_test;

#[cfg(test)]
mod world_test;

#[cfg(test)]
mod pace_test;

#[cfg(test)]
mod persist_test;

#[cfg(test)]
mod sim_test;

#[cfg(test)]
mod state_test;

#[cfg(test)]
mod ui_event_test;

#[cfg(test)]
mod ui_test;
