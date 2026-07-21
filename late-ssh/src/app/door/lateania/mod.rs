// Lateania - a D&D MUD inside late.sh.
//
// World & design by Tasmania (Tony Hosaroygard) - hardlygospel.github.io
// With heartfelt thanks to the creator of late.sh and every developer who
// contributes to it. This world stands on the foundation you built.
pub mod abilities;
pub mod appearance;
pub mod archipelago;
pub mod classes;
pub mod crafting;
pub mod damage;
pub mod housing;
pub mod input;
pub mod items;
pub mod persist;
pub mod pets;
pub mod screen;
pub mod skills;
pub mod state;
pub mod stats;
pub mod svc;
pub mod taming;
pub mod ui;
pub mod world;

#[cfg(test)]
mod abilities_test;

#[cfg(test)]
mod appearance_test;

#[cfg(test)]
mod archipelago_test;

#[cfg(test)]
mod damage_test;

#[cfg(test)]
mod persist_test;

#[cfg(test)]
mod pets_test;

#[cfg(test)]
mod skills_test;
