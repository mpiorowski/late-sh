use super::*;

#[test]
fn every_exit_resolves_to_a_real_room() {
    let world = seed_world();
    for room in world.rooms.values() {
        for (dir, target) in &room.exits {
            assert!(
                world.rooms.contains_key(target),
                "room {} ({}) has a {} exit to missing room {}",
                room.id,
                room.name,
                dir.label(),
                target
            );
        }
    }
}

#[test]
fn exits_are_reciprocal_where_expected() {
    // Embergate square (1) <-> south gate (5): going south then north returns.
    let world = seed_world();
    let square = world.room(1).expect("square exists");
    let gate_id = square.exits.get(&Dir::South).copied().expect("south exit");
    let gate = world.room(gate_id).expect("gate exists");
    assert_eq!(gate.exits.get(&Dir::North).copied(), Some(1));
}

#[test]
fn every_home_has_a_way_back_out() {
    use super::super::housing as housing_mod;
    let world = seed_world();
    // Can `from` reach `target` by following exits across the whole graph?
    let can_reach = |from: RoomId, target: RoomId| -> bool {
        let mut seen = std::collections::HashSet::from([from]);
        let mut stack = vec![from];
        while let Some(r) = stack.pop() {
            if r == target {
                return true;
            }
            if let Some(room) = world.room(r) {
                for &to in room.exits.values() {
                    if seen.insert(to) {
                        stack.push(to);
                    }
                }
            }
        }
        false
    };
    // No home may be a trap: every housing room must be able to get back to
    // the start room (this catches a door whose only exit leads deeper).
    for &id in world.rooms.keys() {
        if housing_mod::is_housing_room(id) {
            assert!(
                can_reach(id, world.start_room),
                "housing room {id} is trapped - no way back out without recall"
            );
        }
    }
}

#[test]
fn city_districts_are_a_walkable_street_not_dead_end_rooms() {
    let world = seed_world();
    // Each capital's district lives at 3000 + c*10: a spine plus four haunts.
    for c in 0..4 {
        let base = 3000 + c * 10;
        let haunts: Vec<RoomId> = (base + 1..base + 5).collect();
        // Every haunt exists and can be walked into a sibling haunt (a street),
        // not merely dead-end back at the spine.
        let connects_to_sibling = haunts.iter().any(|&id| {
            world
                .room(id)
                .is_some_and(|r| r.exits.values().any(|to| haunts.contains(to)))
        });
        assert!(
            world.room(base).is_some(),
            "district spine {base} should exist"
        );
        assert!(
            connects_to_sibling,
            "city district at {base} is dead-end rooms off a hub, not a walkable street"
        );
    }
}

#[test]
fn start_room_exists_and_is_safe() {
    let world = seed_world();
    let start = world.room(world.start_room).expect("start room exists");
    assert!(start.safe, "players should spawn in a safe room");
}

#[test]
fn world_has_expected_size_and_every_mob_homes_to_a_real_room() {
    let world = seed_world();
    let count_in = |lo: RoomId, hi: RoomId| {
        world
            .rooms
            .keys()
            .filter(|id| **id >= lo && **id < hi)
            .count()
    };
    // 198 base + extension rooms, 100 overworld rooms, and the 1000
    // procedural Frontier rooms (rooms 2000+) all sit below room 5000.
    let original = count_in(0, 5000);
    assert_eq!(
        original, 1318,
        "expected 1318 original rooms (incl. 20 city-district rooms)"
    );
    // The two maze regions are full grids of rooms; the cave is sparse
    // (only the largest connected pocket survives), so it is bounded but
    // not exact.
    let catacombs = count_in(CATACOMBS_BASE, THORNWOOD_BASE);
    let thornwood = count_in(THORNWOOD_BASE, CAVERNS_BASE);
    let caverns = count_in(
        CAVERNS_BASE,
        CAVERNS_BASE + (CAVERNS_W * CAVERNS_H) as RoomId,
    );
    assert_eq!(catacombs, CATACOMBS_W * CATACOMBS_H, "catacombs room count");
    assert_eq!(thornwood, THORNWOOD_W * THORNWOOD_H, "thornwood room count");
    assert!(
        (40..=CAVERNS_W * CAVERNS_H).contains(&caverns),
        "drowned caverns should be a sane size, got {caverns}"
    );
    // The housing district: the close plus one home of each tier.
    use super::super::housing as housing_mod;
    let housing = count_in(housing_mod::HOUSING_BASE, housing_mod::HOUSING_BASE + 1000);
    let expected_housing = 1 + housing_mod::TIERS.iter().map(|t| t.rooms()).sum::<usize>();
    assert_eq!(housing, expected_housing, "housing district room count");
    // The Sundered Reaches: a second continent of braided mazes and organic
    // caverns. Mazes fill their cell field; caverns are sparse, so the total
    // is a sane band below the 1000-cell id range rather than an exact count.
    let reaches = count_in(
        REACHES_BASE,
        REACHES_BASE + REACHES_ZONES as RoomId * REACHES_ZONE_STRIDE,
    );
    assert!(
        (750..=1000).contains(&reaches),
        "the Sundered Reaches should be ~900 rooms, got {reaches}"
    );
    // Kaelmyr, the Ashen Reach: a third continent of braided mazes and organic
    // calderas (rooms 12000+). Mazes fill their cell field; calderas are
    // sparse, so the total is a sane band rather than an exact count.
    let kaelmyr = count_in(
        KAELMYR_BASE,
        KAELMYR_BASE + KAELMYR_ZONES as RoomId * KAELMYR_ZONE_STRIDE,
    );
    assert!(
        (1800..=KAELMYR_ZONES * KAELMYR_W * KAELMYR_H).contains(&kaelmyr),
        "Kaelmyr should be ~2000 rooms, got {kaelmyr}"
    );
    // The Sunderlakes: a peaceful water country of reed-mazes and flooded
    // caverns (rooms 16000+). Mazes fill their cell field; caverns are
    // sparse, so the total is a sane band rather than an exact count.
    let lakes = count_in(
        LAKES_BASE,
        LAKES_BASE + LAKES_ZONES as RoomId * LAKES_ZONE_STRIDE,
    );
    assert!(
        (900..=LAKES_ZONES * LAKES_W * LAKES_H).contains(&lakes),
        "the Sunderlakes should be ~1200 rooms, got {lakes}"
    );
    // Broceliande, the Greenwood: a fourth continent of braided briar-mazes
    // and organic fern-caverns (rooms 22000+). Mazes fill their cell field;
    // caverns are sparse, so the total is a sane band rather than an exact
    // count.
    let broceliande = count_in(
        BROCELIANDE_BASE,
        BROCELIANDE_BASE + BROCELIANDE_ZONES as RoomId * BROCELIANDE_ZONE_STRIDE,
    );
    assert!(
        (1600..=BROCELIANDE_ZONES * BROCELIANDE_W * BROCELIANDE_H).contains(&broceliande),
        "Broceliande should be ~2000 rooms, got {broceliande}"
    );
    // The Shattered Archipelago: portal villages + maze/cavern islands.
    use super::super::archipelago as arch;
    let villages = count_in(arch::VILLAGE_BASE, arch::VILLAGE_BASE + 1000);
    assert_eq!(villages, arch::VILLAGES.len(), "one room per village");
    let islands = count_in(
        arch::ARCH_BASE,
        arch::ARCH_BASE + arch::ISLAND_COUNT as RoomId * arch::ARCH_STRIDE,
    );
    assert!(
        (750..=1000).contains(&islands),
        "the archipelago should be ~900 rooms, got {islands}"
    );
    // No stray rooms outside the known groups.
    assert_eq!(
        world.rooms.len(),
        original
            + catacombs
            + thornwood
            + caverns
            + housing
            + reaches
            + kaelmyr
            + lakes
            + broceliande
            + villages
            + islands,
        "every room should belong to a known region"
    );
    for spawn in &world.spawns {
        assert!(
            world.rooms.contains_key(&spawn.home),
            "mob {} ({}) homes to missing room {}",
            spawn.id,
            spawn.name,
            spawn.home
        );
    }
}

