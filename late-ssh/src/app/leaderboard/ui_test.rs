use late_core::models::leaderboard::{DailyPuzzle, RankedEntry, ScoreGame};
use ratatui::text::Line;
use uuid::Uuid;

use super::{
    super::state::{Board, LeaderboardPageState},
    entry_line, rail_lines, window_lines,
};

fn text(line: &Line) -> String {
    line.spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect()
}

fn entry(rank: i64, username: &str, user_id: Uuid, value: i64) -> RankedEntry {
    RankedEntry {
        username: username.to_string(),
        user_id,
        rank,
        value,
    }
}

fn viewer() -> Uuid {
    Uuid::from_u128(1)
}

#[test]
fn entry_line_right_aligns_value_and_truncates_long_names() {
    let board = Board::Score(ScoreGame::ALL[0]);
    let width = 30usize;

    let rendered = text(&entry_line(
        &entry(1, "alice", viewer(), 12_345),
        board,
        false,
        width,
    ));
    assert!(rendered.starts_with("  #1"), "{rendered}");
    assert!(rendered.contains("alice"), "{rendered}");
    assert!(
        rendered.ends_with("12,345"),
        "value sits on the right edge: {rendered}"
    );

    let long = "a-very-long-username-that-cannot-fit";
    let rendered = text(&entry_line(
        &entry(1, long, viewer(), 12_345),
        board,
        false,
        width,
    ));
    assert!(!rendered.contains(long), "{rendered}");
    assert!(
        rendered.contains('…'),
        "truncated name gets an ellipsis: {rendered}"
    );
    assert!(rendered.ends_with("12,345"), "{rendered}");

    let labeled = text(&entry_line(
        &entry(4, "bob", viewer(), 7),
        Board::Daily(DailyPuzzle::ALL[0]),
        false,
        width,
    ));
    assert!(labeled.ends_with("7 wins"), "{labeled}");
}

#[test]
fn window_swaps_last_two_rows_for_ellipsis_and_own_row_below_the_fold() {
    let board = Board::Score(ScoreGame::ALL[0]);
    let entries: Vec<RankedEntry> = (1..=10)
        .map(|rank| {
            entry(
                rank,
                &format!("player{rank}"),
                Uuid::from_u128(rank as u128 + 100),
                1_000 - rank,
            )
        })
        .collect();
    let capacity = 5usize;

    // Viewer at rank 9, below the 5 visible leaders: 3 leaders, then … + own.
    let own = entries[8].user_id;
    let lines = window_lines("monthly", &entries, board, own, capacity, 40);
    assert_eq!(lines.len(), capacity + 1, "heading plus capacity rows");
    assert!(text(&lines[1]).contains("player1"), "{}", text(&lines[1]));
    assert!(text(&lines[3]).contains("player3"), "{}", text(&lines[3]));
    assert_eq!(text(&lines[4]).trim(), "…");
    assert!(
        text(&lines[5]).contains("player9"),
        "own row closes the window: {}",
        text(&lines[5])
    );

    // Viewer inside the visible leaders: plain top-N, no tail.
    let own = entries[1].user_id;
    let lines = window_lines("monthly", &entries, board, own, capacity, 40);
    assert_eq!(lines.len(), capacity + 1);
    assert!(text(&lines[5]).contains("player5"), "{}", text(&lines[5]));
    assert!(!lines.iter().any(|line| text(line).trim() == "…"));

    // Viewer not on the board at all: same plain top-N.
    let lines = window_lines("monthly", &entries, board, viewer(), capacity, 40);
    assert_eq!(lines.len(), capacity + 1);
    assert!(!lines.iter().any(|line| text(line).trim() == "…"));

    // Empty board renders the invitation copy.
    let lines = window_lines("monthly", &[], board, viewer(), capacity, 40);
    assert_eq!(lines.len(), 2);
    assert!(
        text(&lines[1]).contains("be the first"),
        "{}",
        text(&lines[1])
    );
}

#[test]
fn rail_groups_boards_under_headers_at_roster_boundaries() {
    let state = LeaderboardPageState::new();
    let (lines, selected_line) = rail_lines(&state);

    // Bespoke boards under "Boards", then one header per roster group, each
    // preceded by a blank separator.
    assert!(text(&lines[0]).contains("Boards"), "{}", text(&lines[0]));
    assert!(
        text(&lines[1]).starts_with(" > "),
        "first board selected: {}",
        text(&lines[1])
    );
    assert_eq!(selected_line, 1);
    assert_eq!(text(&lines[3]), "");
    assert!(
        text(&lines[4]).contains("Daily Wins"),
        "{}",
        text(&lines[4])
    );

    // Header at 4, daily boards at 5..5+N, blank, then the High Scores header.
    let high_scores_header = 5 + DailyPuzzle::ALL.len() + 1;
    assert_eq!(text(&lines[high_scores_header - 1]), "");
    assert!(
        text(&lines[high_scores_header]).contains("High Scores"),
        "{}",
        text(&lines[high_scores_header])
    );

    // Three headers and two separators around the full board list, nothing else.
    assert_eq!(
        lines.len(),
        state.boards().len() + 3 + 2,
        "every board renders exactly once"
    );
}
