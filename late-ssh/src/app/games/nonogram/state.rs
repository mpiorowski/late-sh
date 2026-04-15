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

const CELL_EMPTY: u8 = 0;
const CELL_FILLED: u8 = 1;
const CELL_MARKED_EMPTY: u8 = 2;

pub struct State {
    pub user_id: Uuid,
    pub mode: Mode,
    pub cursor: (usize, usize),
    library: Library,
    selected_pack: usize,
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

        for pack in library.packs() {
            let daily_snapshot = saved_games
                .iter()
                .find(|game| {
                    game.mode == "daily"
                        && game.size_key == pack.size_key
                        && is_current_daily_game(game.puzzle_date, today)
                })
                .and_then(|game| snapshot_from_game(game, pack))
                .unwrap_or_else(|| {
                    generate_snapshot(pack, Mode::Daily, &svc, today)
                        .expect("daily nonogram pack should always have a puzzle")
                });
            daily_snapshots.insert(pack.size_key.clone(), daily_snapshot);

            if let Some(snapshot) = saved_games
                .iter()
                .find(|game| game.mode == "personal" && game.size_key == pack.size_key)
                .and_then(|game| snapshot_from_game(game, pack))
            {
                personal_snapshots.insert(pack.size_key.clone(), snapshot);
            }
        }

        let mut state = Self {
            user_id,
            mode: Mode::Daily,
            cursor: (0, 0),
            library,
            selected_pack: 0,
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
        !self.library.is_empty()
    }

    pub fn pack_count(&self) -> usize {
        self.library.packs().len()
    }

    pub fn selected_pack(&self) -> Option<&NonogramPack> {
        self.library.pack(self.selected_pack)
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
            .insert(pack.size_key.clone(), snapshot.clone());
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

    pub fn next_pack(&mut self) {
        if self.library.is_empty() {
            return;
        }
        self.store_active_snapshot();
        self.selected_pack = (self.selected_pack + 1) % self.library.packs().len();
        self.load_mode_snapshot_for_selected_pack();
    }

    pub fn prev_pack(&mut self) {
        if self.library.is_empty() {
            return;
        }
        self.store_active_snapshot();
        self.selected_pack = (self.selected_pack + self.library.packs().len().saturating_sub(1))
            % self.library.packs().len();
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
            if self.mode == Mode::Daily
                && let Some(size_key) = self.selected_pack().map(|pack| pack.size_key.clone())
            {
                self.svc.record_win_task(self.user_id, size_key);
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
            Mode::Daily => self.daily_snapshots.get(&pack.size_key).cloned(),
            Mode::Personal => self.personal_snapshots.get(&pack.size_key).cloned(),
        }
        .or_else(|| {
            let snapshot = generate_snapshot(&pack, self.mode, &self.svc, self.svc.today())?;
            if self.mode == Mode::Daily {
                self.daily_snapshots
                    .insert(pack.size_key.clone(), snapshot.clone());
            } else {
                self.personal_snapshots
                    .insert(pack.size_key.clone(), snapshot.clone());
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
        let Some(size_key) = self.selected_pack().map(|pack| pack.size_key.clone()) else {
            return;
        };
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
                self.daily_snapshots.insert(size_key, snapshot);
            }
            Mode::Personal => {
                self.personal_snapshots.insert(size_key, snapshot);
            }
        }
    }

    fn save_async(&self) {
        let Some(pack) = self.selected_pack() else {
            return;
        };
        if self.current_puzzle_id.is_empty() {
            return;
        }

        self.svc.save_game_task(GameParams {
            user_id: self.user_id,
            mode: self.mode.as_str().to_string(),
            size_key: pack.size_key.clone(),
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

fn is_current_daily_game(puzzle_date: Option<NaiveDate>, today: NaiveDate) -> bool {
    puzzle_date == Some(today)
}

fn puzzle_date_for_mode(mode: Mode, today: NaiveDate) -> Option<NaiveDate> {
    match mode {
        Mode::Daily => Some(today),
        Mode::Personal => None,
    }
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
mod tests {
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
            packs: vec![NonogramPack {
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
            }],
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
}