#[test]
fn the_reaches_are_mazes_and_caverns_not_grids() {
    let world = seed_world();
    let reaches: Vec<&Room> = world
        .rooms
        .values()
        .filter(|r| is_reaches_room(r.id))
        .collect();
    // Plenty of rooms - a real continent.
    assert!(reaches.len() >= 750, "the Reaches are sizeable");
    // A uniform grid has no dead-ends; a braided maze/cavern has many. The
    // presence of degree-1 rooms (and varied degree overall) proves shape.
    let dead_ends = reaches.iter().filter(|r| r.exits.len() == 1).count();
    assert!(
        dead_ends >= 20,
        "the Reaches should wind into dead-ends, not be square blocks (got {dead_ends})"
    );
    let degrees: std::collections::HashSet<usize> =
        reaches.iter().map(|r| r.exits.len()).collect();
    assert!(
        degrees.len() >= 3,
        "rooms should vary in how many ways they branch (got {degrees:?})"
    );
}

#[test]
fn kaelmyr_is_mazes_and_calderas_not_grids() {
    let world = seed_world();
    let kaelmyr: Vec<&Room> = world
        .rooms
        .values()
        .filter(|r| is_kaelmyr_room(r.id))
        .collect();
    // A real continent of rooms (~2000).
    assert!(kaelmyr.len() >= 1800, "Kaelmyr is a sizeable continent");
    // A uniform grid has no dead-ends; braided mazes and calderas have many.
    let dead_ends = kaelmyr.iter().filter(|r| r.exits.len() == 1).count();
    assert!(
        dead_ends >= 20,
        "Kaelmyr should wind into dead-ends, not be square blocks (got {dead_ends})"
    );
    let degrees: std::collections::HashSet<usize> =
        kaelmyr.iter().map(|r| r.exits.len()).collect();
    assert!(
        degrees.len() >= 3,
        "Kaelmyr rooms should vary in how many ways they branch (got {degrees:?})"
    );
}

#[test]
fn kaelmyr_is_reachable_gated_and_behaviour_driven() {
    let world = seed_world();
    // The whole continent hangs off Yssgar's chamber in the Reaches, so a BFS
    // from the Reaches base reaches into Kaelmyr.
    let mut seen = HashSet::new();
    let mut stack = vec![world.start_room];
    while let Some(id) = stack.pop() {
        if !seen.insert(id) {
            continue;
        }
        if let Some(r) = world.room(id) {
            for to in r.exits.values() {
                stack.push(*to);
            }
        }
    }
    assert!(
        world.rooms.keys().any(|id| is_kaelmyr_room(*id)),
        "Kaelmyr rooms exist"
    );
    assert!(
        world
            .rooms
            .keys()
            .filter(|id| is_kaelmyr_room(**id))
            .all(|id| seen.contains(id)),
        "every Kaelmyr room must be reachable from the start"
    );
    // The entrance hangs off a real Reaches room via Up, and that room links
    // back down into Kaelmyr - the gated sea-gate spine, reciprocal.
    let entrance = world.room(KAELMYR_BASE).expect("Kaelmyr ash-gate exists");
    let up = entrance.exits.get(&Dir::Up).copied();
    assert!(
        up.is_some_and(is_reaches_room),
        "the Kaelmyr entrance rises into the Reaches"
    );
    let reaches_room = world.room(up.unwrap()).expect("the reaches gate room");
    assert!(
        reaches_room.exits.get(&Dir::Down) == Some(&KAELMYR_BASE),
        "the Reaches gate descends into Kaelmyr"
    );
    // Kaelmyr foes are all behaviour-driven, with several distinct behaviours.
    // Filter by home room (not an open id bound), so later continents with
    // even higher mob ids can't leak into Kaelmyr's count.
    let spawns: Vec<&MobSpawn> = world
        .spawns
        .iter()
        .filter(|s| s.id >= KAELMYR_SPAWN_ID_START && is_kaelmyr_room(s.home))
        .collect();
    assert!(!spawns.is_empty(), "Kaelmyr should be populated");
    let mut kinds = HashSet::new();
    for s in &spawns {
        let b = world.behavior_of(s.id);
        assert_ne!(
            b,
            MobBehavior::Sentinel,
            "{} should have a behavior",
            s.name
        );
        kinds.insert(std::mem::discriminant(&b));
    }
    assert!(kinds.len() >= 4, "Kaelmyr should field varied behaviours");
    // Every zone has exactly one boss, and Kaelmyr loot resolves and stays
    // clear of the Frontier/Reaches catalogs.
    let bosses = spawns.iter().filter(|s| s.boss).count();
    assert_eq!(bosses, KAELMYR_ZONES, "one boss per Kaelmyr zone");
    for s in &spawns {
        for id in s.loot {
            assert!(
                (3400..3600).contains(id),
                "{} should drop Kaelmyr catalog loot (3400..3600), got {id}",
                s.name
            );
            assert!(
                crate::app::door::lateania::items::item(*id).is_some(),
                "{} drops missing item {id}",
                s.name
            );
        }
    }
}

