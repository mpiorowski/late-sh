use chrono::NaiveDate;

use crate::app::arcade::rubiks_cube::state::{Face, Sticker, scrambled_stickers};
use crate::app::arcade::share::{Glyph, Row, ShareCard};

use super::card;

#[test]
fn card_is_the_scrambled_face_then_the_solved_face() {
    let scrambled = [
        Sticker::Red,
        Sticker::Blue,
        Sticker::Green,
        Sticker::Yellow,
        Sticker::Green,
        Sticker::White,
        Sticker::Orange,
        Sticker::Green,
        Sticker::Red,
    ];
    let card = card(
        NaiveDate::from_ymd_opt(2026, 6, 18).unwrap(),
        36,
        scrambled,
        Sticker::Green,
    );
    assert_eq!(
        card,
        ShareCard {
            title: "late.sh Rubik's Cube #1".to_string(),
            rows: vec![
                Row::Glyphs(vec![Glyph::Red, Glyph::Blue, Glyph::Green]),
                Row::Glyphs(vec![Glyph::Yellow, Glyph::Green, Glyph::White]),
                Row::Glyphs(vec![Glyph::Orange, Glyph::Green, Glyph::Red]),
                Row::Text("⬇️ 36 moves".to_string()),
                Row::Glyphs(vec![Glyph::Green; 3]),
                Row::Glyphs(vec![Glyph::Green; 3]),
                Row::Glyphs(vec![Glyph::Green; 3]),
            ],
        }
    );
}

#[test]
fn the_daily_scramble_is_stable_and_not_solved() {
    let date = NaiveDate::from_ymd_opt(2026, 9, 7).unwrap();
    let a = scrambled_stickers(date);
    let b = scrambled_stickers(date);
    assert_eq!(a, b);
    let front = a[Face::Front.index()];
    assert!(front.iter().any(|sticker| *sticker != front[0]));
}
