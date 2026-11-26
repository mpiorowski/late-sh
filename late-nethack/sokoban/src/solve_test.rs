use crate::level::parse;
use crate::solve::{SolveError, solve};

#[test]
fn solves_stock_levels_from_the_start() {
    // the quick ones: a debug build is slow, `SOKOBAN.md` has the release timings of all 8
    for name in ["soko2-1", "soko2-2", "soko4-1"] {
        let text = std::fs::read_to_string(format!("{}/levels/{name}.txt", env!("CARGO_MANIFEST_DIR"))).unwrap();
        let (level, start) = parse(&text).unwrap();
        let pushes = solve(&level, start).unwrap_or_else(|e| panic!("{name}: {e:?}"));
        let mut s = start;
        for p in &pushes {
            s = level.push(&s, p.boulder, p.dir).unwrap();
        }
        assert!(level.solved(&s), "{name}");
    }
}

#[test]
fn a_boulder_stuck_in_a_corner_is_unsolvable() {
    let (level, start) = parse("|0...|\n|..@^|").unwrap();
    assert_eq!(solve(&level, start).err(), Some(SolveError::Unsolvable));
}
