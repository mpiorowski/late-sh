use super::{Dir, ParseError, flip, parse};

#[test]
fn a_push_into_an_open_hole_fills_it_and_the_hero_follows() {
    let (level, s) = parse("|@0^.|").unwrap();
    let n = level.push(&s, 1, Dir::East).unwrap();
    assert_eq!(level.render(&n), "|.@..|");
    assert!(level.solved(&n));
}

#[test]
fn a_boulder_rolls_over_the_trap_into_the_first_open_hole() {
    let (level, s) = parse("|@0R.^^|").unwrap();
    let n = level.push(&s, 1, Dir::East).unwrap();
    assert_eq!(level.render(&n), "|.@R..^|");
}

#[test]
fn a_hit_boulder_takes_over_the_roll_until_the_wall() {
    let (level, s) = parse("|@0R.0.|^|").unwrap();
    let n = level.push(&s, 1, Dir::East).unwrap();
    assert_eq!(level.render(&n), "|.@R.00|^|");
    // the hit boulder has nowhere to go: two would share a square, never planned
    let (level, s) = parse("|@0R.0|^|").unwrap();
    assert_eq!(level.push(&s, 1, Dir::East), None);
    let (level, s) = parse("|@0R..|^|").unwrap();
    let n = level.push(&s, 1, Dir::East).unwrap();
    assert_eq!(level.render(&n), "|.@R.0|^|");
}

#[test]
fn boulders_cross_stairs_but_not_walls_or_boulders() {
    let (level, s) = parse("|@0>^|").unwrap();
    let n = level.push(&s, 1, Dir::East).unwrap();
    assert_eq!(level.render(&n), "|.@0^|");
    let (level, s) = parse("|@00^|").unwrap();
    assert_eq!(level.push(&s, 1, Dir::East), None);
    let (level, s) = parse("|^@0|").unwrap();
    assert_eq!(level.push(&s, 2, Dir::East), None);
}

#[test]
fn the_path_walks_around_boulders_and_open_holes() {
    let (level, s) = parse("|@0.|\n|.^.|\n|...|").unwrap();
    let dirs = level.path(&s, 2).unwrap();
    let keys: String = dirs.iter().map(|d| d.key()).collect();
    assert_eq!(keys, "jjllkk");
}

#[test]
fn render_round_trips_a_board_with_walls_and_stairs() {
    let text = " ----\n-|.@|--\n|.0..>|\n|.^R..|\n-------";
    let (level, s) = parse(text).unwrap();
    assert_eq!(level.render(&s), text);
}

#[test]
fn parse_reports_boards_it_cannot_solve() {
    assert_eq!(parse("|.0^|").err(), Some(ParseError::NoHero));
    assert_eq!(parse("|@0@^|").err(), Some(ParseError::TwoHeroes));
    assert_eq!(parse("|@0.|").err(), Some(ParseError::NoHoles));
}

#[test]
fn flip_mirrors_rows_and_columns() {
    assert_eq!(flip("|@0\n|^", true, false), "0@|\n ^|");
    assert_eq!(flip("|@0\n|^", false, true), "|^\n|@0");
}

#[test]
fn a_mimic_is_floor_that_renders_back_as_m() {
    let (level, s) = parse("|@M0^|").unwrap();
    assert_eq!(level.mimics, vec![1]);
    assert!(level.walkable(1, &s));
    assert_eq!(level.render(&s), "|@M0^|");
}
