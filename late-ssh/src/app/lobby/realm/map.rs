//! Which worlds a realm can be fought over.
//!
//! The atlas itself — territories, grids, mips, Earth — lives in
//! `app::common::worldmap`, because a map of the world is not a game rule and
//! `/map` wanted the same data. What is here is realm's own: the
//! roster its create dialog offers, and the lookup that turns a game's frozen
//! `map_id` (plus, for a generated world, its spec) into a board.

pub use crate::app::common::worldmap::data::{
    Territory, TerritoryGrid, TerritoryId, WATER, WorldMap, map_by_id,
};

use std::sync::Arc;

use super::mapgen::GeneratedMapSpec;

/// A map as the create dialog offers it. The world a realm is fought over
/// is the creator's choice, not a property of the rules — the same rules on
/// a different map is a different game, and one ruleset should not have to
/// be copied per map to say so.
pub struct RealmMapInfo {
    pub id: &'static str,
    pub display_name: &'static str,
    /// Shown under the list while this row is highlighted.
    pub blurb: &'static str,
}

/// What can be picked when making a realm. `testmap` is deliberately absent:
/// it is twelve territories built for tests, and a game on it would be over
/// in an afternoon.
pub const MAPS: &[RealmMapInfo] = &[
    RealmMapInfo {
        id: "earth",
        display_name: "Earth",
        blurb: "242 countries on real borders — the whole world to take",
    },
    RealmMapInfo {
        id: GENERATED_MAP_ID,
        display_name: "Uncharted",
        blurb: "a world drawn for this game alone — you set its shape next",
    },
];

pub fn map_info_by_id(id: &str) -> Option<&'static RealmMapInfo> {
    MAPS.iter().find(|m| m.id == id)
}

/// The id every generated world carries. It is not a map on its own — the
/// spec beside it in the snapshot is what says which world.
pub const GENERATED_MAP_ID: &str = "generated";

/// The one lookup the rest of realm uses: a map id plus, for a generated
/// world, the spec that describes it. A generated id without a spec has no
/// map, which is a corrupt game rather than a missing feature, so it is None
/// and the caller reports it.
pub fn map_handle(id: &str, spec: Option<&GeneratedMapSpec>) -> Option<Arc<WorldMap>> {
    if id == GENERATED_MAP_ID {
        return spec.map(super::mapgen::map_for_spec);
    }
    map_by_id(id)
}
