//! The Quartets share card: the result in the title (the strikes on a
//! solved wall, the quartets found on one out of strikes), then one square
//! per try in the colour of its split: green a quartet, yellow three and
//! one, blue two pairs, orange one pair, red all apart. The last four,
//! which lock in by themselves, are no try. No words, so it spoils nothing.

use chrono::NaiveDate;

use crate::app::arcade::share::{self, Glyph, Row, ShareCard};

use super::state::{Outcome, QUARTETS, Split, State};

/// Whether the day's wall has ended, solved or out of strikes.
pub fn is_ready(state: &State) -> bool {
    state.is_loaded() && state.outcome() != Outcome::Playing
}

pub fn from_state(state: &State) -> Option<ShareCard> {
    if !is_ready(state) {
        return None;
    }
    let splits: Vec<Split> = state
        .tries()
        .iter()
        .map(|tried| Split::of(*tried))
        .collect();
    Some(card(state.puzzle_date(), state.found().len(), &splits))
}

pub fn card(puzzle_date: NaiveDate, found: usize, splits: &[Split]) -> ShareCard {
    let number = share::puzzle_number(puzzle_date);
    let strikes = splits
        .iter()
        .filter(|split| **split != Split::Quartet)
        .count();
    let result = match (found == QUARTETS, strikes) {
        (true, 0) => "clean".to_string(),
        (true, 1) => "1 strike".to_string(),
        (true, strikes) => format!("{strikes} strikes"),
        (false, _) => format!("{found}/{QUARTETS}"),
    };
    ShareCard {
        title: share::title("Quartets", number, Some(&result)),
        rows: vec![Row::Glyphs(
            splits.iter().map(|split| glyph(*split)).collect(),
        )],
    }
}

fn glyph(split: Split) -> Glyph {
    match split {
        Split::Quartet => Glyph::Green,
        Split::ThreeOne => Glyph::Yellow,
        Split::TwoTwo => Glyph::Blue,
        Split::TwoOneOne => Glyph::Orange,
        Split::Apart => Glyph::Red,
    }
}

#[cfg(test)]
#[path = "share_test.rs"]
mod share_test;