#[test]
fn the_sunderlakes_are_mazes_and_caverns_not_grids() {
    let world = seed_world();
    let lakes: Vec<&Room> = world
        .rooms
        .values()
        .filter(|r| is_lakes_room(r.id))
        .collect();
    // A real, sizeable water country (~1200 rooms).
    assert!(lakes.len() >= 900, "the Sunderlakes are sizeable");
    // A uniform grid has no dead-ends; braided reed-mazes and flooded
    // caverns have many. Dead-ends + varied branching prove the shape.
    let dead_ends = lakes.iter().filter(|r| r.exits.len() == 1).count();
    assert!(
        dead_ends >= 20,
        "the Sunderlakes should wind into dead-ends, not be square blocks (got {dead_ends})"
    );
    let degrees: std::collections::HashSet<usize> =
        lakes.iter().map(|r| r.exits.len()).collect();
    assert!(
        degrees.len() >= 3,
        "Sunderlakes rooms should vary in how many ways they branch (got {degrees:?})"
    );
}

#[test]
fn the_sunderlakes_are_reachable_peaceful_and_full_of_fish() {
    let world = seed_world();
    // Reachable by a normal walk from the start (hung off Melvanala's lake).
    let mut seen = HashSet::new();
    let mut stack = vec![world.start_room];
    while let Some(id) = stack.pop() {
        if !seen.insert(id) {
            continue;
        }
        if let Some(r) = world.room(id) {
            for to in r.exits.values() {
                stack.push(*to);
            }
        }
    }
    assert!(
        world.rooms.keys().any(|id| is_lakes_room(*id)),
        "Sunderlakes rooms exist"
    );
    assert!(
        world
            .rooms
            .keys()
            .filter(|id| is_lakes_room(**id))
            .all(|id| seen.contains(id)),
        "every Sunderlakes room must be reachable from the start"
    );
    // The entrance landing rises into the Melvanala high lake and back down.
    let entrance = world.room(LAKES_BASE).expect("Sunderlakes landing exists");
    assert!(entrance.safe, "the Anglers' Dock landing is a safe haven");
    assert!(
        entrance.exits.values().any(|to| *to == MELVANALA_SQUARE),
        "the Sunderlakes hang off the Melvanala lake"
    );
    // Peaceful: fewer, weaker foes than Kaelmyr. Every zone has one notable.
    let spawns: Vec<&MobSpawn> = world
        .spawns
        .iter()
        .filter(|s| s.id >= LAKES_SPAWN_ID_START && is_lakes_room(s.home))
        .collect();
    let bosses = spawns.iter().filter(|s| s.boss).count();
    assert_eq!(bosses, LAKES_ZONES, "one notable per Sunderlakes zone");
    let king = world
        .spawns
        .iter()
        .find(|s| s.name == "the King Who Was Promised Nothing")
        .expect("the Frontier king spawns");
    assert!(
        spawns.iter().all(|s| s.damage < king.damage),
        "the Sunderlakes stay gentler than the endgame"
    );
    // The heart of the region: forty fish, caught at Fishing nodes across the
    // lakes, every node yielding a real fish gated by the Fishing skill.
    let fish_nodes: Vec<&ResourceNode> = NODES
        .iter()
        .filter(|nn| nn.skill == GatherSkill::Fishing && is_lakes_room(nn.home))
        .collect();
    assert_eq!(
        fish_nodes.len(),
        super::super::items::FISH_COUNT as usize,
        "one fishing spot per fish species is seeded in the lakes"
    );
    let mut species = HashSet::new();
    for nn in &fish_nodes {
        assert!(
            world.rooms.contains_key(&nn.home),
            "fishing spot {:?} homes to a real lake room",
            nn.name
        );
        let fid = nn.yield_item;
        assert!(
            (super::super::items::FISH_BASE
                ..super::super::items::FISH_BASE + super::super::items::FISH_COUNT)
                .contains(&fid),
            "a lake fishing spot yields a fish (4600 band), got {fid}"
        );
        assert!(
            super::super::items::item(fid).is_some(),
            "fishing spot yields a real fish item {fid}"
        );
        species.insert(fid);
    }
    assert_eq!(
        species.len(),
        super::super::items::FISH_COUNT as usize,
        "all forty fish species are catchable"
    );
    // The gates rise: the shallowest spot is open to any angler, the deepest
    // demands real Fishing training.
    let min_gate = fish_nodes.iter().map(|nn| nn.level_req).min().unwrap();
    let max_gate = fish_nodes.iter().map(|nn| nn.level_req).max().unwrap();
    assert!(min_gate <= 2, "shallow fish are open to beginners");
    assert!(max_gate >= 40, "the prized deep fish need a trained angler");
}

