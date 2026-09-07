use chrono::NaiveDate;

use crate::app::arcade::rubiks_cube::state::Face;
use crate::app::arcade::share::{Glyph, MAX_ROWS, Row, ShareCard};

use super::{RIBBON_WIDTH, card};

#[test]
fn ribbon_colours_each_turn_by_face_and_wraps_at_twelve() {
    let faces = [
        Face::Up,
        Face::Down,
        Face::Front,
        Face::Back,
        Face::Right,
        Face::Left,
        Face::Up,
        Face::Up,
        Face::Up,
        Face::Up,
        Face::Up,
        Face::Up,
        Face::Front,
    ];
    let card = card(NaiveDate::from_ymd_opt(2026, 6, 18).unwrap(), 13, &faces);
    let mut first = vec![
        Glyph::White,
        Glyph::Yellow,
        Glyph::Green,
        Glyph::Blue,
        Glyph::Red,
        Glyph::Orange,
    ];
    first.extend(vec![Glyph::White; 6]);
    assert_eq!(
        card,
        ShareCard {
            title: "late.sh Rubik's Cube #1 · 13 moves".to_string(),
            rows: vec![Row::Glyphs(first), Row::Glyphs(vec![Glyph::Green])],
        }
    );
}

#[test]
fn a_long_solve_keeps_the_tail_that_fits_the_card() {
    let faces = vec![Face::Right; MAX_ROWS * RIBBON_WIDTH + 5];
    let card = card(NaiveDate::from_ymd_opt(2026, 6, 18).unwrap(), 101, &faces);
    assert_eq!(card.rows.len(), MAX_ROWS);
    assert_eq!(card.title, "late.sh Rubik's Cube #1 · 101 moves");
}
