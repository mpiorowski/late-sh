use super::{
    Coord, MAX_VIEWPORT_COLS, MapCamera, PAN_LIMIT, collisions, derive_bounds, derive_coords,
    dump_level, visible,
};
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

#[test]
fn world_coords_is_cached_and_complete() {
    let via_cache = super::world_coords();
    let fresh = derive_coords(&seed_world());
    assert_eq!(*via_cache, fresh, "cached coords must match a fresh derive");
    assert!(!via_cache.is_empty());
}

#[test]
fn viewport_centres_on_the_player() {
    let world = seed_world();
    let coords = derive_coords(&world);
    let center = coords[&world.start_room];
    let (cols, rows) = (21, 11);
    let grid = super::viewport(&coords, center, cols, rows);

    assert_eq!(grid.len(), rows as usize);
    assert!(grid.iter().all(|r| r.len() == cols as usize));
    let (cx, cy) = (cols as usize / 2, rows as usize / 2);
    assert_eq!(
        grid[cy][cx],
        Some(world.start_room),
        "the centre cell holds the player's room"
    );
    let filled = grid.iter().flatten().filter(|c| c.is_some()).count();
    assert!(filled > 1, "the start town should have visible neighbours");
}

// Eyeball helper (run with --nocapture): a biome-lettered map around the start.
#[test]
fn viewport_biome_dump_is_coherent() {
    use crate::app::door::lateania::world::{Biome, biome_of};
    let world = seed_world();
    let coords = derive_coords(&world);
    let center = coords[&world.start_room];
    let grid = super::viewport(&coords, center, 60, 20);
    let mut out = String::new();
    for row in &grid {
        for cell in row {
            let ch = match cell {
                Some(id) if *id == world.start_room => '@',
                Some(id) => match biome_of(*id) {
                    Biome::Heartland => 'h',
                    Biome::Plains => '.',
                    Biome::Urban => '#',
                    Biome::Forest => 'f',
                    Biome::Water => '~',
                    Biome::Islands => 'i',
                    Biome::Ash => 'a',
                    Biome::Cavern => 'c',
                    Biome::Badlands => 'b',
                },
                None => ' ',
            };
            out.push(ch);
        }
        out.push('\n');
    }
    eprintln!("\n{out}");
    assert!(out.contains('@'), "player should be on the map");
}

#[test]
fn fog_of_war_hides_unvisited_rooms_but_keeps_the_player() {
    use std::collections::HashSet;
    let world = seed_world();
    let coords = derive_coords(&world);
    let center = coords[&world.start_room];
    let (cols, rows) = (21, 11);

    // Nothing visited: only the player's own room shows.
    let empty = HashSet::new();
    let fogged = super::viewport_explored(&coords, center, cols, rows, &empty, world.start_room);
    let shown: Vec<_> = fogged.iter().flatten().flatten().copied().collect();
    assert_eq!(
        shown,
        vec![world.start_room],
        "only the player shows under full fog"
    );

    // With everything visited, fog matches the plain viewport everywhere except
    // the player's own cell, which they always win (see the collision test).
    let all: HashSet<_> = world.rooms.keys().copied().collect();
    let lit = super::viewport_explored(&coords, center, cols, rows, &all, world.start_room);
    let plain = super::viewport(&coords, center, cols, rows);
    let (cx, cy) = (cols as usize / 2, rows as usize / 2);
    assert_eq!(lit[cy][cx], Some(world.start_room));
    for (r, (lit_row, plain_row)) in lit.iter().zip(plain.iter()).enumerate() {
        for (c, (l, p)) in lit_row.iter().zip(plain_row.iter()).enumerate() {
            if (r, c) != (cy, cx) {
                assert_eq!(l, p, "cell ({r}, {c}) diverged from the fog-less view");
            }
        }
    }
}

#[test]
fn the_player_holds_their_own_cell_against_a_collision() {
    use std::collections::HashSet;
    // The hand-authored core stacks whole regions on shared cells (the Mistfen
    // under Whisperwood, the Obsidian Throne under Frostspire, house interiors
    // under Embergate). Standing in the higher-id room of such a pair must not
    // hide the player: resolving the cell by lowest id before the fog used to
    // drop their `@` and point the inspector at somewhere else entirely.
    let world = seed_world();
    let coords = derive_coords(&world);
    let clashes = collisions(&coords);
    let (&cell, ids) = clashes
        .iter()
        .find(|(_, ids)| ids.len() > 1)
        .expect("the hand-authored core still collides somewhere");
    let loser = *ids.last().expect("a colliding cell has room ids");

    // Everything visited, so only the tie-break can hide the player.
    let all: HashSet<_> = world.rooms.keys().copied().collect();
    let (cols, rows) = (21, 11);
    let grid = super::viewport_explored(&coords, cell, cols, rows, &all, loser);
    assert_eq!(
        grid[rows as usize / 2][cols as usize / 2],
        Some(loser),
        "the player's own room must win its cell (collided with {ids:?})"
    );
}

