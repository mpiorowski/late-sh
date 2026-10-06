use chrono::NaiveDate;
use late_core::models::chips::Difficulty;

use crate::app::arcade::share::{Glyph, MAX_ROWS, Row, ShareCard};

use super::card;

#[test]
fn card_is_the_scramble_then_the_solved_stripes() {
    // 3x3: tiles 1-3 are red, 4-6 yellow, 7-8 green, the gap dark.
    let scrambled = [4, 1, 7, 0, 5, 2, 8, 3, 6];
    let card = card(
        NaiveDate::from_ymd_opt(2026, 8, 31).unwrap(),
        Difficulty::Easy,
        42,
        &scrambled,
    );
    assert_eq!(
        card,
        ShareCard {
            title: "late.sh Sliding Puzzle #143 · easy 3×3".to_string(),
            rows: vec![
                Row::Glyphs(vec![Glyph::Yellow, Glyph::Red, Glyph::Green]),
                Row::Glyphs(vec![Glyph::Dark, Glyph::Yellow, Glyph::Red]),
                Row::Glyphs(vec![Glyph::Green, Glyph::Red, Glyph::Yellow]),
                Row::Text("⬇️ 42 moves".to_string()),
                Row::Glyphs(vec![Glyph::Red; 3]),
                Row::Glyphs(vec![Glyph::Yellow; 3]),
                Row::Glyphs(vec![Glyph::Green, Glyph::Green, Glyph::Dark]),
            ],
        }
    );
}

#[test]
fn the_hard_board_fills_the_card_exactly() {
    let scrambled: Vec<u8> = (0..25).map(|i| ((i * 7) % 25) as u8).collect();
    let card = card(
        NaiveDate::from_ymd_opt(2026, 8, 30).unwrap(),
        Difficulty::Hard,
        300,
        &scrambled,
    );
    assert_eq!(card.title, "late.sh Sliding Puzzle #142 · hard 5×5");
    assert_eq!(card.rows.len(), MAX_ROWS);
    assert_eq!(
        card.rows[MAX_ROWS - 1],
        Row::Glyphs(vec![
            Glyph::Blue,
            Glyph::Blue,
            Glyph::Blue,
            Glyph::Blue,
            Glyph::Dark
        ])
    );
}