#[test]
fn broceliande_is_mazes_and_caverns_not_grids() {
    let world = seed_world();
    let wood: Vec<&Room> = world
        .rooms
        .values()
        .filter(|r| is_broceliande_room(r.id))
        .collect();
    // A real, sizeable green continent (~2000 rooms).
    assert!(wood.len() >= 1600, "Broceliande is a sizeable continent");
    // A uniform grid has no dead-ends; braided briar-mazes and organic
    // fern-caverns have many. Dead-ends + varied branching prove the shape.
    let dead_ends = wood.iter().filter(|r| r.exits.len() == 1).count();
    assert!(
        dead_ends >= 20,
        "Broceliande should wind into dead-ends, not be square blocks (got {dead_ends})"
    );
    let degrees: std::collections::HashSet<usize> =
        wood.iter().map(|r| r.exits.len()).collect();
    assert!(
        degrees.len() >= 3,
        "Broceliande rooms should vary in how many ways they branch (got {degrees:?})"
    );
}

#[test]
fn broceliande_is_reachable_gated_and_behaviour_driven() {
    let world = seed_world();
    // Reachable by a normal walk from the start (hung off the Verdant
    // Highlands' Faerie Hollow).
    let mut seen = HashSet::new();
    let mut stack = vec![world.start_room];
    while let Some(id) = stack.pop() {
        if !seen.insert(id) {
            continue;
        }
        if let Some(r) = world.room(id) {
            for to in r.exits.values() {
                stack.push(*to);
            }
        }
    }
    assert!(
        world.rooms.keys().any(|id| is_broceliande_room(*id)),
        "Broceliande rooms exist"
    );
    assert!(
        world
            .rooms
            .keys()
            .filter(|id| is_broceliande_room(**id))
            .all(|id| seen.contains(id)),
        "every Broceliande room must be reachable from the start"
    );
    // The first forest gate is a safe haven hung off a Verdant Highlands room.
    let entrance = world
        .room(BROCELIANDE_BASE)
        .expect("Broceliande forest gate exists");
    assert!(entrance.safe, "the Woodward's Holt landing is a safe haven");
    assert!(
        entrance.exits.values().any(|to| (680u32..692).contains(to)),
        "Broceliande hangs off the Verdant Highlands by a walk"
    );
    // Foes are behaviour-driven with several distinct behaviours; filter by
    // home room so nothing else can leak into the count.
    let spawns: Vec<&MobSpawn> = world
        .spawns
        .iter()
        .filter(|s| s.id >= BROCELIANDE_SPAWN_ID_START && is_broceliande_room(s.home))
        .collect();
    assert!(!spawns.is_empty(), "Broceliande should be populated");
    let mut kinds = HashSet::new();
    for s in &spawns {
        let b = world.behavior_of(s.id);
        assert_ne!(
            b,
            MobBehavior::Sentinel,
            "{} should have a behavior",
            s.name
        );
        kinds.insert(std::mem::discriminant(&b));
    }
    assert!(
        kinds.len() >= 4,
        "Broceliande should field varied behaviours"
    );
    // Every zone has exactly one notable, and its loot all resolves.
    let bosses = spawns.iter().filter(|s| s.boss).count();
    assert_eq!(bosses, BROCELIANDE_ZONES, "one boss per Broceliande zone");
    for s in &spawns {
        for id in s.loot {
            assert!(
                crate::app::door::lateania::items::item(*id).is_some(),
                "{} drops missing item {id}",
                s.name
            );
        }
    }
    // A moderate continent: gentler than the endgame Frontier king.
    let king = world
        .spawns
        .iter()
        .find(|s| s.name == "the King Who Was Promised Nothing")
        .expect("the Frontier king spawns");
    assert!(
        spawns.iter().all(|s| s.damage < king.damage),
        "Broceliande stays below the endgame king's bite"
    );
}

#[test]
fn catacombs_are_a_braided_maze_not_a_grid() {
    let world = seed_world();
    let catacomb_rooms: Vec<&Room> = world
        .rooms
        .values()
        .filter(|r| {
            r.id >= CATACOMBS_BASE
                && (r.id as usize) < CATACOMBS_BASE as usize + CATACOMBS_W * CATACOMBS_H
        })
        .collect();
    assert_eq!(catacomb_rooms.len(), CATACOMBS_W * CATACOMBS_H);
    // A maze has dead-ends (one exit, ignoring the safe entrance's portal)
    // and junctions (3+ exits); a uniform grid would have neither in the
    // interior. Confirm both shapes exist.
    let dead_ends = catacomb_rooms
        .iter()
        .filter(|r| !r.safe && r.exits.len() == 1)
        .count();
    let junctions = catacomb_rooms.iter().filter(|r| r.exits.len() >= 3).count();
    assert!(dead_ends > 0, "a maze should have dead-ends, found none");
    assert!(junctions > 0, "a maze should have junctions, found none");
    // Reachable from the start, and reciprocal: every exit's target links back.
    for r in &catacomb_rooms {
        for to in r.exits.values() {
            let dest = world.room(*to).expect("catacomb exit resolves");
            assert!(
                dest.exits.values().any(|back| *back == r.id),
                "room {} -> {} is one-way",
                r.id,
                to
            );
        }
    }
}

#[test]
fn catacombs_have_behavior_driven_mobs() {
    let world = seed_world();
    let catacomb_spawns: Vec<&MobSpawn> = world
        .spawns
        .iter()
        .filter(|s| {
            s.id >= CATACOMBS_SPAWN_ID_START && s.id < CATACOMBS_SPAWN_ID_START + 10_000
        })
        .collect();
    assert!(
        !catacomb_spawns.is_empty(),
        "the catacombs should be populated"
    );
    // Every catacomb mob has a non-Sentinel behavior, and several distinct
    // behaviors appear across the region.
    let mut kinds = std::collections::HashSet::new();
    for s in &catacomb_spawns {
        let b = world.behavior_of(s.id);
        assert_ne!(
            b,
            MobBehavior::Sentinel,
            "{} should have a behavior",
            s.name
        );
        kinds.insert(std::mem::discriminant(&b));
    }
    assert!(
        kinds.len() >= 4,
        "expected several distinct mob behaviors, found {}",
        kinds.len()
    );
}

