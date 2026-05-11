use std::collections::HashMap;

use chrono::NaiveDate;
use rand_core::{OsRng, RngCore};
use uuid::Uuid;

use super::svc::MinesweeperService;
use late_core::models::minesweeper::{Game, GameParams};

const CELL_HIDDEN: u8 = 0;
const CELL_REVEALED: u8 = 1;
const CELL_FLAGGED: u8 = 2;
const CELL_MINE_HIT: u8 = 3;

pub const MAX_LIVES: u8 = 3;

pub const DIFFICULTIES: [DifficultyConfig; 3] = [
    DifficultyConfig {
        key: "easy",
        rows: 9,
        cols: 9,
        mines: 10,
    },
    DifficultyConfig {
        key: "medium",
        rows: 13,
        cols: 13,
        mines: 30,
    },
    DifficultyConfig {
        key: "hard",
        rows: 16,
        cols: 16,
        mines: 40,
    },
];

#[derive(Clone, Copy)]
pub struct DifficultyConfig {
    pub key: &'static str,
    pub rows: usize,
    pub cols: usize,
    pub mines: usize,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Daily,
    Personal,
}

impl Mode {
    fn as_str(&self) -> &'static str {
        match self {
            Mode::Daily => "daily",
            Mode::Personal => "personal",
        }
    }
}

#[derive(Clone, Debug)]
struct BoardSnapshot {
    seed: u64,
    mine_map: Vec<Vec<bool>>,
    player_grid: Vec<Vec<u8>>,
    lives: u8,
    is_game_over: bool,
}

pub struct State {
    pub user_id: Uuid,
    pub mode: Mode,
    pub selected_difficulty: usize,
    pub cursor: (usize, usize),
    seed: u64,
    mine_map: Vec<Vec<bool>>,
    player_grid: Vec<Vec<u8>>,
    pub lives: u8,
    pub is_game_over: bool,
    daily_snapshots: HashMap<String, BoardSnapshot>,
    personal_snapshots: HashMap<String, BoardSnapshot>,
    pub svc: MinesweeperService,
}

impl State {
    pub fn new(user_id: Uuid, svc: MinesweeperService, saved_games: Vec<Game>) -> Self {
        let today = svc.today();
        let mut daily_snapshots = HashMap::new();
        let mut personal_snapshots = HashMap::new();

        for diff in &DIFFICULTIES {
            let daily_snapshot = saved_games
                .iter()
                .find(|g| {
                    g.mode == "daily"
                        && g.difficulty_key == diff.key
                        && is_current_daily_game(g.puzzle_date, today)
                })
                .map(|g| snapshot_from_game(g, diff))
                .unwrap_or_else(|| generate_snapshot(Mode::Daily, diff, &svc));
            daily_snapshots.insert(diff.key.to_string(), daily_snapshot);

            if let Some(snapshot) = saved_games
                .iter()
                .find(|g| g.mode == "personal" && g.difficulty_key == diff.key)
                .map(|g| snapshot_from_game(g, diff))
            {
                personal_snapshots.insert(diff.key.to_string(), snapshot);
            }
        }

        let mut state = Self {
            user_id,
            mode: Mode::Daily,
            selected_difficulty: 1,
            cursor: (0, 0),
            seed: 0,
            mine_map: Vec::new(),
            player_grid: Vec::new(),
            lives: MAX_LIVES,
            is_game_over: false,
            daily_snapshots,
            personal_snapshots,
            svc,
        };
        state.load_mode_snapshot_for_selected_difficulty();
        state
    }

    pub fn difficulty(&self) -> &DifficultyConfig {
        &DIFFICULTIES[self.selected_difficulty]
    }