#[test]
fn fog_hides_an_unvisited_room_squatting_a_visited_one() {
    use std::collections::HashSet;
    // A cell shared by two rooms where only the higher-id one has been visited:
    // the map must show the room the player actually knows, not blank.
    let world = seed_world();
    let coords = derive_coords(&world);
    let clashes = collisions(&coords);
    let (&cell, ids) = clashes
        .iter()
        .find(|(_, ids)| ids.len() > 1)
        .expect("the hand-authored core still collides somewhere");
    let known = *ids.last().expect("a colliding cell has room ids");

    let visited: HashSet<_> = [known].into_iter().collect();
    let (cols, rows) = (21, 11);
    // Player elsewhere entirely, so only `visited` decides this cell.
    let grid = super::viewport_explored(&coords, cell, cols, rows, &visited, world.start_room);
    assert_eq!(
        grid[rows as usize / 2][cols as usize / 2],
        Some(known),
        "a visited room must not be hidden by an unvisited squatter"
    );
}

#[test]
fn one_screen_never_shows_two_reserved_blocks() {
    use crate::app::door::lateania::world::{KAELMYR_BASE, region_layout};
    // The module promises that seams between reserved blocks never share the
    // screen. That only holds while neighbouring blocks sit further apart than
    // the widest map we paint. At the original margin of 4, an 80-column map
    // showed five unrelated zones side by side and a forest slab pasted onto
    // Embergate's town square.
    let world = seed_world();
    let coords = derive_coords(&world);
    let bounds = derive_bounds(&coords);
    let here = region_layout(KAELMYR_BASE).expect("kaelmyr decodes to a grid cell");
    let player = coords[&KAELMYR_BASE];

    // Standing still, and panned as far as the camera will go in each
    // direction: a fully panned viewport must still not reach the next block.
    let mut centers = vec![player];
    for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
        let mut cam = MapCamera::default();
        for _ in 0..(PAN_LIMIT + 10) {
            cam.pan(player, bounds, dx, dy);
        }
        centers.push(cam.center(player));
    }

    for center in centers {
        for (id, c) in visible(
            &coords,
            center,
            MAX_VIEWPORT_COLS / 2,
            MAX_VIEWPORT_COLS / 2,
        ) {
            let there = region_layout(id);
            assert!(
                there.is_some_and(|p| (p.region, p.zone) == (here.region, here.zone)),
                "from {center:?}, room {id} at {c:?} shares the screen with {}/{}, \
                 but belongs to {:?}",
                here.region,
                here.zone,
                there.map(|p| (p.region, p.zone)),
            );
        }
    }
}

#[test]
fn the_camera_pans_and_clamps_to_the_field() {
    let world = seed_world();
    let coords = derive_coords(&world);
    let bounds = derive_bounds(&coords);
    let player = coords[&world.start_room];

    let mut cam = MapCamera::default();
    assert_eq!(cam.center(player), player, "a fresh camera sits on you");

    cam.pan(player, bounds, 1, 0);
    cam.pan(player, bounds, 0, 1);
    assert_eq!(cam.scroll(), (1, 1));
    assert_eq!(
        cam.center(player),
        Coord {
            x: player.x + 1,
            y: player.y + 1,
            z: player.z
        }
    );

    // Panning east forever stops within reach of the player rather than running
    // off into unbounded blank (and never far enough to see the next block).
    for _ in 0..(PAN_LIMIT * 3) {
        cam.pan(player, bounds, 1, 0);
    }
    assert_eq!(
        cam.center(player).x,
        (player.x + PAN_LIMIT).min(bounds.max.x),
        "pan clamps east"
    );
    for _ in 0..(PAN_LIMIT * 6) {
        cam.pan(player, bounds, -1, 0);
    }
    assert_eq!(
        cam.center(player).x,
        (player.x - PAN_LIMIT).max(bounds.min.x),
        "pan clamps west"
    );

    cam.recenter();
    assert_eq!(cam.center(player), player, "Enter puts you back under it");
}

#[test]
fn the_camera_only_visits_levels_that_exist() {
    let world = seed_world();
    let coords = derive_coords(&world);
    let bounds = derive_bounds(&coords);
    let player = coords[&world.start_room];

    let mut cam = MapCamera::default();
    for _ in 0..50 {
        cam.change_level(player, bounds, -1);
    }
    assert_eq!(
        cam.center(player).z,
        bounds.min.z,
        "down stops at the deepest"
    );
    for _ in 0..100 {
        cam.change_level(player, bounds, 1);
    }
    assert_eq!(
        cam.center(player).z,
        bounds.max.z,
        "up stops at the highest"
    );

    cam.recenter();
    assert_eq!(cam.level_offset(), 0);
}

#[test]
fn pois_index_bosses_tameables_and_monsters() {
    let world = seed_world();
    let pois = super::pois();
    assert!(!pois.is_empty(), "the world has points of interest");

    // Every boss's home room is a POI with the boss set.
    let mut boss_homes = 0;
    for spawn in world.spawns.iter().filter(|s| s.boss) {
        boss_homes += 1;
        assert!(
            super::poi(spawn.home).is_some_and(|p| p.boss == Some(spawn.name)),
            "boss {} missing from its home POI",
            spawn.name
        );
    }
    assert!(boss_homes >= 1, "there is at least one boss");
    assert!(
        pois.values()
            .any(|p| p.boss.is_some() && !p.reward.is_empty()),
        "at least one boss lists a guaranteed reward"
    );
    assert!(
        pois.values().any(|p| p.tameable.is_some()),
        "some rooms host a tameable beast"
    );

    // Every spawn appears in its home room's monster list.
    for spawn in &world.spawns {
        assert!(
            super::poi(spawn.home).is_some_and(|p| p.monsters.contains(&spawn.name)),
            "spawn {} missing from its home POI",
            spawn.name
        );
    }
}
