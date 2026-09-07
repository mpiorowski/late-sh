use chrono::NaiveDate;
use late_core::models::chips::Difficulty;

use crate::app::arcade::share::{Glyph, Row, ShareCard};

use super::card;

#[test]
fn heatmap_grades_cells_by_thirds_of_the_busiest_cell() {
    // 3x3 board, busiest cell visited 9 times.
    let visits = [0, 1, 3, 4, 6, 7, 9, 2, 5];
    let card = card(
        NaiveDate::from_ymd_opt(2026, 8, 31).unwrap(),
        Difficulty::Easy,
        42,
        &visits,
    );
    assert_eq!(
        card,
        ShareCard {
            title: "late.sh Sliding Puzzle #2 · easy 3×3 · 42 moves".to_string(),
            rows: vec![
                Row::Glyphs(vec![Glyph::White, Glyph::Yellow, Glyph::Yellow]),
                Row::Glyphs(vec![Glyph::Orange, Glyph::Orange, Glyph::Red]),
                Row::Glyphs(vec![Glyph::Red, Glyph::Yellow, Glyph::Orange]),
            ],
        }
    );
}

#[test]
fn a_restored_board_with_no_visits_is_all_white() {
    let card = card(
        NaiveDate::from_ymd_opt(2026, 8, 30).unwrap(),
        Difficulty::Medium,
        80,
        &[0; 16],
    );
    assert_eq!(card.title, "late.sh Sliding Puzzle #1 · medium 4×4 · 80 moves");
    assert_eq!(card.rows.len(), 4);
    assert!(
        card.rows
            .iter()
            .all(|row| *row == Row::Glyphs(vec![Glyph::White; 4]))
    );
}
