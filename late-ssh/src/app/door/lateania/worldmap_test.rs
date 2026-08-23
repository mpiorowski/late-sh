use super::{
    Coord, MAX_VIEWPORT_COLS, MapCamera, PAN_LIMIT, collisions, derive_bounds, derive_coords,
    dump_level, visible, zone_interleaves,
};
use crate::app::door::lateania::world::{RoomId, region_atlas_entry, seed_world};

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
fn housing_interiors_never_share_a_cell_with_the_town() {
    use crate::app::door::lateania::housing::{HOUSING_BASE, is_housing_room};

    let world = seed_world();
    let coords = derive_coords(&world);
    let clashes = collisions(&coords);
    for (c, ids) in &clashes {
        let interior = ids
            .iter()
            .any(|&id| id != HOUSING_BASE && is_housing_room(id));
        assert!(
            !interior,
            "house interior collided with the world at {c:?}: rooms {ids:?} - the \
             field would draw another room's paths around a player standing inside",
        );
    }
}

#[test]
fn every_walkable_room_is_reachable_from_the_start_room() {
    // `link` refuses to clobber an occupied exit, but a room authored with no
    // path to it at all would still boot fine and still get coordinates (a
    // cut-off component seeds its own island), so nothing else notices a
    // severed wing. Walk the whole exit graph from the start room; only the
    // lands with no walking entrance at all (reached by waystone portal) may
    // stay unvisited.
    use std::collections::VecDeque;
    let world = seed_world();
    let portal_only: std::collections::HashSet<&str> = super::portal_lands().into_iter().collect();
    let mut seen = std::collections::HashSet::from([world.start_room]);
    let mut queue = VecDeque::from([world.start_room]);
    while let Some(rid) = queue.pop_front() {
        let Some(room) = world.room(rid) else {
            continue;
        };
        for &dest in room.exits.values() {
            if seen.insert(dest) {
                queue.push_back(dest);
            }
        }
    }
    let mut cut_off: Vec<RoomId> = world
        .rooms
        .keys()
        .copied()
        .filter(|id| !seen.contains(id))
        .filter(|id| region_atlas_entry(*id).is_none_or(|(name, _)| !portal_only.contains(name)))
        .collect();
    cut_off.sort_unstable();
    cut_off.truncate(12);
    assert!(
        cut_off.is_empty(),
        "rooms exist that no walk from the start can reach (first few): {cut_off:?}"
    );
}

#[test]
fn each_wildbound_gate_sits_directly_above_the_field_cell_it_opens_onto() {
    // The gate town is placed by `wildbound_layout` while the gate's South
    // exit is wired by `extend_wildbound` to the carve's entrance cell. If
    // the two disagree, the map draws a path from the gate into a field room
    // the exit does not lead to, the exact drawn-adjacent-but-not-connected
    // lie the fold detector exists to kill, invisible to it because town and
    // field share a zone.
    use crate::app::door::lateania::world::{Dir, WILDBOUND_BASE, WILDBOUND_BIOME_STRIDE};
    let world = seed_world();
    let coords = derive_coords(&world);
    for b in 0..3u32 {
        let gate = WILDBOUND_BASE + b * WILDBOUND_BIOME_STRIDE + 3;
        let dest = world
            .room(gate)
            .and_then(|r| r.exits.get(&Dir::South))
            .copied()
            .expect("each wildbound gate has a south exit into its field");
        let (g, d) = (coords[&gate], coords[&dest]);
        assert_eq!(
            (d.x - g.x, d.y - g.y, d.z - g.z),
            (0, 1, 0),
            "biome {b}: gate {gate}'s south exit lands in room {dest}, which is not \
             the cell drawn directly below the gate"
        );
    }
}

