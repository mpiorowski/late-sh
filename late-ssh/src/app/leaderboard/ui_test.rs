use late_core::models::leaderboard::{DailyPuzzle, DoorGame, RankedEntry, ScoreGame};
use ratatui::layout::Rect;
use ratatui::text::Line;
use uuid::Uuid;

use super::{
    super::state::{Board, LeaderboardPageState},
    entry_line, rail_lines, standings_columns, window_lines, window_natural_width,
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
        note: None,
    }
}

fn viewer() -> Uuid {
    Uuid::from_u128(1)
}

#[test]
fn rendered_rail_rows_are_clickable_after_scrolling_and_resize() {
    use late_core::models::leaderboard::LeaderboardData;
    use ratatui::{Terminal, backend::TestBackend, layout::Position};
    let mut state = LeaderboardPageState::new();
    let data = LeaderboardData::default();
    let mut terminal = Terminal::new(TestBackend::new(90, 16)).unwrap();
    for selected in [0, 10, state.boards().len() - 1] {
        state.select(selected);
        terminal
            .draw(|frame| {
                super::draw(
                    frame,
                    frame.area(),
                    &super::LeaderboardPageView {
                        state: &state,
                        data: &data,
                        user_id: viewer(),
                    },
                )
            })
            .unwrap();
        let buffer = terminal.backend().buffer();
        let mut found_selected = false;
        for y in 0..15 {
            let row: String = (0..24).map(|x| buffer[(x, y)].symbol()).collect();
            let hit = state.board_at(Position::new(4, y));
            if let Some(index) = hit {
                assert!(row.contains(state.boards()[index].title()), "{row:?}");
                found_selected |= index == selected;
            } else {
                assert!(!row.contains(" > "), "selected row must be clickable");
            }
        }
        assert!(found_selected);
        assert_eq!(state.board_at(Position::new(24, 5)), None, "divider");
    }
    let mut tiny = Terminal::new(TestBackend::new(30, 5)).unwrap();
    tiny.draw(|frame| {
        super::draw(
            frame,
            frame.area(),
            &super::LeaderboardPageView {
                state: &state,
                data: &data,
                user_id: viewer(),
            },
        )
    })
    .unwrap();
    assert!(!state.over_rail(Position::new(4, 5)));
    assert_eq!(state.board_at(Position::new(4, 5)), None);
}

#[test]
fn scrolling_reveals_every_rank_and_restores_the_own_rank_summary() {
    let board = Board::TopChips;
    let entries: Vec<_> = (1..=20)
        .map(|rank| {
            entry(
                rank,
                &format!("player{rank}"),
                Uuid::from_u128(rank as u128),
                rank,
            )
        })
        .collect();
    let own = entries[17].user_id;
    let top = super::scrolled_window_lines("monthly", &entries, board, own, 5, 40, 0);
    assert!(text(&top[4]).contains('…'));
    assert!(text(&top[5]).contains("player18"));
    let mut seen = std::collections::HashSet::new();
    for offset in 1..=15 {
        let lines = super::scrolled_window_lines("monthly", &entries, board, own, 5, 40, offset);
        assert!(text(&lines[0]).contains("monthly"));
        for (row, line) in lines[1..].iter().enumerate() {
            let rank = offset as usize + row + 1;
            assert!(text(line).contains(&format!("player{rank} ")));
            seen.insert(rank);
        }
    }
    assert!((2..=20).all(|rank| seen.contains(&rank)));
    let restored = super::scrolled_window_lines("monthly", &entries, board, own, 5, 40, 0);
    assert_eq!(top, restored);
    let short = super::scrolled_window_lines("monthly", &entries[..8], board, own, 5, 40, 15);
    assert!(text(&short[1]).contains("player4 "));
    assert!(text(&short[5]).contains("player8 "));
}

#[test]
fn badge_guide_scrolls_wrapped_text_and_clamps_after_resize() {
    use late_core::models::leaderboard::LeaderboardData;
    use ratatui::{Terminal, backend::TestBackend};
    let mut state = LeaderboardPageState::new();
    state.select(state.boards().len() - 1);
    let data = LeaderboardData::default();
    let mut terminal = Terminal::new(TestBackend::new(70, 15)).unwrap();
    terminal
        .draw(|frame| {
            super::draw(
                frame,
                frame.area(),
                &super::LeaderboardPageView {
                    state: &state,
                    data: &data,
                    user_id: viewer(),
                },
            )
        })
        .unwrap();
    state.scroll_by(i16::MAX);
    let bottom = state.scroll();
    assert!(bottom > 0);
    state.scroll_by(1);
    assert_eq!(state.scroll(), bottom);
    let mut large = Terminal::new(TestBackend::new(150, 100)).unwrap();
    large
        .draw(|frame| {
            super::draw(
                frame,
                frame.area(),
                &super::LeaderboardPageView {
                    state: &state,
                    data: &data,
                    user_id: viewer(),
                },
            )
        })
        .unwrap();
    assert!(state.scroll() < bottom);
}

