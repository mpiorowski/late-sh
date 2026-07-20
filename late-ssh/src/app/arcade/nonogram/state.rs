use anyhow::{Context, Result};
use chrono::NaiveDate;
use late_core::nonogram::{NonogramPack, NonogramPackIndex, NonogramPuzzle};
use rand_core::{OsRng, RngCore};
use std::collections::HashMap;
use uuid::Uuid;

use super::svc::NonogramService;
use late_core::models::nonogram::{Game, GameParams};

#[derive(Clone, Debug, Default)]
pub struct Library {
    packs: Vec<NonogramPack>,
}

impl Library {
    pub fn packs(&self) -> &[NonogramPack] {
        &self.packs
    }

    pub fn pack(&self, index: usize) -> Option<&NonogramPack> {
        self.packs.get(index)
    }

    pub fn pack_by_size_key(&self, size_key: &str) -> Option<&NonogramPack> {
        self.packs.iter().find(|pack| pack.size_key == size_key)
    }

    pub fn is_empty(&self) -> bool {
        self.packs.is_empty()
    }
}

#[derive(Clone, Copy, Debug)]
pub struct NonogramDifficulty {
    pub key: &'static str,
    pub size_key: &'static str,
}

pub const DIFFICULTIES: [NonogramDifficulty; 3] = [
    NonogramDifficulty {
        key: "easy",
        size_key: "10x10",
    },
    NonogramDifficulty {
        key: "medium",
        size_key: "15x15",
    },
    NonogramDifficulty {
        key: "hard",
        size_key: "20x20",
    },
];

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Daily,
    Personal,
}

impl Mode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Mode::Daily => "daily",
            Mode::Personal => "personal",
        }
    }
}

#[derive(Clone, Debug)]
struct PuzzleSnapshot {
    puzzle_id: String,
    player_grid: Vec<Vec<u8>>,
    is_game_over: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineStatus {
    Pending,
    Satisfied,
    Impossible,
}

const CELL_EMPTY: u8 = 0;
const CELL_FILLED: u8 = 1;
const CELL_MARKED_EMPTY: u8 = 2;

pub struct State {
    pub user_id: Uuid,
    pub mode: Mode,
    pub cursor: (usize, usize),
    library: Library,
    selected_difficulty: usize,
    current_puzzle_id: String,
    player_grid: Vec<Vec<u8>>,
    is_game_over: bool,
    daily_snapshots: HashMap<String, PuzzleSnapshot>,
    personal_snapshots: HashMap<String, PuzzleSnapshot>,
    pub svc: NonogramService,
}

impl State {
    pub fn new(
        user_id: Uuid,
        svc: NonogramService,
        library: Library,
        saved_games: Vec<Game>,
    ) -> Self {
        let today = svc.today();
        let mut daily_snapshots = HashMap::new();
        let mut personal_snapshots = HashMap::new();

        for difficulty in DIFFICULTIES {
            let Some(pack) = library.pack_by_size_key(difficulty.size_key) else {
                continue;
            };
            let daily_snapshot = saved_games
                .iter()
                .find(|game| {
                    game.mode == "daily"
                        && game.difficulty_key == difficulty.key
                        && is_current_daily_game(game.puzzle_date, today)
                })
                .and_then(|game| snapshot_from_game(game, pack))
                .unwrap_or_else(|| {
                    generate_snapshot(pack, Mode::Daily, &svc, today)
                        .expect("daily nonogram pack should always have a puzzle")
                });
            daily_snapshots.insert(difficulty.key.to_string(), daily_snapshot);

            if let Some(snapshot) = saved_games
                .iter()
                .find(|game| game.mode == "personal" && game.difficulty_key == difficulty.key)
                .and_then(|game| snapshot_from_game(game, pack))
            {
                personal_snapshots.insert(difficulty.key.to_string(), snapshot);
            }
        }

        let selected_difficulty = default_selected_difficulty(&library);
        let mut state = Self {
            user_id,
            mode: Mode::Daily,
            cursor: (0, 0),
            library,
            selected_difficulty,
            current_puzzle_id: String::new(),
            player_grid: Vec::new(),
            is_game_over: false,
            daily_snapshots,
            personal_snapshots,
            svc,
        };
        state.load_mode_snapshot_for_selected_pack();
        state
    }

    pub fn has_puzzles(&self) -> bool {
        DIFFICULTIES
            .iter()
            .any(|difficulty| self.library.pack_by_size_key(difficulty.size_key).is_some())
    }

    pub fn pack_count(&self) -> usize {
        DIFFICULTIES.len()
    }