#[test]
fn no_zone_presses_against_another_it_has_no_gate_into() {
    let world = seed_world();
    let coords = derive_coords(&world);
    let report = zone_interleaves(&world, &coords);
    let lines: Vec<String> = report
        .iter()
        .map(|i| {
            let name = |id: RoomId| world.room(id).map(|r| r.name).unwrap_or("?");
            let at = |id: RoomId| {
                coords
                    .get(&id)
                    .map(|c| format!("({}, {}, z{})", c.x, c.y, c.z))
                    .unwrap_or_default()
            };
            let walk = match i.walk {
                Some(w) => format!("{w} moves apart"),
                None => "no walking path".to_string(),
            };
            format!(
                "{} <-> {}: {} touching pairs, e.g. {} '{}' {} beside {} '{}' {}, {}",
                i.zone_a,
                i.zone_b,
                i.touching,
                i.example.0,
                name(i.example.0),
                at(i.example.0),
                i.example.1,
                name(i.example.1),
                at(i.example.1),
                walk,
            )
        })
        .collect();
    assert!(
        report.is_empty(),
        "zones fold onto each other on the map, a place drawn one cell away is \
         really a journey away:\n{}",
        lines.join("\n"),
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

    // With everything visited, the explored view matches the plain viewport
    // everywhere except where rooms genuinely stack on one cell: the explored
    // view resolves a stack with `resolve_collision` (the player's room, then
    // the player's region, then the lowest id), while the plain viewport is
    // collision-naive. Since the unfold the field's stacks are rare
    // (`generated_zones_are_collision_free_and_the_core_stays_tight` counts
    // them), so a divergence anywhere else is a bug, not a tie-break.
    let all: HashSet<_> = world.rooms.keys().copied().collect();
    let lit = super::viewport_explored(&coords, center, cols, rows, &all, world.start_room);
    let plain = super::viewport(&coords, center, cols, rows);
    let (cx, cy) = (cols as usize / 2, rows as usize / 2);
    let stacked = collisions(&coords);
    assert_eq!(lit[cy][cx], Some(world.start_room));
    for (r, (lit_row, plain_row)) in lit.iter().zip(plain.iter()).enumerate() {
        for (c, (l, p)) in lit_row.iter().zip(plain_row.iter()).enumerate() {
            if (r, c) == (cy, cx) || l == p {
                continue;
            }
            let cell = Coord {
                x: center.x - cols / 2 + c as i32,
                y: center.y - rows / 2 + r as i32,
                z: center.z,
            };
            let both_stacked = stacked.get(&cell).is_some_and(|ids| {
                l.is_some_and(|id| ids.contains(&id)) && p.is_some_and(|id| ids.contains(&id))
            });
            assert!(
                both_stacked,
                "cell ({r}, {c}) diverged from the fog-less view for a reason other than \
                 a stacked cell's tie-break: lit={l:?} plain={p:?}"
            );
        }
    }
}

// ---- collision resolution favours where the player actually stands -------

#[test]
fn resolve_collision_prefers_the_players_own_room() {
    // Out of id order on purpose: the player's own room must win regardless.
    assert_eq!(super::resolve_collision(5, 100, 100, None), 100);
    assert_eq!(super::resolve_collision(100, 5, 100, None), 100);
}

#[test]
fn resolve_collision_falls_back_to_lowest_id_without_a_region_match() {
    assert_eq!(super::resolve_collision(50, 20, 999, None), 20);
    assert_eq!(super::resolve_collision(20, 50, 999, None), 20);
}

#[test]
fn a_collision_favours_the_room_that_matches_where_the_player_stands() {
    // Two rooms can still share a cell (a home interior over the street, a
    // wing folded back over its own zone). When the player is in neither, the
    // map must favour the one that shares a region with wherever they *are*,
    // not whichever id is lower - painting an unrelated region around a player
    // who's clearly standing in one specific land is the bug this guards. The
    // pair is named rather than mined out of the field: the field's remaining
    // collisions are all within one region, and the tie-break is about regions.
    use crate::app::door::lateania::world::region_atlas_entry;
    let (lower, higher) = (308, 651);
    assert_ne!(
        region_atlas_entry(lower),
        region_atlas_entry(higher),
        "fixture assumption broke: {lower} and {higher} should be in different regions"
    );
    let (higher_region, _) = region_atlas_entry(higher).expect("higher room has a region");

    // No region context (or the player is elsewhere with no matching room):
    // the lowest id still wins, same as before this feature.
    assert_eq!(
        super::resolve_collision(lower, higher, 999_999, None),
        lower
    );
    // The player stands somewhere in the higher room's region: that room
    // wins the cell instead, even though its id is larger.
    assert_eq!(
        super::resolve_collision(lower, higher, 999_999, Some(higher_region)),
        higher
    );
}

#[test]
fn viewport_explored_paints_a_collision_as_the_players_own_region() {
    use crate::app::door::lateania::world::region_atlas_entry;
    let world = seed_world();
    // Stack a named cross-region pair on one cell: the field no longer folds
    // two regions onto each other by itself, and this is a test of how the
    // viewport resolves such a cell, not of whether one happens to exist.
    let (lower, higher) = (308, 651);
    let mut coords = derive_coords(&world);
    let cell = coords[&lower];
    coords.insert(higher, cell);
    let (higher_region, _) = region_atlas_entry(higher).expect("higher room has a region");
    assert_ne!(
        region_atlas_entry(lower),
        region_atlas_entry(higher),
        "fixture assumption broke: {lower} and {higher} should be in different regions"
    );

    // Some other room in the higher room's region, not part of the collision
    // itself, so only the region match can explain the result.
    let stand_in = world
        .rooms
        .keys()
        .copied()
        .find(|&id| {
            id != lower
                && id != higher
                && region_atlas_entry(id).map(|(name, _)| name) == Some(higher_region)
        })
        .expect("the higher room's region has more than one room");

    let visited: std::collections::HashSet<_> = [lower, higher].into_iter().collect();
    let (cols, rows) = (21, 11);
    let grid = super::viewport_explored(&coords, cell, cols, rows, &visited, stand_in);
    assert_eq!(
        grid[rows as usize / 2][cols as usize / 2],
        Some(higher),
        "standing in {higher_region} should paint the colliding cell as the room in that region"
    );
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

#[test]
fn map_canvas_draws_corridors_between_visited_rooms_and_fogs_the_rest() {
    use super::Tile;
    use std::collections::HashSet;
    let world = seed_world();
    let coords = derive_coords(&world);
    let start = world.start_room;
    let center = coords[&start];
    let (cols, rows) = (41, 21);
    let (cx, cy) = (cols as usize / 2, rows as usize / 2);

    // Everything visited: the centre is the start room, and corridors render.
    let all: HashSet<_> = world.rooms.keys().copied().collect();
    let canvas = super::map_canvas(&coords, center, cols, rows, &all, start);
    assert!(matches!(canvas[cy][cx], Tile::Room(id) if id == start));
    assert!(
        canvas
            .iter()
            .flatten()
            .any(|t| matches!(t, Tile::LinkH | Tile::LinkV)),
        "linked rooms should show corridors"
    );

    // Nothing visited: only the player shows, and no corridors leak the layout.
    let empty = HashSet::new();
    let fogged = super::map_canvas(&coords, center, cols, rows, &empty, start);
    let visible_rooms: Vec<_> = fogged
        .iter()
        .flatten()
        .filter_map(|t| match t {
            Tile::Room(id) => Some(*id),
            _ => None,
        })
        .collect();
    assert_eq!(visible_rooms, vec![start], "only the player under full fog");
    assert!(
        !fogged
            .iter()
            .flatten()
            .any(|t| matches!(t, Tile::LinkH | Tile::LinkV)),
        "no corridors into the unknown"
    );
}

#[test]
fn poi_arrows_point_off_screen_pois_to_the_border() {
    let world = seed_world();
    let coords = derive_coords(&world);
    let center = coords[&world.start_room];
    let (cols, rows) = (21, 11);
    let arrows = super::poi_arrows(&coords, center, cols, rows);

    assert!(!arrows.is_empty(), "distant POIs produce border arrows");
    assert!(
        arrows.iter().any(|a| a.boss),
        "at least one off-map boss arrow"
    );
    for a in &arrows {
        assert!(a.row < rows as usize && a.col < cols as usize);
        let on_border =
            a.row == 0 || a.row == rows as usize - 1 || a.col == 0 || a.col == cols as usize - 1;
        assert!(on_border, "arrow {a:?} must sit on the viewport border");
        assert!(
            "\u{2190}\u{2191}\u{2192}\u{2193}\u{2196}\u{2197}\u{2198}\u{2199}".contains(a.glyph)
        );
    }
}

// A discovered room whose neighbours are all still fog must not read as a
// stranded island: each exit into the unknown gets a faint half-stub of path
// so the player can see a trail continues that way (direction only, no
// spoiler, and never an arrow - arrows read as controls).
#[test]
fn a_discovered_room_ringed_by_fog_shows_exit_hints() {
    use super::Tile;
    let world = seed_world();
    let coords = derive_coords(&world);

    // Find a room with at least one flat (N/S/E/W) exit whose neighbour sits in
    // the adjacent cell, so the hint has an empty cell to land in.
    let (&anchor, _) = world
        .rooms
        .iter()
        .find(|(id, room)| {
            let c = coords[*id];
            room.exits.iter().any(|(_dir, dst)| {
                coords
                    .get(dst)
                    .is_some_and(|dc| dc.z == c.z && (dc.x - c.x).abs() + (dc.y - c.y).abs() == 1)
            })
        })
        .expect("world has a room with a unit-adjacent flat exit");

    // Only the anchor is explored - every neighbour is fog.
    let visited: std::collections::HashSet<_> = std::iter::once(anchor).collect();
    let canvas = super::map_canvas(&coords, coords[&anchor], 21, 21, &visited, anchor);

    let hints = canvas
        .iter()
        .flatten()
        .filter(|t| matches!(t, Tile::Hint(_)))
        .count();
    assert!(
        hints > 0,
        "a discovered room surrounded by fog must sprout at least one exit hint"
    );
    // Hints are path stubs (the corridor glyphs), never arrows.
    for row in &canvas {
        for tile in row {
            if let Tile::Hint(g) = tile {
                assert!(
                    "\u{2500}\u{2502}".contains(*g),
                    "hint glyph {g:?} is not a path stub"
                );
            }
        }
    }
}

// A link to a room the player has *already visited* but that the flat grid
// can't draw right beside it (a scattered branch, or a jump into a whole
// other reserved block like the Sunderlakes off Melvanala) must read
// differently from a plain fog `Hint` - it's a known place, not the edge of
// exploration.
#[test]
fn a_link_to_an_already_visited_scattered_room_shows_a_known_hint() {
    use super::Tile;
    let world = seed_world();
    let coords = derive_coords(&world);

    // Every same-level, non-adjacent link in the world, in a stable order so
    // the test is deterministic despite `world.rooms` being a HashMap.
    let mut room_ids: Vec<_> = world.rooms.keys().copied().collect();
    room_ids.sort_unstable();
    let mut candidates: Vec<(_, _)> = Vec::new();
    for id in room_ids {
        let Some(&c) = coords.get(&id) else { continue };
        let mut dests: Vec<_> = world.rooms[&id].exits.values().copied().collect();
        dests.sort_unstable();
        for dst in dests {
            if let Some(&dc) = coords.get(&dst)
                && dc.z == c.z
                && (dc.x - c.x).abs() + (dc.y - c.y).abs() != 1
            {
                candidates.push((id, dst));
            }
        }
    }
    assert!(
        !candidates.is_empty(),
        "world has a same-level link that isn't unit-adjacent"
    );

    // A candidate's arrow can be shadowed by one of the anchor's own other
    // exits landing on the same adjacent cell first, so scan for one that
    // actually renders rather than assuming the first candidate always will.
    let renders = candidates.into_iter().any(|(anchor, dest)| {
        let visited: std::collections::HashSet<_> = [anchor, dest].into_iter().collect();
        let canvas = super::map_canvas(&coords, coords[&anchor], 21, 21, &visited, anchor);
        canvas
            .iter()
            .flatten()
            .any(|t| matches!(t, Tile::HintKnown(_)))
    });
    assert!(
        renders,
        "at least one already-visited scattered link should show a known hint, not plain fog"
    );
}

// The POI index carries every marker kind the map draws, and the "notable foe"
// marker stays rare: one regional champion per land, never a per-room carpet
// (the endgame is wall-to-wall max-level mobs, so a level threshold would flood).
#[test]
fn poi_index_has_every_marker_kind_and_elite_stays_rare() {
    let p = super::pois();
    let has = |f: fn(&super::Poi) -> bool| p.values().filter(|x| f(x)).count();
    assert!(has(|x| x.boss.is_some()) > 0, "bosses indexed");
    assert!(has(|x| x.tameable.is_some()) > 0, "tameable beasts indexed");
    assert!(has(|x| x.gather.is_some()) > 0, "gather nodes indexed");

    let elite = has(|x| x.elite_foe.is_some());
    assert!(elite > 0, "at least one regional champion");
    // One apex per region: comfortably under any per-region-count ceiling and
    // nowhere near the thousands a raw level threshold would mark.
    assert!(
        elite < 40,
        "elite foe markers must stay rare (one per land), got {elite}"
    );
    // A champion room is never also a boss room (bosses take precedence).
    for poi in p.values() {
        if poi.elite_foe.is_some() {
            assert!(
                poi.boss.is_none(),
                "a champion room must not also be a boss"
            );
        }
    }
}

// Every zone chains to the next one by a stair, so "which way is onward" is
// always a vertical exit - the one thing a flat level cannot draw as a
// corridor. The map has to say so on the room itself, or it hides the only
// route out of the zone the player is standing in.
#[test]
fn a_room_with_a_way_down_shows_a_stair_on_the_map() {
    let coords = super::world_coords();
    // Embergate's square carries both: `Down` is the Frontier descent and `Up`
    // the city districts, each claimed at runtime by its own `extend_*`. So it
    // exercises the real built world and the both-ways glyph at once.
    let square = super::world().start_room;
    let exits = &super::world().rooms[&square].exits;
    assert!(
        exits.contains_key(&super::Dir::Down) && exits.contains_key(&super::Dir::Up),
        "the square keeps its Frontier stair down and its city stair up"
    );
    let visited = std::collections::HashSet::from([square]);
    let canvas = super::map_canvas(coords, coords[&square], 21, 11, &visited, square);
    let stairs: Vec<char> = canvas
        .iter()
        .flatten()
        .filter_map(|t| match t {
            super::Tile::Stair(ch) => Some(*ch),
            _ => None,
        })
        .collect();
    assert_eq!(
        stairs,
        vec!['\u{25be}'],
        "a room with both ways reads as one ▾: down is the way onward, and the \
         exits line carries the up"
    );
}

// The stair layer sits in each room's own corner cell. That only works if a
// corner belongs to exactly one room and to nothing else the canvas draws;
// otherwise a stair would silently erase a corridor, or two rooms would fight
// over one marker.
#[test]
fn stair_corners_never_collide_with_rooms_corridors_or_each_other() {
    let coords = super::world_coords();
    // A dense hand-authored neighbourhood with stairs, houses and roads in it.
    let here = super::world().start_room;
    let visited: std::collections::HashSet<_> = super::world().rooms.keys().copied().collect();
    let canvas = super::map_canvas(coords, coords[&here], 41, 21, &visited, here);
    for (r, row) in canvas.iter().enumerate() {
        for (c, tile) in row.iter().enumerate() {
            // Rooms land on even offsets from the centre cell, corridors on the
            // odd cell between two of them, stairs on the odd/odd corner.
            let (even_col, even_row) = ((c % 2 == 41 / 2 % 2), (r % 2 == 21 / 2 % 2));
            match tile {
                super::Tile::Room(_) => assert!(
                    even_col && even_row,
                    "a room must sit on the room layer at ({c},{r})"
                ),
                super::Tile::Stair(_) => assert!(
                    !even_col && !even_row,
                    "a stair must sit on the free corner layer at ({c},{r})"
                ),
                super::Tile::LinkH | super::Tile::LinkV => assert!(
                    even_col != even_row,
                    "a corridor must sit between two rooms at ({c},{r})"
                ),
                _ => {}
            }
        }
    }
}

// Routing answers the question the picture cannot: not "where is it" but
// "which exit do I take from here". It walks only ground the player has
// already covered, so it can never point at an unexplored shortcut.
#[test]
fn a_route_names_the_first_exit_to_take_and_the_distance() {
    let w = super::world();
    let start = w.start_room;
    // Two real rooms out from the square, chosen by walking the graph rather
    // than assuming any particular exit leads on.
    let (first_dir, second, third) = w.rooms[&start]
        .exits
        .iter()
        .filter_map(|(d, next)| {
            let onward = w.rooms.get(next)?.exits.values().find(|t| **t != start)?;
            Some((*d, *next, *onward))
        })
        .min_by_key(|(_, second, third)| (*second, *third))
        .expect("the square leads two rooms out");
    let visited = std::collections::HashSet::from([start, second, third]);

    let one = super::route(start, second, &visited).expect("a route to the neighbour");
    assert_eq!(one.next, first_dir, "the first step is the exit to take");
    assert_eq!(one.rooms, 1, "a neighbour is one room away");

    let two = super::route(start, third, &visited).expect("a route two rooms out");
    assert_eq!(two.rooms, 2);
    assert_eq!(
        two.next, first_dir,
        "a longer route still names the very next exit, not the last one"
    );

    // Standing on the destination is not a route, and neither is a place the
    // player has never been - no route may reveal unexplored ground.
    assert_eq!(super::route(start, start, &visited), None);
    let unvisited = std::collections::HashSet::from([start]);
    assert_eq!(
        super::route(start, second, &unvisited),
        None,
        "a room the player has never seen is not a destination"
    );
}

// A stub for a link the flat grid cannot draw adjacently must sit on the side
// the player would actually walk out of. The house interiors are the sharpest
// case in the world: each one is its own component in the coordinate field, so
// the close can land thousands of cells to the *west* of a house whose door
// out faces *east*. Siding the stub by coordinate delta drew a path west, and
// walking west then failed - the map inventing a path that is not there is the
// single worst thing it can do.
#[test]
fn a_scattered_links_stub_follows_the_exit_not_the_coordinate_delta() {
    use crate::app::door::lateania::housing::{HOUSING_BASE, plot_base};
    let w = super::world();
    let coords = super::world_coords();
    let entrance = plot_base(2); // Timber Longhouse: its way out faces east.
    assert_eq!(
        w.rooms[&entrance].exits.get(&super::Dir::East),
        Some(&HOUSING_BASE),
        "the longhouse door out faces east onto the close"
    );
    assert!(
        coords[&HOUSING_BASE].x < coords[&entrance].x,
        "and the close sits west of it in the field, which is what used to \
         decide the stub's side"
    );

    let visited: std::collections::HashSet<_> = w.rooms.keys().copied().collect();
    let (cols, rows) = (11, 7);
    let canvas = super::map_canvas(coords, coords[&entrance], cols, rows, &visited, entrance);
    let (cx, cy) = ((cols / 2) as usize, (rows / 2) as usize);
    assert!(
        matches!(canvas[cy][cx + 1], super::Tile::HintKnown(_)),
        "the way out reads on the east side, where walking east is what you do"
    );
    assert_eq!(
        canvas[cy][cx - 1],
        super::Tile::Empty,
        "and nothing suggests a path west, because there is no way west"
    );
}

// The map's gather marker used to carry only a skill name, with no way to
// scout whether a node was even worth the walk before physically standing in
// its room - the level gate was only ever shown as an in-room refusal reason
// after arriving under-levelled. It must be visible on the map itself now.
#[test]
fn gather_poi_carries_the_nodes_real_level_requirement() {
    use crate::app::door::lateania::world::NODES;

    let node = NODES
        .iter()
        .find(|n| n.level_req > 0)
        .expect("at least one gather node has a real level gate");
    let poi = super::poi(node.home).expect("the node's room is indexed");
    let gather = poi.gather.expect("a gather node room carries a GatherPoi");
    assert_eq!(gather.skill, node.skill.key());
    assert_eq!(
        gather.level_req, node.level_req,
        "the map's level requirement must match the real gate, not a placeholder"
    );
}

// Quest-target arrows keep the same honesty rule as POI arrows: a target in
// another reserved block gets no arrow (the coordinate delta there points
// nowhere real) and is counted as "beyond this land" instead, so the map can
// say what it dropped rather than silently under-reporting.
#[test]
fn quest_arrows_stay_honest_across_reserved_blocks() {
    let world = seed_world();
    let coords = derive_coords(&world);
    let center = coords[&world.start_room];
    let (cols, rows) = (21, 11);

    // A same-block off-screen target: a King's Road room a few steps south.
    let near = 10;
    // A cross-block target: the Frontier's first zone entrance sits in its own
    // reserved block, far outside PAN_LIMIT.
    let far = 2000;
    assert!(
        (coords[&far].x - center.x).abs() > super::PAN_LIMIT
            || (coords[&far].y - center.y).abs() > super::PAN_LIMIT
            || coords[&far].z != center.z,
        "test premise: the Frontier target lies beyond the pan range"
    );

    let (arrows, beyond) = super::quest_arrows(&coords, center, cols, rows, &[near, far]);
    assert_eq!(beyond, 1, "the cross-block target is counted, not drawn");
    for a in &arrows {
        assert!(a.row < rows as usize && a.col < cols as usize);
        assert!(
            "\u{2190}\u{2191}\u{2192}\u{2193}\u{2196}\u{2197}\u{2198}\u{2199}".contains(a.glyph)
        );
    }
}

#[test]
fn the_land_graph_is_read_off_the_room_graph_and_covers_every_region() {
    let links = super::land_links();

    // Every atlas region has an entry, so a new country can never be silently
    // missing from the graph the map is drawn from.
    let mut named: Vec<&str> = links.keys().copied().collect();
    let mut names = super::super::world::region_names();
    named.sort_unstable();
    names.sort_unstable();
    assert_eq!(named, names);

    // Roads are two-way, because they are read off real exits in both rooms.
    for (&here, theres) in links {
        for &there in theres {
            assert!(
                links[there].contains(&here),
                "{here} -> {there} has no road back"
            );
        }
    }

    // The two portal-only regions are portal-only because their rooms hold no
    // directional exits at all, not because a table says so.
    assert_eq!(
        super::portal_lands(),
        vec!["Portal Villages", "The Shattered Archipelago"]
    );

    // Kaelmyr's only door is the one inside Yssgar's chamber, and nothing walks
    // from the overworld straight into the Faewood: `extend_silvael` splices
    // the city into the road, so the walk goes savanna -> Silvael -> Aelunor.
    assert_eq!(
        links["Kaelmyr, the Ashen Reach"],
        vec!["The Sundered Reaches"]
    );
    assert_eq!(links["Aelunor, the Faewood"], vec!["Silvael"]);
    assert!(links["Silvael"].contains(&"The Overworld & Capitals"));
    assert!(!links["The Overworld & Capitals"].contains(&"Aelunor, the Faewood"));
}
