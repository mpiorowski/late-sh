use std::collections::HashMap;

use chrono::{Duration, NaiveDate};

use crate::{
    chess_puzzle::{ChessPuzzle, Theme, choose_for_date},
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

/// Unclaimed puzzles come first, picked by the date and tier; once all are
/// used, the one claimed longest ago comes round.
#[test]
fn the_pick_prefers_unused_puzzles_then_the_one_used_longest_ago() {
    let puzzles: Vec<ChessPuzzle> = ["p0", "p1", "p2"]
        .into_iter()
        .map(|id| ChessPuzzle {
            id: id.to_string(),
            fen: String::new(),
            moves: Vec::new(),
            rating: 1000,
            theme: None,
        })
        .collect();
    let day = NaiveDate::from_ymd_opt(2026, 10, 28).expect("valid date");
    let pick = |last_used: &HashMap<String, NaiveDate>, offset: i64| {
        choose_for_date(
            &puzzles,
            last_used,
            day + Duration::days(offset),
            Difficulty::Easy,
        )
        .id
        .clone()
    };

    let mut last_used = HashMap::new();
    let first = pick(&last_used, 0);
    last_used.insert(first.clone(), day);
    let second = pick(&last_used, 1);
    assert_ne!(first, second);
    last_used.insert(second.clone(), day + Duration::days(1));
    let third = pick(&last_used, 2);
    last_used.insert(third.clone(), day + Duration::days(2));

    let mut all = vec![first.clone(), second, third];
    all.sort();
    assert_eq!(all, ["p0", "p1", "p2"]);
    assert_eq!(pick(&last_used, 3), first);
}
