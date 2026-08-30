use chrono::{NaiveDate, Utc};
use late_core::models::{
    chips::Difficulty,
    sliding_puzzle::{Game, GameParams},
};
use rand_core::{OsRng, RngCore};
use uuid::Uuid;

use super::svc::SlidingPuzzleService;

const DIFFICULTIES: [Difficulty; 3] = [Difficulty::Easy, Difficulty::Medium, Difficulty::Hard];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

impl Direction {
    pub const fn inverse(self) -> Self {
        match self {
            Self::Up => Self::Down,
            Self::Down => Self::Up,
            Self::Left => Self::Right,
            Self::Right => Self::Left,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mode {
    Daily,
    Personal,
}

impl Mode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Daily => "daily",
            Self::Personal => "personal",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ResetAction {
    Reset,
    NewPersonal,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GeneratedBoard {
    pub tiles: Vec<u8>,
    pub blank_moves: Vec<Direction>,
}

#[derive(Clone)]
struct Snapshot {
    seed: u64,
    tiles: Vec<u8>,
    moves: u32,
    win_reported: bool,
}

#[derive(Clone)]
pub struct State {
    user_id: Uuid,
    puzzle_date: NaiveDate,
    pub mode: Mode,
    selected_difficulty: usize,
    daily_snapshots: [Snapshot; 3],
    personal_snapshots: [Option<Snapshot>; 3],
    reset_pending: Option<ResetAction>,
    message: String,
    svc: SlidingPuzzleService,
}

impl State {
    pub fn new(user_id: Uuid, svc: SlidingPuzzleService, saved_games: Vec<Game>) -> Self {
        Self::new_for_date(user_id, svc, Utc::now().date_naive(), saved_games)
    }

    pub(crate) fn new_for_date(
        user_id: Uuid,
        svc: SlidingPuzzleService,
        puzzle_date: NaiveDate,
        saved_games: Vec<Game>,
    ) -> Self {
        let daily_snapshots = DIFFICULTIES.map(|difficulty| {
            let seed = daily_seed(puzzle_date, difficulty);
            saved_games
                .iter()
                .find(|game| {
                    game.user_id == user_id
                        && game.mode == "daily"
                        && game.difficulty_key == difficulty.key()
                        && game.puzzle_date == Some(puzzle_date)
                        && game.puzzle_seed as u64 == seed
                })
                .and_then(|game| snapshot_from_game(game, difficulty))
                .unwrap_or_else(|| fresh_snapshot(difficulty, seed))
        });
        let personal_snapshots = DIFFICULTIES.map(|difficulty| {
            saved_games
                .iter()
                .find(|game| {
                    game.user_id == user_id
                        && game.mode == "personal"
                        && game.difficulty_key == difficulty.key()
                        && game.puzzle_date.is_none()
                })
                .and_then(|game| snapshot_from_game(game, difficulty))
        });

        Self {
            user_id,
            puzzle_date,
            mode: Mode::Daily,
            selected_difficulty: 1,
            daily_snapshots,
            personal_snapshots,
            reset_pending: None,
            message: "Slide a tile into the gap: direction key or click.".to_string(),
            svc,
        }
    }

    pub fn difficulty(&self) -> Difficulty {
        DIFFICULTIES[self.selected_difficulty]
    }

    pub fn difficulty_label(&self) -> String {
        let difficulty = self.difficulty();
        format!(
            "{} {}×{}",
            difficulty.key(),
            board_dimension(difficulty),
            board_dimension(difficulty)
        )
    }

    pub fn mode_label(&self) -> &'static str {
        self.mode.as_str()
    }

    pub fn reward_chips(&self) -> Option<i64> {
        (self.mode == Mode::Daily).then(|| self.difficulty().chips())
    }

    pub fn board(&self) -> &[u8] {
        &self.active_snapshot().tiles
    }

    pub fn moves(&self) -> u32 {
        self.active_snapshot().moves
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn reset_pending(&self) -> bool {
        self.reset_pending.is_some()
    }

    pub fn win_reported(&self) -> bool {
        self.active_snapshot().win_reported
    }

    pub fn is_solved(&self) -> bool {
        self.board() == solved_board(self.difficulty())
    }

    pub fn has_started(&self) -> bool {
        self.moves() > 0
    }

    pub fn first_unfinished_daily(&self) -> Option<usize> {
        self.daily_snapshots
            .iter()
            .enumerate()
            .find_map(|(index, snapshot)| {
                (snapshot.moves > 0 && !snapshot.win_reported).then_some(index)
            })
    }

    pub fn has_unfinished_daily(&self) -> bool {
        self.puzzle_date == Utc::now().date_naive() && self.first_unfinished_daily().is_some()
    }

    pub fn is_daily_active(&self) -> bool {
        self.mode == Mode::Daily
    }

    pub fn open_daily(&mut self, difficulty_index: usize) {
        self.clear_reset_pending();
        self.mode = Mode::Daily;
        self.selected_difficulty = difficulty_index.min(DIFFICULTIES.len() - 1);
        self.message = format!("Daily {} board.", self.difficulty_label());
    }

    /// Roll the dailies forward when the UTC date changes under a live
    /// session. Returns true when it moved.
    pub fn ensure_current_daily(&mut self) -> bool {
        let today = Utc::now().date_naive();
        if self.puzzle_date == today {
            return false;
        }
        self.puzzle_date = today;
        self.daily_snapshots = DIFFICULTIES
            .map(|difficulty| fresh_snapshot(difficulty, daily_seed(today, difficulty)));
        self.clear_reset_pending();
        if self.mode == Mode::Daily {
            self.message = "Today's Sliding Puzzle is ready.".to_string();
        }
        true
    }

    pub fn show_daily(&mut self) {
        self.clear_reset_pending();
        self.mode = Mode::Daily;
        self.message = format!("Daily {} board.", self.difficulty_label());
    }

    pub fn show_personal(&mut self) {
        self.clear_reset_pending();
        self.mode = Mode::Personal;
        let generated = self.ensure_personal_snapshot();
        self.message = "Personal board. No reward; n starts a new scramble.".to_string();
        if generated {
            self.save_async();
        }
    }

    pub fn new_personal_board(&mut self) {
        self.clear_reset_pending();
        self.mode = Mode::Personal;
        let difficulty = self.difficulty();
        self.personal_snapshots[self.selected_difficulty] =
            Some(fresh_snapshot(difficulty, OsRng.next_u64()));
        self.message = "New personal board. No rewards.".to_string();
        self.save_async();
    }

    pub fn next_difficulty(&mut self) {
        self.clear_reset_pending();
        self.selected_difficulty = (self.selected_difficulty + 1) % DIFFICULTIES.len();
        let generated = self.mode == Mode::Personal && self.ensure_personal_snapshot();
        self.message = format!("{} {} board.", self.mode_label(), self.difficulty_label());
        if generated {
            self.save_async();
        }
    }

    pub fn prev_difficulty(&mut self) {
        self.clear_reset_pending();
        self.selected_difficulty =
            (self.selected_difficulty + DIFFICULTIES.len() - 1) % DIFFICULTIES.len();
        let generated = self.mode == Mode::Personal && self.ensure_personal_snapshot();
        self.message = format!("{} {} board.", self.mode_label(), self.difficulty_label());
        if generated {
            self.save_async();
        }
    }

    /// Two-press reset confirm. Refused outright on a solved daily: the win
    /// is already banked, so re-scrambling would only put a finished puzzle
    /// back on the board and write the scramble over the solved row.
    pub fn request_reset(&mut self) -> bool {
        if self.mode == Mode::Daily && self.is_solved() {
            self.reset_pending = None;
            self.message = "Today's board is solved and banked; try another difficulty or p for a personal board.".to_string();
            return false;
        }
        let message = match self.mode {
            Mode::Daily => "Press r or 0 again to restore today's scramble.",
            Mode::Personal => "Press r or 0 again to restore this personal scramble.",
        };
        self.request_action(ResetAction::Reset, message)
    }

    pub fn request_new_personal(&mut self) -> bool {
        self.request_action(
            ResetAction::NewPersonal,
            "Press n again for a new personal scramble.",
        )
    }

    pub fn clear_reset_pending(&mut self) {
        if self.reset_pending.take().is_some() {
            self.message = self.idle_message();
        }
    }

    pub fn reset(&mut self) {
        let difficulty = self.difficulty();
        let seed = self.active_snapshot().seed;
        *self.active_snapshot_mut() = fresh_snapshot(difficulty, seed);
        self.reset_pending = None;
        self.message = match self.mode {
            Mode::Daily => "Today's scramble restored.".to_string(),
            Mode::Personal => "Personal scramble restored.".to_string(),
        };
        self.save_async();
    }

    pub fn move_blank(&mut self, direction: Direction) -> bool {
        self.clear_reset_pending();
        if self.is_solved() {
            return false;
        }

        let difficulty = self.difficulty();
        let dimension = board_dimension(difficulty);
        let (solved, moves) = {
            let snapshot = self.active_snapshot_mut();
            if !apply_blank_move(&mut snapshot.tiles, dimension, direction) {
                return false;
            }
            snapshot.moves = snapshot.moves.saturating_add(1);
            let solved = snapshot.tiles == solved_board(difficulty);
            if solved {
                snapshot.win_reported = true;
            }
            (solved, snapshot.moves)
        };

        if solved {
            let moves = moves.min(i32::MAX as u32) as i32;
            if self.mode == Mode::Daily {
                self.message = format!("Solved in {moves} moves.");
                self.complete_async(difficulty, moves);
            } else {
                self.message = format!("Solved in {moves} moves. Personal boards have no reward.");
                self.save_async();
            }
        } else {
            self.message = format!("Move {moves}.");
            self.save_async();
        }
        true
    }

    pub fn move_tile(&mut self, index: usize) -> bool {
        let dimension = board_dimension(self.difficulty());
        let Some(blank) = self.board().iter().position(|tile| *tile == 0) else {
            return false;
        };
        if index >= self.board().len() {
            return false;
        }

        let blank_row = blank / dimension;
        let blank_column = blank % dimension;
        let tile_row = index / dimension;
        let tile_column = index % dimension;
        let direction = if tile_row + 1 == blank_row && tile_column == blank_column {
            Direction::Up
        } else if tile_row == blank_row + 1 && tile_column == blank_column {
            Direction::Down
        } else if tile_row == blank_row && tile_column + 1 == blank_column {
            Direction::Left
        } else if tile_row == blank_row && tile_column == blank_column + 1 {
            Direction::Right
        } else {
            return false;
        };

        self.move_blank(direction)
    }

    fn save_async(&self) {
        if tokio::runtime::Handle::try_current().is_err() {
            return;
        }
        self.svc.save_game_task(self.game_params());
    }

    fn complete_async(&self, difficulty: Difficulty, moves: i32) {
        if tokio::runtime::Handle::try_current().is_err() {
            return;
        }
        self.svc
            .complete_game_task(self.game_params(), difficulty, self.puzzle_date, moves);
    }

    fn game_params(&self) -> GameParams {
        let snapshot = self.active_snapshot();
        GameParams {
            user_id: self.user_id,
            mode: self.mode.as_str().to_string(),
            difficulty_key: self.difficulty().key().to_string(),
            puzzle_date: (self.mode == Mode::Daily).then_some(self.puzzle_date),
            puzzle_seed: snapshot.seed as i64,
            tiles: snapshot.tiles.iter().copied().map(i32::from).collect(),
            moves: snapshot.moves.min(i32::MAX as u32) as i32,
        }
    }

    #[cfg(test)]
    pub(crate) fn set_board_for_test(
        &mut self,
        difficulty: Difficulty,
        tiles: Vec<u8>,
        moves: u32,
    ) {
        self.selected_difficulty = DIFFICULTIES
            .iter()
            .position(|candidate| *candidate == difficulty)
            .expect("known difficulty");
        if self.mode == Mode::Personal {
            self.ensure_personal_snapshot();
        }
        let snapshot = self.active_snapshot_mut();
        snapshot.tiles = tiles;
        snapshot.moves = moves;
        snapshot.win_reported = false;
    }

    #[cfg(test)]
    pub(crate) fn scramble_seed(&self) -> u64 {
        self.active_snapshot().seed
    }

    fn request_action(&mut self, action: ResetAction, message: &str) -> bool {
        if self.reset_pending == Some(action) {
            self.reset_pending = None;
            return true;
        }
        self.reset_pending = Some(action);
        self.message = message.to_string();
        false
    }

    fn ensure_personal_snapshot(&mut self) -> bool {
        let index = self.selected_difficulty;
        if self.personal_snapshots[index].is_some() {
            return false;
        }
        self.personal_snapshots[index] = Some(fresh_snapshot(self.difficulty(), OsRng.next_u64()));
        true
    }

    fn active_snapshot(&self) -> &Snapshot {
        let index = self.selected_difficulty;
        match self.mode {
            Mode::Daily => &self.daily_snapshots[index],
            Mode::Personal => self.personal_snapshots[index]
                .as_ref()
                .expect("personal snapshot is generated before activation"),
        }
    }

    fn active_snapshot_mut(&mut self) -> &mut Snapshot {
        let index = self.selected_difficulty;
        match self.mode {
            Mode::Daily => &mut self.daily_snapshots[index],
            Mode::Personal => self.personal_snapshots[index]
                .as_mut()
                .expect("personal snapshot is generated before activation"),
        }
    }

    fn idle_message(&self) -> String {
        match self.mode {
            Mode::Daily => "Slide a tile into the gap: direction key or click.".to_string(),
            Mode::Personal => "Personal board. No reward; n starts a new scramble.".to_string(),
        }
    }
}

fn fresh_snapshot(difficulty: Difficulty, seed: u64) -> Snapshot {
    Snapshot {
        seed,
        tiles: generate_scramble(difficulty, seed).tiles,
        moves: 0,
        win_reported: false,
    }
}

fn snapshot_from_game(game: &Game, difficulty: Difficulty) -> Option<Snapshot> {
    let tiles = valid_tiles(&game.tiles, difficulty)?;
    let win_reported = tiles == solved_board(difficulty);
    Some(Snapshot {
        seed: game.puzzle_seed as u64,
        tiles,
        moves: game.moves.max(0) as u32,
        win_reported,
    })
}

pub const fn board_len(difficulty: Difficulty) -> usize {
    let dimension = board_dimension(difficulty);
    dimension * dimension
}

pub const fn board_dimension(difficulty: Difficulty) -> usize {
    match difficulty {
        Difficulty::Easy => 3,
        Difficulty::Medium => 4,
        Difficulty::Hard => 5,
    }
}

pub fn solved_board(difficulty: Difficulty) -> Vec<u8> {
    let len = board_len(difficulty);
    (1..len as u8).chain(std::iter::once(0)).collect()
}

pub(crate) fn daily_seed(puzzle_date: NaiveDate, difficulty: Difficulty) -> u64 {
    let mut seed = 0xcbf2_9ce4_8422_2325u64;
    for byte in b"late-sh-sliding-puzzle-daily-v1"
        .iter()
        .copied()
        .chain(puzzle_date.format("%Y-%m-%d").to_string().bytes())
        .chain(difficulty.key().bytes())
    {
        seed ^= u64::from(byte);
        seed = seed.wrapping_mul(0x0000_0100_0000_01b3);
    }
    seed
}

pub fn generate_scramble(difficulty: Difficulty, mut seed: u64) -> GeneratedBoard {
    let dimension = board_dimension(difficulty);
    let move_count = match difficulty {
        Difficulty::Easy => 48,
        Difficulty::Medium => 128,
        Difficulty::Hard => 240,
    };
    let mut tiles = solved_board(difficulty);
    let mut blank_moves = Vec::with_capacity(move_count + 1);
    let mut previous: Option<Direction> = None;

    for _ in 0..move_count {
        let mut legal = legal_blank_moves(&tiles, dimension);
        if let Some(previous) = previous
            && legal.len() > 1
        {
            legal.retain(|direction| *direction != previous.inverse());
        }
        let direction = legal[(next_seed(&mut seed) as usize) % legal.len()];
        let moved = apply_blank_move(&mut tiles, dimension, direction);
        debug_assert!(moved);
        blank_moves.push(direction);
        previous = Some(direction);
    }

    if tiles == solved_board(difficulty) {
        let direction = legal_blank_moves(&tiles, dimension)
            .into_iter()
            .find(|direction| previous.is_none_or(|previous| *direction != previous.inverse()))
            .unwrap_or(Direction::Left);
        let moved = apply_blank_move(&mut tiles, dimension, direction);
        debug_assert!(moved);
        blank_moves.push(direction);
    }

    GeneratedBoard { tiles, blank_moves }
}

/// One LCG step, returned through a splitmix64 finalizer. The raw state's low
/// bits are strictly periodic — bit 0 flips every step whatever the seed — and
/// callers index with `% legal.len()`, so returning it unmixed picked nearly
/// the same scramble every day.
fn next_seed(seed: &mut u64) -> u64 {
    *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
    let mut mixed = *seed;
    mixed = (mixed ^ (mixed >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    mixed = (mixed ^ (mixed >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    mixed ^ (mixed >> 31)
}

fn legal_blank_moves(tiles: &[u8], dimension: usize) -> Vec<Direction> {
    let blank = tiles.iter().position(|tile| *tile == 0).unwrap_or(0);
    let row = blank / dimension;
    let column = blank % dimension;
    let mut legal = Vec::with_capacity(4);
    if row > 0 {
        legal.push(Direction::Up);
    }
    if row + 1 < dimension {
        legal.push(Direction::Down);
    }
    if column > 0 {
        legal.push(Direction::Left);
    }
    if column + 1 < dimension {
        legal.push(Direction::Right);
    }
    legal
}

pub fn apply_blank_move(tiles: &mut [u8], dimension: usize, direction: Direction) -> bool {
    if tiles.len() != dimension * dimension {
        return false;
    }
    let Some(blank) = tiles.iter().position(|tile| *tile == 0) else {
        return false;
    };
    let row = blank / dimension;
    let column = blank % dimension;
    let destination = match direction {
        Direction::Up if row > 0 => blank - dimension,
        Direction::Down if row + 1 < dimension => blank + dimension,
        Direction::Left if column > 0 => blank - 1,
        Direction::Right if column + 1 < dimension => blank + 1,
        _ => return false,
    };
    tiles.swap(blank, destination);
    true
}

fn valid_tiles(values: &[i32], difficulty: Difficulty) -> Option<Vec<u8>> {
    let len = board_len(difficulty);
    if values.len() != len {
        return None;
    }
    let mut seen = vec![false; len];
    let mut tiles = Vec::with_capacity(len);
    for &value in values {
        let value = usize::try_from(value).ok()?;
        if value >= len || seen[value] {
            return None;
        }
        seen[value] = true;
        tiles.push(value as u8);
    }
    is_reachable(&tiles, board_dimension(difficulty)).then_some(tiles)
}

/// Whether sliding tiles can turn this board into the solved one. Each slide
/// swaps the blank with one tile, flipping the arrangement's permutation
/// parity, and moves the blank one step nearer or further from its home
/// corner. The two parities therefore agree on every reachable board and on no
/// other, which covers odd and even dimensions alike.
fn is_reachable(tiles: &[u8], dimension: usize) -> bool {
    let len = tiles.len();
    let home = |tile: u8| {
        if tile == 0 {
            len - 1
        } else {
            tile as usize - 1
        }
    };
    let mut visited = vec![false; len];
    let mut cycles = 0;
    for start in 0..len {
        if visited[start] {
            continue;
        }
        cycles += 1;
        let mut cursor = start;
        while !visited[cursor] {
            visited[cursor] = true;
            cursor = home(tiles[cursor]);
        }
    }
    let swaps = len - cycles;

    let Some(blank) = tiles.iter().position(|tile| *tile == 0) else {
        return false;
    };
    let blank_distance = (dimension - 1 - blank / dimension) + (dimension - 1 - blank % dimension);
    swaps % 2 == blank_distance % 2
}
