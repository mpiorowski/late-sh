use chrono::NaiveDate;

use crate::app::arcade::le_word::state::{LetterScore, score_guess};
use crate::app::arcade::share::{Glyph, Row, ShareCard, ShareFormat, render};

use super::card;

fn day(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).unwrap()
}

#[test]
fn won_card_is_the_guess_grid_with_the_solve_count() {
    let scores = vec![
        score_guess("adieu", "shade"),
        score_guess("shame", "shade"),
        score_guess("shade", "shade"),
    ];
    let card = card(day(2026, 6, 20), &scores, true);
    assert_eq!(
        card,
        ShareCard {
            title: "late.sh Le Word #71 · 3/6".to_string(),
            rows: vec![
                Row::Glyphs(vec![
                    Glyph::Yellow,
                    Glyph::Yellow,
                    Glyph::Dark,
                    Glyph::Yellow,
                    Glyph::Dark,
                ]),
                Row::Glyphs(vec![
                    Glyph::Green,
                    Glyph::Green,
                    Glyph::Green,
                    Glyph::Dark,
                    Glyph::Green,
                ]),
                Row::Glyphs(vec![Glyph::Green; 5]),
            ],
        }
    );
    assert_eq!(
        render(&card, ShareFormat::Emoji),
        "late.sh Le Word #71 · 3/6\n🟨🟨⬛🟨⬛\n🟩🟩🟩⬛🟩\n🟩🟩🟩🟩🟩\nssh late.sh"
    );
}

#[test]
fn lost_card_reads_x_out_of_six() {
    let scores = vec![[LetterScore::Absent; 5]; 6];
    let card = card(day(2026, 6, 18), &scores, false);
    assert_eq!(card.title, "late.sh Le Word #69 · X/6");
    assert_eq!(card.rows.len(), 6);
}
