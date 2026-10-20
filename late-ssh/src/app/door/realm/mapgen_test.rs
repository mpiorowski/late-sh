use super::*;

fn spec(continents: u8, islands: u8, territories: u16) -> GeneratedMapSpec {
    GeneratedMapSpec {
        seed: 42,
        continents,
        islands,
        territories,
        names: Vec::new(),
    }
}

/// The whole persistence story rests on this: a game stores three numbers and
/// a seed, and every rebuild — next tick, next restart, next year — has to be
/// the same world, or two players are fighting over different maps.
#[test]
fn the_same_spec_builds_the_same_world_twice() {
    let a = generate(&spec(4, 6, 90));
    let b = generate(&spec(4, 6, 90));
    assert_eq!(a.grid.cells, b.grid.cells);
    assert_eq!(a.territories.len(), b.territories.len());
    for (x, y) in a.territories.iter().zip(&b.territories) {
        assert_eq!(x.name, y.name);
        assert_eq!(x.center, y.center);
        assert_eq!(x.area_km2, y.area_km2);
        assert_eq!(x.population, y.population);
        assert_eq!(x.neighbors, y.neighbors);
    }
}

#[test]
fn a_different_seed_is_a_different_world() {
    let a = generate(&spec(4, 6, 90));
    let mut other = spec(4, 6, 90);
    other.seed = 43;
    let b = generate(&other);
    assert_ne!(a.grid.cells, b.grid.cells, "the seed has to matter");
}

/// Every invariant the engine relies on, over the corners of the parameter
/// space rather than one comfortable middle.
#[test]
fn every_generated_world_is_a_playable_map() {
    for (continents, islands, territories) in [
        (1, 0, 50),
        (10, 20, 400),
        (0, 20, 50),
        (3, 0, 120),
        (5, 8, 160),
    ] {
        let map = generate(&spec(continents, islands, territories));
        let label = format!("{continents}c/{islands}i/{territories}t");
        assert_eq!(
            map.territories.len(),
            territories as usize,
            "{label}: asked for {territories} territories"
        );
        let mut names: std::collections::HashSet<&str> = Default::default();
        let mut codes: std::collections::HashSet<&str> = Default::default();
        for (index, t) in map.territories.iter().enumerate() {
            assert_eq!(t.id as usize, index, "{label}: ids are positions");
            assert!(names.insert(&t.name), "{label}: {} is used twice", t.name);
            assert!(
                codes.insert(&t.iso),
                "{label}: code {} is used twice",
                t.iso
            );
            assert!(!t.name.is_empty(), "{label}: a nameless country");
            // The cursor lands on centers, so a center outside its own land
            // is a jump into the sea.
            assert_eq!(
                map.grid.territory_at(t.center.0, t.center.1),
                Some(t.id),
                "{label}: {} has its center off its own land",
                t.name
            );
            assert!(t.area_km2 > 0, "{label}: {} has no area", t.name);
            assert!(t.population > 0, "{label}: {} has no people", t.name);
            assert!((0.0..=1.0).contains(&t.weight), "{label}: weight range");
            // Adjacency drives every hop distance in the game; one-way
            // borders would make attacks legal in one direction only.
            for n in &t.neighbors {
                assert!(
                    map.territory(*n).unwrap().neighbors.contains(&t.id),
                    "{label}: {} -> {n} is one-way",
                    t.name
                );
                assert_ne!(*n, t.id, "{label}: {} borders itself", t.name);
            }
        }
    }
}

/// A world has to be findable: if the land is one blob in a corner, or spread
/// so thin that nothing borders anything, the game underneath stops working.
#[test]
fn the_land_looks_like_a_world() {
    let map = generate(&spec(5, 8, 160));
    let land = map.grid.cells.iter().filter(|c| **c != WATER).count() as f64;
    let total = map.grid.cells.len() as f64;
    // Cells, not area — the poles are empty, so the cell share sits under the
    // 29% area target.
    assert!(
        (0.10..0.45).contains(&(land / total)),
        "land share is {:.3}",
        land / total
    );
    let borders: usize = map.territories.iter().map(|t| t.neighbors.len()).sum();
    let average = borders as f64 / map.territories.len() as f64;
    assert!(
        average > 2.0,
        "average of {average:.2} land borders is a dust cloud"
    );
    let coastal = map.territories.iter().filter(|t| t.coastal).count();
    assert!(coastal > 0, "a world with no coast");
    assert!(coastal < map.territories.len(), "a world with no interior");
}

