use chrono::Duration;
use late_core::db::{Db, DbConfig};
use tokio::sync::broadcast;

use super::*;
use crate::app::activity::event::ActivityEvent;

/// The light glyph per mask, so boards read as pictures in these tests.
const GLYPHS: [char; 16] = [
    ' ', '╵', '╶', '└', '╷', '│', '┌', '├', '╴', '┘', '─', '┴', '┐', '┤', '┬', '┼',
];

fn picture(tiles: &[u8], dimension: usize) -> Vec<String> {
    tiles
        .chunks(dimension)
        .map(|row| row.iter().map(|mask| GLYPHS[usize::from(*mask)]).collect())
        .collect()
}

fn masks(picture: &[&str]) -> Vec<u8> {
    picture
        .iter()
        .flat_map(|row| row.chars())
        .map(|glyph| {
            GLYPHS
                .iter()
                .position(|candidate| *candidate == glyph)
                .expect("a tile glyph") as u8
        })
        .collect()
}

/// A solved 5x5: the middle row runs wall to wall and every column hangs
/// off it. The source, the centre, is the `┼`.
const COMB: [&str; 5] = ["╷╷╷╷╷", "│││││", "├┼┼┼┤", "│││││", "╵╵╵╵╵"];

fn service() -> NetwalkService {
    let db = Db::new(&DbConfig::default()).expect("inert test pool");
    let (activity, _) = broadcast::channel::<ActivityEvent>(8);
    NetwalkService::new(db, activity)
}

fn date() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 10, 28).expect("valid date")
}

fn saved(
    user_id: Uuid,
    mode: &str,
    difficulty: Difficulty,
    puzzle_date: Option<NaiveDate>,
    seed: u64,
    tiles: &[u8],
    moves: i32,
) -> Game {
    Game {
        id: Uuid::now_v7(),
        created: Utc::now(),
        updated: Utc::now(),
        user_id,
        mode: mode.to_string(),
        difficulty_key: difficulty.key().to_string(),
        puzzle_date,
        puzzle_seed: seed as i64,
        tiles: tiles.iter().copied().map(i32::from).collect(),
        moves,
    }
}

/// Everyone's daily, and every saved board on load, is rebuilt from the
/// seed, so a change to the seed or the generator re-deals today's board
/// under the people playing it. This pins one day's easy board whole.
#[test]
fn the_daily_board_for_a_day_is_pinned() {
    let puzzle = generate(Difficulty::Easy, daily_seed(date(), Difficulty::Easy));
    assert_eq!(
        picture(&puzzle.solution, 5),
        ["╷╶┬─╴", "│┌┴─╴", "├┴┬┬┐", "│╶┤╵╵", "└╴└─╴"]
    );
    assert_eq!(
        picture(&puzzle.scrambled, 5),
        ["╴╴┬│╴", "─┌┴─╴", "├├┬┬└", "─╵┴╴╴", "┐╶┘─╶"]
    );
}

/// At every size and for any seed: the network is a spanning tree (solved,
/// every tile on it, n-1 links), the scramble is unsolved and only turns
/// tiles, and the same seed deals the same board.
#[test]
fn every_board_is_a_spanning_tree_scrambled_by_turns_only() {
    for difficulty in DIFFICULTIES {
        let dimension = board_dimension(difficulty);
        for seed in 0..20u64 {
            let puzzle = generate(difficulty, seed);
            let links: u32 = puzzle.solution.iter().map(|mask| mask.count_ones()).sum();
            assert!(
                is_solved(&puzzle.solution, dimension),
                "{difficulty:?} {seed}"
            );
            assert!(!puzzle.solution.contains(&0), "{difficulty:?} {seed}");
            assert_eq!(links as usize, 2 * (dimension * dimension - 1));
            assert!(
                !is_solved(&puzzle.scrambled, dimension),
                "{difficulty:?} {seed}"
            );
            let scrambled: Vec<i32> = puzzle.scrambled.iter().copied().map(i32::from).collect();
            assert_eq!(
                valid_tiles(&scrambled, &puzzle.solution),
                Some(puzzle.scrambled.clone())
            );
            assert_eq!(generate(difficulty, seed), puzzle);
        }
    }
    assert_ne!(
        daily_seed(date(), Difficulty::Easy),
        daily_seed(date(), Difficulty::Medium)
    );
    assert_ne!(
        daily_seed(date(), Difficulty::Easy),
        daily_seed(date() + Duration::days(1), Difficulty::Easy)
    );
}

