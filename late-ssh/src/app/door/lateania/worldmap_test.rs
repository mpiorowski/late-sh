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
    // The start room seeds the hand-authored core component at (0, 0, 0) before
    // being shifted east clear of the generated zones: y and z stay anchored on
    // the surface, x is non-negative (past the reserved zone blocks).
    assert_eq!(start.y, 0, "start room should anchor y");
    assert_eq!(start.z, 0, "start room should sit on the surface");
    assert!(start.x >= 0, "start room x should be non-negative");
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
fn generated_zones_are_collision_free_and_the_core_stays_tight() {
    use crate::app::door::lateania::world::region_layout;

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

    // Slice 1b lays every procedurally-generated zone from its generator grid at
    // a reserved origin, so NO generated room may share a cell with anything.
    // The key guarantee: every remaining collision is hand-authored core (the
    // capitals/roads, which are walked out by exits and can loop non-Euclidean).
    for ids in clashes.values() {
        for &id in ids {
            assert!(
                region_layout(id).is_none(),
                "generated room {id} collided - a zone was not laid from its grid",
            );
        }
    }

    // The hand-authored remainder is a small tail (~1% today, was ~13% under the
    // slice-1 naive BFS). Streaming never shows two of these together; this just
    // guards against a layout regression.
    assert!(
        rate < 0.03,
        "spatial collisions rose to {:.2}% of rooms (expected ~1% hand-authored tail)",
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
