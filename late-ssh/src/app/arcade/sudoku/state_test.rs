use super::*;
use chrono::NaiveDate;

fn test_state() -> State {
    let db = late_core::db::Db::new(&late_core::db::DbConfig::default()).expect("lazy db");
    State::new(
        Uuid::nil(),
        SudokuService::new(db, tokio::sync::broadcast::channel(4).0),
        Vec::new(),
    )
}

#[test]
fn reset_confirmation_is_per_action_kind() {
    let mut state = test_state();

    // Two presses of the same key confirm and fire.
    assert!(!state.request_reset(ResetKind::Reset));
    assert!(state.request_reset(ResetKind::Reset));
    assert_eq!(state.reset_pending, None);

    // A press for a different kind re-arms for that kind instead of
    // firing the originally-armed action.
    assert!(!state.request_reset(ResetKind::NewBoard));
    assert!(!state.request_reset(ResetKind::Reset));
    assert_eq!(state.reset_pending, Some(ResetKind::Reset));
    assert!(state.request_reset(ResetKind::Reset));
    assert_eq!(state.reset_pending, None);
}

#[test]
fn same_seed_generates_same_board() {
    let a = generate_board_from_seed(42, Difficulty::Medium).to_string();
    let b = generate_board_from_seed(42, Difficulty::Medium).to_string();
    assert_eq!(a, b);
}

#[test]
fn different_seeds_generate_different_boards() {
    let a = generate_board_from_seed(42, Difficulty::Medium).to_string();
    let b = generate_board_from_seed(43, Difficulty::Medium).to_string();
    assert_ne!(a, b);
}

#[test]
fn different_difficulties_generate_different_clue_counts() {
    let easy = generate_board_from_seed(42, Difficulty::Easy).to_string();
    let hard = generate_board_from_seed(42, Difficulty::Hard).to_string();
    let easy_clues = easy.bytes().filter(|&b| b != b'0').count();
    let hard_clues = hard.bytes().filter(|&b| b != b'0').count();
    assert!(easy_clues > hard_clues);
}

#[test]
fn current_daily_game_must_match_today() {
    let today = NaiveDate::from_ymd_opt(2026, 3, 25).expect("date");
    assert!(is_current_daily_game(Some(today), today));
    assert!(!is_current_daily_game(
        NaiveDate::from_ymd_opt(2026, 3, 24),
        today
    ));
}

#[test]
fn puzzle_date_only_exists_for_daily() {
    let today = NaiveDate::from_ymd_opt(2026, 3, 25).expect("date");
    assert_eq!(puzzle_date_for_mode(Mode::Daily, today), Some(today));
    assert_eq!(puzzle_date_for_mode(Mode::Personal, today), None);
}

#[test]
fn snapshot_from_game_restores_grid_mask_and_seed() {
    let mut grid = [[0u8; 9]; 9];
    let mut fixed_mask = [[false; 9]; 9];
    grid[0][0] = 1;
    fixed_mask[0][0] = true;

    let game = Game {
        id: Uuid::nil(),
        created: chrono::Utc::now(),
        updated: chrono::Utc::now(),
        user_id: Uuid::nil(),
        mode: "personal".to_string(),
        difficulty_key: "medium".to_string(),
        puzzle_date: None,
        puzzle_seed: 123,
        grid: serde_json::to_value(grid).expect("grid json"),
        fixed_mask: serde_json::to_value(fixed_mask).expect("mask json"),
        is_game_over: true,
        score: 0,
    };

    let snapshot = snapshot_from_game(&game);

    assert_eq!(snapshot.seed, 123);
    assert_eq!(snapshot.grid[0][0], 1);
    assert!(snapshot.fixed_mask[0][0]);
    assert!(snapshot.is_game_over);
}

#[test]
fn difficulty_key_maps_correctly() {
    assert_eq!(difficulty_from_key("easy"), Difficulty::Easy);
    assert_eq!(difficulty_from_key("medium"), Difficulty::Medium);
    assert_eq!(difficulty_from_key("hard"), Difficulty::Hard);
    assert_eq!(difficulty_from_key("unknown"), Difficulty::Medium);
}
