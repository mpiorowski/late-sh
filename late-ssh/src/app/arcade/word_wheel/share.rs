//! The Word Wheel share card: the rank and the counts, then the score as
//! ten squares (yellow for each tenth of the wheel's top score reached),
//! then one green square per full wheel found. No letters and no words,
//! so it spoils nothing. A card is ready once Bronze is.

use chrono::NaiveDate;

use crate::app::arcade::share::{self, Glyph, Row, ShareCard};

use super::state::{Rank, State};

/// Squares in the score row.
const SCORE_SLOTS: u32 = 10;

/// Whether the day's wheel has reached Bronze, its first paid rank.
pub fn is_ready(state: &State) -> bool {
    state.is_loaded() && state.rank() >= Rank::Bronze
}

pub fn from_state(state: &State) -> Option<ShareCard> {
    if !is_ready(state) {
        return None;
    }
    Some(card(
        state.puzzle_date(),
        state.rank(),
        state.found().len(),
        state.full_wheels_found(),
        state.score(),
        state.max_score(),
    ))
}

pub fn card(
    puzzle_date: NaiveDate,
    rank: Rank,
    words: usize,
    full_wheels: usize,
    score: u32,
    max_score: u32,
) -> ShareCard {
    let number = share::puzzle_number(puzzle_date);
    let filled = (score * SCORE_SLOTS) / max_score.max(1);
    let words = match words {
        1 => "1 word".to_string(),
        words => format!("{words} words"),
    };
    let summary = match full_wheels {
        0 => format!("{} · {words}", rank.label()),
        1 => format!("{} · {words} · 1 full wheel", rank.label()),
        full_wheels => format!("{} · {words} · {full_wheels} full wheels", rank.label()),
    };
    let mut rows = vec![
        Row::Text(summary),
        Row::Glyphs(
            (0..SCORE_SLOTS)
                .map(|slot| {
                    if slot < filled {
                        Glyph::Yellow
                    } else {
                        Glyph::White
                    }
                })
                .collect(),
        ),
    ];
    if full_wheels > 0 {
        rows.push(Row::Glyphs(vec![Glyph::Green; full_wheels]));
    }
    ShareCard {
        title: share::title("Word Wheel", number, None),
        rows,
    }
}

#[cfg(test)]
#[path = "share_test.rs"]
mod share_test;
