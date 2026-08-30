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

fn saved_game(grid: Grid, fixed_mask: Mask, notes: serde_json::Value) -> Game {
    Game {
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
        notes,
        is_game_over: false,
        score: 0,
    }
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
        notes: serde_json::to_value([[0u16; 9]; 9]).expect("notes json"),
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
fn snapshot_from_game_restores_pencil_notes() {
    let mut notes: Notes = [[0; 9]; 9];
    notes[0][0] = 0b0_0000_0001; // candidate 1
    notes[4][7] = 0b1_0000_0000; // candidate 9
    notes[8][8] = 0b0_0101_0101; // candidates 1, 3, 5, 7

    let snapshot = snapshot_from_game(&saved_game(
        [[0; 9]; 9],
        [[false; 9]; 9],
        serde_json::to_value(notes).expect("notes json"),
    ));

    assert_eq!(snapshot.notes, notes);
}

#[test]
fn snapshot_from_game_masks_note_bits_and_clears_settled_cells() {
    let mut grid: Grid = [[0; 9]; 9];
    let mut fixed_mask: Mask = [[false; 9]; 9];
    grid[0][0] = 5;
    fixed_mask[0][0] = true; // a given clue
    grid[1][1] = 7; // a value the player placed

    let mut notes: Notes = [[0; 9]; 9];
    notes[0][0] = 0x01ff; // marks stored against a given clue
    notes[1][1] = 0x01ff; // marks stored against a filled cell
    notes[2][2] = 0xffff; // bits above candidate 9

    let snapshot = snapshot_from_game(&saved_game(
        grid,
        fixed_mask,
        serde_json::to_value(notes).expect("notes json"),
    ));

    assert_eq!(snapshot.notes[0][0], 0);
    assert_eq!(snapshot.notes[1][1], 0);
    assert_eq!(snapshot.notes[2][2], 0x01ff);
}

#[test]
fn snapshot_from_game_rejects_malformed_pencil_notes() {
    let valid = serde_json::to_value([[1u16; 9]; 9]).expect("notes json");
    let ragged_row = {
        let mut rows = vec![vec![0u16; 9]; 9];
        rows[4].pop();
        serde_json::json!(rows)
    };
    let mut string_cell = valid.clone();
    string_cell[0][0] = serde_json::json!("3");
    let mut negative_cell = valid.clone();
    negative_cell[0][0] = serde_json::json!(-1);

    let mut grid: Grid = [[0; 9]; 9];
    grid[3][3] = 6;

    for (label, malformed) in [
        ("null", serde_json::Value::Null),
        ("object", serde_json::json!({ "0": [0] })),
        ("eight rows", serde_json::json!(vec![vec![0u16; 9]; 8])),
        ("ragged row", ragged_row),
        ("string cell", string_cell),
        ("negative cell", negative_cell),
    ] {
        let snapshot = snapshot_from_game(&saved_game(grid, [[false; 9]; 9], malformed));

        assert_eq!(
            snapshot.notes, [[0; 9]; 9],
            "{label} notes should restore empty"
        );
        // Malformed marks degrade to none; the board itself still restores.
        assert_eq!(snapshot.grid[3][3], 6, "{label} should not lose the board");
    }
}

#[test]
fn save_params_carry_pencil_notes() {
    let mut state = test_state();
    state.notes[2][3] = 0b0_0000_0011;

    let params = state.save_params();

    assert_eq!(
        params.notes,
        serde_json::to_value(state.notes).expect("notes json")
    );
}

#[test]
fn difficulty_key_maps_correctly() {
    assert_eq!(difficulty_from_key("easy"), Difficulty::Easy);
    assert_eq!(difficulty_from_key("medium"), Difficulty::Medium);
    assert_eq!(difficulty_from_key("hard"), Difficulty::Hard);
    assert_eq!(difficulty_from_key("unknown"), Difficulty::Medium);
}
