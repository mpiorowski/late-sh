use late_core::models::leaderboard::{DailyPuzzle, ScoreGame};

use crate::app::leaderboard::state::*;

/// The page list is roster-derived: the two bespoke boards lead, then every
/// daily puzzle, then every score game. A roster addition in late-core must
/// grow this list without any page change.
#[test]
fn board_list_follows_the_rosters() {
    let boards = Board::all();
    assert_eq!(boards[0], Board::TopChips);
    assert_eq!(boards[1], Board::ArcadeWins);
    assert_eq!(
        boards.len(),
        2 + DailyPuzzle::ALL.len() + ScoreGame::ALL.len()
    );
    assert_eq!(boards[2], Board::Daily(DailyPuzzle::ALL[0]));
    assert_eq!(
        boards[2 + DailyPuzzle::ALL.len()],
        Board::Score(ScoreGame::ALL[0])
    );
}

#[test]
fn selection_wraps_both_ways() {
    let mut state = LeaderboardPageState::new();
    assert_eq!(state.selected_board(), Board::TopChips);
    state.select_previous();
    assert_eq!(
        state.selected_index(),
        state.boards().len() - 1,
        "previous from the first board wraps to the last"
    );
    state.select_next();
    assert_eq!(state.selected_board(), Board::TopChips);
}
