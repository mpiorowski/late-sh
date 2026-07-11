//! Hard-coded track catalogue.
//!
//! Tracks live in their own files in this directory and are registered here.

pub mod batin;
pub mod crazy;
pub mod eurotrip;
pub mod fantasy;
pub mod presets;
pub mod route66;
pub mod solar_system;

use super::track::Track;

/// Every track available in the picker, in display order.
pub const ALL_TRACKS: &[&Track] = &[
    &batin::TRACK,
    &route66::TRACK,
    &eurotrip::TRACK,
    &fantasy::TRACK,
    &solar_system::TRACK,
    &crazy::TRACK,
];

/// Default track loaded when none has been selected yet.
pub const DEFAULT_TRACK: &Track = &batin::TRACK;