#[test]
fn thornwood_is_a_maze_hung_off_melvanala() {
    let world = seed_world();
    let rooms: Vec<&Room> = world
        .rooms
        .values()
        .filter(|r| r.id >= THORNWOOD_BASE && r.id < CAVERNS_BASE)
        .collect();
    assert_eq!(rooms.len(), THORNWOOD_W * THORNWOOD_H);
    let dead_ends = rooms
        .iter()
        .filter(|r| !r.safe && r.exits.len() == 1)
        .count();
    let junctions = rooms.iter().filter(|r| r.exits.len() >= 3).count();
    assert!(
        dead_ends > 0 && junctions > 0,
        "thornwood should read as a maze"
    );
    // The capital links into the wood, and the link is reciprocal.
    let gate = world.room(THORNWOOD_BASE).expect("bramble gate exists");
    assert!(gate.exits.values().any(|to| *to == MELVANALA_SQUARE));
    assert!(
        world
            .room(MELVANALA_SQUARE)
            .expect("melvanala square")
            .exits
            .values()
            .any(|to| *to == THORNWOOD_BASE)
    );
}

#[test]
fn drowned_caverns_are_one_connected_organic_cave() {
    let world = seed_world();
    let cave: Vec<RoomId> = world
        .rooms
        .keys()
        .copied()
        .filter(|id| {
            *id >= CAVERNS_BASE && *id < CAVERNS_BASE + (CAVERNS_W * CAVERNS_H) as RoomId
        })
        .collect();
    // Organic, not a grid: a sparse subset of the cell field survives.
    assert!(
        cave.len() < CAVERNS_W * CAVERNS_H,
        "cave should be sparse, not a full grid"
    );
    // Every exit is reciprocal and resolves.
    for &id in &cave {
        for to in world.room(id).unwrap().exits.values() {
            let dest = world.room(*to).expect("cavern exit resolves");
            assert!(
                dest.exits.values().any(|back| *back == id),
                "cavern room {id} -> {to} is one-way"
            );
        }
    }
    // The whole cave is one connected pocket: BFS from the tide-mouth
    // entrance reaches every cavern room (staying within the region).
    let entrance = *cave
        .iter()
        .find(|id| world.room(**id).unwrap().safe)
        .expect("cave has a safe entrance");
    let in_cave: HashSet<RoomId> = cave.iter().copied().collect();
    let mut seen = HashSet::from([entrance]);
    let mut queue = VecDeque::from([entrance]);
    while let Some(r) = queue.pop_front() {
        for to in world.room(r).unwrap().exits.values() {
            if in_cave.contains(to) && seen.insert(*to) {
                queue.push_back(*to);
            }
        }
    }
    assert_eq!(seen.len(), cave.len(), "all cavern rooms must be reachable");
}

#[test]
fn new_regions_are_populated_with_varied_behaviors() {
    let world = seed_world();
    for (lo, hi, label) in [
        (
            THORNWOOD_SPAWN_ID_START,
            THORNWOOD_SPAWN_ID_START + 10_000,
            "thornwood",
        ),
        (
            CAVERNS_SPAWN_ID_START,
            CAVERNS_SPAWN_ID_START + 10_000,
            "caverns",
        ),
    ] {
        let spawns: Vec<&MobSpawn> = world
            .spawns
            .iter()
            .filter(|s| s.id >= lo && s.id < hi)
            .collect();
        assert!(!spawns.is_empty(), "{label} should be populated");
        let mut kinds = HashSet::new();
        for s in &spawns {
            let b = world.behavior_of(s.id);
            assert_ne!(
                b,
                MobBehavior::Sentinel,
                "{} should have a behavior",
                s.name
            );
            kinds.insert(std::mem::discriminant(&b));
        }
        assert!(kinds.len() >= 4, "{label} should field varied behaviors");
    }
}

#[test]
fn living_world_regulars_stay_below_their_bosses() {
    let world = seed_world();
    for (lo, hi, label) in [
        (
            CATACOMBS_SPAWN_ID_START,
            CATACOMBS_SPAWN_ID_START + 10_000,
            "catacombs",
        ),
        (
            THORNWOOD_SPAWN_ID_START,
            THORNWOOD_SPAWN_ID_START + 10_000,
            "thornwood",
        ),
        (
            CAVERNS_SPAWN_ID_START,
            CAVERNS_SPAWN_ID_START + 10_000,
            "caverns",
        ),
    ] {
        let spawns: Vec<&MobSpawn> = world
            .spawns
            .iter()
            .filter(|s| s.id >= lo && s.id < hi)
            .collect();
        let boss_damage = spawns
            .iter()
            .filter(|s| s.boss)
            .map(|s| s.damage)
            .max()
            .expect("region has a boss");
        let too_strong: Vec<_> = spawns
            .iter()
            .filter(|s| !s.boss && s.damage >= boss_damage)
            .map(|s| (s.name, s.damage, boss_damage))
            .collect();
        assert!(
            too_strong.is_empty(),
            "{label} regulars should not meet or exceed boss damage: {too_strong:?}"
        );
    }
}

#[test]
fn living_world_loot_stays_out_of_the_frontier_catalog() {
    let world = seed_world();
    for spawn in world.spawns.iter().filter(|s| {
        (CATACOMBS_SPAWN_ID_START..CATACOMBS_SPAWN_ID_START + 10_000).contains(&s.id)
            || (THORNWOOD_SPAWN_ID_START..THORNWOOD_SPAWN_ID_START + 10_000).contains(&s.id)
            || (CAVERNS_SPAWN_ID_START..CAVERNS_SPAWN_ID_START + 10_000).contains(&s.id)
    }) {
        for id in spawn.loot {
            assert!(
                !(3000..3200).contains(id),
                "{} should not drop Frontier catalog item {}",
                spawn.name,
                id
            );
        }
    }
}

#[test]
fn there_are_at_least_fifty_distinct_enemy_types() {
    let world = seed_world();
    let mut names: Vec<&str> = world.spawns.iter().map(|s| s.name).collect();
    names.sort_unstable();
    names.dedup();
    assert!(
        names.len() >= 50,
        "expected 50+ distinct enemy types, found {}",
        names.len()
    );
}

#[test]
fn mob_spawn_ids_are_unique() {
    let world = seed_world();
    let mut ids: Vec<u32> = world.spawns.iter().map(|s| s.id).collect();
    ids.sort_unstable();
    let count = ids.len();
    ids.dedup();
    assert_eq!(count, ids.len(), "duplicate mob spawn id");
}

