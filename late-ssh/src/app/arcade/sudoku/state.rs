use std::{
    collections::HashMap,
    sync::mpsc::{self, Receiver, Sender},
};

use chrono::NaiveDate;
use rand_core::{OsRng, RngCore};
use rumenx_sudoku::{Board, Difficulty, set_rand_seed};
use uuid::Uuid;

use super::svc::SudokuService;
use late_core::models::sudoku::{Game, GameParams};

pub type Grid = [[u8; 9]; 9];
pub type Mask = [[bool; 9]; 9];

pub const DIFFICULTIES: [&str; 3] = ["easy", "medium", "hard"];

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

fn difficulty_from_key(key: &str) -> Difficulty {
    match key {
        "easy" => Difficulty::Easy,
        "hard" => Difficulty::Hard,
        _ => Difficulty::Medium,
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct BoardSnapshot {
    seed: u64,
    grid: Grid,
    fixed_mask: Mask,
    is_game_over: bool,
}

struct DailyGenerationResult {
    difficulty_key: String,
    snapshot: BoardSnapshot,
}

pub struct State {
    pub user_id: Uuid,
    pub mode: Mode,
    pub selected_difficulty: usize,
    pub seed: u64,
    pub grid: Grid,
    pub fixed_mask: Mask,
    pub cursor: (usize, usize),
    pub is_game_over: bool,
    pub reset_pending: bool,
    daily_snapshots: HashMap<String, BoardSnapshot>,
    personal_snapshots: HashMap<String, BoardSnapshot>,
    daily_generation_rx: Option<Receiver<DailyGenerationResult>>,
    pub svc: SudokuService,
}

impl State {
    pub fn new(user_id: Uuid, svc: SudokuService, saved_games: Vec<Game>) -> Self {
        let today = svc.today();
        let mut daily_snapshots = HashMap::new();
        let mut personal_snapshots = HashMap::new();
        let (daily_generation_tx, daily_generation_rx) = mpsc::channel();
        let mut pending_daily_generations = 0usize;

        for &dk in &DIFFICULTIES {
            if let Some(snapshot) = saved_games
                .iter()
                .find(|game| {
                    game.mode == "daily"
                        && game.difficulty_key == dk
                        && is_current_daily_game(game.puzzle_date, today)
                })
                .map(snapshot_from_game)
            {
                daily_snapshots.insert(dk.to_string(), snapshot);
            } else {
                pending_daily_generations += 1;
                spawn_daily_generation(dk.to_string(), svc.clone(), daily_generation_tx.clone());
            }

            if let Some(snapshot) = saved_games
                .iter()
                .find(|game| game.mode == "personal" && game.difficulty_key == dk)
                .map(snapshot_from_game)
            {
                personal_snapshots.insert(dk.to_string(), snapshot);
            }
        }

        let mut state = Self {
            user_id,
            mode: Mode::Daily,
            selected_difficulty: 1, // default to medium
            seed: 0,
            grid: [[0; 9]; 9],
            fixed_mask: [[false; 9]; 9],
            cursor: (0, 0),
            is_game_over: false,
            reset_pending: false,
            daily_snapshots,
            personal_snapshots,
            daily_generation_rx: (pending_daily_generations > 0).then_some(daily_generation_rx),
            svc,
        };
        state.load_mode_snapshot_for_selected_difficulty();
        state
    }

    pub fn ensure_loaded(&mut self) {
        self.load_mode_snapshot_for_selected_difficulty();
    }

    pub fn poll_daily_generation(&mut self) {
        let Some(rx) = self.daily_generation_rx.take() else {
            return;
        };

        let mut disconnected = false;
        loop {
            match rx.try_recv() {
                Ok(result) => {
                    let should_apply = self.mode == Mode::Daily
                        && self.difficulty_key() == result.difficulty_key
                        && !self.daily_snapshots.contains_key(&result.difficulty_key);
                    self.daily_snapshots
                        .insert(result.difficulty_key, result.snapshot);
                    if should_apply {
                        self.apply_snapshot(result.snapshot);
                    }
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    disconnected = true;
                    break;
                }
            }
        }

        if !disconnected {
            self.daily_generation_rx = Some(rx);
        } else {
            self.install_daily_fallbacks_for_missing();
        }
    }

    pub fn is_loading(&self) -> bool {
        self.mode == Mode::Daily && !self.daily_snapshots.contains_key(self.difficulty_key())
    }

    pub fn difficulty_key(&self) -> &'static str {
        DIFFICULTIES[self.selected_difficulty]
    }

    pub fn show_personal(&mut self) {
        self.clear_reset_pending();
        self.store_active_snapshot();
        self.mode = Mode::Personal;
        self.load_mode_snapshot_for_selected_difficulty();
    }

    pub fn show_daily(&mut self) {
        self.clear_reset_pending();
        self.store_active_snapshot();
        self.mode = Mode::Daily;
        self.load_mode_snapshot_for_selected_difficulty();
    }

    pub fn next_difficulty(&mut self) {
        self.clear_reset_pending();
        self.store_active_snapshot();
        self.selected_difficulty = (self.selected_difficulty + 1) % DIFFICULTIES.len();
        self.load_mode_snapshot_for_selected_difficulty();
    }

    pub fn prev_difficulty(&mut self) {
        self.clear_reset_pending();
        self.store_active_snapshot();
        self.selected_difficulty =
            (self.selected_difficulty + DIFFICULTIES.len() - 1) % DIFFICULTIES.len();
        self.load_mode_snapshot_for_selected_difficulty();
    }

    pub fn new_personal_board(&mut self) {
        self.clear_reset_pending();
        self.store_active_snapshot();
        let dk = self.difficulty_key().to_string();
        let snapshot = generate_snapshot(Mode::Personal, &dk, &self.svc);
        self.personal_snapshots.insert(dk, snapshot);
        self.mode = Mode::Personal;
        self.apply_snapshot(snapshot);
        self.save_async();
    }

    fn save_async(&self) {
        self.svc.save_game_task(GameParams {
            user_id: self.user_id,
            mode: self.mode.as_str().to_string(),
            difficulty_key: self.difficulty_key().to_string(),
            puzzle_date: puzzle_date_for_mode(self.mode, self.svc.today()),
            puzzle_seed: self.seed as i64,
            grid: serde_json::to_value(self.grid).unwrap_or_default(),
            fixed_mask: serde_json::to_value(self.fixed_mask).unwrap_or_default(),
            is_game_over: self.is_game_over,
            score: 0,
        });
    }

    // --- Interaction ---

    pub fn reset_board(&mut self) {
        if self.is_game_over || self.is_loading() {
            return;
        }
        self.clear_reset_pending();
        for r in 0..9 {
            for c in 0..9 {
                if !self.fixed_mask[r][c] {
                    self.grid[r][c] = 0;
                }
            }
        }
        self.cursor = (0, 0);
        self.store_active_snapshot();
        self.save_async();
    }

    pub fn move_cursor(&mut self, dr: isize, dc: isize) {
        if self.is_game_over || self.is_loading() {
            return;
        }
        self.clear_reset_pending();
        let r = (self.cursor.0 as isize + dr).clamp(0, 8) as usize;
        let c = (self.cursor.1 as isize + dc).clamp(0, 8) as usize;
        self.cursor = (r, c);
    }

    pub fn set_digit(&mut self, val: u8) {
        if self.is_game_over || self.is_loading() {
            return;
        }
        self.clear_reset_pending();
        let (r, c) = self.cursor;
        if self.fixed_mask[r][c] {
            return;
        }

        self.grid[r][c] = val;

        if val != 0 {
            self.check_win();
        }
        self.store_active_snapshot();
        self.save_async();
    }

    pub fn request_reset(&mut self) -> bool {
        if self.reset_pending {
            self.reset_pending = false;
            return true;
        }
        self.reset_pending = true;
        false
    }

    pub fn clear_reset_pending(&mut self) {
        self.reset_pending = false;
    }

    fn check_win(&mut self) {
        let mut s = String::with_capacity(81);
        for r in 0..9 {
            for c in 0..9 {
                let val = self.grid[r][c];
                if val == 0 {
                    return;
                }
                s.push((val + b'0') as char);
            }
        }

        if let Ok(board) = s.parse::<Board>()
            && board.solve().is_some()
        {
            self.is_game_over = true;
            self.store_active_snapshot();
            if self.mode == Mode::Daily {
                self.svc
                    .record_win_task(self.user_id, self.difficulty_key().to_string(), 1);
            }
        }
    }

    fn apply_snapshot(&mut self, snapshot: BoardSnapshot) {
        self.seed = snapshot.seed;
        self.grid = snapshot.grid;
        self.fixed_mask = snapshot.fixed_mask;
        self.is_game_over = snapshot.is_game_over;
        self.cursor = (0, 0);
    }

    fn clear_board(&mut self) {
        self.seed = 0;
        self.grid = [[0; 9]; 9];
        self.fixed_mask = [[false; 9]; 9];
        self.is_game_over = false;
        self.cursor = (0, 0);
    }

    fn store_active_snapshot(&mut self) {
        if self.is_loading() {
            return;
        }

        let snapshot = BoardSnapshot {
            seed: self.seed,
            grid: self.grid,
            fixed_mask: self.fixed_mask,
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

    fn install_daily_fallbacks_for_missing(&mut self) {
        let active_key = self.difficulty_key().to_string();
        let mut active_snapshot = None;

        for &dk in &DIFFICULTIES {
            if self.daily_snapshots.contains_key(dk) {
                continue;
            }

            tracing::warn!(
                difficulty_key = dk,
                "sudoku daily generation worker ended without a board; using fallback puzzle"
            );
            let snapshot = fallback_daily_snapshot(dk, &self.svc);
            self.daily_snapshots.insert(dk.to_string(), snapshot);
            if self.mode == Mode::Daily && dk == active_key {
                active_snapshot = Some(snapshot);
            }
        }

        if let Some(snapshot) = active_snapshot {
            self.apply_snapshot(snapshot);
        }
    }

    fn load_mode_snapshot_for_selected_difficulty(&mut self) {
        let dk = self.difficulty_key().to_string();

        let mut generated = false;
        let snapshot = match self.mode {
            Mode::Daily => self.daily_snapshots.get(&dk).copied(),
            Mode::Personal => self.personal_snapshots.get(&dk).copied().or_else(|| {
                let snapshot = generate_snapshot(self.mode, &dk, &self.svc);
                self.personal_snapshots.insert(dk.clone(), snapshot);
                generated = true;
                Some(snapshot)
            }),
        };

        if let Some(snapshot) = snapshot {
            self.apply_snapshot(snapshot);
            if self.mode == Mode::Personal && generated {
                self.save_async();
            }
        } else if self.mode == Mode::Daily {
            self.clear_board();
        }
    }
}

fn spawn_daily_generation(
    difficulty_key: String,
    svc: SudokuService,
    tx: Sender<DailyGenerationResult>,
) {
    let job = move || generate_and_send_daily_snapshot(difficulty_key, svc, tx);
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        drop(handle.spawn_blocking(job));
    } else {
        let _ = std::thread::Builder::new()
            .name("sudoku-daily-generation".to_string())
            .spawn(job);
    }
}

fn generate_and_send_daily_snapshot(
    difficulty_key: String,
    svc: SudokuService,
    tx: Sender<DailyGenerationResult>,
) {
    let snapshot = generate_snapshot(Mode::Daily, &difficulty_key, &svc);
    let _ = tx.send(DailyGenerationResult {
        difficulty_key,
        snapshot,
    });
}

fn generate_snapshot(mode: Mode, difficulty_key: &str, svc: &SudokuService) -> BoardSnapshot {
    let seed = match mode {
        Mode::Daily => svc.get_daily_seed(difficulty_key),
        Mode::Personal => OsRng.next_u64(),
    };
    let difficulty = difficulty_from_key(difficulty_key);
    let board = generate_board_from_seed(seed, difficulty);
    let mut grid = [[0; 9]; 9];
    let mut fixed_mask = [[false; 9]; 9];

    apply_board_to_grid(&board, &mut grid, &mut fixed_mask);

    BoardSnapshot {
        seed,
        grid,
        fixed_mask,
        is_game_over: false,
    }
}

fn generate_board_from_seed(seed: u64, difficulty: Difficulty) -> Board {
    set_rand_seed(seed);

    Board::generate(difficulty, 100)
        .or_else(|_| Board::generate(Difficulty::Easy, 100))
        .expect("sudoku board generation should succeed")
}

fn fallback_daily_snapshot(difficulty_key: &str, svc: &SudokuService) -> BoardSnapshot {
    let seed = svc.get_daily_seed(difficulty_key);
    snapshot_from_puzzle(seed, fallback_puzzle_for_difficulty(difficulty_key))
}

fn fallback_puzzle_for_difficulty(difficulty_key: &str) -> &'static str {
    match difficulty_key {
        "easy" => {
            "530070000600195000098000060800060003400803001700020006060000280000419005000080079"
        }
        "hard" => {
            "000000907000420180000705026100904000050000040000507009920108000034059000507000000"
        }
        _ => "000260701680070090190004500820100040004602900050003028009300074040050036703018000",
    }
}

fn snapshot_from_puzzle(seed: u64, puzzle: &str) -> BoardSnapshot {
    let mut grid = [[0; 9]; 9];
    let mut fixed_mask = [[false; 9]; 9];

    for (idx, byte) in puzzle.as_bytes().iter().copied().enumerate().take(81) {
        let row = idx / 9;
        let col = idx % 9;
        let value = byte.saturating_sub(b'0').min(9);
        grid[row][col] = value;
        fixed_mask[row][col] = value != 0;
    }

    BoardSnapshot {
        seed,
        grid,
        fixed_mask,
        is_game_over: false,
    }
}

fn apply_board_to_grid(board: &Board, grid: &mut Grid, fixed_mask: &mut Mask) {
    *grid = grid_from_board(board);

    for r in 0..9 {
        for c in 0..9 {
            fixed_mask[r][c] = grid[r][c] != 0;
        }
    }
}

fn grid_from_board(board: &Board) -> Grid {
    let board_str = board.to_string();
    let bytes = board_str.as_bytes();
    let mut grid = [[0; 9]; 9];

    for (idx, byte) in bytes.iter().copied().enumerate().take(81) {
        let row = idx / 9;
        let col = idx % 9;
        grid[row][col] = byte.saturating_sub(b'0');
    }

    grid
}

fn snapshot_from_game(game: &Game) -> BoardSnapshot {
    let mut grid = [[0; 9]; 9];
    let mut fixed_mask = [[false; 9]; 9];

    if let Some(arr) = game.grid.as_array() {
        for (r, row_val) in arr.iter().enumerate().take(9) {
            if let Some(row_arr) = row_val.as_array() {
                for (c, cell_val) in row_arr.iter().enumerate().take(9) {
                    grid[r][c] = cell_val.as_u64().unwrap_or(0) as u8;
                }
            }
        }
    }

    if let Some(arr) = game.fixed_mask.as_array() {
        for (r, row_val) in arr.iter().enumerate().take(9) {
            if let Some(row_arr) = row_val.as_array() {
                for (c, cell_val) in row_arr.iter().enumerate().take(9) {
                    fixed_mask[r][c] = cell_val.as_bool().unwrap_or(false);
                }
            }
        }
    }

    BoardSnapshot {
        seed: game.puzzle_seed as u64,
        grid,
        fixed_mask,
        is_game_over: game.is_game_over,
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

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
}
