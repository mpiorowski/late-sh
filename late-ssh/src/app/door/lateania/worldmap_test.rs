use super::{Coord, collisions, derive_coords, dump_level};
use crate::app::door::lateania::world::seed_world;

#[test]
fn every_room_gets_a_coordinate() {
    let world = seed_world();
    let coords = derive_coords(&world);
    for &id in world.rooms.keys() {
        assert!(
            coords.contains_key(&id),
            "room {id} was left without a coordinate"
        );
    }
    assert_eq!(coords.len(), world.rooms.len(), "coord/room count mismatch");
}

#[test]
fn derivation_is_deterministic() {
    let world = seed_world();
    assert_eq!(
        derive_coords(&world),
        derive_coords(&world),
        "coordinate derivation must be identical every boot"
    );
}

#[test]
fn start_room_anchors_the_surface() {
    let world = seed_world();
    let coords = derive_coords(&world);
    let start = coords[&world.start_room];
    // The main landmass is the first component, laid out on the surface with the
    // start room at its western edge.
    assert_eq!(start.y, 0, "start room should anchor y");
    assert_eq!(start.z, 0, "start room should sit on the surface");
    // The first component is shifted so its western edge is x=0; the start room
    // sits somewhere inside it, so its x is non-negative but not necessarily 0.
    assert!(start.x >= 0, "start room should sit inside the first component");
}

#[test]
fn vertical_exits_populate_z_levels() {
    let world = seed_world();
    let coords = derive_coords(&world);
    assert!(
        coords.values().any(|c| c.z != 0),
        "the world's up/down exits should produce at least one off-surface level"
    );
}

#[test]
fn spatial_collisions_stay_within_the_naive_budget() {
    let world = seed_world();
    let coords = derive_coords(&world);
    let clashes = collisions(&coords);
    let clashing_rooms: usize = clashes.values().map(|ids| ids.len()).sum();
    let rate = clashing_rooms as f64 / coords.len() as f64;

    // Report the real numbers so regressions are visible in test output.
    eprintln!(
        "worldmap: {} rooms, {} colliding cells, {} rooms involved ({:.2}%)",
        coords.len(),
        clashes.len(),
        clashing_rooms,
        rate * 100.0,
    );

    // This is slice 1: a *naive* BFS embedding. Cross-component overlap is
    // engineered away, but each procedural region is still BFS-flattened onto
    // one plane, so its internal grids/zones can pile onto each other - about
    // 13% of rooms today. Slice 1b lays regions from their generator grids at
    // reserved origins to drive this toward zero. Until then this guards against
    // the layout *regressing* past the measured baseline.
    assert!(
        rate < 0.15,
        "spatial collisions spiked to {:.2}% of rooms (naive-BFS baseline is ~13%)",
        rate * 100.0
    );
}

#[test]
fn dump_level_draws_the_neighbourhood_around_the_player() {
    let world = seed_world();
    let coords = derive_coords(&world);
    let picture = dump_level(&coords, world.start_room, 6);
    // A 13x13 window (radius 6) centred on the player.
    assert_eq!(picture.lines().count(), 13);
    assert!(picture.contains('@'), "the centre room should be marked");
    assert!(
        picture.contains('#'),
        "neighbouring rooms should show near the start town"
    );
}

#[test]
fn coord_ordering_is_stable_for_reports() {
    // `collisions` and `dump_level` lean on `Coord: Ord`; pin the ordering so a
    // derive change can't silently scramble reports.
    let mut cs = [
        Coord { x: 1, y: 0, z: 0 },
        Coord { x: 0, y: 1, z: 0 },
        Coord { x: 0, y: 0, z: 1 },
        Coord { x: 0, y: 0, z: 0 },
    ];
    cs.sort();
    assert_eq!(
        cs,
        [
            Coord { x: 0, y: 0, z: 0 },
            Coord { x: 0, y: 0, z: 1 },
            Coord { x: 0, y: 1, z: 0 },
            Coord { x: 1, y: 0, z: 0 },
        ]
    );
}
