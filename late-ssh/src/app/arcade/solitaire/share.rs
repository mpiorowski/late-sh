//! The Solitaire share card: the four foundation piles as bars out of
//! thirteen. A finished deal shows four full bars; a card is only offered
//! on a win, since Klondike never declares a loss.

use chrono::NaiveDate;
use late_core::models::leaderboard::DailyPuzzle;

use crate::app::arcade::share::{self, Row, ShareCard};

use super::state::{Mode, State, Suit};

/// The card for a won daily deal, or `None` on a personal deal or while
/// the deal is still open.
pub fn from_state(state: &State) -> Option<ShareCard> {
    if state.mode != Mode::Daily || !state.is_game_over {
        return None;
    }
    let piles: Vec<(Option<Suit>, usize)> = state
        .foundations
        .iter()
        .map(|pile| (pile.first().map(|card| card.suit), pile.len()))
        .collect();
    Some(card(
        state.daily_date(),
        state.draw_count(),
        state.score(),
        &piles,
    ))
}

pub fn card(
    puzzle_date: NaiveDate,
    draw_count: usize,
    score: usize,
    piles: &[(Option<Suit>, usize)],
) -> ShareCard {
    let number = share::puzzle_number(share::epoch(DailyPuzzle::Solitaire), puzzle_date);
    let result = format!("draw {draw_count} · {score} pts");
    let rows = piles
        .iter()
        .map(|(suit, height)| {
            let glyph = match suit {
                Some(Suit::Spades) => '♠',
                Some(Suit::Hearts) => '♥',
                Some(Suit::Diamonds) => '♦',
                Some(Suit::Clubs) => '♣',
                None => '·',
            };
            let filled = "▓".repeat(*height);
            let empty = "░".repeat(13usize.saturating_sub(*height));
            Row::Text(format!("{glyph} {filled}{empty} {height}"))
        })
        .collect();
    ShareCard {
        title: share::title("Solitaire", number, &result),
        rows,
    }
}

#[cfg(test)]
#[path = "share_test.rs"]
mod share_test;
