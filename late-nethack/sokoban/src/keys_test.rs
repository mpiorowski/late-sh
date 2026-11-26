use crate::keys::{batches, steps};
use crate::level::parse;
use crate::solve::solve;

const BOARD: &str = "\
|@...|
|.0..|
|..0R^^|";

#[test]
fn steps_and_batches_type_the_same_keys_and_end_solved() {
    let (level, start) = parse(BOARD).unwrap();
    let pushes = solve(&level, start).unwrap();
    let steps = steps(&level, start, &pushes);
    let typed: String = steps.iter().map(|s| s.key).collect();
    let batched: String = batches(&level, start, &pushes).iter().map(|b| b.keys.as_str()).collect();
    assert_eq!(typed, batched);
    let last = steps.last().unwrap().after;
    assert!(level.solved(&last));
    assert_eq!(level.render(&last), "|....|\n|....|\n|..@R..|");
}

#[test]
fn every_step_moves_the_hero_one_square() {
    let (level, start) = parse(BOARD).unwrap();
    let pushes = solve(&level, start).unwrap();
    let mut at = level.pos[start.hero];
    for s in steps(&level, start, &pushes) {
        let next = level.pos[s.after.hero];
        assert_eq!(at.0.abs_diff(next.0) + at.1.abs_diff(next.1), 1, "key {}", s.key);
        at = next;
    }
}
