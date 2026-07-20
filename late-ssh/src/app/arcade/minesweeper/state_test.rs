use super::*;

// ── Generation ──

#[test]
fn same_seed_generates_same_mines() {
    let a = generate_mine_map(42, 9, 9, 10);
    let b = generate_mine_map(42, 9, 9, 10);
    assert_eq!(a, b);
}

#[test]
fn different_seeds_generate_different_mines() {
    let a = generate_mine_map(42, 9, 9, 10);
    let b = generate_mine_map(43, 9, 9, 10);
    assert_ne!(a, b);
}

#[test]
fn mine_count_matches_requested() {
    for diff in &DIFFICULTIES {
        let map = generate_mine_map(99, diff.rows, diff.cols, diff.mines);
        let count: usize = map.iter().flatten().filter(|&&m| m).count();
        assert_eq!(count, diff.mines, "difficulty: {}", diff.key);
    }
}

#[test]
fn zero_mines_produces_empty_map() {
    let map = generate_mine_map(42, 5, 5, 0);
    assert!(map.iter().flatten().all(|&m| !m));
}

#[test]
fn map_dimensions_match_requested() {
    let map = generate_mine_map(42, 13, 16, 30);
    assert_eq!(map.len(), 13);
    assert!(map.iter().all(|row| row.len() == 16));
}

// ── First-click safety ──

#[test]
fn first_click_safety_clears_center_and_neighbors() {
    let mut map = generate_mine_map(42, 9, 9, 10);
    ensure_safe_first_click(&mut map, 4, 4, 42);
    for dr in -1..=1i32 {
        for dc in -1..=1i32 {
            assert!(
                !map[(4i32 + dr) as usize][(4i32 + dc) as usize],
                "cell ({}, {}) should be safe",
                4i32 + dr,
                4i32 + dc
            );
        }
    }
    let count: usize = map.iter().flatten().filter(|&&m| m).count();
    assert_eq!(count, 10);
}

#[test]
fn first_click_safety_at_corner() {
    let mut map = generate_mine_map(42, 9, 9, 10);
    ensure_safe_first_click(&mut map, 0, 0, 42);
    // Corner has only 3 neighbors + itself = 4 safe cells
    for &(r, c) in &[(0, 0), (0, 1), (1, 0), (1, 1)] {
        assert!(!map[r][c], "cell ({r}, {c}) should be safe");
    }
    let count: usize = map.iter().flatten().filter(|&&m| m).count();
    assert_eq!(count, 10);
}

#[test]
fn first_click_safety_noop_when_already_safe() {
    // 5x5 grid, mines only in bottom row
    let mut map = vec![
        vec![false, false, false, false, false],
        vec![false, false, false, false, false],
        vec![false, false, false, false, false],
        vec![false, false, false, false, false],
        vec![true, true, true, false, false],
    ];
    let before = map.clone();
    ensure_safe_first_click(&mut map, 1, 1, 42);
    assert_eq!(map, before, "no mines near click, map should be unchanged");
}

#[test]
fn first_click_safety_preserves_count_with_dense_mines() {
    // 5x5 with 15 mines — very dense, click in center
    let mut map = generate_mine_map(77, 5, 5, 15);
    ensure_safe_first_click(&mut map, 2, 2, 77);
    let count: usize = map.iter().flatten().filter(|&&m| m).count();
    assert_eq!(count, 15, "mine count must be preserved");
    // Center + 8 neighbors all safe
    for dr in -1..=1i32 {
        for dc in -1..=1i32 {
            assert!(!map[(2 + dr) as usize][(2 + dc) as usize]);
        }
    }
}

// ── Adjacent mine count ──

#[test]
fn adjacent_count_correct() {
    let mine_map = vec![
        vec![true, false, false],
        vec![false, false, false],
        vec![false, false, true],
    ];
    assert_eq!(adjacent_mine_count(&mine_map, 1, 1), 2);
    assert_eq!(adjacent_mine_count(&mine_map, 0, 0), 0); // mine itself not counted
    assert_eq!(adjacent_mine_count(&mine_map, 0, 1), 1);
    assert_eq!(adjacent_mine_count(&mine_map, 2, 2), 0);
}