    pub fn difficulty_key(&self) -> &'static str {
        DIFFICULTIES[self.selected_difficulty].key
    }

    pub fn selected_pack(&self) -> Option<&NonogramPack> {
        self.library
            .pack_by_size_key(DIFFICULTIES[self.selected_difficulty].size_key)
    }

    pub fn puzzle(&self) -> Option<&NonogramPuzzle> {
        self.selected_pack()?
            .puzzles
            .iter()
            .find(|puzzle| puzzle.id == self.current_puzzle_id)
    }

    pub fn current_puzzle_id(&self) -> &str {
        &self.current_puzzle_id
    }

    pub fn player_grid(&self) -> &[Vec<u8>] {
        &self.player_grid
    }

    pub fn is_game_over(&self) -> bool {
        self.is_game_over
    }

    pub fn filled_count(&self) -> usize {
        self.player_grid
            .iter()
            .flatten()
            .filter(|&&cell| cell == CELL_FILLED)
            .count()
    }

    pub fn target_count(&self) -> usize {
        self.puzzle()
            .map(|puzzle| {
                puzzle
                    .solution
                    .iter()
                    .flatten()
                    .filter(|&&cell| cell == 1)
                    .count()
            })
            .unwrap_or(0)
    }

    pub fn satisfied_rows_and_cols(&self) -> Option<(Vec<bool>, Vec<bool>)> {
        let puzzle = self.puzzle()?;
        Some(row_col_satisfaction(puzzle, &self.player_grid))
    }

    pub fn row_col_statuses(&self) -> Option<(Vec<LineStatus>, Vec<LineStatus>)> {
        let puzzle = self.puzzle()?;
        Some(row_col_statuses(puzzle, &self.player_grid))
    }

    /// Index of the first daily difficulty with player marks on the grid and
    /// no win yet: the live grid when it is the active daily, the stored
    /// snapshot otherwise.
    pub fn first_unfinished_daily(&self) -> Option<usize> {
        DIFFICULTIES
            .iter()
            .enumerate()
            .find_map(|(index, difficulty)| {
                let started = if self.mode == Mode::Daily && index == self.selected_difficulty {
                    !self.is_game_over && grid_has_player_marks(&self.player_grid)
                } else {
                    self.daily_snapshots
                        .get(difficulty.key)
                        .is_some_and(|snapshot| {
                            !snapshot.is_game_over && grid_has_player_marks(&snapshot.player_grid)
                        })
                };
                started.then_some(index)
            })
    }

    /// True while the active board is a daily (not a personal board). The
    /// backtick workspace cycle only counts daily boards as stops.
    pub fn is_daily_active(&self) -> bool {
        self.mode == Mode::Daily
    }

    /// Jump straight to a daily board: the backtick workspace entry path.
    pub fn open_daily(&mut self, difficulty_index: usize) {
        self.store_active_snapshot();
        self.mode = Mode::Daily;
        self.selected_difficulty = difficulty_index.min(DIFFICULTIES.len() - 1);
        self.load_mode_snapshot_for_selected_pack();
    }

    pub fn show_personal(&mut self) {
        self.store_active_snapshot();
        self.mode = Mode::Personal;
        self.load_mode_snapshot_for_selected_pack();
    }

    pub fn show_daily(&mut self) {
        self.store_active_snapshot();
        self.mode = Mode::Daily;
        self.load_mode_snapshot_for_selected_pack();
    }

    pub fn new_personal_board(&mut self) {
        self.store_active_snapshot();
        let Some(pack) = self.selected_pack().cloned() else {
            return;
        };

        let Some(snapshot) = generate_snapshot(&pack, Mode::Personal, &self.svc, self.svc.today())
        else {
            return;
        };

        self.personal_snapshots
            .insert(self.difficulty_key().to_string(), snapshot.clone());
        self.mode = Mode::Personal;
        self.apply_snapshot(snapshot);
        self.save_async();
    }

    pub fn reset_board(&mut self) {
        if self.is_game_over {
            return;
        }
        let Some(puzzle) = self.puzzle() else {
            return;
        };
        self.player_grid = vec![vec![CELL_EMPTY; puzzle.width as usize]; puzzle.height as usize];
        self.cursor = (0, 0);
        self.save_async();
    }

    pub fn move_cursor(&mut self, dr: isize, dc: isize) {
        let Some(puzzle) = self.puzzle() else {
            return;
        };
        if self.is_game_over {
            return;
        }

        let row = (self.cursor.0 as isize + dr).clamp(0, puzzle.height as isize - 1) as usize;
        let col = (self.cursor.1 as isize + dc).clamp(0, puzzle.width as isize - 1) as usize;
        self.cursor = (row, col);
    }

