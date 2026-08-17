use super::*;
use std::collections::{HashSet, VecDeque};

/// Every floor cell has to be reachable from every other one. Food spawns on
/// any non-wall cell and an arena only clears when the last food is eaten, so
/// a sealed-off pocket would wedge the table forever — no vote, no timeout,
/// just a level nobody can finish. Reachability is checked on a torus because
/// movement wraps (`wrap_pos`).
#[test]
fn every_level_is_one_connected_region() {
    for (index, source) in LEVEL_SOURCES.iter().enumerate() {
        let level = parse_level(source).expect("level parses");
        let (w, h) = (level.width, level.height);
        let floor: Vec<(usize, usize)> = (0..h)
            .flat_map(|y| (0..w).map(move |x| (x, y)))
            .filter(|(x, y)| !level.is_wall(*x, *y))
            .collect();
        assert!(!floor.is_empty(), "level {} has no floor", index + 1);

        let mut seen: HashSet<(usize, usize)> = HashSet::from([floor[0]]);
        let mut queue = VecDeque::from([floor[0]]);
        while let Some((x, y)) = queue.pop_front() {
            for (dx, dy) in [(0i32, -1i32), (0, 1), (-1, 0), (1, 0)] {
                let nx = (x as i32 + dx).rem_euclid(w as i32) as usize;
                let ny = (y as i32 + dy).rem_euclid(h as i32) as usize;
                if !level.is_wall(nx, ny) && seen.insert((nx, ny)) {
                    queue.push_back((nx, ny));
                }
            }
        }
        assert_eq!(
            seen.len(),
            floor.len(),
            "level {} ({}) strands {} floor cells; food could spawn where no \
             snake can reach it and the arena would never clear",
            index + 1,
            level.name,
            floor.len() - seen.len()
        );
    }
}

/// Five snakes plus their growth have to fit with room to manoeuvre.
#[test]
fn every_level_has_room_for_a_full_table() {
    for (index, source) in LEVEL_SOURCES.iter().enumerate() {
        let level = parse_level(source).expect("level parses");
        let floor = (0..level.height)
            .flat_map(|y| (0..level.width).map(move |x| (x, y)))
            .filter(|(x, y)| !level.is_wall(*x, *y))
            .count();
        let needed = 5 * level.initial_length as usize + 40;
        assert!(
            floor >= needed,
            "level {} ({}) has {floor} floor cells, needs {needed} for five \
             snakes of {}",
            index + 1,
            level.name,
            level.initial_length
        );
    }
}

#[test]
fn all_shipped_levels_parse() {
    for (index, source) in LEVEL_SOURCES.iter().enumerate() {
        let level = parse_level(source)
            .unwrap_or_else(|error| panic!("level {} failed: {error:#}", index + 1));
        assert!(level.width <= MAX_WIDTH);
        assert!(level.height <= MAX_HEIGHT);
        assert!(level.tick_millis >= 60, "level {} too fast", index + 1);
        assert!(
            level
                .cells
                .iter()
                .any(|cell| matches!(cell, Cell::Empty | Cell::Warp)),
            "level {} has no floor",
            index + 1
        );
    }
    assert_eq!(LEVELS.len(), LEVEL_SOURCES.len());
}

#[test]
fn parser_rejects_ragged_rows() {
    let source = "name: X\npoints-needed: 1\ntick-millis: 100\ninitial-length: 3\ngrowth-factor: 3\n\n###\n##\n";
    assert!(parse_level(source).is_err());
}

#[test]
fn parser_reads_warp_cells() {
    let source = "name: X\npoints-needed: 1\ntick-millis: 100\ninitial-length: 3\ngrowth-factor: 3\n\n#~#\n#.#\n###\n";
    let level = parse_level(source).unwrap();
    assert_eq!(level.cell(1, 0), Cell::Warp);
    assert_eq!(level.cell(1, 1), Cell::Empty);
    assert!(level.is_wall(0, 0));
}