    pub fn difficulty_key(&self) -> &'static str {
        DIFFICULTIES[self.selected_difficulty].key
    }

    pub fn mine_map(&self) -> &[Vec<bool>] {
        &self.mine_map
    }

    pub fn player_grid(&self) -> &[Vec<u8>] {
        &self.player_grid
    }

    pub fn revealed_count(&self) -> usize {
        self.player_grid
            .iter()
            .flatten()
            .filter(|&&c| c == CELL_REVEALED)
            .count()
    }

    pub fn safe_cell_count(&self) -> usize {
        let diff = self.difficulty();
        diff.rows * diff.cols - diff.mines
    }

    pub fn flag_count(&self) -> usize {
        self.player_grid
            .iter()
            .flatten()
            .filter(|&&c| c == CELL_FLAGGED)
            .count()
    }

    pub fn hit_mine_count(&self) -> usize {
        self.player_grid
            .iter()
            .flatten()
            .filter(|&&c| c == CELL_MINE_HIT)
            .count()
    }

    pub fn accounted_mine_count(&self) -> usize {
        accounted_mine_count(&self.player_grid, self.mine_count())
    }

    pub fn mine_count(&self) -> usize {
        self.difficulty().mines
    }

    fn first_click_done(&self) -> bool {
        self.player_grid
            .iter()
            .flatten()
            .any(|&c| c == CELL_REVEALED || c == CELL_MINE_HIT)
    }

    // --- Mode / difficulty switching ---

    pub fn show_personal(&mut self) {
        self.store_active_snapshot();
        self.mode = Mode::Personal;
        self.load_mode_snapshot_for_selected_difficulty();
    }

    pub fn show_daily(&mut self) {
        self.store_active_snapshot();
        self.mode = Mode::Daily;
        self.load_mode_snapshot_for_selected_difficulty();
    }

    pub fn next_difficulty(&mut self) {
        self.store_active_snapshot();
        self.selected_difficulty = (self.selected_difficulty + 1) % DIFFICULTIES.len();
        self.load_mode_snapshot_for_selected_difficulty();
    }

    pub fn prev_difficulty(&mut self) {
        self.store_active_snapshot();
        self.selected_difficulty =
            (self.selected_difficulty + DIFFICULTIES.len() - 1) % DIFFICULTIES.len();
        self.load_mode_snapshot_for_selected_difficulty();
    }

    pub fn new_personal_board(&mut self) {
        self.store_active_snapshot();
        let dk = self.difficulty_key().to_string();
        let diff = *self.difficulty();
        let snapshot = generate_snapshot(Mode::Personal, &diff, &self.svc);
        self.personal_snapshots.insert(dk, snapshot.clone());
        self.mode = Mode::Personal;
        self.apply_snapshot(snapshot);
        self.save_async();
    }

    // --- Interaction ---

    pub fn move_cursor(&mut self, dr: isize, dc: isize) {
        if self.is_game_over {
            return;
        }
        let diff = self.difficulty();
        let r = (self.cursor.0 as isize + dr).clamp(0, diff.rows as isize - 1) as usize;
        let c = (self.cursor.1 as isize + dc).clamp(0, diff.cols as isize - 1) as usize;
        self.cursor = (r, c);
    }

    pub fn reveal(&mut self) {
        if self.is_game_over {
            return;
        }
        let (row, col) = self.cursor;
        let diff = *self.difficulty();
        if row >= diff.rows || col >= diff.cols {
            return;
        }
        match self.player_grid[row][col] {
            CELL_REVEALED => {
                self.chord_reveal(row, col, &diff);
                self.store_active_snapshot();
                self.save_async();
                return;
            }
            CELL_MINE_HIT | CELL_FLAGGED => return,
            _ => {}
        }

        // First click safety: relocate mines away from clicked cell + neighbors
        if !self.first_click_done() {
            ensure_safe_first_click(&mut self.mine_map, row, col, self.seed);
        }

        if self.mine_map[row][col] {
            // Hit a mine
            self.player_grid[row][col] = CELL_MINE_HIT;
            self.lives = self.lives.saturating_sub(1);
            if self.lives == 0 {
                self.is_game_over = true;
                // Reveal all mines on game over
                for r in 0..diff.rows {
                    for c in 0..diff.cols {
                        if self.mine_map[r][c] && self.player_grid[r][c] == CELL_HIDDEN {
                            self.player_grid[r][c] = CELL_MINE_HIT;
                        }
                    }
                }
            }
        } else {
            flood_reveal(&self.mine_map, &mut self.player_grid, row, col);
            self.check_win();
        }

        self.store_active_snapshot();
        self.save_async();
    }

    fn chord_reveal(&mut self, row: usize, col: usize, diff: &DifficultyConfig) {
        let number = adjacent_mine_count(&self.mine_map, row, col);
        if number == 0 {
            return;
        }

        let mut neighbors = Vec::with_capacity(8);
        for dr in -1..=1i32 {
            for dc in -1..=1i32 {
                if dr == 0 && dc == 0 {
                    continue;
                }
                let r = row as i32 + dr;
                let c = col as i32 + dc;
                if r < 0 || r >= diff.rows as i32 || c < 0 || c >= diff.cols as i32 {
                    continue;
                }
                neighbors.push((r as usize, c as usize));
            }
        }

        for (r, c) in neighbors {
            if self.player_grid[r][c] != CELL_HIDDEN {
                continue;
            }
            if self.mine_map[r][c] {
                self.player_grid[r][c] = CELL_MINE_HIT;
                self.lives = self.lives.saturating_sub(1);
                if self.lives == 0 {
                    self.is_game_over = true;
                    for rr in 0..diff.rows {
                        for cc in 0..diff.cols {
                            if self.mine_map[rr][cc] && self.player_grid[rr][cc] == CELL_HIDDEN {
                                self.player_grid[rr][cc] = CELL_MINE_HIT;
                            }
                        }
                    }
                    return;
                }
            } else {
                flood_reveal(&self.mine_map, &mut self.player_grid, r, c);
            }
        }
        self.check_win();
    }

    pub fn toggle_flag(&mut self) {
        if self.is_game_over {
            return;
        }
        let (row, col) = self.cursor;
        let diff = self.difficulty();
        if row >= diff.rows || col >= diff.cols {
            return;
        }

        self.player_grid[row][col] = match self.player_grid[row][col] {
            CELL_HIDDEN => CELL_FLAGGED,
            CELL_FLAGGED => CELL_HIDDEN,
            other => other,
        };
        self.store_active_snapshot();
        self.save_async();
    }

    fn check_win(&mut self) {
        if self.is_game_over {
            return;
        }
        if self.revealed_count() == self.safe_cell_count() {
            self.is_game_over = true;
            if self.mode == Mode::Daily {
                self.svc.record_win_task(
                    self.user_id,
                    self.difficulty_key().to_string(),
                    self.lives as i32,
                );
            }
        }
    }

    // --- Snapshot management ---

    fn apply_snapshot(&mut self, snapshot: BoardSnapshot) {
        self.seed = snapshot.seed;
        self.mine_map = snapshot.mine_map;
        self.player_grid = snapshot.player_grid;
        self.lives = snapshot.lives;
        self.is_game_over = snapshot.is_game_over;
        self.cursor = (0, 0);
    }

    fn store_active_snapshot(&mut self) {
        let snapshot = BoardSnapshot {
            seed: self.seed,
            mine_map: self.mine_map.clone(),
            player_grid: self.player_grid.clone(),
            lives: self.lives,
            is_game_over: self.is_game_over,
        };
        let dk = self.difficulty_key().to_string();
        match self.mode {
            Mode::Daily => {
                self.daily_snapshots.insert(dk, snapshot);
            }
            Mode::Personal => {
                self.personal_snapshots.insert(dk, snapshot);
            }
        }
    }

    fn load_mode_snapshot_for_selected_difficulty(&mut self) {
        let dk = self.difficulty_key().to_string();
        let diff = *self.difficulty();

        let mut generated = false;
        let snapshot = match self.mode {
            Mode::Daily => self.daily_snapshots.get(&dk).cloned(),
            Mode::Personal => self.personal_snapshots.get(&dk).cloned(),
        }
        .or_else(|| {
            let snapshot = generate_snapshot(self.mode, &diff, &self.svc);
            match self.mode {
                Mode::Daily => {
                    self.daily_snapshots.insert(dk.clone(), snapshot.clone());
                }
                Mode::Personal => {
                    self.personal_snapshots.insert(dk.clone(), snapshot.clone());
                    generated = true;
                }
            }
            Some(snapshot)
        });

        if let Some(snapshot) = snapshot {
            self.apply_snapshot(snapshot);
            if self.mode == Mode::Personal && generated {
                self.save_async();
            }
        }
    }

    fn save_async(&self) {
        self.svc.save_game_task(GameParams {
            user_id: self.user_id,
            mode: self.mode.as_str().to_string(),
            difficulty_key: self.difficulty_key().to_string(),
            puzzle_date: puzzle_date_for_mode(self.mode, self.svc.today()),
            puzzle_seed: self.seed as i64,
            mine_map: serde_json::to_value(&self.mine_map).unwrap_or_default(),
            player_grid: serde_json::to_value(&self.player_grid).unwrap_or_default(),
            lives: self.lives as i32,
            is_game_over: self.is_game_over,
            score: self.lives as i32,
        });
    }
}

