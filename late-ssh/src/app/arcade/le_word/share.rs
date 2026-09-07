//! The Le Word share card: the guess grid, one glyph per letter, which
//! brags about the solve without giving the word away.

use chrono::NaiveDate;
use late_core::models::leaderboard::DailyPuzzle;

use crate::app::arcade::share::{self, Glyph, Row, ShareCard};

use super::state::{LetterScore, MAX_GUESSES, State, WORD_LEN};

/// The card for a finished daily, or `None` while the round is still open.
pub fn from_state(state: &State) -> Option<ShareCard> {
    if !state.is_game_over {
        return None;
    }
    let scores: Vec<[LetterScore; WORD_LEN]> = state
        .guesses
        .iter()
        .map(|guess| state.scores_for_guess(guess))
        .collect();
    Some(card(state.puzzle_date, &scores, state.won))
}

pub fn card(puzzle_date: NaiveDate, scores: &[[LetterScore; WORD_LEN]], won: bool) -> ShareCard {
    let number = share::puzzle_number(share::epoch(DailyPuzzle::LeWord), puzzle_date);
    let result = if won {
        format!("{}/{MAX_GUESSES}", scores.len())
    } else {
        format!("X/{MAX_GUESSES}")
    };
    let rows = scores
        .iter()
        .map(|row| Row::Glyphs(row.iter().map(|score| glyph(*score)).collect()))
        .collect();
    ShareCard {
        title: share::title("Le Word", number, &result),
        rows,
    }
}

fn glyph(score: LetterScore) -> Glyph {
    match score {
        LetterScore::Correct => Glyph::Green,
        LetterScore::Present => Glyph::Yellow,
        LetterScore::Absent => Glyph::Dark,
    }
}

#[cfg(test)]
#[path = "share_test.rs"]
mod share_test;
