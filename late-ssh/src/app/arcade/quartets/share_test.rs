use chrono::NaiveDate;

use super::card;
use crate::app::arcade::{
    quartets::state::Split,
    share::{Glyph, Row, ShareCard},
};

/// The result in the title, then one square per try in its split's
/// colour, for a clean solve, a solve with strikes, and a wall out of
/// strikes.
#[test]
fn card_is_the_result_and_a_square_per_try() {
    let date = NaiveDate::from_ymd_opt(2026, 10, 28).unwrap();
    assert_eq!(
        [
            card(date, 4, &[Split::Quartet; 3]),
            card(
                date,
                4,
                &[
                    Split::ThreeOne,
                    Split::Quartet,
                    Split::TwoTwo,
                    Split::Quartet,
                    Split::Quartet,
                ],
            ),
            card(
                date,
                1,
                &[
                    Split::Quartet,
                    Split::TwoOneOne,
                    Split::Apart,
                    Split::ThreeOne,
                ],
            ),
        ],
        [
            ShareCard {
                title: "late.sh Quartets #201 · clean".to_string(),
                rows: vec![Row::Glyphs(vec![Glyph::Green; 3])],
            },
            ShareCard {
                title: "late.sh Quartets #201 · 2 strikes".to_string(),
                rows: vec![Row::Glyphs(vec![
                    Glyph::Yellow,
                    Glyph::Green,
                    Glyph::Blue,
                    Glyph::Green,
                    Glyph::Green,
                ])],
            },
            ShareCard {
                title: "late.sh Quartets #201 · 1/4".to_string(),
                rows: vec![Row::Glyphs(vec![
                    Glyph::Green,
                    Glyph::Orange,
                    Glyph::Red,
                    Glyph::Yellow,
                ])],
            },
        ]
    );
}
