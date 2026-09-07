use chrono::NaiveDate;

use crate::app::arcade::share::{Glyph, Row, ShareCard};
use crate::app::arcade::sudoku::state::box_is_complete;

use super::card;

#[test]
fn boxes_are_coloured_by_the_third_of_the_solve_that_finished_them() {
    // Finished in reading order: boxes 0,1,2 first, 3,4,5 next, 6,7,8 last.
    let ranks = [1, 2, 3, 4, 5, 6, 7, 8, 9];
    let card = card(NaiveDate::from_ymd_opt(2026, 4, 13).unwrap(), "hard", &ranks);
    assert_eq!(
        card,
        ShareCard {
            title: "late.sh Sudoku #3 · hard".to_string(),
            rows: vec![
                Row::Glyphs(vec![Glyph::Green; 3]),
                Row::Glyphs(vec![Glyph::Yellow; 3]),
                Row::Glyphs(vec![Glyph::Red; 3]),
            ],
        }
    );
}

#[test]
fn a_restored_board_with_no_history_is_all_green() {
    let card = card(NaiveDate::from_ymd_opt(2026, 4, 11).unwrap(), "easy", &[0; 9]);
    assert!(
        card.rows
            .iter()
            .all(|row| *row == Row::Glyphs(vec![Glyph::Green; 3]))
    );
}

#[test]
fn partial_history_splits_only_the_recorded_boxes() {
    // Four boxes recorded (ranks 1..=4), the rest restored: thirds of 4 are
    // sized 2, so ranks 1-2 green, 3-4 yellow, unrecorded green.
    let ranks = [1, 0, 2, 0, 3, 0, 4, 0, 0];
    let card = card(NaiveDate::from_ymd_opt(2026, 4, 11).unwrap(), "medium", &ranks);
    assert_eq!(
        card.rows,
        vec![
            Row::Glyphs(vec![Glyph::Green, Glyph::Green, Glyph::Green]),
            Row::Glyphs(vec![Glyph::Green, Glyph::Yellow, Glyph::Green]),
            Row::Glyphs(vec![Glyph::Yellow, Glyph::Green, Glyph::Green]),
        ]
    );
}

#[test]
fn box_is_complete_needs_nine_distinct_digits() {
    let mut grid = [[0u8; 9]; 9];
    assert!(!box_is_complete(&grid, 0));
    let digits = [[1, 2, 3], [4, 5, 6], [7, 8, 9]];
    for r in 0..3 {
        for c in 0..3 {
            grid[3 + r][6 + c] = digits[r][c];
        }
    }
    assert!(box_is_complete(&grid, 5));
    grid[3][6] = 2;
    assert!(!box_is_complete(&grid, 5));
}
