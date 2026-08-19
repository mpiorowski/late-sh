use late_core::models::leaderboard::{
    BoardWindows, DailyPuzzle, DoorGame, LeaderboardData, ScoreGame,
};

use crate::app::leaderboard::state::*;

/// The page list is roster-derived: the three bespoke boards lead, then the
/// game boards (the Lateania boards, then each door's board triple), then
/// every daily puzzle, then every score game. A roster addition in late-core
/// must grow this list without any page change.
#[test]
fn board_list_follows_the_rosters() {
    let boards = Board::all();
    assert_eq!(boards[0], Board::TopChips);
    assert_eq!(boards[1], Board::ArcadeWins);
    assert_eq!(boards[2], Board::TimeOnline);
    assert_eq!(boards[3], Board::LateaniaAdventurers);
    assert_eq!(boards[4], Board::LateaniaFrontier);
    assert_eq!(boards[5], Board::DoorWins(DoorGame::ALL[0]));
    assert_eq!(boards[6], Board::DoorDepth(DoorGame::ALL[0]));
    assert_eq!(boards[7], Board::DoorScore(DoorGame::ALL[0]));
    let door_boards = 3 * DoorGame::ALL.len();
    assert_eq!(
        boards.len(),
        5 + door_boards + DailyPuzzle::ALL.len() + ScoreGame::ALL.len() + 1
    );
    assert_eq!(boards[5 + door_boards], Board::Daily(DailyPuzzle::ALL[0]));
    assert_eq!(
        boards[5 + door_boards + DailyPuzzle::ALL.len()],
        Board::Score(ScoreGame::ALL[0])
    );
    assert_eq!(
        boards.last(),
        Some(&Board::BadgeGuide),
        "guide trails every board"
    );
}

#[test]
fn online_time_uses_compact_two_unit_values() {
    let board = Board::TimeOnline;
    assert_eq!(board.title(), "Late Time");
    assert_eq!(board.format_value(999), "<1s");
    assert_eq!(board.format_value(42_000), "42s");
    assert_eq!(board.format_value((12 * 60 + 34) * 1_000), "12m 34s");
    assert_eq!(
        board.format_value((12 * 60 * 60 + 34 * 60) * 1_000),
        "12h 34m"
    );
    assert_eq!(
        board.format_value((12 * 24 * 60 * 60 + 3 * 60 * 60) * 1_000),
        "12d 3h"
    );

    let data = LeaderboardData {
        online_time: BoardWindows::default(),
        ..LeaderboardData::default()
    };
    assert!(matches!(board.standings(&data), Standings::Paired { .. }));
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
