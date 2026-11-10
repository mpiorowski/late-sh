use late_core::models::leaderboard::{
    BoardWindows, DailyPuzzle, DoorGame, LeaderboardData, ScoreGame,
};

use crate::app::leaderboard::state::*;

#[test]
fn top_chat_month_change_resets_scroll_and_hides_previous_month() {
    use chrono::{Datelike, Duration, Utc};
    use ratatui::layout::Rect;
    let mut state = LeaderboardPageState::new();
    state.select(1);
    let month = Utc::now().date_naive().with_day(1).unwrap();
    assert!(state.sync_month(month));
    state.set_content_area(Rect::new(25, 1, 70, 30), 100);
    state.scroll_by(20);
    assert!(!state.sync_month(month));
    assert_eq!(state.scroll(), 20);
    let next = (month + Duration::days(32)).with_day(1).unwrap();
    assert!(state.sync_month(next));
    assert_eq!(state.scroll(), 0);
    let mut data = LeaderboardData::default();
    data.top_chat.month_start = (month - Duration::days(1)).with_day(1).unwrap();
    data.top_chat
        .all_rooms
        .push(late_core::models::leaderboard::RankedEntry {
            username: "old".into(),
            user_id: uuid::Uuid::nil(),
            rank: 1,
            value: 99,
            note: None,
        });
    // Read in the month it was taken for, the snapshot shows, each race
    // under its own heading; read a month later, none of it does.
    assert!(matches!(
        Board::TopChat.standings(&data, data.top_chat.month_start),
        Standings::TopChat([("all rooms", all_rooms), ("outside the lounge", outside), ..])
            if all_rooms.len() == 1 && outside.is_empty()
    ));
    assert!(matches!(
        Board::TopChat.standings(&data, month),
        Standings::TopChat(panels) if panels.iter().all(|(_, entries)| entries.is_empty())
    ));
}

#[test]
fn wheel_selection_stops_at_ends_and_only_board_changes_reset_scroll() {
    use ratatui::layout::Rect;
    let mut state = LeaderboardPageState::new();
    state.set_content_area(Rect::new(25, 1, 55, 20), 100);
    state.scroll_by(10);
    state.select(0);
    assert_eq!(state.scroll(), 10);
    state.wheel_select(-1);
    assert_eq!(state.selected_index(), 0);
    assert_eq!(state.scroll(), 10);
    state.wheel_select(1);
    assert_eq!(state.selected_index(), 1);
    assert_eq!(state.scroll(), 0);
    state.select(state.boards().len() - 1);
    state.wheel_select(1);
    assert_eq!(state.selected_index(), state.boards().len() - 1);
}

/// The page list is roster-derived: the five bespoke boards lead (Top
/// Drinkers first), then the game boards (the Lateania boards, then each
/// door's board triple), then every daily puzzle, then every score game. A
/// roster addition in late-core must grow this list without any page change.
#[test]
fn board_list_follows_the_rosters() {
    let boards = Board::all();
    assert_eq!(boards[0], Board::TopDrinkers);
    assert_eq!(boards[1], Board::TopChat);
    assert_eq!(boards[2], Board::TopChips);
    assert_eq!(boards[3], Board::ArcadeWins);
    assert_eq!(boards[4], Board::TimeOnline);
    assert_eq!(boards[5], Board::LateaniaAdventurers);
    assert_eq!(boards[6], Board::LateaniaPvp);
    assert_eq!(boards[7], Board::DoorWins(DoorGame::ALL[0]));
    assert_eq!(boards[8], Board::DoorDepth(DoorGame::ALL[0]));
    assert_eq!(boards[9], Board::DoorScore(DoorGame::ALL[0]));
    let door_boards = 3 * DoorGame::ALL.len();
    assert_eq!(
        boards.len(),
        7 + door_boards + DailyPuzzle::ALL.len() + ScoreGame::ALL.len() + 1
    );
    assert_eq!(boards[7 + door_boards], Board::Daily(DailyPuzzle::ALL[0]));
    assert_eq!(
        boards[7 + door_boards + DailyPuzzle::ALL.len()],
        Board::Score(ScoreGame::ALL[0])
    );
    assert_eq!(
        boards.last(),
        Some(&Board::BadgeGuide),
        "guide trails every board"
    );
}

/// Top Drinkers races by month and by year, never all time.
#[test]
fn top_drinkers_shows_monthly_and_yearly_buzz() {
    let board = Board::TopDrinkers;
    assert_eq!(board.title(), "Top Drinkers");
    assert_eq!(board.format_value(12_400), "12,400 buzz");
    let data = LeaderboardData::default();
    assert!(matches!(
        board.standings(&data, chrono::NaiveDate::default()),
        Standings::MonthlyYearly { .. }
    ));
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
    assert!(matches!(
        board.standings(&data, chrono::NaiveDate::default()),
        Standings::Paired { .. }
    ));
}

#[test]
fn selection_wraps_both_ways() {
    let mut state = LeaderboardPageState::new();
    assert_eq!(state.selected_board(), Board::TopDrinkers);
    state.select_previous();
    assert_eq!(
        state.selected_index(),
        state.boards().len() - 1,
        "previous from the first board wraps to the last"
    );
    state.select_next();
    assert_eq!(state.selected_board(), Board::TopDrinkers);
}
