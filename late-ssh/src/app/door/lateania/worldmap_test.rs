use super::{Coord, collisions, derive_coords, dump_level, visible};
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

#[test]
fn biomes_cover_the_regions() {
    use crate::app::door::lateania::world::{
        BROCELIANDE_BASE, Biome, KAELMYR_BASE, LAKES_BASE, biome_of, region_layout,
    };
    let world = seed_world();

    assert_eq!(biome_of(world.start_room), Biome::Heartland);
    assert_eq!(biome_of(KAELMYR_BASE), Biome::Ash); // zone 0, open ground
    assert_eq!(biome_of(LAKES_BASE), Biome::Water);
    assert_eq!(biome_of(BROCELIANDE_BASE), Biome::Forest);
    // Kaelmyr zone 2 is carved as a cavern, so its biome overrides Ash.
    // KAELMYR_W * KAELMYR_H = 13 * 9 = 117 cells per zone.
    assert_eq!(biome_of(KAELMYR_BASE + 2 * 117), Biome::Cavern);

    // Every real archipelago room reads as Islands; every catacombs/caverns
    // room reads as Cavern.
    for &id in world.rooms.keys() {
        if crate::app::door::lateania::archipelago::is_archipelago_room(id) {
            assert_eq!(biome_of(id), Biome::Islands, "arch room {id}");
        }
        if let Some(p) = region_layout(id)
            && matches!(p.region, "catacombs" | "caverns")
        {
            assert_eq!(biome_of(id), Biome::Cavern, "underground room {id}");
        }
    }
}

#[test]
fn region_atlas_names_the_start_region() {
    use crate::app::door::lateania::world::region_atlas_entry;
    let world = seed_world();
    let (name, tier) = region_atlas_entry(world.start_room).expect("start room is in the atlas");
    assert!(name.contains("Embergate"), "got region name {name:?}");
    assert!(!tier.is_empty(), "region should carry a danger tier");
}

#[test]
fn visible_returns_one_level_within_radius() {
    let world = seed_world();
    let coords = derive_coords(&world);
    let center = coords[&world.start_room];

    let win = visible(&coords, center, 3, 3);
    assert!(
        win.iter().any(|(id, _)| *id == world.start_room),
        "the centre room is in its own window"
    );
    for (_, c) in &win {
        assert_eq!(c.z, center.z, "window stays on one level");
        assert!(
            (c.x - center.x).abs() <= 3 && (c.y - center.y).abs() <= 3,
            "cell {c:?} is outside the radius"
        );
    }
    // Widening the window never drops rooms, and the result is (y, x)-sorted.
    assert!(visible(&coords, center, 8, 8).len() >= win.len());
    let sorted = {
        let mut s = win.clone();
        s.sort_by_key(|(_, c)| (c.y, c.x));
        s
    };
    assert_eq!(win, sorted, "visible() must return (y, x)-sorted cells");
}