#[test]
fn every_boss_has_a_guaranteed_loot_table() {
    let world = seed_world();
    let bosses: Vec<_> = world.spawns.iter().filter(|s| s.boss).collect();
    assert!(bosses.len() >= 7, "expected at least 7 zone bosses");
    for boss in bosses {
        assert!(!boss.loot.is_empty(), "boss {} has no loot", boss.name);
        for id in boss.loot {
            assert!(
                crate::app::door::lateania::items::item(*id).is_some(),
                "boss {} drops missing item {}",
                boss.name,
                id
            );
        }
    }
}

#[test]
fn all_mob_loot_references_real_items() {
    let world = seed_world();
    for spawn in &world.spawns {
        for id in spawn.loot {
            assert!(
                crate::app::door::lateania::items::item(*id).is_some(),
                "mob {} drops missing item {}",
                spawn.name,
                id
            );
        }
    }
}

#[test]
fn every_room_reachable_from_start() {
    let world = seed_world();
    let mut seen = std::collections::HashSet::new();
    let mut stack = vec![world.start_room];
    while let Some(id) = stack.pop() {
        if !seen.insert(id) {
            continue;
        }
        if let Some(room) = world.room(id) {
            for target in room.exits.values() {
                stack.push(*target);
            }
        }
        // A waystone portal connects to the whole fast-travel network, so
        // the portal villages and island landings are reachable even though
        // they have no directional exit into them.
        if features_at(id)
            .iter()
            .any(|f| f.kind == FeatureKind::Portal)
        {
            for (_, dest) in super::super::archipelago::portal_destinations() {
                stack.push(dest);
            }
        }
    }
    assert_eq!(
        seen.len(),
        world.rooms.len(),
        "some rooms are unreachable from the start room"
    );
}

#[test]
fn world_atlas_tracks_exploration_and_bosses_per_region() {
    let world = seed_world();
    // A blank explorer: nothing mapped, but every region reports real totals.
    let none = HashSet::new();
    let fresh = world.region_progress(&none, 1);
    assert!(!fresh.is_empty(), "the atlas has regions");
    assert!(
        fresh.iter().all(|r| r.explored == 0),
        "an unexplored world reads as zero everywhere"
    );
    assert!(
        fresh.iter().all(|r| r.total > 0),
        "every atlas region contains rooms"
    );
    assert!(
        fresh.iter().filter(|r| r.bosses > 0).count() >= 4,
        "several regions lair bosses (where the loot is)"
    );
    // Visiting a couple of rooms lights up exactly their region's progress.
    let visited: HashSet<RoomId> = HashSet::from([1u32, 2u32]);
    let seen = world.region_progress(&visited, 1);
    let home = seen
        .iter()
        .find(|r| r.name.starts_with("Embergate"))
        .expect("Embergate region exists");
    assert_eq!(
        home.explored, 2,
        "the two visited rooms are counted at home"
    );
}

#[test]
fn the_atlas_covers_every_continent_including_kaelmyr() {
    let world = seed_world();
    let none = HashSet::new();
    for probe in [
        KAELMYR_BASE,
        LAKES_BASE,
        BROCELIANDE_BASE,
        REACHES_BASE,
        2_000,
    ] {
        let regions = world.region_progress(&none, probe);
        assert!(
            regions.iter().any(|r| r.here),
            "room {probe} should fall inside an atlas region"
        );
    }
    // Exactly one region claims the player at a time.
    let regions = world.region_progress(&none, KAELMYR_BASE);
    assert_eq!(regions.iter().filter(|r| r.here).count(), 1);
}

#[test]
fn continent_waystones_stand_in_real_safe_rooms() {
    let world = seed_world();
    for (label, room, _) in CONTINENT_WAYSTONES {
        let r = world
            .room(*room)
            .unwrap_or_else(|| panic!("waystone room for {label} exists"));
        assert!(r.safe, "the {label} waystone stands in a safe room");
        assert!(
            features_at(*room)
                .iter()
                .any(|f| f.kind == FeatureKind::Portal),
            "the {label} room carries a Portal feature"
        );
    }
    // Destinations are unique across the whole network.
    let dests = waystone_destinations();
    let mut rooms: Vec<RoomId> = dests.iter().map(|(_, r, _)| *r).collect();
    rooms.sort_unstable();
    rooms.dedup();
    assert_eq!(rooms.len(), dests.len(), "destination rooms are unique");
}

#[test]
fn the_archipelago_is_mazes_and_caverns_with_a_boss_per_isle() {
    use super::super::archipelago as arch;
    let world = seed_world();
    // Every island has a named boss (a boss mob homed inside its block).
    for i in 0..arch::ISLAND_COUNT {
        let base = arch::island_entrance(i);
        let end = base + arch::ARCH_STRIDE;
        let has_boss = world
            .spawns
            .iter()
            .any(|sp| sp.boss && (base..end).contains(&sp.home));
        assert!(has_boss, "island {i} should have a boss");
    }
    // Not grids: the isles wind into dead-ends and vary in branching.
    let rooms: Vec<&Room> = world
        .rooms
        .values()
        .filter(|r| arch::is_archipelago_room(r.id))
        .collect();
    let dead_ends = rooms.iter().filter(|r| r.exits.len() == 1).count();
    assert!(
        dead_ends >= 15,
        "islands should wind into dead-ends, not be square blocks (got {dead_ends})"
    );
}

#[test]
fn overworld_adds_one_hundred_new_rooms() {
    let world = seed_world();
    // The overworld occupies ids 600..2000; the Frontier starts at 2000.
    let new_rooms = world
        .rooms
        .keys()
        .filter(|id| (600..2000).contains(*id))
        .count();
    assert_eq!(
        new_rooms, 100,
        "expected exactly 100 new overworld rooms (600-1999)"
    );
}