/// Sizes have to vary or the map reads as graph paper, and the strength
/// weights built on top of area and population go flat with it.
#[test]
fn countries_come_in_different_sizes() {
    let map = generate(&spec(5, 8, 160));
    let mut areas: Vec<u64> = map.territories.iter().map(|t| t.area_km2).collect();
    areas.sort_unstable();
    let small = areas[areas.len() / 10];
    let large = areas[areas.len() * 9 / 10];
    assert!(
        large > small * 3,
        "p90 {large} is not much bigger than p10 {small}"
    );
}

/// The globe is Earth's globe. Everything distance-tuned in the rulesets —
/// `sea_hop_km`, the distance decay — is calibrated in kilometres on a
/// 510-million-km² sphere, so a generated world has to be one too.
#[test]
fn the_world_is_earth_sized_whatever_the_parameters() {
    for (continents, islands) in [(1u8, 0u8), (10, 20), (0, 20)] {
        let map = generate(&spec(continents, islands, 120));
        let land: u64 = map.territories.iter().map(|t| t.area_km2).sum();
        let earth_land = 510_000_000.0 * LAND_FRACTION;
        let ratio = land as f64 / earth_land;
        assert!(
            (0.6..1.4).contains(&ratio),
            "{continents}c/{islands}i: land is {ratio:.2}x Earth's"
        );
    }
}

/// Islands are the alternative to a border war: they have to actually be
/// islands, reachable only by sea.
#[test]
fn islands_are_separate_from_the_mainland() {
    let map = generate(&spec(2, 12, 120));
    let mainland: Vec<TerritoryId> = vec![0];
    let reachable = map.hop_distances(&mainland);
    assert!(
        reachable.len() < map.territories.len(),
        "everything is walkable, so nothing is an island"
    );
}

#[test]
fn a_world_with_no_islands_has_no_strays() {
    let map = generate(&spec(1, 0, 60));
    let reachable = map.hop_distances(&[0]);
    assert_eq!(
        reachable.len(),
        map.territories.len(),
        "one continent, no islands: everything should be walkable"
    );
}

#[test]
fn the_parameters_are_clamped_rather_than_trusted() {
    let wild = GeneratedMapSpec {
        seed: 7,
        continents: 200,
        islands: 200,
        territories: 5_000,
        names: Vec::new(),
    }
    .normalized();
    assert_eq!(wild.continents, GEN_MAX_CONTINENTS);
    assert_eq!(wild.islands, GEN_MAX_ISLANDS);
    assert_eq!(wild.territories, GEN_MAX_TERRITORIES);

    // No land at all is the one request that cannot be honoured.
    let empty = GeneratedMapSpec {
        seed: 7,
        continents: 0,
        islands: 0,
        territories: 50,
        names: Vec::new(),
    }
    .normalized();
    assert_eq!((empty.continents, empty.islands), (1, 0));
}

/// Named worlds come from the AI service, which can return too few, repeat
/// itself, or be switched off entirely. None of those may produce a map with
/// a blank or duplicated country on it.
#[test]
fn supplied_names_are_used_topped_up_and_deduplicated() {
    let mut named = spec(1, 0, 50);
    named.names = vec![
        "Ashfall".to_string(),
        "ashfall".to_string(), // the same name in a different case
        "  ".to_string(),      // and a blank one
        "Brackmoor".to_string(),
    ];
    let map = generate(&named);
    assert_eq!(map.territories[0].name, "Ashfall");
    assert_eq!(map.territories[1].name, "Brackmoor");
    let names: std::collections::HashSet<&str> =
        map.territories.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(
        names.len(),
        map.territories.len(),
        "no repeats after topping up"
    );
    assert!(map.territories.iter().all(|t| !t.name.trim().is_empty()));
}

#[test]
fn the_name_pool_is_a_thousand_distinct_names() {
    let pool = &*NAME_POOL;
    assert_eq!(pool.len(), 1_000);
    let unique: std::collections::HashSet<&str> = pool.iter().copied().collect();
    assert_eq!(unique.len(), 1_000, "the pool repeats itself");
    assert!(
        pool.iter().all(|n| n.is_ascii() && !n.trim().is_empty()),
        "names have to render in a terminal"
    );
    // Two worlds should not read as the same atlas in a different order.
    let a: std::collections::HashSet<String> = pool_names(1, 50).into_iter().collect();
    let b: std::collections::HashSet<String> = pool_names(2, 50).into_iter().collect();
    assert!(
        a.intersection(&b).count() < 25,
        "seeds barely change the draw"
    );
}

