use chrono::NaiveDate;

use crate::app::arcade::share::{MAX_ROWS, Row, ShareCard};

use super::card;

#[test]
fn card_is_the_picture_in_half_blocks_under_a_sized_header() {
    let picture = vec![
        vec![false, true, true, false],
        vec![true, false, false, true],
        vec![true, true, true, true],
        vec![false, true, true, false],
    ];
    let card = card(
        NaiveDate::from_ymd_opt(2026, 4, 12).unwrap(),
        "easy",
        &picture,
    );
    assert_eq!(
        card,
        ShareCard {
            title: "late.sh Nonograms #2 · easy 4×4".to_string(),
            rows: vec![Row::Text("▄▀▀▄".to_string()), Row::Text("▀██▀".to_string()),],
        }
    );
}

#[test]
fn the_hard_board_fills_the_card_exactly() {
    // 20x20, the largest daily: two picture rows per half-block row.
    let picture: Vec<Vec<bool>> = (0..20)
        .map(|r| (0..20).map(|c| (r + c) % 2 == 0).collect())
        .collect();
    let card = card(
        NaiveDate::from_ymd_opt(2026, 4, 11).unwrap(),
        "hard",
        &picture,
    );
    assert_eq!(card.title, "late.sh Nonograms #1 · hard 20×20");
    assert_eq!(card.rows.len(), MAX_ROWS);
    assert!(card.rows.iter().all(|row| match row {
        Row::Text(text) => text.chars().count() == 20,
        Row::Glyphs(_) => false,
    }));
}