#[test]
fn every_room_has_a_paragraph_description() {
    // "A paragraph of detail" - every authored room reads as real prose, not
    // a stub. The bar is a minimum length plus more than one sentence.
    const MIN_CHARS: usize = 180;
    let world = seed_world();
    let mut short: Vec<(RoomId, usize)> = world
        .rooms
        .values()
        .filter(|r| {
            let len = r.desc.chars().count();
            let sentences = r.desc.matches(['.', '!', '?']).count();
            len < MIN_CHARS || sentences < 2
        })
        .map(|r| (r.id, r.desc.chars().count()))
        .collect();
    short.sort_unstable();
    assert!(
        short.is_empty(),
        "{} room(s) lack a paragraph-length description: {:?}",
        short.len(),
        short
    );
}

#[test]
fn frontier_quests_map_each_boss_back_to_its_zone() {
    assert_eq!(frontier_zone_count(), 20);
    for z in 0..frontier_zone_count() {
        let (_zname, boss) = frontier_zone_info(z).expect("zone exists");
        assert_eq!(
            frontier_zone_of_boss(boss),
            Some(z),
            "boss {boss} should credit zone {z}"
        );
    }
    assert_eq!(frontier_zone_of_boss("not a boss"), None);
}

#[test]
fn regular_mobs_respawn_fast_enough_for_grinding() {
    let world = seed_world();
    let slow: Vec<_> = world
        .spawns
        .iter()
        .filter(|spawn| !spawn.boss && spawn.respawn_secs > 76)
        .map(|spawn| (spawn.name, spawn.respawn_secs))
        .collect();

    assert!(
        slow.is_empty(),
        "regular grind mobs should not have long respawns: {slow:?}"
    );
}

#[test]
fn regular_mobs_keep_grind_rewards_after_boss_tuning() {
    let world = seed_world();
    let first_road_mob = world
        .spawns
        .iter()
        .find(|spawn| spawn.home == 6 && !spawn.boss)
        .expect("first road mob exists");
    assert!(
        first_road_mob.xp >= 14,
        "early mobs should still be worth killing"
    );

    let frontier_regular = world
        .spawns
        .iter()
        .find(|spawn| spawn.id >= FRONTIER_SPAWN_ID_START && !spawn.boss)
        .expect("frontier regular mob exists");
    assert!(
        frontier_regular.xp >= 60,
        "frontier regulars should reward deliberate grinding"
    );
}

#[test]
fn first_frontier_regulars_are_endgame_mobs_but_not_bosses() {
    let world = seed_world();
    let first_frontier_regular = world
        .spawns
        .iter()
        .find(|spawn| spawn.id >= FRONTIER_SPAWN_ID_START && !spawn.boss)
        .expect("frontier regular mob exists");
    let first_frontier_boss = world
        .spawns
        .iter()
        .find(|spawn| spawn.id >= FRONTIER_SPAWN_ID_START && spawn.boss)
        .expect("frontier boss exists");
    let strongest_living_boss_damage = world
        .spawns
        .iter()
        .filter(|spawn| is_living_dark_spawn(spawn.id) && spawn.boss)
        .map(|spawn| spawn.damage)
        .max()
        .expect("living-dark bosses exist");

    assert!(
        first_frontier_regular.damage > strongest_living_boss_damage,
        "first Frontier regulars should assume the living-dark arc is cleared"
    );
    assert!(
        first_frontier_regular.damage < first_frontier_boss.damage
            && first_frontier_regular.max_hp < first_frontier_boss.max_hp,
        "first Frontier regulars should still be below the first boss"
    );
}

#[test]
fn town_and_capitals_have_wildlife() {
    assert!(!critters_at(1).is_empty(), "the town square has wildlife");
    assert!(
        critters_at(1)
            .iter()
            .any(|c| matches!(c.kind, CritterKind::Boon(_))),
        "a boon creature lives in the town square"
    );
    assert!(
        WILDLIFE.iter().any(|c| c.kind == CritterKind::Game),
        "small game lives out in the wilds"
    );
}

#[test]
fn town_square_has_a_recall_fountain_and_bank() {
    // The recall destination carries a healing fountain, and room 1 is safe
    // so the fountain actually restores vitals. It also carries the bank
    // that protects gold from death loss.
    let features = features_at(1);
    assert!(
        features.iter().any(|f| f.kind == FeatureKind::Fountain),
        "the town square needs a fountain"
    );
    assert!(
        features.iter().any(|f| f.kind == FeatureKind::Bank),
        "the town square needs a bank"
    );
    assert!(seed_world().room(1).expect("town square exists").safe);
}

#[test]
fn every_capital_has_a_fountain_and_a_plaque() {
    let world = seed_world();
    for square in [TASMANIA_SQUARE, MELVANALA_SQUARE, MATLATESH_SQUARE] {
        let room = world.room(square).expect("capital square exists");
        assert!(room.safe, "capital {square} must be a safe haven");
        let feats = features_at(square);
        assert!(
            feats.iter().any(|f| f.kind == FeatureKind::Fountain),
            "capital {square} has no healing fountain"
        );
        assert!(
            feats.iter().any(|f| f.kind == FeatureKind::Plaque),
            "capital {square} has no dedication plaque"
        );
    }
}

#[test]
fn every_feature_lives_in_a_real_room() {
    let world = seed_world();
    for feature in FEATURES {
        assert!(
            world.rooms.contains_key(&feature.room),
            "feature {:?} references missing room {}",
            feature.name,
            feature.room
        );
    }
}

#[test]
fn craft_stations_stand_in_real_rooms_and_cover_every_trade() {
    let world = seed_world();
    for skill in CraftSkill::ALL {
        let rooms: Vec<RoomId> = FEATURES
            .iter()
            .filter(|f| f.kind == FeatureKind::CraftStation(skill))
            .map(|f| f.room)
            .collect();
        assert!(!rooms.is_empty(), "no station trains {}", skill.label());
        for r in rooms {
            assert!(
                world.rooms.contains_key(&r),
                "{} station in missing room {}",
                skill.label(),
                r
            );
        }
    }
    assert!(
        !craft_stations_at(3).is_empty(),
        "Embergate's crafters' row exposes stations"
    );
}

#[test]
fn every_node_lives_in_a_real_room() {
    let world = seed_world();
    for n in NODES {
        assert!(
            world.rooms.contains_key(&n.home),
            "node {:?} references missing room {}",
            n.name,
            n.home
        );
    }
}

