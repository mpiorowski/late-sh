use chrono::NaiveDate;

use crate::app::arcade::share::{Row, ShareCard};
use crate::app::arcade::solitaire::state::Suit;

use super::card;

#[test]
fn card_draws_one_bar_per_foundation() {
    let piles = [
        (Some(Suit::Spades), 13),
        (Some(Suit::Hearts), 13),
        (Some(Suit::Diamonds), 13),
        (Some(Suit::Clubs), 13),
    ];
    let card = card(
        NaiveDate::from_ymd_opt(2026, 4, 11).unwrap(),
        3,
        520,
        &piles,
    );
    assert_eq!(
        card,
        ShareCard {
            title: "late.sh Solitaire #1 · draw 3 · 520 pts".to_string(),
            rows: vec![
                Row::Text("♠ ▓▓▓▓▓▓▓▓▓▓▓▓▓ 13".to_string()),
                Row::Text("♥ ▓▓▓▓▓▓▓▓▓▓▓▓▓ 13".to_string()),
                Row::Text("♦ ▓▓▓▓▓▓▓▓▓▓▓▓▓ 13".to_string()),
                Row::Text("♣ ▓▓▓▓▓▓▓▓▓▓▓▓▓ 13".to_string()),
            ],
        }
    );
}

#[test]
fn an_empty_foundation_shows_a_dot_and_an_empty_bar() {
    let piles = [(None, 0), (Some(Suit::Hearts), 4)];
    let card = card(NaiveDate::from_ymd_opt(2026, 4, 11).unwrap(), 1, 40, &piles);
    assert_eq!(
        card.rows,
        vec![
            Row::Text("· ░░░░░░░░░░░░░ 0".to_string()),
            Row::Text("♥ ▓▓▓▓░░░░░░░░░ 4".to_string()),
        ]
    );
}