#[test]
fn adjacent_count_corner_cell() {
    // Mine at every position except (0,0)
    let mine_map = vec![
        vec![false, true, true],
        vec![true, true, true],
        vec![true, true, true],
    ];
    // (0,0) has 3 neighbors, all mines
    assert_eq!(adjacent_mine_count(&mine_map, 0, 0), 3);
}

#[test]
fn adjacent_count_surrounded_by_mines() {
    let mine_map = vec![
        vec![true, true, true],
        vec![true, false, true],
        vec![true, true, true],
    ];
    assert_eq!(adjacent_mine_count(&mine_map, 1, 1), 8);
}

#[test]
fn adjacent_count_no_mines() {
    let mine_map = vec![
        vec![false, false, false],
        vec![false, false, false],
        vec![false, false, false],
    ];
    for r in 0..3 {
        for c in 0..3 {
            assert_eq!(adjacent_mine_count(&mine_map, r, c), 0);
        }
    }
}

// ── Flood reveal ──

#[test]
fn flood_reveal_opens_empty_region() {
    let mine_map = vec![
        vec![true, false, false],
        vec![false, false, false],
        vec![false, false, false],
    ];
    let mut player_grid = vec![vec![CELL_HIDDEN; 3]; 3];
    flood_reveal(&mine_map, &mut player_grid, 2, 2);
    assert_eq!(player_grid[2][2], CELL_REVEALED);
    assert_eq!(player_grid[2][1], CELL_REVEALED);
    assert_eq!(player_grid[2][0], CELL_REVEALED);
    assert_eq!(player_grid[1][2], CELL_REVEALED);
    assert_eq!(player_grid[1][1], CELL_REVEALED);
    assert_eq!(player_grid[0][1], CELL_REVEALED);
    // Mine itself stays hidden
    assert_eq!(player_grid[0][0], CELL_HIDDEN);
}

#[test]
fn flood_reveal_stops_at_numbered_cells() {
    // Mines at (0,0) and (0,4) — row 1 has numbers, row 2+ is open
    let mine_map = vec![
        vec![true, false, false, false, true],
        vec![false, false, false, false, false],
        vec![false, false, false, false, false],
    ];
    let mut player_grid = vec![vec![CELL_HIDDEN; 5]; 3];
    flood_reveal(&mine_map, &mut player_grid, 2, 2);
    // Row 2 should all be revealed (0 adjacent mines for center cells)
    for (c, cell) in player_grid[2].iter().enumerate() {
        assert_eq!(*cell, CELL_REVEALED, "row 2, col {c}");
    }
    // Row 1 cells adjacent to mines are numbered → revealed but don't propagate
    assert_eq!(player_grid[1][0], CELL_REVEALED); // adj=1, reached from flood
    assert_eq!(player_grid[1][1], CELL_REVEALED); // adj=1
    assert_eq!(player_grid[1][2], CELL_REVEALED); // adj=0, floods
    assert_eq!(player_grid[1][3], CELL_REVEALED); // adj=1
    assert_eq!(player_grid[1][4], CELL_REVEALED); // adj=1
    // Row 0 numbered cells next to mines — reached via row 1
    assert_eq!(player_grid[0][1], CELL_REVEALED); // adj=1
    assert_eq!(player_grid[0][2], CELL_REVEALED); // adj=0
    assert_eq!(player_grid[0][3], CELL_REVEALED); // adj=1
    // Mines stay hidden
    assert_eq!(player_grid[0][0], CELL_HIDDEN);
    assert_eq!(player_grid[0][4], CELL_HIDDEN);
}

#[test]
fn flood_reveal_skips_flagged_cells() {
    let mine_map = vec![
        vec![false, false, false],
        vec![false, false, false],
        vec![false, false, false],
    ];
    let mut player_grid = vec![vec![CELL_HIDDEN; 3]; 3];
    player_grid[1][1] = CELL_FLAGGED;
    flood_reveal(&mine_map, &mut player_grid, 0, 0);
    // All cells revealed except the flagged one
    for (r, row) in player_grid.iter().enumerate() {
        for (c, cell) in row.iter().enumerate() {
            if r == 1 && c == 1 {
                assert_eq!(*cell, CELL_FLAGGED);
            } else {
                assert_eq!(*cell, CELL_REVEALED, "({r},{c})");
            }
        }
    }
}