#[test]
fn every_node_yields_a_real_material_matching_its_skill_and_tier() {
    use super::super::items;
    for n in NODES {
        assert!(
            (n.tier as u32) < items::MATERIAL_TIERS,
            "node {:?} tier {} out of range",
            n.name,
            n.tier
        );
        // Two kinds of yield: the classic tiered material (derived from
        // skill + tier) and an explicit catalog item (the Sunderlakes fish,
        // seeded via `node_yielding`). Both must resolve through `item`.
        if (items::FISH_BASE..items::FISH_BASE + items::FISH_COUNT).contains(&n.yield_item) {
            assert_eq!(
                n.skill,
                GatherSkill::Fishing,
                "only Fishing nodes yield fish ({:?})",
                n.name
            );
        } else {
            assert_eq!(
                n.yield_item,
                items::material_id(n.skill.index(), n.tier as u32),
                "node {:?} material yield must follow its skill + tier",
                n.name
            );
        }
        assert!(
            items::item(n.yield_item).is_some(),
            "node {:?} yields missing item {}",
            n.name,
            n.yield_item
        );
        assert!(
            n.level_req >= 1,
            "node {:?} needs a real skill gate",
            n.name
        );
    }
}

#[test]
fn node_indices_round_trip_and_cover_every_skill() {
    // `node_index` is exercised exactly as the service uses it: on the
    // 'static refs handed out by `nodes_at` (const promotion makes the two
    // NODES views share storage, as with critters). Every node must be
    // reachable and map back to a unique index.
    let world = seed_world();
    let mut seen = std::collections::HashSet::new();
    for &id in world.rooms.keys() {
        for n in nodes_at(id) {
            let idx = node_index(n).expect("a node from nodes_at has an index");
            seen.insert(idx);
        }
    }
    assert_eq!(
        seen.len(),
        NODES.len(),
        "every node is reachable via nodes_at and indexes uniquely"
    );
    // At least one node per gathering skill, so every trade has somewhere to
    // train.
    for skill in GatherSkill::ALL {
        assert!(
            NODES.iter().any(|n| n.skill == skill),
            "no node trains {}",
            skill.label()
        );
    }
}

#[test]
fn minimap_centres_on_the_player_and_reveals_frontiers() {
    let world = seed_world();
    let start = world.start_room;
    // Only the start room is visited: it sits dead centre, and at least one
    // unexplored exit shows up as a frontier marker.
    let visited = HashSet::from([start]);
    let map = world.minimap(start, None, &visited, 3, 2);
    let centre = (map.grid.len() / 2, map.grid[0].len() / 2);
    assert_eq!(map.grid[centre.0][centre.1], MapCell::Current);
    let frontiers = map
        .grid
        .iter()
        .flatten()
        .filter(|c| **c == MapCell::Frontier)
        .count();
    assert!(
        frontiers >= 1,
        "the start room should reveal somewhere to go"
    );
}

#[test]
fn minimap_draws_a_corridor_between_visited_rooms() {
    let world = seed_world();
    let start = world.start_room;
    let neighbour = world
        .room(start)
        .unwrap()
        .exits
        .iter()
        .filter(|(dir, _)| dir.delta_2d().is_some())
        .map(|(_, dest)| *dest)
        .next()
        .expect("start has a planar exit");
    let visited = HashSet::from([start, neighbour]);
    let map = world.minimap(start, None, &visited, 3, 2);
    let visited_cells = map
        .grid
        .iter()
        .flatten()
        .filter(|c| **c == MapCell::Visited)
        .count();
    assert!(visited_cells >= 1, "the visited neighbour should be drawn");
    let corridors = map
        .grid
        .iter()
        .flatten()
        .filter(|c| matches!(**c, MapCell::ConnH | MapCell::ConnV))
        .count();
    assert!(corridors >= 1, "a corridor should join the two rooms");
}

#[test]
fn minimap_marks_previous_room_and_trail() {
    let world = seed_world();
    let start = world.start_room;
    let previous = world
        .room(start)
        .unwrap()
        .exits
        .iter()
        .filter(|(dir, _)| dir.delta_2d().is_some())
        .map(|(_, dest)| *dest)
        .next()
        .expect("start has a planar exit");
    let visited = HashSet::from([start, previous]);

    let map = world.minimap(start, Some(previous), &visited, 3, 2);

    assert!(
        map.grid.iter().flatten().any(|c| *c == MapCell::Previous),
        "the room just left should be marked"
    );
    assert!(
        map.grid
            .iter()
            .flatten()
            .any(|c| matches!(*c, MapCell::TrailH | MapCell::TrailV)),
        "the route from previous room to current room should be highlighted"
    );
}

#[test]
fn reaches_zone_labels_are_not_doubled() {
    let world = seed_world();
    for room in world.rooms.values() {
        assert!(
            !room.zone.starts_with("The The "),
            "room {} has a doubled zone label {:?}",
            room.id,
            room.zone
        );
    }
    assert!(
        world.rooms.values().any(|r| r.zone == "The Sundering Deep"),
        "the deepest Reaches zone should carry its board-quest label"
    );
}

#[test]
fn yssgar_out_toughens_and_out_earns_the_frontier_king() {
    // The Reaches deliberately ride the Frontier's balance multipliers, so
    // pin the intended outcome: the new continent's crowned boss stands
    // above the King Who Was Promised Nothing in threat and in XP.
    let world = seed_world();
    let king = world
        .spawns
        .iter()
        .find(|s| s.name == "the King Who Was Promised Nothing")
        .expect("the Frontier king spawns");
    let yssgar = world
        .spawns
        .iter()
        .find(|s| s.name == "Yssgar, the Sundering Deep")
        .expect("the Reaches' crowned boss spawns");
    assert!(
        yssgar.max_hp > king.max_hp,
        "Yssgar should out-last the King"
    );
    assert!(
        yssgar.damage > king.damage,
        "Yssgar should out-hit the King"
    );
    assert!(yssgar.xp > king.xp, "Yssgar should out-reward the King");
}