    pub fn toggle_cell(&mut self) {
        if self.is_game_over {
            return;
        }
        let (row, col) = self.cursor;
        if row >= self.player_grid.len()
            || self
                .player_grid
                .get(row)
                .is_none_or(|line| col >= line.len())
        {
            return;
        }

        self.player_grid[row][col] = match self.player_grid[row][col] {
            CELL_FILLED => CELL_EMPTY,
            CELL_MARKED_EMPTY => return,
            _ => CELL_FILLED,
        };
        self.after_edit();
    }

    pub fn toggle_mark(&mut self) {
        if self.is_game_over {
            return;
        }
        let (row, col) = self.cursor;
        if row >= self.player_grid.len()
            || self
                .player_grid
                .get(row)
                .is_none_or(|line| col >= line.len())
        {
            return;
        }

        self.player_grid[row][col] = match self.player_grid[row][col] {
            CELL_MARKED_EMPTY => CELL_EMPTY,
            _ => CELL_MARKED_EMPTY,
        };
        self.after_edit();
    }

    pub fn clear_cell(&mut self) {
        if self.is_game_over {
            return;
        }
        let (row, col) = self.cursor;
        if row >= self.player_grid.len()
            || self
                .player_grid
                .get(row)
                .is_none_or(|line| col >= line.len())
        {
            return;
        }

        self.player_grid[row][col] = CELL_EMPTY;
        self.after_edit();
    }

    pub fn next_difficulty(&mut self) {
        if !self.has_puzzles() {
            return;
        }
        self.store_active_snapshot();
        self.selected_difficulty = (self.selected_difficulty + 1) % DIFFICULTIES.len();
        self.load_mode_snapshot_for_selected_pack();
    }

    pub fn prev_difficulty(&mut self) {
        if !self.has_puzzles() {
            return;
        }
        self.store_active_snapshot();
        self.selected_difficulty =
            (self.selected_difficulty + DIFFICULTIES.len().saturating_sub(1)) % DIFFICULTIES.len();
        self.load_mode_snapshot_for_selected_pack();
    }

    fn after_edit(&mut self) {
        self.check_win();
        self.store_active_snapshot();
        self.save_async();
    }

    fn check_win(&mut self) {
        if self.is_game_over {
            return;
        }
        let Some(puzzle) = self.puzzle() else {
            return;
        };

        let solved = board_matches_clues(puzzle, &self.player_grid);

        if solved {
            self.is_game_over = true;
            if self.mode == Mode::Daily {
                self.svc
                    .record_win_task(self.user_id, self.difficulty_key().to_string());
            }
        }
    }

    fn load_mode_snapshot_for_selected_pack(&mut self) {
        let Some(pack) = self.selected_pack().cloned() else {
            self.current_puzzle_id.clear();
            self.player_grid.clear();
            self.is_game_over = false;
            self.cursor = (0, 0);
            return;
        };

        let mut generated = false;
        let snapshot = match self.mode {
            Mode::Daily => self.daily_snapshots.get(self.difficulty_key()).cloned(),
            Mode::Personal => self.personal_snapshots.get(self.difficulty_key()).cloned(),
        }
        .or_else(|| {
            let snapshot = generate_snapshot(&pack, self.mode, &self.svc, self.svc.today())?;
            if self.mode == Mode::Daily {
                self.daily_snapshots
                    .insert(self.difficulty_key().to_string(), snapshot.clone());
            } else {
                self.personal_snapshots
                    .insert(self.difficulty_key().to_string(), snapshot.clone());
            }
            generated = true;
            Some(snapshot)
        });

        if let Some(snapshot) = snapshot {
            self.apply_snapshot(snapshot);
            if self.mode == Mode::Personal && generated {
                self.save_async();
            }
        }
    }

    fn apply_snapshot(&mut self, snapshot: PuzzleSnapshot) {
        self.current_puzzle_id = snapshot.puzzle_id;
        self.player_grid = snapshot.player_grid;
        self.is_game_over = snapshot.is_game_over;
        self.cursor = (0, 0);
    }

    fn store_active_snapshot(&mut self) {
        let difficulty_key = self.difficulty_key().to_string();
        if self.current_puzzle_id.is_empty() {
            return;
        }

        let snapshot = PuzzleSnapshot {
            puzzle_id: self.current_puzzle_id.clone(),
            player_grid: self.player_grid.clone(),
            is_game_over: self.is_game_over,
        };

        match self.mode {
            Mode::Daily => {
                self.daily_snapshots.insert(difficulty_key, snapshot);
            }
            Mode::Personal => {
                self.personal_snapshots.insert(difficulty_key, snapshot);
            }
        }
    }