#[test]
fn flood_reveal_no_mines_reveals_entire_board() {
    let mine_map = vec![vec![false; 5]; 5];
    let mut player_grid = vec![vec![CELL_HIDDEN; 5]; 5];
    flood_reveal(&mine_map, &mut player_grid, 0, 0);
    assert!(
        player_grid.iter().flatten().all(|&c| c == CELL_REVEALED),
        "entire board should be revealed when there are no mines"
    );
}

#[test]
fn flood_reveal_single_cell_with_adjacent_mine() {
    // Click on a cell with adjacent mines — only that cell revealed
    let mine_map = vec![vec![true, false], vec![false, false]];
    let mut player_grid = vec![vec![CELL_HIDDEN; 2]; 2];
    flood_reveal(&mine_map, &mut player_grid, 0, 1);
    assert_eq!(player_grid[0][1], CELL_REVEALED);
    // Others not flood-revealed since (0,1) has adj=1
    assert_eq!(player_grid[1][0], CELL_HIDDEN);
    assert_eq!(player_grid[1][1], CELL_HIDDEN);
}

// ── Snapshot round-trip ──

#[test]
fn snapshot_from_game_round_trip() {
    let diff = &DIFFICULTIES[0]; // easy 9x9
    let mine_map = generate_mine_map(123, diff.rows, diff.cols, diff.mines);
    let mut player_grid = vec![vec![CELL_HIDDEN; diff.cols]; diff.rows];
    player_grid[0][0] = CELL_REVEALED;
    player_grid[1][1] = CELL_FLAGGED;
    player_grid[2][2] = CELL_MINE_HIT;

    let game = Game {
        id: Uuid::nil(),
        created: chrono::Utc::now(),
        updated: chrono::Utc::now(),
        user_id: Uuid::nil(),
        mode: "daily".to_string(),
        difficulty_key: "easy".to_string(),
        puzzle_date: None,
        puzzle_seed: 123,
        mine_map: serde_json::to_value(&mine_map).unwrap(),
        player_grid: serde_json::to_value(&player_grid).unwrap(),
        lives: 2,
        is_game_over: false,
        score: 2,
    };

    let snapshot = snapshot_from_game(&game, diff);
    assert_eq!(snapshot.seed, 123);
    assert_eq!(snapshot.lives, 2);
    assert!(!snapshot.is_game_over);
    assert_eq!(snapshot.mine_map, mine_map);
    assert_eq!(snapshot.player_grid[0][0], CELL_REVEALED);
    assert_eq!(snapshot.player_grid[1][1], CELL_FLAGGED);
    assert_eq!(snapshot.player_grid[2][2], CELL_MINE_HIT);
    assert_eq!(snapshot.player_grid[3][3], CELL_HIDDEN);
}

// ── Date helpers ──

#[test]
fn puzzle_date_only_exists_for_daily() {
    let today = NaiveDate::from_ymd_opt(2026, 4, 2).expect("date");
    assert_eq!(puzzle_date_for_mode(Mode::Daily, today), Some(today));
    assert_eq!(puzzle_date_for_mode(Mode::Personal, today), None);
}

#[test]
fn current_daily_game_must_match_today() {
    let today = NaiveDate::from_ymd_opt(2026, 4, 2).expect("date");
    assert!(is_current_daily_game(Some(today), today));
    assert!(!is_current_daily_game(
        NaiveDate::from_ymd_opt(2026, 4, 1),
        today
    ));
    assert!(!is_current_daily_game(None, today));
}

#[test]
fn accounted_mines_include_hit_mines() {
    let mut player_grid = vec![vec![CELL_HIDDEN; 13]; 13];
    player_grid[0][0] = CELL_FLAGGED;
    player_grid[0][1] = CELL_FLAGGED;
    player_grid[1][0] = CELL_MINE_HIT;
    player_grid[1][1] = CELL_MINE_HIT;

    assert_eq!(accounted_mine_count(&player_grid, 30), 4);
}
