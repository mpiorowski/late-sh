use chrono::NaiveDate;

use crate::app::arcade::minesweeper::state::Click;
use crate::app::arcade::share::{Glyph, Row, ShareCard};

use super::card;

#[test]
fn cleared_card_shows_the_last_ten_clicks() {
    let mut clicks = vec![Click::Safe; 12];
    clicks[10] = Click::Flag;
    clicks[11] = Click::Boom;
    let card = card(
        NaiveDate::from_ymd_opt(2026, 4, 11).unwrap(),
        "medium",
        30,
        2,
        &clicks,
    );
    let mut strip = vec![Glyph::Green; 8];
    strip.push(Glyph::Flag);
    strip.push(Glyph::Boom);
    assert_eq!(
        card,
        ShareCard {
            title: "late.sh Minesweeper #1 · medium · cleared · 2/3 lives".to_string(),
            rows: vec![Row::Text("💣 30 mines".to_string()), Row::Glyphs(strip)],
        }
    );
}

#[test]
fn a_lost_field_reads_boom_and_a_restored_board_has_no_strip() {
    let card = card(
        NaiveDate::from_ymd_opt(2026, 4, 12).unwrap(),
        "hard",
        40,
        0,
        &[],
    );
    assert_eq!(
        card.title,
        "late.sh Minesweeper #2 · hard · boom · 0/3 lives"
    );
    assert_eq!(card.rows, vec![Row::Text("💣 40 mines".to_string())]);
}
