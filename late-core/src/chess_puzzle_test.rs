use chrono::NaiveDate;

use crate::{
    chess_puzzle::{ChessPuzzle, Theme, select_for_date},
    models::chips::Difficulty,
};

/// A puzzle is named by its most telling tag: a mate count over the
/// tactic, and nothing at all when Lichess only tagged its length and
/// phase.
#[test]
fn the_naming_theme_prefers_a_mate_count_and_skips_bare_tags() {
    assert_eq!(
        Theme::from_lichess_tags("fork mateIn2 middlegame short"),
        Some(Theme::MateIn2)
    );
    assert_eq!(
        Theme::from_lichess_tags("crushing hangingPiece long middlegame"),
        Some(Theme::HangingPiece)
    );
    assert_eq!(Theme::from_lichess_tags("advantage endgame short"), None);
    for theme in Theme::ALL {
        assert_eq!(Theme::from_lichess_tags(theme.lichess_tag()), Some(theme));
    }
}

/// The daily pick is a pure function of the date and tier: the same puzzle
/// for every session, a different draw per tier, and pinned so a changed
/// hash cannot silently swap a day's board under players mid-solve.
#[test]
fn the_daily_pick_is_pinned_per_date_and_tier() {
    let puzzles: Vec<ChessPuzzle> = (0..100)
        .map(|index| ChessPuzzle {
            id: format!("p{index}"),
            fen: String::new(),
            moves: Vec::new(),
            rating: 1000,
            theme: None,
        })
        .collect();
    let date = NaiveDate::from_ymd_opt(2026, 10, 28).unwrap();
    let picks = [Difficulty::Easy, Difficulty::Medium, Difficulty::Hard]
        .map(|difficulty| select_for_date(&puzzles, date, difficulty).id.clone());
    let next_day = select_for_date(&puzzles, date.succ_opt().unwrap(), Difficulty::Easy);
    assert_eq!(
        (picks, next_day.id.as_str()),
        (
            ["p24".to_string(), "p9".to_string(), "p3".to_string()],
            "p5"
        )
    );
}