#[test]
fn paired_columns_share_scroll_and_keep_headings_fixed() {
    use late_core::models::leaderboard::{BoardWindows, LeaderboardData};
    use ratatui::{Terminal, backend::TestBackend};
    let mut state = LeaderboardPageState::new();
    state.select(2); // Late Time has paired windows.
    let entries: Vec<_> = (1..=40)
        .map(|rank| {
            entry(
                rank,
                &format!("player{rank}"),
                Uuid::from_u128(rank as u128),
                rank,
            )
        })
        .collect();
    let data = LeaderboardData {
        online_time: BoardWindows {
            monthly: entries[..8].to_vec(),
            all_time: entries,
        },
        ..LeaderboardData::default()
    };
    let mut terminal = Terminal::new(TestBackend::new(110, 12)).unwrap();
    let draw = |terminal: &mut Terminal<TestBackend>| {
        terminal
            .draw(|frame| {
                super::draw(
                    frame,
                    frame.area(),
                    &super::LeaderboardPageView {
                        state: &state,
                        data: &data,
                        user_id: Uuid::nil(),
                    },
                )
            })
            .unwrap();
    };
    draw(&mut terminal);
    state.scroll_by(2);
    draw(&mut terminal);
    let row = |y| {
        (25..110)
            .map(|x| terminal.backend().buffer()[(x, y)].symbol())
            .collect::<String>()
    };
    assert!(row(4).contains("monthly") && row(4).contains("all-time"));
    assert_eq!(row(5).matches("player3").count(), 2);
    state.scroll_by(i16::MAX);
    draw(&mut terminal);
    let bottom: String = (25..110)
        .map(|x| terminal.backend().buffer()[(x, 10)].symbol())
        .collect();
    assert!(
        bottom.contains("player8") && bottom.contains("player40"),
        "{bottom}"
    );
}