/// Power runs from the source only through connections that meet: one
/// tile turned sideways cuts itself and everything past it off.
#[test]
fn power_reaches_only_through_connections_that_meet() {
    let comb = masks(&COMB);
    assert!(is_solved(&comb, 5));

    let cut = masks(&["╷╷╷╷╷", "─││││", "├┼┼┼┤", "│││││", "╵╵╵╵╵"]);
    let lit: Vec<String> = lit_tiles(&cut, 5)
        .chunks(5)
        .map(|row| row.iter().map(|lit| if *lit { '#' } else { '.' }).collect())
        .collect();
    assert_eq!(lit, [".####", ".####", "#####", "#####", "#####"]);
    assert!(!is_solved(&cut, 5));
}

/// The cursor starts on the source and stops at the edge; each turn is a
/// move; the turn that lights the last tile solves the board, reports the
/// win once, and a solved board turns no more.
#[test]
fn the_last_turn_solves_the_board_and_a_solved_board_stays_put() {
    let mut state = State::new_for_date(Uuid::now_v7(), service(), date(), Vec::new());
    state.set_board_for_test(
        Difficulty::Easy,
        masks(&["╴╷╷╷╷", "│││││", "├┼┼┼┤", "│││││", "╵╵╵╵╵"]),
        3,
    );
    assert_eq!(state.cursor(), 12);
    for side in [Side::West, Side::West, Side::North, Side::North] {
        assert!(state.move_cursor(side));
    }
    assert!(!state.move_cursor(Side::North), "the board does not wrap");
    assert!(!state.move_cursor(Side::West), "the board does not wrap");

    assert!(state.rotate_at_cursor(Turn::CounterClockwise));
    assert_eq!(
        (
            picture(state.board(), 5),
            state.moves(),
            state.cursor(),
            state.is_solved(),
            state.win_reported(),
            state.message(),
        ),
        (
            COMB.map(String::from).to_vec(),
            4,
            0,
            true,
            true,
            "Connected in 4 moves."
        )
    );

    assert!(!state.rotate_at_cursor(Turn::Clockwise));
    assert!(!state.click(1));
    assert_eq!((state.moves(), state.cursor()), (4, 1));
}

/// A locked tile refuses to turn, and so does one that looks the same
/// every way round; neither costs a move. Unlocked, the tile turns.
#[test]
fn locked_and_symmetric_tiles_refuse_a_turn_without_costing_a_move() {
    let mut state = State::new_for_date(Uuid::now_v7(), service(), date(), Vec::new());
    state.set_board_for_test(
        Difficulty::Easy,
        masks(&["╴╷╷╷╷", "│││││", "├┼┼┼┤", "│││││", "╵╵╵╵╵"]),
        0,
    );

    assert!(!state.rotate_at_cursor(Turn::Clockwise));
    assert_eq!(state.message(), "That tile looks the same every way round.");

    assert!(state.move_cursor(Side::North));
    state.toggle_lock();
    assert!(state.is_locked(7));
    assert!(!state.rotate_at_cursor(Turn::Clockwise));
    assert_eq!(state.message(), "That tile is locked; f unlocks it.");
    assert_eq!(state.moves(), 0);

    state.toggle_lock();
    assert!(state.rotate_at_cursor(Turn::Clockwise));
    assert_eq!(
        (picture(state.board(), 5), state.moves(), state.message()),
        (
            ["╴╷╷╷╷", "││─││", "├┼┼┼┤", "│││││", "╵╵╵╵╵"]
                .map(String::from)
                .to_vec(),
            1,
            "Move 1."
        )
    );
}