    fn save_async(&self) {
        if self.selected_pack().is_none() {
            return;
        }
        if self.current_puzzle_id.is_empty() {
            return;
        }

        self.svc.save_game_task(GameParams {
            user_id: self.user_id,
            mode: self.mode.as_str().to_string(),
            difficulty_key: self.difficulty_key().to_string(),
            puzzle_date: puzzle_date_for_mode(self.mode, self.svc.today()),
            puzzle_id: self.current_puzzle_id.clone(),
            player_grid: serde_json::to_value(&self.player_grid).unwrap_or_default(),
            is_game_over: self.is_game_over,
            score: self.filled_count() as i32,
        });
    }
}

fn generate_snapshot(
    pack: &NonogramPack,
    mode: Mode,
    _svc: &NonogramService,
    today: NaiveDate,
) -> Option<PuzzleSnapshot> {
    let puzzle = match mode {
        Mode::Daily => pack.select_for_date(today),
        Mode::Personal => {
            if pack.puzzles.is_empty() {
                None
            } else {
                let idx = (OsRng.next_u64() as usize) % pack.puzzles.len();
                pack.puzzles.get(idx)
            }
        }
    }?;

    Some(PuzzleSnapshot {
        puzzle_id: puzzle.id.clone(),
        player_grid: empty_player_grid(puzzle),
        is_game_over: false,
    })
}

fn empty_player_grid(puzzle: &NonogramPuzzle) -> Vec<Vec<u8>> {
    vec![vec![0; puzzle.width as usize]; puzzle.height as usize]
}

