use super::*;
use late_core::nonogram::derive_clues;

fn sample_library() -> Library {
    let solution = vec![
        vec![0, 1, 1, 1, 0],
        vec![1, 0, 0, 0, 1],
        vec![1, 0, 1, 0, 1],
        vec![1, 0, 0, 0, 1],
        vec![0, 1, 1, 1, 0],
    ];
    let (row_clues, col_clues) = derive_clues(&solution);
    Library {
        packs: Arc::new(vec![NonogramPack {
            size_key: "5x5".to_string(),
            width: 5,
            height: 5,
            puzzles: vec![NonogramPuzzle {
                id: "5x5-000000".to_string(),
                width: 5,
                height: 5,
                row_clues,
                col_clues,
                solution,
                difficulty: "easy".to_string(),
                source: Some("test".to_string()),
                seed: Some(1),
            }],
        }]),
    }
}

#[test]
fn puzzle_date_only_exists_for_daily() {
    let today = NaiveDate::from_ymd_opt(2026, 3, 29).expect("date");
    assert_eq!(puzzle_date_for_mode(Mode::Daily, today), Some(today));
    assert_eq!(puzzle_date_for_mode(Mode::Personal, today), None);
}

#[test]
fn pack_navigation_is_stable_on_empty_library() {
    let state = Library::default();
    assert!(state.pack(0).is_none());
}

#[test]
fn sample_library_has_deterministic_daily_pick() {
    let library = sample_library();
    let date = NaiveDate::from_ymd_opt(2026, 3, 29).expect("date");
    assert_eq!(
        library
            .pack(0)
            .expect("pack")
            .select_for_date(date)
            .expect("puzzle")
            .id,
        "5x5-000000"
    );
}

#[test]
fn board_matches_clues_treats_marks_as_empty() {
    let puzzle = &sample_library().packs[0].puzzles[0];
    let player_grid = vec![
        vec![2, 1, 1, 1, 2],
        vec![1, 2, 0, 0, 1],
        vec![1, 0, 1, 2, 1],
        vec![1, 2, 0, 0, 1],
        vec![0, 1, 1, 1, 2],
    ];

    assert!(board_matches_clues(puzzle, &player_grid));
}

#[test]
fn board_matches_clues_rejects_wrong_filled_pattern() {
    let puzzle = &sample_library().packs[0].puzzles[0];
    let player_grid = vec![
        vec![1, 1, 1, 0, 0],
        vec![1, 0, 0, 0, 1],
        vec![1, 0, 1, 0, 1],
        vec![1, 0, 0, 0, 1],
        vec![0, 1, 1, 1, 0],
    ];

    assert!(!board_matches_clues(puzzle, &player_grid));
}

#[test]
fn row_col_satisfaction_all_true_on_solution() {
    let puzzle = &sample_library().packs[0].puzzles[0];
    let (rows, cols) = row_col_satisfaction(puzzle, &puzzle.solution);
    assert!(rows.iter().all(|&r| r), "all rows should be satisfied");
    assert!(cols.iter().all(|&c| c), "all cols should be satisfied");
}

#[test]
fn row_col_satisfaction_empty_grid_is_all_false() {
    let puzzle = &sample_library().packs[0].puzzles[0];
    let empty = vec![vec![0u8; puzzle.width as usize]; puzzle.height as usize];
    let (rows, cols) = row_col_satisfaction(puzzle, &empty);
    let any_empty_row_in_solution = puzzle.row_clues.iter().any(|c| c.is_empty());
    let any_empty_col_in_solution = puzzle.col_clues.iter().any(|c| c.is_empty());
    assert_eq!(
        rows.iter().any(|&r| r),
        any_empty_row_in_solution,
        "only empty-clue rows can be satisfied by an empty grid"
    );
    assert_eq!(
        cols.iter().any(|&c| c),
        any_empty_col_in_solution,
        "only empty-clue cols can be satisfied by an empty grid"
    );
}

#[test]
fn row_col_satisfaction_partial_only_matching_lines_true() {
    let puzzle = &sample_library().packs[0].puzzles[0];
    let mut grid = vec![vec![0u8; puzzle.width as usize]; puzzle.height as usize];
    grid[0] = vec![0, 1, 1, 1, 0];

    let (rows, cols) = row_col_satisfaction(puzzle, &grid);
    assert!(rows[0], "row 0 matches its clue");
    assert!(!rows[1..].iter().any(|&r| r), "other rows not satisfied");
    assert_eq!(rows.len(), puzzle.height as usize);
    assert_eq!(cols.len(), puzzle.width as usize);
}

#[test]
fn row_col_satisfaction_treats_marks_as_empty() {
    let puzzle = &sample_library().packs[0].puzzles[0];
    let marked_grid = vec![
        vec![2, 1, 1, 1, 2],
        vec![1, 2, 0, 0, 1],
        vec![1, 0, 1, 2, 1],
        vec![1, 2, 0, 0, 1],
        vec![0, 1, 1, 1, 2],
    ];
    let (rows, cols) = row_col_satisfaction(puzzle, &marked_grid);
    assert!(
        rows.iter().all(|&r| r),
        "marks treated as empty → all rows satisfied"
    );
    assert!(
        cols.iter().all(|&c| c),
        "marks treated as empty → all cols satisfied"
    );
}

#[test]
fn line_status_reports_impossible_when_mark_splits_required_run() {
    let cells = vec![CELL_FILLED, CELL_MARKED_EMPTY, CELL_FILLED, CELL_EMPTY];
    assert_eq!(line_status(false, &cells, &[3]), LineStatus::Impossible);
}

#[test]
fn line_status_stays_pending_when_unknowns_can_complete_run() {
    let cells = vec![CELL_FILLED, CELL_EMPTY, CELL_FILLED, CELL_EMPTY];
    assert_eq!(line_status(false, &cells, &[3]), LineStatus::Pending);
}
