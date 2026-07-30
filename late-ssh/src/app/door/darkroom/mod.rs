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
//! - `data`    — upstream balance tables, prose, and timing constants
//! - `pace`    — our pacing layer: credit accrual, the daily cap, the slowdown
//! - `model`   — the persistent `Game` and the rules acting on it
//! - `sim`     — the settle-forward clock (no timers, no game loop)
//! - `persist` — JSON save/load envelope with a schema version
//! - `svc`     — DB-backed load/save service (cheap to clone)
//! - `state`   — per-session state: the loaded game, panel, cursor, log
//! - `ui`      — the live page and the Games-hub landing card
//! - `screen`  — the `DoorGame` impl, launcher/active input, and `leave`
pub mod data;
pub mod model;
pub mod pace;
pub mod persist;
pub mod screen;
pub mod sim;
pub mod state;
pub mod svc;
pub mod ui;

#[cfg(test)]
mod model_test;

#[cfg(test)]
mod pace_test;

#[cfg(test)]
mod sim_test;

#[cfg(test)]
mod state_test;