#[test]
fn zoom_levels_are_built_and_keep_their_land() {
    let map = generate(&spec(4, 6, 120));
    assert!(map.max_zoom() > 0, "a 1024-wide world needs zoom levels");
    assert!(map.level(map.max_zoom()).width <= 160);
    for level in &map.mips {
        assert!(level.cells.iter().any(|c| *c != WATER), "land survives");
    }
}

/// The cache is what makes rebuilding affordable; a spec that is equal has to
/// hit it, and a spec that differs by one field must not.
#[test]
fn the_cache_returns_the_same_world_for_the_same_spec() {
    let one = map_for_spec(&spec(2, 3, 60));
    let two = map_for_spec(&spec(2, 3, 60));
    assert!(Arc::ptr_eq(&one, &two), "same spec should not rebuild");
    let other = map_for_spec(&spec(2, 3, 61));
    assert!(!Arc::ptr_eq(&one, &other));
    assert_ne!(spec(2, 3, 60).key(), spec(2, 3, 61).key());
}

/// Determinism across *processes*, not just within one. The in-run test above
/// would pass even if the build depended on hash ordering, which it did once —
/// a `HashMap` walked in its own order handed cells to territories in a
/// different order, and summing their areas in a different order changes the
/// last bit of a float. A pinned fingerprint is what catches that.
#[test]
fn a_built_world_has_a_stable_fingerprint() {
    let map = generate(&spec(3, 5, 80));
    let mut hasher = blake3::Hasher::new();
    for cell in &map.grid.cells {
        hasher.update(&cell.to_le_bytes());
    }
    for t in &map.territories {
        hasher.update(t.name.as_bytes());
        hasher.update(&t.population.to_le_bytes());
        hasher.update(&t.area_km2.to_le_bytes());
        hasher.update(&t.center.0.to_le_bytes());
        hasher.update(&t.center.1.to_le_bytes());
        for n in &t.neighbors {
            hasher.update(&n.to_le_bytes());
        }
    }
    assert_eq!(
        hasher.finalize().to_hex().as_str(),
        "5a89d33e397b3957b7b918119fdfd30ad6344782ee21241971854209bac3e2df"
    );
}

/// Continents have to look drawn, not compassed. The measure is the
/// isoperimetric quotient — `4πA / P²`, which is 1.0 for a circle and falls
/// as a coastline gets involved. The first cut of this generator produced
/// lobed discs at ~0.6; Earth's own Afro-Eurasia, measured the same way on
/// its (four times finer) grid, is 0.014. Somewhere well under a third is
/// what "a continent rather than a blob" means here.
#[test]
fn continents_are_not_discs() {
    for seed in [1u64, 2, 3] {
        let mut shape = spec(4, 6, 120);
        shape.seed = seed;
        let map = generate(&shape);
        let (w, h) = (map.grid.width as i32, map.grid.height as i32);
        let mass_of: Vec<u32> = map
            .territories
            .iter()
            .map(|t| t.landmasses.first().copied().unwrap_or(u32::MAX))
            .collect();
        let mut area = std::collections::HashMap::<u32, i64>::new();
        let mut perimeter = std::collections::HashMap::<u32, i64>::new();
        for y in 0..h {
            for x in 0..w {
                let cell = map.grid.cells[(y * w + x) as usize];
                if cell == WATER {
                    continue;
                }
                let mass = mass_of[cell as usize];
                *area.entry(mass).or_default() += 1;
                for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                    let nx = (x + dx).rem_euclid(w);
                    let ny = y + dy;
                    let neighbour = if ny < 0 || ny >= h {
                        WATER
                    } else {
                        map.grid.cells[(ny * w + nx) as usize]
                    };
                    if neighbour == WATER {
                        *perimeter.entry(mass).or_default() += 1;
                    }
                }
            }
        }
        let (biggest, cells) = area
            .iter()
            .max_by_key(|(_, cells)| **cells)
            .expect("a world has land");
        let a = *cells as f64;
        let p = perimeter[biggest] as f64;
        let quotient = 4.0 * std::f64::consts::PI * a / (p * p);
        assert!(
            quotient < 0.30,
            "seed {seed}: biggest landmass is too round ({quotient:.3})"
        );
        // And not shredded into lace, which would be its own kind of
        // unreadable and would make every country a coastal one.
        assert!(
            quotient > 0.02,
            "seed {seed}: biggest landmass is a filigree ({quotient:.3})"
        );
    }
}