#[test]
fn two_row_viewport_keeps_rank_one_reachable_and_wide_names_fit() {
    let entries: Vec<_> = (1..=10)
        .map(|rank| entry(rank, "界界界界界", Uuid::from_u128(rank as u128), rank))
        .collect();
    let lines = super::scrolled_window_lines(
        "monthly",
        &entries,
        Board::TopChips,
        entries[8].user_id,
        2,
        20,
        0,
    );
    assert_eq!(lines.len(), 3);
    assert!(text(&lines[1]).contains("#1"));
    assert!(text(&lines[2]).contains("#2"));
    assert!(lines[1].width() <= 20);
    assert!(text(&lines[1]).contains('…'));
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
fn window_caps_value_column_at_widest_visible_row() {
    let board = Board::Score(ScoreGame::ALL[0]);
    let entries = vec![
        entry(1, "alice", Uuid::from_u128(10), 12_345),
        entry(2, "longusername", Uuid::from_u128(11), 7),
        entry(
            3,
            "invisible-name-that-must-not-widen-the-column",
            Uuid::from_u128(12),
            999_999,
        ),
    ];

    let lines = window_lines("monthly", &entries, board, viewer(), 2, 100);
    let first = text(&lines[1]);
    let second = text(&lines[2]);

    assert_eq!(first, "  #1  alice    12,345");
    assert_eq!(second, "  #2  longusername  7");
    assert_eq!(first.chars().count(), second.chars().count());
    assert!(first.chars().count() < 100, "{first}");
}

#[test]
fn paired_windows_use_natural_widths_with_a_bounded_gap() {
    let board = Board::Daily(DailyPuzzle::ALL[0]);
    let entries = vec![
        entry(1, "short", Uuid::from_u128(10), 15),
        entry(2, "longest-visible-name", Uuid::from_u128(11), 7),
    ];
    let width = window_natural_width("monthly", &entries, board, viewer(), 10);
    let area = Rect::new(25, 4, 100, 30);

    let [monthly, all_time] = standings_columns(area, width, width);

    assert_eq!(monthly.x, area.x);
    assert_eq!(monthly.width as usize, width);
    assert_eq!(all_time.width as usize, width);
    assert_eq!(all_time.x - monthly.right(), 3);

    let just_fits = Rect::new(25, 4, width as u16 * 2 + 1, 30);
    let [monthly, all_time] = standings_columns(just_fits, width, width);
    assert_eq!(all_time.x - monthly.right(), 1);
}

#[test]
fn paired_windows_keep_one_cell_between_them_when_width_is_constrained() {
    let area = Rect::new(25, 4, 25, 30);
    let [monthly, all_time] = standings_columns(area, 30, 30);

    assert_eq!(monthly.width, 12);
    assert_eq!(all_time.width, 12);
    assert_eq!(all_time.x - monthly.right(), 1);
    assert_eq!(all_time.right(), area.right());
}

#[test]
fn compact_value_column_includes_the_own_row_tail() {
    let board = Board::Score(ScoreGame::ALL[0]);
    let mut entries: Vec<RankedEntry> = (1..=8)
        .map(|rank| {
            entry(
                rank,
                &format!("player{rank}"),
                Uuid::from_u128(rank as u128 + 100),
                1_000 - rank,
            )
        })
        .collect();
    let own = Uuid::from_u128(999);
    entries.push(entry(9, "viewer-with-long-name", own, 7));

    let lines = window_lines("monthly", &entries, board, own, 5, 100);
    let first = text(&lines[1]);
    let own = text(&lines[5]);

    assert_eq!(own, "  #9  viewer-with-long-name  7");
    assert_eq!(first.chars().count(), own.chars().count());
    assert!(own.chars().count() < 100, "{own}");
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
    let (lines, selected_line, _) = rail_lines(&state);

    // The bespoke boards lead under "Boards", every game board follows under
    // "Games", then one header per roster group, each preceded by a blank
    // separator.
    assert!(text(&lines[0]).contains("Boards"), "{}", text(&lines[0]));
    assert!(
        text(&lines[1]).starts_with(" > "),
        "first board selected: {}",
        text(&lines[1])
    );
    assert_eq!(selected_line, 1);
    // The three bespoke boards, then a blank and the "Games" header.
    let games_header = 1 + 3 + 1;
    assert_eq!(text(&lines[games_header - 1]), "");
    assert!(
        text(&lines[games_header]).contains("Games"),
        "{}",
        text(&lines[games_header])
    );

    // The Games group: the two Lateania boards plus each door's board
    // triple, blank, then the Daily Wins group.
    let games_rows = 2 + 3 * DoorGame::ALL.len();
    let daily_header = games_header + games_rows + 1 + 1;
    assert_eq!(text(&lines[daily_header - 1]), "");
    assert!(
        text(&lines[daily_header]).contains("Daily Wins"),
        "{}",
        text(&lines[daily_header])
    );

    // Daily boards after their header, blank, then the High Scores header.
    let high_scores_header = daily_header + 1 + DailyPuzzle::ALL.len() + 1;
    assert_eq!(text(&lines[high_scores_header - 1]), "");
    assert!(
        text(&lines[high_scores_header]).contains("High Scores"),
        "{}",
        text(&lines[high_scores_header])
    );

    // Score boards after their header, blank, then the Reference header
    // (just the trailing Badge Guide entry).
    let reference_header = high_scores_header + 1 + ScoreGame::ALL.len() + 1;
    assert_eq!(text(&lines[reference_header - 1]), "");
    assert!(
        text(&lines[reference_header]).contains("Reference"),
        "{}",
        text(&lines[reference_header])
    );

    // Five headers and four separators around the full board list, nothing else.
    assert_eq!(
        lines.len(),
        state.boards().len() + 5 + 4,
        "every board renders exactly once"
    );
}

#[test]
fn lateania_rows_show_the_class_note_until_width_runs_short() {
    let board = Board::LateaniaAdventurers;
    let mut adventurer = entry(1, "mat", viewer(), 50);
    adventurer.note = Some("Runemaster".to_string());

    let roomy = text(&entry_line(&adventurer, board, false, 40));
    assert!(roomy.contains("mat · Runemaster"), "{roomy}");
    assert!(roomy.ends_with("lvl 50"), "{roomy}");

    // Too narrow for the note: the name keeps the room, the note vanishes.
    let tight = text(&entry_line(&adventurer, board, false, 18));
    assert!(!tight.contains("Runemaster"), "{tight}");
    assert!(tight.contains("mat"), "{tight}");
    assert!(tight.ends_with("lvl 50"), "{tight}");

    // Past the cap the board counts paragon levels on top of it.
    let paragon = text(&entry_line(
        &entry(1, "mat", viewer(), 137),
        board,
        false,
        40,
    ));
    assert!(paragon.ends_with("lvl 100 +37"), "{paragon}");
    let capped = text(&entry_line(
        &entry(1, "mat", viewer(), 100),
        board,
        false,
        40,
    ));
    assert!(capped.ends_with("lvl 100"), "{capped}");

    let kills = text(&entry_line(
        &entry(2, "bob", viewer(), 14),
        Board::LateaniaPvp,
        false,
        30,
    ));
    assert!(kills.ends_with("14 kills"), "{kills}");
}
