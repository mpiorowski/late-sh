use chrono::NaiveDate;

use super::card;
use crate::app::arcade::{
    share::{Glyph, Row, ShareCard},
    word_wheel::state::Rank,
};

/// The rank and counts, the score in tenths, a green square per full
/// wheel, and no row for full wheels when there are none. No letters, no
/// words.
#[test]
fn card_is_the_rank_the_score_in_tenths_and_the_full_wheels() {
    let date = NaiveDate::from_ymd_opt(2026, 10, 28).unwrap();
    let tenths = |filled: usize| {
        let mut glyphs = vec![Glyph::Yellow; filled];
        glyphs.resize(10, Glyph::White);
        Row::Glyphs(glyphs)
    };
    assert_eq!(
        card(date, Rank::Silver, 31, 1, 52, 110),
        ShareCard {
            title: "late.sh Word Wheel #201".to_string(),
            rows: vec![
                Row::Text("Silver · 31 words · 1 full wheel".to_string()),
                tenths(4),
                Row::Glyphs(vec![Glyph::Green]),
            ],
        }
    );
    assert_eq!(
        card(date, Rank::Bronze, 12, 0, 25, 117),
        ShareCard {
            title: "late.sh Word Wheel #201".to_string(),
            rows: vec![Row::Text("Bronze · 12 words".to_string()), tenths(2),],
        }
    );
}