fn board_matches_clues(puzzle: &NonogramPuzzle, player_grid: &[Vec<u8>]) -> bool {
    let normalized = player_grid
        .iter()
        .take(puzzle.height as usize)
        .map(|row| {
            row.iter()
                .take(puzzle.width as usize)
                .map(|&cell| u8::from(cell == CELL_FILLED))
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    let (row_clues, col_clues) = late_core::nonogram::derive_clues(&normalized);
    row_clues == puzzle.row_clues && col_clues == puzzle.col_clues
}

fn row_col_satisfaction(
    puzzle: &NonogramPuzzle,
    player_grid: &[Vec<u8>],
) -> (Vec<bool>, Vec<bool>) {
    let normalized: Vec<Vec<u8>> = player_grid
        .iter()
        .take(puzzle.height as usize)
        .map(|row| {
            row.iter()
                .take(puzzle.width as usize)
                .map(|&cell| u8::from(cell == CELL_FILLED))
                .collect()
        })
        .collect();

    let (derived_rows, derived_cols) = late_core::nonogram::derive_clues(&normalized);

    let rows = puzzle
        .row_clues
        .iter()
        .zip(derived_rows.iter())
        .map(|(e, d)| d == e)
        .collect();

    let cols = puzzle
        .col_clues
        .iter()
        .zip(derived_cols.iter())
        .map(|(e, d)| d == e)
        .collect();

    (rows, cols)
}

fn row_col_statuses(
    puzzle: &NonogramPuzzle,
    player_grid: &[Vec<u8>],
) -> (Vec<LineStatus>, Vec<LineStatus>) {
    let (satisfied_rows, satisfied_cols) = row_col_satisfaction(puzzle, player_grid);

    let rows = (0..puzzle.height as usize)
        .map(|row| {
            let cells = player_grid
                .get(row)
                .map(|line| line.iter().copied().take(puzzle.width as usize).collect())
                .unwrap_or_else(|| vec![CELL_EMPTY; puzzle.width as usize]);
            line_status(
                satisfied_rows.get(row).copied().unwrap_or(false),
                &cells,
                &puzzle.row_clues[row],
            )
        })
        .collect();

    let cols = (0..puzzle.width as usize)
        .map(|col| {
            let cells = (0..puzzle.height as usize)
                .map(|row| {
                    player_grid
                        .get(row)
                        .and_then(|line| line.get(col))
                        .copied()
                        .unwrap_or(CELL_EMPTY)
                })
                .collect::<Vec<_>>();
            line_status(
                satisfied_cols.get(col).copied().unwrap_or(false),
                &cells,
                &puzzle.col_clues[col],
            )
        })
        .collect();

    (rows, cols)
}

fn line_status(satisfied: bool, cells: &[u8], clues: &[u8]) -> LineStatus {
    if satisfied {
        LineStatus::Satisfied
    } else if line_can_still_match(cells, clues) {
        LineStatus::Pending
    } else {
        LineStatus::Impossible
    }
}

fn line_can_still_match(cells: &[u8], clues: &[u8]) -> bool {
    let mut states = vec![(0usize, 0u8)];

    for &cell in cells {
        let mut next = Vec::new();
        for &(clue_idx, run_len) in &states {
            if cell != CELL_MARKED_EMPTY {
                push_filled_state(&mut next, clues, clue_idx, run_len);
            }
            if cell != CELL_FILLED {
                push_empty_state(&mut next, clues, clue_idx, run_len);
            }
        }
        next.sort_unstable();
        next.dedup();
        states = next;
        if states.is_empty() {
            return false;
        }
    }

    states
        .into_iter()
        .any(|(clue_idx, run_len)| line_end_matches(clues, clue_idx, run_len))
}

fn push_filled_state(states: &mut Vec<(usize, u8)>, clues: &[u8], clue_idx: usize, run_len: u8) {
    if clue_idx < clues.len() && run_len < clues[clue_idx] {
        states.push((clue_idx, run_len + 1));
    }
}

fn push_empty_state(states: &mut Vec<(usize, u8)>, clues: &[u8], clue_idx: usize, run_len: u8) {
    if run_len == 0 {
        states.push((clue_idx, 0));
    } else if clue_idx < clues.len() && run_len == clues[clue_idx] {
        states.push((clue_idx + 1, 0));
    }
}

fn line_end_matches(clues: &[u8], clue_idx: usize, run_len: u8) -> bool {
    if run_len == 0 {
        clue_idx == clues.len()
    } else {
        clue_idx < clues.len() && run_len == clues[clue_idx] && clue_idx + 1 == clues.len()
    }
}

fn snapshot_from_game(game: &Game, pack: &NonogramPack) -> Option<PuzzleSnapshot> {
    let puzzle = pack
        .puzzles
        .iter()
        .find(|puzzle| puzzle.id == game.puzzle_id)?;
    let mut player_grid = empty_player_grid(puzzle);

    if let Some(rows) = game.player_grid.as_array() {
        for (row_idx, row_value) in rows.iter().enumerate().take(puzzle.height as usize) {
            if let Some(row) = row_value.as_array() {
                for (col_idx, cell) in row.iter().enumerate().take(puzzle.width as usize) {
                    player_grid[row_idx][col_idx] = match cell.as_u64().unwrap_or(0) as u8 {
                        CELL_FILLED => CELL_FILLED,
                        CELL_MARKED_EMPTY => CELL_MARKED_EMPTY,
                        _ => CELL_EMPTY,
                    };
                }
            }
        }
    }

    Some(PuzzleSnapshot {
        puzzle_id: game.puzzle_id.clone(),
        player_grid,
        is_game_over: game.is_game_over,
    })
}

fn grid_has_player_marks(grid: &[Vec<u8>]) -> bool {
    grid.iter()
        .any(|row| row.iter().any(|cell| *cell != CELL_EMPTY))
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

fn default_selected_difficulty(library: &Library) -> usize {
    DIFFICULTIES
        .iter()
        .position(|difficulty| {
            difficulty.key == "medium" && library.pack_by_size_key(difficulty.size_key).is_some()
        })
        .or_else(|| {
            DIFFICULTIES
                .iter()
                .position(|difficulty| library.pack_by_size_key(difficulty.size_key).is_some())
        })
        .unwrap_or(0)
}

// Nonogram packs embedded at compile time — no runtime file I/O needed.
const INDEX_JSON: &[u8] = include_bytes!("../../../../assets/nonograms/index.json");
const PACK_10X10: &[u8] = include_bytes!("../../../../assets/nonograms/10x10.json");
const PACK_15X15: &[u8] = include_bytes!("../../../../assets/nonograms/15x15.json");
const PACK_20X20: &[u8] = include_bytes!("../../../../assets/nonograms/20x20.json");

pub fn load_default_library() -> Result<Library> {
    let index: NonogramPackIndex =
        serde_json::from_slice(INDEX_JSON).context("failed to parse embedded index.json")?;

    let mut packs = Vec::with_capacity(index.packs.len());
    for entry in index.packs {
        let pack_bytes = match entry.size_key.as_str() {
            "10x10" => PACK_10X10,
            "15x15" => PACK_15X15,
            "20x20" => PACK_20X20,
            other => anyhow::bail!("unknown embedded pack size: {other}"),
        };
        let pack: NonogramPack = serde_json::from_slice(pack_bytes)
            .with_context(|| format!("failed to parse embedded {}.json", entry.size_key))?;
        pack.validate()?;
        packs.push(pack);
    }

    Ok(Library { packs })
}

#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;

