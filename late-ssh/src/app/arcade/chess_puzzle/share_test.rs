use chrono::NaiveDate;
use late_core::{chess_puzzle::Theme, models::chips::Difficulty};

use super::card;
use crate::app::arcade::share::{Glyph, Row, ShareCard};

/// One square per move the player found, green first try and yellow after
/// a wrong move, then the theme. No square or piece is named.
#[test]
fn card_is_one_square_per_move_found_then_the_theme() {
    let date = NaiveDate::from_ymd_opt(2026, 10, 28).unwrap();
    assert_eq!(
        card(
            date,
            Difficulty::Medium,
            &[true, false, true],
            Some(Theme::Fork)
        ),
        ShareCard {
            title: "late.sh Chess Puzzle #201 · medium".to_string(),
            rows: vec![
                Row::Glyphs(vec![Glyph::Green, Glyph::Yellow, Glyph::Green]),
                Row::Text("Fork".to_string()),
            ],
        }
    );
    assert_eq!(
        card(date, Difficulty::Hard, &[true], None),
        ShareCard {
            title: "late.sh Chess Puzzle #201 · hard".to_string(),
            rows: vec![Row::Glyphs(vec![Glyph::Green])],
        }
    );
}