/// A saved board comes back as saved, solved ones banked; a row whose
/// tiles are not its seed's network turned in place is corrupt and the
/// day's fresh scramble takes its slot.
#[test]
fn saved_boards_restore_and_corrupt_rows_regenerate() {
    let user_id = Uuid::now_v7();
    let easy_seed = daily_seed(date(), Difficulty::Easy);
    let easy = generate(Difficulty::Easy, easy_seed);
    let medium_seed = daily_seed(date(), Difficulty::Medium);
    let mut corrupt = generate(Difficulty::Medium, medium_seed).solution;
    // A leaf where the network has a three-way tile: no turn makes it so.
    let three_way = corrupt
        .iter()
        .position(|mask| mask.count_ones() == 3)
        .expect("a three-way tile");
    corrupt[three_way] = Side::North.bit();
    let personal = generate(Difficulty::Hard, 42);
    let mut personal_tiles = personal.scrambled.clone();
    personal_tiles[0] = rotate(personal_tiles[0], Turn::Clockwise);

    let mut state = State::new_for_date(
        user_id,
        service(),
        date(),
        vec![
            saved(
                user_id,
                "daily",
                Difficulty::Easy,
                Some(date()),
                easy_seed,
                &easy.solution,
                9,
            ),
            saved(
                user_id,
                "daily",
                Difficulty::Medium,
                Some(date()),
                medium_seed,
                &corrupt,
                5,
            ),
            saved(
                user_id,
                "personal",
                Difficulty::Hard,
                None,
                42,
                &personal_tiles,
                1,
            ),
        ],
    );
    assert_eq!(
        (
            state.board(),
            state.moves(),
            state.is_solved(),
            state.win_reported()
        ),
        (easy.solution.as_slice(), 9, true, true)
    );
    state.next_difficulty();
    assert_eq!(
        (state.board(), state.moves()),
        (
            generate(Difficulty::Medium, medium_seed)
                .scrambled
                .as_slice(),
            0
        )
    );
    state.next_difficulty();
    state.show_personal();
    assert_eq!(
        (state.board(), state.moves(), state.scramble_seed()),
        (personal_tiles.as_slice(), 1, 42)
    );
}

/// When the UTC day turns under a session, the dailies are dealt afresh
/// for the new day and the personal boards are left alone.
#[test]
fn the_utc_rollover_deals_new_dailies_and_keeps_personal_boards() {
    let user_id = Uuid::now_v7();
    let today = Utc::now().date_naive();
    let yesterday = today - Duration::days(1);
    let personal = generate(Difficulty::Easy, 7);
    let mut state = State::new_for_date(
        user_id,
        service(),
        yesterday,
        vec![saved(
            user_id,
            "personal",
            Difficulty::Easy,
            None,
            7,
            &personal.scrambled,
            2,
        )],
    );
    state.set_board_for_test(
        Difficulty::Easy,
        masks(&["╴╷╷╷╷", "│││││", "├┼┼┼┤", "│││││", "╵╵╵╵╵"]),
        3,
    );

    assert!(state.ensure_current_daily());
    assert!(!state.ensure_current_daily());
    assert_eq!(
        (
            state.puzzle_date(),
            state.board(),
            state.moves(),
            state.message()
        ),
        (
            today,
            generate(Difficulty::Easy, daily_seed(today, Difficulty::Easy))
                .scrambled
                .as_slice(),
            0,
            "Today's Netwalk is ready."
        )
    );
    state.show_personal();
    assert_eq!(
        (state.board(), state.moves()),
        (personal.scrambled.as_slice(), 2)
    );
}

/// `r` twice puts the board back to its scramble with no moves, no locks
/// and the cursor home; on a solved daily it is refused, the win already
/// banked.
#[test]
fn reset_restores_the_scramble_and_is_refused_on_a_solved_daily() {
    let mut state = State::new_for_date(Uuid::now_v7(), service(), date(), Vec::new());
    let scramble = state.board().to_vec();
    state.toggle_lock();
    assert!(state.move_cursor(Side::East));
    assert!(state.rotate_at_cursor(Turn::Clockwise));
    assert_eq!(state.moves(), 1);
    assert!(!state.request_reset());
    assert!(state.request_reset());
    state.reset();
    assert_eq!(
        (
            state.board(),
            state.moves(),
            state.is_locked(12),
            state.cursor(),
            state.message()
        ),
        (
            scramble.as_slice(),
            0,
            false,
            12,
            "Today's scramble restored."
        )
    );

    state.set_board_for_test(Difficulty::Easy, masks(&COMB), 6);
    assert!(!state.request_reset());
    assert!(!state.request_reset());
    assert_eq!(picture(state.board(), 5), COMB.map(String::from).to_vec());
    assert_eq!(
        state.message(),
        "Today's board is solved and banked; try another difficulty or p for a personal board."
    );
}
