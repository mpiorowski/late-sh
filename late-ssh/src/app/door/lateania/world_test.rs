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
    // Aelunor, the Faewood: a sixth continent of twelve organic fae-glades
    // (rooms 25000+, cavern-carved only - never a maze, never a grid). Each
    // zone is sparse, so the total is a sane band rather than an exact count.
    let aelunor = count_in(
        AELUNOR_BASE,
        AELUNOR_BASE + AELUNOR_ZONES as RoomId * AELUNOR_ZONE_STRIDE,
    );
    assert!(
        (250..=AELUNOR_ZONES * AELUNOR_W * AELUNOR_H).contains(&aelunor),
        "Aelunor should be ~300 rooms, got {aelunor}"
    );
    // Silvael: the Faewood's own city (rooms 26000+). A fixed, fully
    // hand-authored set, so this is an exact count rather than a band.
    let silvael = count_in(SILVAEL_BASE, SILVAEL_BASE + SILVAEL_ROOM_COUNT);
    assert_eq!(silvael, 8, "eight Silvael rooms");
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
    // The Wildbound Waste: a fifth, pvp continent of three chained
    // maze/cavern biomes plus their three small gate towns (rooms 30000+).
    // Mazes fill their cell field; caverns are sparse, so the total is a sane
    // band rather than an exact count.
    let wildbound = count_in(WILDBOUND_BASE, WILDBOUND_BASE + 3 * WILDBOUND_BIOME_STRIDE);
    assert!(
        (900..=3 * WILDBOUND_BIOME_STRIDE as usize).contains(&wildbound),
        "the Wildbound Waste should be ~1000+ rooms, got {wildbound}"
    );
    // Wayfarer's Hollow: the five-room new-player tutorial zone (rooms
    // 40000+), hung off the Gilded Flagon. A fixed, fully hand-authored set,
    // so this is an exact count rather than a band.
    let tutorial = count_in(TUTORIAL_BASE, TUTORIAL_BASE + 10);
    assert_eq!(tutorial, 5, "five tutorial rooms");
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
            + aelunor
            + silvael
            + villages
            + islands
            + wildbound
            + tutorial,
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
    let degrees: std::collections::HashSet<usize> = reaches.iter().map(|r| r.exits.len()).collect();
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
    let degrees: std::collections::HashSet<usize> = kaelmyr.iter().map(|r| r.exits.len()).collect();
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
    let degrees: std::collections::HashSet<usize> = lakes.iter().map(|r| r.exits.len()).collect();
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
    // (The Wildbound tier-6 fishing springs also sit in the lakes but yield
    // the tiered Abyss Eel material, not a catalog fish - exclude them here.)
    let fish_nodes: Vec<&ResourceNode> = NODES
        .iter()
        .filter(|nn| nn.skill == GatherSkill::Fishing && is_lakes_room(nn.home) && nn.tier < 5)
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
    let degrees: std::collections::HashSet<usize> = wood.iter().map(|r| r.exits.len()).collect();
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
        .filter(|s| s.id >= CATACOMBS_SPAWN_ID_START && s.id < CATACOMBS_SPAWN_ID_START + 10_000)
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
        .filter(|id| *id >= CAVERNS_BASE && *id < CAVERNS_BASE + (CAVERNS_W * CAVERNS_H) as RoomId)
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
    for (label, room) in CONTINENT_WAYSTONES {
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
    let mut rooms: Vec<RoomId> = dests.iter().map(|(_, r)| *r).collect();
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

    // The Frontier assumes the living-dark arc is cleared: its first regulars
    // read at or past the Archdemon, the crown that opens that arc.
    let archdemon = CROWNS
        .iter()
        .find(|c| c.name == "the Archdemon Mal'gareth")
        .expect("the Archdemon is a crown");
    assert!(
        first_frontier_regular.level() >= archdemon.level,
        "first Frontier regulars should assume the living-dark arc is cleared (L{})",
        first_frontier_regular.level()
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

#[test]
fn sunderlakes_mobs_are_peaceful_not_endgame_scaled() {
    let world = seed_world();
    let hp_of = |pred: fn(RoomId) -> bool| -> Vec<i32> {
        world
            .spawns
            .iter()
            .filter(|s| !s.boss && pred(s.home))
            .map(|s| s.max_hp)
            .collect()
    };
    let lakes = hp_of(is_lakes_room);
    let kaelmyr = hp_of(is_kaelmyr_room);
    assert!(!lakes.is_empty() && !kaelmyr.is_empty());
    // The Sunderlakes are peaceful mid-game: their toughest regular mob must be
    // weaker than the softest Kaelmyr (endgame) mob, i.e. no scaling overlap.
    let lakes_max = *lakes.iter().max().unwrap();
    let kaelmyr_min = *kaelmyr.iter().min().unwrap();
    assert!(
        lakes_max < kaelmyr_min,
        "toughest Sunderlakes mob ({lakes_max} hp) should be weaker than the softest Kaelmyr mob ({kaelmyr_min} hp)"
    );
}

// The displayed level is "come at this level": a crown reads its target, and
// everything else reads by its bite off the crown ladder (`MobSpawn::level`).
// Verify the ladder holds together: crowns read their targets, a harder bite
// never reads lower, the first road reads as a starting zone, the Frontier
// opens past the Archdemon, Kaelmyr's deepest zone reads at the last crown,
// and nothing reads past the cap.
#[test]
fn displayed_levels_read_by_bite_along_the_crown_ladder() {
    let world = seed_world();
    let crown_of = |name: &str| CROWNS.iter().find(|c| c.name == name).expect("a crown");
    for crown in CROWNS {
        let spawn = world
            .spawns
            .iter()
            .find(|s| s.name == crown.name)
            .expect("every crown spawns");
        assert_eq!(
            spawn.level(),
            crown.level,
            "{} reads its target",
            crown.name
        );
    }
    let is_crown = |s: &MobSpawn| CROWNS.iter().any(|c| c.name == s.name);
    for boss in [false, true] {
        let mut ranked: Vec<&MobSpawn> = world
            .spawns
            .iter()
            .filter(|s| s.boss == boss && !is_crown(s))
            .collect();
        ranked.sort_by_key(|s| s.damage);
        for w in ranked.windows(2) {
            assert!(
                w[1].level() >= w[0].level(),
                "level must not fall as the bite rises ({} L{} vs {} L{})",
                w[0].name,
                w[0].level(),
                w[1].name,
                w[1].level()
            );
        }
    }
    for s in &world.spawns {
        assert!(
            s.level() <= super::super::classes::Class::MAX_LEVEL,
            "{} reads past the cap",
            s.name
        );
    }
    let zone_of = |s: &MobSpawn| world.room(s.home).expect("mob home exists").zone;
    let treant_zone = zone_of(
        world
            .spawns
            .iter()
            .find(|s| s.name == "the Elder Treant")
            .expect("the Treant"),
    );
    let treant = crown_of("the Elder Treant");
    for s in world
        .spawns
        .iter()
        .filter(|s| !s.boss && zone_of(s) == treant_zone)
    {
        assert!(
            s.level() < treant.level,
            "{} on the Treant's doorstep reads L{}, at or past the crown",
            s.name,
            s.level()
        );
    }
    let first_frontier = world
        .spawns
        .iter()
        .find(|s| s.id >= FRONTIER_SPAWN_ID_START && !s.boss)
        .expect("frontier regular mob exists");
    let archdemon = crown_of("the Archdemon Mal'gareth");
    assert!(
        first_frontier.level() >= archdemon.level,
        "the Frontier opens past the Archdemon, got L{}",
        first_frontier.level()
    );
    let ascendant = crown_of("Kaethyr Ascendant, Who Sang the God Awake");
    let deepest = world
        .spawns
        .iter()
        .filter(|s| band_of(s.id) == Band::Kaelmyr && !is_crown(s))
        .map(|s| s.level())
        .max()
        .expect("kaelmyr spawns");
    assert!(
        (ascendant.level - 5..=ascendant.level).contains(&deepest),
        "Kaelmyr's deepest reads L{deepest}, not at the last crown (L{})",
        ascendant.level
    );
}

// The iron rule of the minimap: a drawn line means you can walk it. The old
// renderer drew a connector for every exit BY DIRECTION, so when the world's
// non-Euclidean folds laid the destination elsewhere, a phantom corridor
// appeared joining two rooms with no exit between them ("You can't go north"
// under a drawn |, as reported at the Cartographers' Loft on the Saltwind
// Wharves). Sweep every room in the world as the map centre and verify every
// connector joins rooms that really share an exit on that axis.
#[test]
fn every_minimap_line_is_walkable() {
    use crate::app::door::lateania::world::MapCell;
    let world = seed_world();
    let visited: HashSet<RoomId> = world.rooms.keys().copied().collect();
    let (hr, vr) = (3i32, 2i32);
    let mut phantoms = 0usize;
    let mut checked = 0usize;
    for &current in world.rooms.keys() {
        let coords = world.minimap_coords(current, &visited, hr, vr);
        let map = world.minimap(current, None, &visited, hr, vr);
        // Invert: grid cell -> room id.
        let mut at: std::collections::HashMap<(usize, usize), RoomId> =
            std::collections::HashMap::new();
        for (&rid, &(x, y)) in &coords {
            at.insert((((y + vr) * 2) as usize, ((x + hr) * 2) as usize), rid);
        }
        for (r, row) in map.grid.iter().enumerate() {
            for (c, &cell) in row.iter().enumerate() {
                let horizontal = match cell {
                    MapCell::ConnH | MapCell::TrailH => true,
                    MapCell::ConnV | MapCell::TrailV => false,
                    _ => continue,
                };
                checked += 1;
                let ((c1, c2), (d1, d2)) = if horizontal {
                    (((r, c - 1), (r, c + 1)), (Dir::East, Dir::West))
                } else {
                    (((r - 1, c), (r + 1, c)), (Dir::South, Dir::North))
                };
                let linked = |from: RoomId, to: RoomId| {
                    world
                        .room(from)
                        .is_some_and(|room| room.exits.values().any(|&d| d == to))
                };
                // `d1` walks from c1 toward c2; `d2` walks back.
                let has_exit = |from: RoomId, dir: Dir| {
                    world
                        .room(from)
                        .is_some_and(|room| room.exits.contains_key(&dir))
                };
                match (at.get(&c1).copied(), at.get(&c2).copied()) {
                    // Both ends are drawn rooms: they must truly be linked.
                    (Some(a), Some(b)) => {
                        if !linked(a, b) && !linked(b, a) {
                            phantoms += 1;
                        }
                    }
                    // A frontier corridor: truthful only if the drawn room
                    // really has an exit running that way.
                    (Some(a), None) => {
                        if map.grid[c2.0][c2.1] != MapCell::Frontier || !has_exit(a, d1) {
                            phantoms += 1;
                        }
                    }
                    (None, Some(b)) => {
                        if map.grid[c1.0][c1.1] != MapCell::Frontier || !has_exit(b, d2) {
                            phantoms += 1;
                        }
                    }
                    // A line joining nothing to nothing.
                    (None, None) => phantoms += 1,
                }
            }
        }
    }
    assert!(checked > 0, "the sweep drew no connectors at all");
    assert_eq!(
        phantoms, 0,
        "{phantoms} phantom corridors drawn (of {checked} connectors): a map line must always be walkable"
    );
}

#[test]
fn regional_notables_carry_their_own_wildbound_finds() {
    // Every zone/island's notable-loot table should genuinely include that
    // zone's two new finds, not just the borrowed fallback catalog.
    for zone in 0..14 {
        let loot = lakes_notable_loot(zone);
        for id in super::super::items::sunderlakes_find_ids(zone) {
            assert!(
                loot.contains(&id),
                "Sunderlakes zone {zone}'s notable should carry find {id}"
            );
        }
    }
    for zone in 0..20 {
        let loot = broceliande_notable_loot(zone);
        for id in super::super::items::broceliande_find_ids(zone) {
            assert!(
                loot.contains(&id),
                "Broceliande zone {zone}'s notable should carry find {id}"
            );
        }
    }
    for isle in 0..20 {
        let loot = archipelago_boss_loot(isle);
        for id in super::super::items::archipelago_find_ids(isle) {
            assert!(
                loot.contains(&id),
                "Archipelago isle {isle}'s boss should carry find {id}"
            );
        }
    }
}

// ---- Genesys: a living, breathing world - villagers ----------------------

#[test]
fn genesys_adds_at_least_a_hundred_villagers() {
    assert!(
        VILLAGERS.len() >= 100,
        "expected at least 100 villagers, got {}",
        VILLAGERS.len()
    );
}

#[test]
fn every_public_safe_room_has_a_villager() {
    // Private home interiors (each tier's own hearth/back/upper rooms) are
    // excluded on purpose - a villager standing inside your own house would
    // be strange. Every genuinely public safe space gets one.
    const HOME_INTERIORS: &[RoomId] = &[
        9010, 9020, 9021, 9030, 9031, 9032, 9040, 9041, 9042, 9043, 9050, 9051, 9052, 9053, 9054,
    ];
    let world = seed_world();
    let missing: Vec<RoomId> = world
        .rooms
        .values()
        .filter(|r| r.safe && !HOME_INTERIORS.contains(&r.id))
        .filter(|r| !VILLAGERS.iter().any(|v| v.room == r.id))
        .map(|r| r.id)
        .collect();
    assert!(
        missing.is_empty(),
        "these public safe rooms have no villager: {missing:?}"
    );
}

#[test]
fn every_villager_has_real_content_and_a_real_home() {
    let world = seed_world();
    let mut seen_rooms = std::collections::HashSet::new();
    for v in VILLAGERS {
        assert_eq!(v.kind, FeatureKind::Villager);
        assert!(!v.name.is_empty());
        assert!(
            v.desc.len() >= 20,
            "{} has a suspiciously short line: {:?}",
            v.name,
            v.desc
        );
        assert!(
            world.rooms.contains_key(&v.room),
            "villager {} references missing room {}",
            v.name,
            v.room
        );
        // Multiple villagers may share a room only if genuinely distinct people
        // (never the exact same name twice in the same room).
        assert!(
            seen_rooms.insert((v.room, v.name)),
            "duplicate villager {} in room {}",
            v.name,
            v.room
        );
    }
}

// ---- Genesys: birds, mythical creatures, and adoptable strays -------------

#[test]
fn genesys_adds_real_birds_and_adoptable_creatures() {
    let birds_with_perch = WILDLIFE.iter().filter(|c| c.perch_note.is_some()).count();
    let mythical = WILDLIFE.iter().filter(|c| c.mythical).count();
    let adoptable = WILDLIFE.iter().filter(|c| c.adoptable).count();
    assert!(
        birds_with_perch >= 15,
        "expected at least 15 birds with a perched alternative, got {birds_with_perch}"
    );
    assert!(
        mythical >= 10,
        "expected at least 10 mythical creatures, got {mythical}"
    );
    assert!(
        adoptable >= 15,
        "expected at least 15 adoptable strays, got {adoptable}"
    );
    // Every adoptable stray must have a real home and a real name; every
    // perch note must actually differ from the flying note (no copy-paste).
    let world = seed_world();
    for c in WILDLIFE {
        assert!(
            world.rooms.contains_key(&c.home),
            "{} has no real home",
            c.name
        );
        if let Some(perch) = c.perch_note {
            assert_ne!(
                perch, c.note,
                "{} - perch note should differ from the flying note",
                c.name
            );
        }
    }
}

#[test]
fn display_note_toggles_between_flying_and_perched() {
    let bird = WILDLIFE
        .iter()
        .find(|c| c.perch_note.is_some())
        .expect("at least one bird has a perch alternative");
    let flying = (0..30u64).find(|&t| bird.display_note(t) == bird.note);
    let perched = (0..30u64).find(|&t| bird.display_note(t) == bird.perch_note.unwrap());
    assert!(flying.is_some(), "should read as flying at some moment");
    assert!(perched.is_some(), "should read as perched at some moment");
}

#[test]
fn non_flying_critters_never_show_a_perch_note() {
    for c in WILDLIFE {
        if c.perch_note.is_none() {
            for t in 0..10u64 {
                assert_eq!(c.display_note(t), c.note);
            }
        }
    }
}

#[test]
fn wildbound_waste_is_hung_off_the_sand_wyrms_maw() {
    let world = seed_world();
    let gateway = world
        .room(WILDBOUND_GATEWAY)
        .expect("Sand-Wyrm's Maw exists");
    let town = gateway
        .exits
        .get(&Dir::South)
        .copied()
        .expect("the Maw's south exit leads into the Waste");
    assert_eq!(
        town, WILDBOUND_BASE,
        "leads straight to the first gate town"
    );
    assert!(
        world
            .room(WILDBOUND_BASE)
            .expect("first town square")
            .exits
            .values()
            .any(|to| *to == WILDBOUND_GATEWAY),
        "the walk back out is reciprocal"
    );
}

#[test]
fn wildbound_towns_are_safe_islands_in_a_pvp_continent() {
    let world = seed_world();
    let mut safe_towns = 0;
    let mut pvp_fields = 0;
    for b in 0..3u32 {
        let base = WILDBOUND_BASE + b * WILDBOUND_BIOME_STRIDE;
        // The four town rooms (square, shelter, outfitter, gate) are safe
        // havens, never pvp ground.
        for offset in 0..4 {
            let room = world
                .room(base + offset)
                .unwrap_or_else(|| panic!("town room {} of biome {b} should exist", base + offset));
            assert!(
                room.safe && !room.pvp,
                "town room {} must be a safe haven",
                room.id
            );
            safe_towns += 1;
        }
        // Every other room in the biome's block is contested: pvp, never safe.
        for id in (base + 10)..(base + WILDBOUND_BIOME_STRIDE) {
            if let Some(room) = world.room(id) {
                assert!(
                    room.pvp && !room.safe,
                    "field room {id} in biome {b} must be pvp ground, not a haven"
                );
                pvp_fields += 1;
            }
        }
    }
    assert_eq!(safe_towns, 12, "three towns of four rooms each");
    assert!(pvp_fields >= 900, "a real continent of contested ground");
}

#[test]
fn wildbound_biomes_are_mazes_and_caverns_not_grids() {
    let world = seed_world();
    for b in 0..3u32 {
        let base = WILDBOUND_BASE + b * WILDBOUND_BIOME_STRIDE;
        let field: Vec<&Room> = world
            .rooms
            .values()
            .filter(|r| r.id >= base + 10 && r.id < base + WILDBOUND_BIOME_STRIDE)
            .collect();
        let dead_ends = field.iter().filter(|r| r.exits.len() == 1).count();
        let junctions = field.iter().filter(|r| r.exits.len() >= 3).count();
        assert!(
            dead_ends > 0 && junctions > 0,
            "biome {b} should read as a maze/cavern, not a uniform grid"
        );
    }
}

#[test]
fn wildbound_template_pool_is_three_hundred_mobs_plus_three_apex_bosses() {
    // The *template pool* (20 base creatures x 5 tiers x 3 biomes) is exactly
    // 300, independent of which combinations this particular seeded world
    // happens to roll into an actual room (see the variety check below).
    let pool: usize = WILDBOUND_BIOMES
        .iter()
        .map(|b| b.creatures.len() * WILDBOUND_TIER_AFFIX.len())
        .sum();
    assert_eq!(pool, 300, "20 creatures x 5 tiers x 3 biomes");
    assert_eq!(
        WILDBOUND_BIOMES.len(),
        3,
        "three biomes, each with its apex"
    );

    let world = seed_world();
    let wildbound: Vec<&MobSpawn> = world
        .spawns
        .iter()
        // Bounded above by Aelunor's own spawn-id band (1,600,000+), which
        // now sits just past Wildbound's - an unbounded `>=` here used to
        // silently sweep Aelunor's dozen zone bosses in as "Wildbound apex
        // bosses" too.
        .filter(|s| (WILDBOUND_SPAWN_ID_START..AELUNOR_SPAWN_ID_START).contains(&s.id))
        .collect();
    let distinct_names: std::collections::HashSet<&str> =
        wildbound.iter().map(|s| s.name).collect();
    let bosses = wildbound.iter().filter(|s| s.boss).count();
    assert_eq!(bosses, 3, "one apex boss per biome");
    assert!(
        distinct_names.len() >= 200,
        "the seeded world should draw wide variety from the 300-mob pool, got {}",
        distinct_names.len()
    );
    for spawn in &wildbound {
        assert!(
            world.rooms.contains_key(&spawn.home),
            "{} homes to missing room {}",
            spawn.name,
            spawn.home
        );
    }
    let levels: Vec<i32> = wildbound.iter().map(|s| s.level()).collect();
    // Ungated and walked into off the Sahra: it spans from early levels up to
    // the crowned endgame's doorstep, never the last crown itself.
    assert!(
        levels.iter().any(|&l| l < 30) && levels.iter().any(|&l| l > 50),
        "the Waste should span from early levels to the endgame's doorstep, got {levels:?}"
    );
}

#[test]
fn a_wildbound_apex_boss_pays_off_its_own_biome_not_the_frontier_crown() {
    // The Waste is walked into off the Sahra Wastes with no title at all, and
    // its authored stats sit in `tune_spawn_balance`'s gentle overworld bucket
    // on purpose, so what its apexes pay has to answer to the biome they
    // guard. The boss branch of `wildbound_loot` used to hand all three the
    // catalog's top table, which meant the 1500hp Duskmire boss dropped - on
    // every kill, since `roll_loot` never rolls for a boss - what the King Who
    // Was Promised Nothing guards at the end of twenty Frontier zones.
    use super::super::items::{FRONTIER_TIERS, frontier_loot};
    let world = seed_world();
    let tier_of = |loot: &'static [u32]| {
        (0..FRONTIER_TIERS)
            .find(|t| frontier_loot(*t) == loot)
            .expect("the Waste borrows the Frontier catalog, one tier per table")
    };

    for (b, biome) in WILDBOUND_BIOMES.iter().enumerate() {
        let base = WILDBOUND_BASE + (b as u32) * WILDBOUND_BIOME_STRIDE;
        let mobs: Vec<&MobSpawn> = world
            .spawns
            .iter()
            .filter(|s| (base..base + WILDBOUND_BIOME_STRIDE).contains(&s.home))
            .collect();
        let boss = mobs.iter().find(|s| s.boss).expect("one apex per biome");
        let boss_tier = tier_of(boss.loot);
        let deepest_regular = mobs
            .iter()
            .filter(|s| !s.boss)
            .map(|s| tier_of(s.loot))
            .max()
            .expect("the field is populated");

        assert!(
            boss_tier > deepest_regular,
            "{} should out-pay {}'s own deep trash (tier {deepest_regular}), got tier {boss_tier}",
            boss.name,
            biome.zone
        );
        assert!(
            boss_tier < FRONTIER_TIERS - 1,
            "the catalog's top table belongs to the Frontier's crown, not {} (tier {boss_tier})",
            boss.name
        );
        if let Some(next) = WILDBOUND_BIOMES.get(b + 1) {
            assert!(
                boss_tier <= next.loot_base,
                "{} should not out-pay the shallow end of {} (tier {}), got tier {boss_tier}",
                boss.name,
                next.zone,
                next.loot_base
            );
        }
    }
}

#[test]
fn aelunor_high_end_loot_is_a_lucky_find_not_the_default_drop() {
    // Aelunor is a lottery, not a shortcut past the Frontier. Two rules make
    // that true, and both live in `extend_aelunor`/`aelunor_loot`:
    //
    //   1. A Legendary spawn stays a genuinely rare roll at every depth. The
    //      affix roll used to climb linearly with the zone, so every spawn
    //      past zone 8 was Legendary - "rarity" was really just depth.
    //   2. A plain spawn's table stays in the catalog's lower half. Only the
    //      rare affixes reach the top bands, so the wood's ~660hp mobs can't
    //      hand out what the Frontier's ~3280hp mobs guard behind four Bane
    //      titles, on a walk in from the Amber Savanna with no gate at all.
    use super::super::items::{Rarity, item};
    let world = seed_world();
    let wood: Vec<&MobSpawn> = world
        .spawns
        .iter()
        .filter(|s| is_aelunor_room(s.home) && !s.boss)
        .collect();
    assert!(
        wood.len() > 100,
        "the wood should be populated, got {} spawns",
        wood.len()
    );

    let legendary = |s: &MobSpawn| s.name.starts_with("Legendary ");
    let share = |pool: &[&MobSpawn]| match pool.len() {
        0 => 0,
        n => pool.iter().filter(|s| legendary(s)).count() * 100 / n,
    };
    assert!(
        share(&wood) < 15,
        "a Legendary should be a lucky find across the wood, got {}% of spawns",
        share(&wood)
    );
    let deepest: Vec<&MobSpawn> = wood
        .iter()
        .copied()
        .filter(|s| (s.home - AELUNOR_BASE) / AELUNOR_ZONE_STRIDE == AELUNOR_ZONES as u32 - 1)
        .collect();
    assert!(
        share(&deepest) < 25,
        "even the Deep Heart keeps Legendaries a minority, got {}%",
        share(&deepest)
    );
    assert!(
        wood.iter().any(|s| legendary(s)),
        "but the tail is real - some spawns do roll Legendary"
    );

    // A plain, unaffixed spawn never carries endgame gear, however deep it is.
    for s in wood.iter().filter(|s| AELUNOR_CREATURES.contains(&s.name)) {
        for id in s.loot {
            let it = item(*id).unwrap_or_else(|| panic!("{} drops unknown item {id}", s.name));
            assert!(
                !matches!(it.rarity, Rarity::Epic | Rarity::Legendary),
                "{} is a plain spawn but drops {} ({})",
                s.name,
                it.name,
                it.rarity.label()
            );
        }
    }
    // The jackpot is real, though: the rare rolls do reach the top bands.
    assert!(
        wood.iter().any(|s| s
            .loot
            .iter()
            .any(|id| item(*id).is_some_and(|it| it.rarity == Rarity::Legendary))),
        "a Legendary spawn should be worth the walk"
    );
}

#[test]
fn a_legendary_aelunor_spawn_is_an_elite_that_guards_its_prize() {
    // The affix jumps the drop table twelve tiers wherever it lands, so what
    // carries it has to stand as far above the local floor as the prize does.
    // Otherwise the lottery is only a shortcut: a first-glade Legendary would
    // hand a wanderer Epic-band gear off an ordinary fight. The premium is
    // quadratic in the affix and flat across zones for exactly that reason -
    // the prize doesn't get smaller near the eaves, so neither does the guard.
    let world = seed_world();
    let wood: Vec<&MobSpawn> = world
        .spawns
        .iter()
        .filter(|s| is_aelunor_room(s.home) && !s.boss)
        .collect();
    let zone_of = |s: &MobSpawn| (s.home - AELUNOR_BASE) / AELUNOR_ZONE_STRIDE;
    let mut checked = 0;
    for legend in wood.iter().filter(|s| s.name.starts_with("Legendary ")) {
        // The toughest ordinary spawn in the same glade, deepest cell included.
        let Some(plain) = wood
            .iter()
            .filter(|s| zone_of(s) == zone_of(legend) && AELUNOR_CREATURES.contains(&s.name))
            .max_by_key(|s| s.max_hp)
        else {
            continue;
        };
        assert!(
            legend.max_hp * 10 >= plain.max_hp * 16,
            "{} ({} hp) barely outweighs the glade's toughest common {} ({} hp) - \
             a Legendary should read as a mini-boss",
            legend.name,
            legend.max_hp,
            plain.name,
            plain.max_hp,
        );
        assert!(
            legend.damage > plain.damage,
            "{} ({} dmg) hits no harder than the common {} ({} dmg)",
            legend.name,
            legend.damage,
            plain.name,
            plain.damage,
        );
        checked += 1;
    }
    assert!(checked > 0, "some glade should hold a Legendary to check");
}

#[test]
fn tutorial_zone_is_safe_reachable_and_teaches_every_core_system() {
    let world = seed_world();
    // All five rooms exist, and every one but the training yard is safe -
    // the yard needs `safe: false` for its dummy to be fightable at all.
    for offset in 0..5u32 {
        let room = world
            .room(TUTORIAL_BASE + offset)
            .unwrap_or_else(|| panic!("tutorial room {} should exist", TUTORIAL_BASE + offset));
        if offset == 1 {
            assert!(!room.safe, "the Training Yard must allow combat");
        } else {
            assert!(room.safe, "room {} should be a haven", room.id);
        }
        assert!(!room.pvp, "no tutorial room is contested ground");
    }
    // Reachable from the real start room by a normal walk (via the tavern).
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
    assert!(
        can_reach(world.start_room, TUTORIAL_BASE),
        "Wayfarer's Hollow must be reachable from Embergate"
    );
    assert!(
        can_reach(TUTORIAL_BASE, world.start_room),
        "and there must be a normal walk back"
    );
    // A brand-new join lands here, not at World::start_room directly.
    assert_eq!(tutorial_start_room(), TUTORIAL_BASE);
    assert_ne!(
        tutorial_start_room(),
        world.start_room,
        "the tutorial is distinct from Embergate itself"
    );
    // Combat: a near-harmless training dummy lives in the Training Yard.
    let dummy = world
        .spawns
        .iter()
        .find(|s| s.home == TUTORIAL_BASE + 1)
        .expect("the Training Yard has a dummy");
    assert!(!dummy.boss);
    assert!(
        dummy.damage <= 2,
        "the dummy must never meaningfully hurt a newcomer"
    );
    assert!(dummy.max_hp >= 30, "should survive a few practice rounds");
    // Gathering: one node per trade, all in the Gathering Glade.
    let glade_skills: std::collections::HashSet<GatherSkill> = NODES
        .iter()
        .filter(|n| n.home == TUTORIAL_BASE + 2)
        .map(|n| n.skill)
        .collect();
    assert_eq!(
        glade_skills.len(),
        5,
        "every gathering trade has a node here"
    );
    // Crafting: one station per trade, all in the Tinker's Hall.
    let stations = craft_stations_at(TUTORIAL_BASE + 3);
    assert_eq!(stations.len(), 5, "every craft trade has a station here");
    // Classes: the Tome of the Seventeen Callings stands in the Hall of Callings.
    assert!(
        features_at(TUTORIAL_BASE + 4)
            .iter()
            .any(|f| f.kind == FeatureKind::Plaque && f.name.contains("Seventeen Callings")),
        "the Hall of Callings should hold the class tome"
    );
    // Every safe tutorial room (all but the yard) has a villager, same
    // invariant as everywhere else in the world.
    for offset in [0u32, 2, 3, 4] {
        let room = TUTORIAL_BASE + offset;
        assert!(
            VILLAGERS.iter().any(|v| v.room == room),
            "safe tutorial room {room} needs a villager"
        );
    }
}

#[test]
fn zone_level_bands_are_sane_and_cover_the_road() {
    let world = seed_world();
    // Every zone that homes a mob gets a band, and every band is ordered.
    for spawn in &world.spawns {
        let zone = world.room(spawn.home).expect("mob home exists").zone;
        let (lo, hi) = world
            .zone_band(zone)
            .unwrap_or_else(|| panic!("zone {zone} homes a mob but has no band"));
        assert!(lo <= hi, "zone {zone} band is inverted: {lo}-{hi}");
        let level = spawn.level();
        assert!(
            (lo..=hi).contains(&level),
            "zone {zone} band {lo}-{hi} misses its own mob at level {level}"
        );
    }
    // The starting road reads as low-level ground, and a mob-less haven reads
    // as no band at all rather than a made-up number.
    let (lo, _) = world.zone_band("King's Road").expect("the road has mobs");
    assert!(lo <= 3, "the King's Road should read as a starting zone");
    assert!(world.zone_band("Hearthward Close").is_none());
    // The atlas carries the same bands per region.
    let progress = world.region_progress(&std::collections::HashSet::new(), 1);
    let road = progress
        .iter()
        .find(|r| r.name.contains("King's Road"))
        .expect("home region listed");
    assert!(road.levels.is_some(), "the home region has hostile levels");
}

// ---- The world resist/weak pass (spec: CONTEXT.md, same-named section) -------------
//
// One theme per generated zone; regulars wear the theme's resist/weak, bosses
// keep their authored profiles. The tests below pin the placement to the theme
// tables, hold the school census inside declared bands, and run the routed
// grind-rate model that keeps the pass meaningful without rebalancing anyone.

/// Every themed region: name, theme table, base room id, and rooms per zone.
/// A spawn's home maps back to its zone by `(home - base) / stride`.
fn themed_regions() -> [(&'static str, &'static [ZoneTheme], u32, u32); 7] {
    use super::super::archipelago;
    [
        (
            "Frontier",
            &FRONTIER_ZONE_THEMES,
            FRONTIER_BASE,
            FRONTIER_W * FRONTIER_H,
        ),
        (
            "Reaches",
            &REACHES_ZONE_THEMES,
            REACHES_BASE,
            REACHES_ZONE_STRIDE,
        ),
        (
            "Kaelmyr",
            &KAELMYR_ZONE_THEMES,
            KAELMYR_BASE,
            KAELMYR_ZONE_STRIDE,
        ),
        (
            "Sunderlakes",
            &LAKES_ZONE_THEMES,
            LAKES_BASE,
            LAKES_ZONE_STRIDE,
        ),
        (
            "Broceliande",
            &BROCELIANDE_ZONE_THEMES,
            BROCELIANDE_BASE,
            BROCELIANDE_ZONE_STRIDE,
        ),
        (
            "Aelunor",
            &AELUNOR_ZONE_THEMES,
            AELUNOR_BASE,
            AELUNOR_ZONE_STRIDE,
        ),
        (
            "Archipelago",
            &archipelago::ISLAND_THEMES,
            archipelago::ARCH_BASE,
            archipelago::ARCH_STRIDE,
        ),
    ]
}

fn zone_index(home: RoomId, base: u32, stride: u32, zones: usize) -> Option<usize> {
    (home >= base && home < base + stride * zones as u32).then(|| ((home - base) / stride) as usize)
}

/// The seven schools a theme may name (Physical is banned from both slots).
const THEMED_SCHOOLS: [DamageType; 7] = [
    DamageType::Fire,
    DamageType::Frost,
    DamageType::Holy,
    DamageType::Shadow,
    DamageType::Poison,
    DamageType::Arcane,
    DamageType::Lightning,
];

#[test]
fn every_generated_zone_spawn_wears_its_zone_theme() {
    // Aelunor's glade bosses are the one authored exception inside a themed
    // region: they carry a hand-written Shadow/resist-Physical/weak-Holy
    // profile that is the region's whole school game, so they are named here
    // rather than silently skipped.
    const AELUNOR: &str = "Aelunor";
    let world = seed_world();
    let mut regulars = 0usize;
    let mut bosses = 0usize;
    for (region, themes, base, stride) in themed_regions() {
        for spawn in &world.spawns {
            let Some(z) = zone_index(spawn.home, base, stride, themes.len()) else {
                continue;
            };
            let theme = themes[z];
            if spawn.boss {
                bosses += 1;
                if region == AELUNOR {
                    continue;
                }
                // A zone boss wears the zone's weakness and never its resist:
                // the fight players provision for is where the prep mechanic
                // has to exist, and a resist there would be a class tax with
                // no counterplay. Asserted, not assumed - this branch used to
                // `continue` on a comment claiming bosses were untouched,
                // which stopped being true the moment they were.
                assert_eq!(
                    spawn.profile.resist, None,
                    "{region} zone {z}: boss {} must not resist a school",
                    spawn.name
                );
                assert_eq!(
                    spawn.profile.weak,
                    theme.weak(),
                    "{region} zone {z}: boss {} wears the zone weakness",
                    spawn.name
                );
                continue;
            }
            regulars += 1;
            assert_eq!(
                spawn.profile.resist,
                theme.resist(),
                "{region} zone {z}: {} wears the zone resist",
                spawn.name
            );
            assert_eq!(
                spawn.profile.weak,
                theme.weak(),
                "{region} zone {z}: {} wears the zone weakness",
                spawn.name
            );
        }
    }
    assert!(
        regulars > 2000,
        "the pass covers the generated regions ({regulars} regulars seen)"
    );
    assert!(
        bosses >= 100,
        "the zone bosses were seen and checked ({bosses})"
    );
}

#[test]
fn no_regular_resists_physical_and_nothing_is_weak_to_physical() {
    let world = seed_world();
    // Nothing in the whole world, authored or generated, boss or regular, is
    // ever weak to Physical: the neutral auto-attack baseline never inflates.
    for spawn in &world.spawns {
        assert_ne!(
            spawn.profile.weak,
            Some(DamageType::Physical),
            "{} must not be weak to Physical",
            spawn.name
        );
    }
    // No regular anywhere resists Physical: a Physical resist is a 50% tax on
    // the seven Physical-locked classes with no counterplay, so it lives on
    // bosses only, where "bring a caster, an oil, or the smith" is the point.
    for spawn in world.spawns.iter().filter(|s| !s.boss) {
        assert_ne!(
            spawn.profile.resist,
            Some(DamageType::Physical),
            "regular {} must not resist Physical",
            spawn.name
        );
    }
    // The theme vocabulary itself can never emit Physical, and every theme
    // carries a weakness (weak-forward).
    for theme in ZoneTheme::ALL {
        assert_ne!(theme.resist(), Some(DamageType::Physical), "{theme:?}");
        assert_ne!(theme.weak(), Some(DamageType::Physical), "{theme:?}");
        assert!(theme.weak().is_some(), "{theme:?} must carry a weakness");
    }
}

#[test]
fn every_boss_carries_a_weakness() {
    // Bosses are the fights players actually prepare for, so the prep
    // mechanic must exist there: every boss in the world names a weakness
    // (weak-forward: pure reward - an unprepared fighter loses nothing,
    // a provisioned one is paid). Resists stay rare authored events.
    let world = seed_world();
    let neutral: Vec<&str> = world
        .spawns
        .iter()
        .filter(|s| s.boss && s.profile.weak.is_none())
        .map(|s| s.name)
        .collect();
    assert!(neutral.is_empty(), "bosses without a weakness: {neutral:?}");
}

#[test]
fn the_school_census_stays_inside_its_declared_bands() {
    let regions = themed_regions();
    let total_zones: usize = regions.iter().map(|(_, t, _, _)| t.len()).sum();
    let school_pos = |d: DamageType| {
        THEMED_SCHOOLS
            .iter()
            .position(|s| *s == d)
            .expect("themed school")
    };

    let mut weak = [0usize; 7];
    let mut resist = [0usize; 7];
    for (region, themes, _, _) in &regions {
        let mut region_weak = [0usize; 7];
        let mut region_resists = 0usize;
        for theme in *themes {
            let w = school_pos(theme.weak().expect("weak-forward"));
            weak[w] += 1;
            region_weak[w] += 1;
            if let Some(r) = theme.resist() {
                resist[school_pos(r)] += 1;
                region_resists += 1;
            }
        }
        // Walls are events: resist zones stay a rough third of a region at
        // most, and no single school owns more than a quarter of a region's
        // weaknesses, so every region offers several different answers.
        assert!(
            region_resists <= themes.len().div_ceil(3),
            "{region}: {region_resists} resist zones of {}",
            themes.len()
        );
        let lanes = region_weak.iter().filter(|c| **c > 0).count();
        assert!(
            lanes >= 5,
            "{region}: only {lanes} weak schools represented"
        );
        let max_lane = region_weak.iter().max().copied().unwrap_or(0);
        assert!(
            max_lane <= themes.len().div_ceil(4),
            "{region}: one school owns {max_lane} of {} zones",
            themes.len()
        );
    }
    // Global bands per school. Holy keeps predators (rule 4: without resist
    // zones the two Holy classes silently become the school winners), no
    // school's weakness count runs away, and every school has a real lane.
    for (i, school) in THEMED_SCHOOLS.iter().enumerate() {
        assert!(
            (10..=30).contains(&weak[i]),
            "{school:?}: {} weak zones of {total_zones} is outside 10..=30",
            weak[i]
        );
        assert!(
            resist[i] <= 10,
            "{school:?}: {} resist zones is past the band",
            resist[i]
        );
    }
    assert!(
        resist[school_pos(DamageType::Holy)] >= 4,
        "Holy needs its predators"
    );
    let total_resists: usize = resist.iter().sum();
    assert!(
        total_resists * 3 <= total_zones,
        "{total_resists} resist zones of {total_zones}: walls must stay rare"
    );
}

/// The per-class offensive school mix at the Lv45 anchor, read from the real
/// ability roster: each Strike/DoT/Finisher unlocked by 45 contributes its
/// total effect per cooldown tick, normalized to shares per school.
fn class_school_mix(class: super::super::classes::Class) -> Vec<(DamageType, f64)> {
    use super::super::abilities::{ABILITIES, AbilityEffect};
    let mut weights: Vec<(DamageType, f64)> = Vec::new();
    for a in ABILITIES {
        if a.class != class || a.level_req > 45 {
            continue;
        }
        let ticks = match a.effect {
            AbilityEffect::Strike | AbilityEffect::Finisher => 1.0,
            AbilityEffect::DamageOverTime => 1.0 + a.duration as f64,
            _ => continue,
        };
        let dps = a.magnitude as f64 * ticks / a.cooldown_ticks.max(1) as f64;
        match weights.iter_mut().find(|(d, _)| *d == a.damage_type) {
            Some((_, w)) => *w += dps,
            None => weights.push((a.damage_type, dps)),
        }
    }
    let total: f64 = weights.iter().map(|(_, w)| w).sum();
    for (_, w) in &mut weights {
        *w /= total;
    }
    weights
}

#[test]
fn the_world_pass_redistributes_grind_rates_but_never_rebalances_a_class() {
    // The grind-rate model, from CONTEXT.md ("The world resist/weak pass"): at band
    // gear every class is ~75% auto damage (always Physical, and regulars are
    // never weak to or resistant against it), ~25% abilities in the class's
    // school mix, and a weapon oil adds a flat rider in the coat's school.
    // Before this pass every generated regular was (None, None), so the
    // "before" rate is exactly 1.0 in every zone: each assertion below is a
    // live before/after budget.
    //
    // The rider is *derived from the engine*, never declared here. It used to
    // be a bare 0.15 and the real coat was worth three to six times that,
    // which no assertion in this file could see. Now it is read off the real
    // coat curve against the real attack bar (both pinned to a live character
    // by svc_test), at the tier where the coat weighs heaviest - so if anyone
    // retunes a coat, this budget moves with it.
    use super::super::svc::{AUTO_SHARE, OIL_PER_TICK, TIER_ATTACK_BAR};
    const AUTO: f64 = AUTO_SHARE;
    const ABILITIES_SHARE: f64 = 1.0 - AUTO_SHARE;
    // The rider a typical coated character carries: the coat curve's mean
    // share of the attack bar, converted to a share of total output. The mean
    // is the right input because the model asks what routing is worth to a
    // player, not what the worst-rounded tier looks like - and no tier can
    // hide behind it, because `the_coat_curves_stay_inside_their_share_of_the
    // _bar` pins every tier to a tight band on the same two constants.
    let oil_rider = (0..6)
        .map(|t| OIL_PER_TICK[t] as f64 / TIER_ATTACK_BAR[t] as f64 * AUTO_SHARE)
        .sum::<f64>()
        / 6.0;
    assert!(
        oil_rider <= 0.16,
        "the oil rider is worth {oil_rider:.3} of output: past what this budget was written for"
    );
    let oil_schools = super::super::items::OIL_SCHOOLS;

    let mult = |theme: ZoneTheme, school: DamageType| -> f64 {
        if theme.weak() == Some(school) {
            1.5
        } else if theme.resist() == Some(school) {
            0.5
        } else {
            1.0
        }
    };

    let regions = themed_regions();
    let mut routed_best: Vec<(super::super::classes::Class, f64)> = Vec::new();
    for class in super::super::classes::Class::ALL {
        let mix = class_school_mix(class);
        let ability_mult =
            |theme: ZoneTheme| -> f64 { mix.iter().map(|(d, w)| w * mult(theme, *d)).sum::<f64>() };

        // Uncoated redistribution budget: within a zone a class may swing up
        // to +-15%, and its average across every themed zone stays within a
        // few percent of the old all-neutral world. Redistribution yes,
        // rebalancing no.
        let mut rates: Vec<f64> = Vec::new();
        for (region, themes, _, _) in &regions {
            for (z, theme) in themes.iter().enumerate() {
                let rate = AUTO + ABILITIES_SHARE * ability_mult(*theme);
                assert!(
                    (0.85..=1.15).contains(&rate),
                    "{class:?} in {region} zone {z}: {rate:.3} is outside the +-15% band"
                );
                rates.push(rate);
            }
        }
        let avg = rates.iter().sum::<f64>() / rates.len() as f64;
        assert!(
            (0.97..=1.03).contains(&avg),
            "{class:?}: themed-zone average {avg:.3} moved past the budget"
        );

        // The routed model: a player picks the zone and the coat. Neutral
        // play is a coated weapon on unthemed ground (1 + the rider).
        // Floor: in every region there is a zone-and-coat answer worth at
        // least +5%, so the school game is worth playing everywhere, for
        // everyone. The legacy poison coat is left out of the model; it only
        // adds options, never removes one.
        let mut global_best: f64 = 0.0;
        for (region, themes, _, _) in &regions {
            let mut region_best: f64 = 0.0;
            for theme in *themes {
                let coat_best = oil_schools
                    .iter()
                    .map(|s| mult(*theme, *s))
                    .fold(0.0f64, f64::max);
                let rate = AUTO + ABILITIES_SHARE * ability_mult(*theme) + oil_rider * coat_best;
                region_best = region_best.max(rate);
            }
            let edge = region_best / (1.0 + oil_rider);
            assert!(
                edge >= 1.05,
                "{class:?} in {region}: best routed edge {edge:.3} is under the +5% floor"
            );
            global_best = global_best.max(edge);
        }
        routed_best.push((class, global_best));
    }

    // Ceiling: nobody's best-case routing runs away. The two mono-Holy
    // classes top the table by design (a Holy oil stacks with their own
    // school in the Undead/Haunted lanes - the deliberate buff to today's
    // weakest classes), and even they stay under +18%; the spread between
    // the best- and worst-served class stays within 12 points.
    let max = routed_best.iter().map(|(_, e)| *e).fold(0.0f64, f64::max);
    let min = routed_best.iter().map(|(_, e)| *e).fold(f64::MAX, f64::min);
    for (class, edge) in &routed_best {
        assert!(
            *edge <= 1.18,
            "{class:?}: routed best {edge:.3} is past the +18% ceiling"
        );
    }
    assert!(
        max - min <= 0.12,
        "routed spread {max:.3} - {min:.3} is past 12 points"
    );
}
