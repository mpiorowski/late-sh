use chrono::NaiveDate;

use crate::app::arcade::share::{Glyph, Row, ShareCard};
use crate::app::arcade::sudoku::state::Mask;

use super::card;

#[test]
fn card_is_the_grid_with_clues_white_and_fills_green() {
    let mut given: Mask = [[false; 9]; 9];
    for (r, c) in [(0, 1), (0, 5), (0, 8), (4, 4), (8, 0), (8, 3)] {
        given[r][c] = true;
    }
    let card = card(
        NaiveDate::from_ymd_opt(2026, 4, 13).unwrap(),
        "hard",
        &given,
    );
    let g = Glyph::Green;
    let w = Glyph::White;
    let plain = Row::Glyphs(vec![g; 9]);
    assert_eq!(
        card,
        ShareCard {
            title: "late.sh Sudoku #3 · hard".to_string(),
            rows: vec![
                Row::Glyphs(vec![g, w, g, g, g, w, g, g, w]),
                plain.clone(),
                plain.clone(),
                plain.clone(),
                Row::Glyphs(vec![g, g, g, g, w, g, g, g, g]),
                plain.clone(),
                plain.clone(),
                plain,
                Row::Glyphs(vec![w, g, g, w, g, g, g, g, g]),
            ],
        }
    );
}
