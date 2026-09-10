//! The Rubik's Cube share card: the front face of today's scramble, an
//! arrow with the move count, and the same face solved. Before and after,
//! which nobody needs explained. The scramble is the day's puzzle, so it
//! spoils nothing.

use chrono::NaiveDate;

use crate::app::arcade::share::{self, Glyph, Row, ShareCard};

use super::state::{Face, State, Sticker, scrambled_stickers};

/// Whether today's cube has been turned and is solved.
pub fn is_ready(state: &State) -> bool {
    state.has_started() && state.is_solved()
}

/// The card for a solved daily cube, or `None` while it is still scrambled.
pub fn from_state(state: &State) -> Option<ShareCard> {
    if !is_ready(state) {
        return None;
    }
    let front = Face::Front.index();
    Some(card(
        state.puzzle_date(),
        state.user_moves(),
        scrambled_stickers(state.puzzle_date())[front],
        state.stickers()[front][0],
    ))
}

/// `scrambled_front` is the front face as the day started, row-major;
/// `solved_front` is the one colour that face ended up in. A cube solved
/// in any orientation is solved, so the finished face is whichever colour
/// landed in front, not always green.
pub fn card(
    puzzle_date: NaiveDate,
    moves: u32,
    scrambled_front: [Sticker; 9],
    solved_front: Sticker,
) -> ShareCard {
    let number = share::puzzle_number(puzzle_date);
    let mut rows: Vec<Row> = scrambled_front
        .chunks(3)
        .map(|row| Row::Glyphs(row.iter().map(|sticker| glyph(*sticker)).collect()))
        .collect();
    rows.push(share::arrow_row(moves));
    rows.extend((0..3).map(|_| Row::Glyphs(vec![glyph(solved_front); 3])));
    ShareCard {
        title: share::title("Rubik's Cube", number, None),
        rows,
    }
}

fn glyph(sticker: Sticker) -> Glyph {
    match sticker {
        Sticker::White => Glyph::White,
        Sticker::Yellow => Glyph::Yellow,
        Sticker::Orange => Glyph::Orange,
        Sticker::Red => Glyph::Red,
        Sticker::Green => Glyph::Green,
        Sticker::Blue => Glyph::Blue,
    }
}

#[cfg(test)]
#[path = "share_test.rs"]
mod share_test;
