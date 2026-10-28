use chrono::NaiveDate;
use late_core::models::chips::Difficulty;

use super::card;
use crate::app::arcade::{
    netwalk::state::{daily_seed, generate},
    share::{Glyph, MAX_ROWS, Row, ShareCard},
};

const G: Glyph = Glyph::Green;
const D: Glyph = Glyph::Dark;

/// The card is the day's scramble as it started, green where power
/// already reached, then the move count. Nothing on it shows the wiring.
#[test]
fn card_is_the_starting_power_map_then_the_move_count() {
    let date = NaiveDate::from_ymd_opt(2026, 10, 28).unwrap();
    let scrambled = generate(Difficulty::Easy, daily_seed(date, Difficulty::Easy)).scrambled;
    assert_eq!(
        card(date, Difficulty::Easy, 42, &scrambled),
        ShareCard {
            title: "late.sh Netwalk #201 · easy 5×5".to_string(),
            rows: vec![
                Row::Glyphs(vec![D, D, G, D, D]),
                Row::Glyphs(vec![D, G, G, G, G]),
                Row::Glyphs(vec![D, G, G, G, D]),
                Row::Glyphs(vec![D, G, G, G, D]),
                Row::Glyphs(vec![D, D, D, D, D]),
                Row::Text("⬇️ 42 moves".to_string()),
            ],
        }
    );
}

/// The hard board's nine rows and the count fit the card.
#[test]
fn the_hard_card_fits() {
    let date = NaiveDate::from_ymd_opt(2026, 10, 28).unwrap();
    let scrambled = generate(Difficulty::Hard, daily_seed(date, Difficulty::Hard)).scrambled;
    let card = card(date, Difficulty::Hard, 120, &scrambled);
    assert_eq!(card.title, "late.sh Netwalk #201 · hard 9×9");
    assert_eq!(card.rows.len(), 10);
    assert!(card.rows.len() <= MAX_ROWS);
    for row in &card.rows[..9] {
        assert!(
            matches!(row, Row::Glyphs(glyphs) if glyphs.len() == 9),
            "{row:?}"
        );
    }
}