// --- Board generation ---

fn generate_snapshot(
    mode: Mode,
    diff: &DifficultyConfig,
    svc: &MinesweeperService,
) -> BoardSnapshot {
    let seed = match mode {
        Mode::Daily => svc.get_daily_seed(diff.key),
        Mode::Personal => OsRng.next_u64(),
    };
    let mine_map = generate_mine_map(seed, diff.rows, diff.cols, diff.mines);
    let player_grid = vec![vec![CELL_HIDDEN; diff.cols]; diff.rows];

    BoardSnapshot {
        seed,
        mine_map,
        player_grid,
        lives: MAX_LIVES,
        is_game_over: false,
    }
}

fn generate_mine_map(seed: u64, rows: usize, cols: usize, mine_count: usize) -> Vec<Vec<bool>> {
    let total = rows * cols;
    let mut positions: Vec<usize> = (0..total).collect();

    // Fisher-Yates shuffle using a simple LCG seeded PRNG
    let mut rng_state = seed;
    for i in (1..total).rev() {
        rng_state = lcg_next(rng_state);
        let j = (rng_state >> 33) as usize % (i + 1);
        positions.swap(i, j);
    }

    let mut map = vec![vec![false; cols]; rows];
    for &pos in &positions[..mine_count] {
        map[pos / cols][pos % cols] = true;
    }
    map
}

