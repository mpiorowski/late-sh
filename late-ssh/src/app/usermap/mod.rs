//! `/map`: the world, shaded by how many people are on it right now.
//!
//! `/active` answers "who is here" as a list of names. This answers "where
//! are they" on the same Earth the realm board is fought over — the map is
//! `common::worldmap`, shared rather than copied. A country nobody has
//! claimed on their profile is simply not shaded: this is a map of people who
//! said where they are.

pub mod state;
pub mod ui;

#[cfg(test)]
mod state_test;

#[cfg(test)]
#[path = "ui_test.rs"]
mod ui_test;