/// Ensure the first-clicked cell and its neighbors are mine-free.
fn ensure_safe_first_click(mine_map: &mut [Vec<bool>], row: usize, col: usize, seed: u64) {
    let rows = mine_map.len();
    let cols = mine_map[0].len();

    let mut safe_cells = Vec::with_capacity(9);
    for dr in -1..=1i32 {
        for dc in -1..=1i32 {
            let r = row as i32 + dr;
            let c = col as i32 + dc;
            if r >= 0 && r < rows as i32 && c >= 0 && c < cols as i32 {
                safe_cells.push((r as usize, c as usize));
            }
        }
    }

    let mut rng_state = seed.wrapping_add(0xdeadbeef);
    for &(sr, sc) in &safe_cells {
        if mine_map[sr][sc] {
            mine_map[sr][sc] = false;
            loop {
                rng_state = lcg_next(rng_state);
                let pos = (rng_state >> 33) as usize % (rows * cols);
                let r = pos / cols;
                let c = pos % cols;
                if !mine_map[r][c] && !safe_cells.contains(&(r, c)) {
                    mine_map[r][c] = true;
                    break;
                }
            }
        }
    }
}

pub fn adjacent_mine_count(mine_map: &[Vec<bool>], row: usize, col: usize) -> u8 {
    let rows = mine_map.len();
    let cols = mine_map[0].len();
    let mut count = 0u8;
    for dr in -1..=1i32 {
        for dc in -1..=1i32 {
            if dr == 0 && dc == 0 {
                continue;
            }
            let r = row as i32 + dr;
            let c = col as i32 + dc;
            if r >= 0
                && r < rows as i32
                && c >= 0
                && c < cols as i32
                && mine_map[r as usize][c as usize]
            {
                count += 1;
            }
        }
    }
    count
}

fn flood_reveal(mine_map: &[Vec<bool>], player_grid: &mut [Vec<u8>], row: usize, col: usize) {
    let rows = mine_map.len();
    let cols = mine_map[0].len();
    let mut stack = vec![(row, col)];

    while let Some((r, c)) = stack.pop() {
        if player_grid[r][c] != CELL_HIDDEN {
            continue;
        }
        player_grid[r][c] = CELL_REVEALED;

        if adjacent_mine_count(mine_map, r, c) == 0 {
            for dr in -1..=1i32 {
                for dc in -1..=1i32 {
                    if dr == 0 && dc == 0 {
                        continue;
                    }
                    let nr = r as i32 + dr;
                    let nc = c as i32 + dc;
                    if nr >= 0 && nr < rows as i32 && nc >= 0 && nc < cols as i32 {
                        stack.push((nr as usize, nc as usize));
                    }
                }
            }
        }
    }
}

fn snapshot_from_game(game: &Game, diff: &DifficultyConfig) -> BoardSnapshot {
    let mut mine_map = vec![vec![false; diff.cols]; diff.rows];
    if let Some(arr) = game.mine_map.as_array() {
        for (r, row_val) in arr.iter().enumerate().take(diff.rows) {
            if let Some(row_arr) = row_val.as_array() {
                for (c, cell_val) in row_arr.iter().enumerate().take(diff.cols) {
                    mine_map[r][c] = cell_val.as_bool().unwrap_or(false);
                }
            }
        }
    }

    let mut player_grid = vec![vec![CELL_HIDDEN; diff.cols]; diff.rows];
    if let Some(arr) = game.player_grid.as_array() {
        for (r, row_val) in arr.iter().enumerate().take(diff.rows) {
            if let Some(row_arr) = row_val.as_array() {
                for (c, cell_val) in row_arr.iter().enumerate().take(diff.cols) {
                    player_grid[r][c] = cell_val.as_u64().unwrap_or(0) as u8;
                }
            }
        }
    }

    BoardSnapshot {
        seed: game.puzzle_seed as u64,
        mine_map,
        player_grid,
        lives: game.lives as u8,
        is_game_over: game.is_game_over,
    }
}

/// Simple LCG (Knuth) for deterministic mine placement.
fn lcg_next(state: u64) -> u64 {
    state
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407)
}

fn is_current_daily_game(puzzle_date: Option<NaiveDate>, today: NaiveDate) -> bool {
    puzzle_date == Some(today)
}

fn puzzle_date_for_mode(mode: Mode, today: NaiveDate) -> Option<NaiveDate> {
    match mode {
        Mode::Daily => Some(today),
        Mode::Personal => None,
    }
}

fn accounted_mine_count(player_grid: &[Vec<u8>], mine_count: usize) -> usize {
    player_grid
        .iter()
        .flatten()
        .filter(|&&c| c == CELL_FLAGGED || c == CELL_MINE_HIT)
        .count()
        .min(mine_count)
}

#[cfg(test)]
mod tests {
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
}
