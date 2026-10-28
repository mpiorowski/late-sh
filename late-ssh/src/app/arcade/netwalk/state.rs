//! Netwalk: rotate tiles until one network, fed from the centre tile,
//! reaches every tile with no connection left hanging. This file holds the
//! pure rules (the seeded generator, the lit and solved checks) and the
//! per-session state; persistence goes through `svc.rs`.
//!
//! A board is a spanning tree of the grid with its tiles turned. Turning a
//! tile never changes how many connections it has, so any arrangement that
//! lights every tile is a spanning tree too: the solved check is the rule,
//! never a comparison with the generated network, and any wiring that
//! lights the board wins.

use chrono::{NaiveDate, Utc};
use late_core::models::{
    chips::Difficulty,
    netwalk::{Game, GameParams},
};
use rand_core::{OsRng, RngCore};
use uuid::Uuid;

use crate::metrics::{ArcadeDifficulty, ArcadeFinish, ArcadeMode};

use super::svc::NetwalkService;

const DIFFICULTIES: [Difficulty; 3] = [Difficulty::Easy, Difficulty::Medium, Difficulty::Hard];

/// One side of a tile: a bit of its connection mask, and a step on the grid.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Side {
    North,
    East,
    South,
    West,
}

impl Side {
    pub const ALL: [Self; 4] = [Self::North, Self::East, Self::South, Self::West];

    pub const fn bit(self) -> u8 {
        match self {
            Self::North => 1,
            Self::East => 2,
            Self::South => 4,
            Self::West => 8,
        }
    }

    pub const fn opposite(self) -> Self {
        match self {
            Self::North => Self::South,
            Self::East => Self::West,
            Self::South => Self::North,
            Self::West => Self::East,
        }
    }
}

/// Which way a tile turns.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Turn {
    Clockwise,
    CounterClockwise,
}

/// A connection mask turned a quarter: every side moves to the next one.
pub const fn rotate(mask: u8, turn: Turn) -> u8 {
    match turn {
        Turn::Clockwise => ((mask << 1) | (mask >> 3)) & 0xF,
        Turn::CounterClockwise => ((mask >> 1) | (mask << 3)) & 0xF,
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

    /// Sentence-case name, for the start of a message line.
    fn title(self) -> &'static str {
        match self {
            Self::Daily => "Daily",
            Self::Personal => "Personal",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ResetAction {
    Reset,
    NewPersonal,
}

/// A generated board: the network as built, and the scramble it starts in.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Puzzle {
    pub solution: Vec<u8>,
    pub scrambled: Vec<u8>,
}

/// One board in play. `seed`, `tiles` and `moves` persist; the cursor and
/// the locks are this session's only.
struct Board {
    seed: u64,
    tiles: Vec<u8>,
    moves: u32,
    win_reported: bool,
    cursor: usize,
    locked: Vec<bool>,
}

pub struct State {
    user_id: Uuid,
    puzzle_date: NaiveDate,
    mode: Mode,
    selected_difficulty: usize,
    daily_boards: [Board; 3],
    personal_boards: [Option<Board>; 3],
    reset_pending: Option<ResetAction>,
    message: String,
    svc: NetwalkService,
}

impl State {
    pub fn new(user_id: Uuid, svc: NetwalkService, saved_games: Vec<Game>) -> Self {
        Self::new_for_date(user_id, svc, Utc::now().date_naive(), saved_games)
    }

    pub(crate) fn new_for_date(
        user_id: Uuid,
        svc: NetwalkService,
        puzzle_date: NaiveDate,
        saved_games: Vec<Game>,
    ) -> Self {
        let daily_boards = DIFFICULTIES.map(|difficulty| {
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
                .and_then(|game| board_from_game(game, difficulty))
                .unwrap_or_else(|| fresh_board(difficulty, seed))
        });
        let personal_boards = DIFFICULTIES.map(|difficulty| {
            saved_games
                .iter()
                .find(|game| {
                    game.user_id == user_id
                        && game.mode == "personal"
                        && game.difficulty_key == difficulty.key()
                        && game.puzzle_date.is_none()
                })
                .and_then(|game| board_from_game(game, difficulty))
        });

        Self {
            user_id,
            puzzle_date,
            mode: Mode::Daily,
            selected_difficulty: 0,
            daily_boards,
            personal_boards,
            reset_pending: None,
            message: idle_message(Mode::Daily).to_string(),
            svc,
        }
    }

    pub fn difficulty(&self) -> Difficulty {
        DIFFICULTIES[self.selected_difficulty]
    }

    pub fn dimension(&self) -> usize {
        board_dimension(self.difficulty())
    }

    pub fn difficulty_label(&self) -> String {
        let dimension = self.dimension();
        format!("{} {dimension}×{dimension}", self.difficulty().key())
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    pub fn mode_label(&self) -> &'static str {
        self.mode.as_str()
    }

    pub fn reward_chips(&self) -> Option<i64> {
        (self.mode == Mode::Daily).then(|| self.difficulty().chips())
    }

    pub fn puzzle_date(&self) -> NaiveDate {
        self.puzzle_date
    }

    pub fn board(&self) -> &[u8] {
        &self.active_board().tiles
    }

    pub fn moves(&self) -> u32 {
        self.active_board().moves
    }

    pub fn cursor(&self) -> usize {
        self.active_board().cursor
    }

    pub fn is_locked(&self, index: usize) -> bool {
        self.active_board().locked[index]
    }

    /// Which tiles the source reaches right now.
    pub fn lit(&self) -> Vec<bool> {
        lit_tiles(self.board(), self.dimension())
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn reset_pending(&self) -> bool {
        self.reset_pending.is_some()
    }

    pub fn win_reported(&self) -> bool {
        self.active_board().win_reported
    }

    pub fn is_solved(&self) -> bool {
        is_solved(self.board(), self.dimension())
    }

    pub fn has_started(&self) -> bool {
        self.moves() > 0
    }

    /// The seed the active board was generated from, so the share card can
    /// redraw the day's starting position.
    pub fn scramble_seed(&self) -> u64 {
        self.active_board().seed
    }

    pub fn first_unfinished_daily(&self) -> Option<usize> {
        self.daily_boards
            .iter()
            .position(|board| board.moves > 0 && !board.win_reported)
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
        self.message = self.board_message();
    }

    /// Roll the dailies forward when the UTC date changes under a live
    /// session. Returns true when it moved.
    pub fn ensure_current_daily(&mut self) -> bool {
        let today = Utc::now().date_naive();
        if self.puzzle_date == today {
            return false;
        }
        self.puzzle_date = today;
        self.daily_boards =
            DIFFICULTIES.map(|difficulty| fresh_board(difficulty, daily_seed(today, difficulty)));
        self.clear_reset_pending();
        if self.mode == Mode::Daily {
            self.message = "Today's Netwalk is ready.".to_string();
        }
        true
    }

    /// The one "which board is this" line, whichever key asked for it.
    fn board_message(&self) -> String {
        format!("{} {} board.", self.mode.title(), self.difficulty_label())
    }

    pub fn show_daily(&mut self) {
        self.clear_reset_pending();
        self.mode = Mode::Daily;
        self.message = self.board_message();
    }

    pub fn show_personal(&mut self) {
        self.clear_reset_pending();
        self.mode = Mode::Personal;
        let generated = self.ensure_personal_board();
        self.message = idle_message(Mode::Personal).to_string();
        if generated {
            self.save_async();
        }
    }

    pub fn new_personal_board(&mut self) {
        self.clear_reset_pending();
        self.mode = Mode::Personal;
        let difficulty = self.difficulty();
        self.personal_boards[self.selected_difficulty] =
            Some(fresh_board(difficulty, OsRng.next_u64()));
        self.message = "New personal network. No rewards.".to_string();
        self.save_async();
    }

    pub fn next_difficulty(&mut self) {
        self.select_difficulty((self.selected_difficulty + 1) % DIFFICULTIES.len());
    }

    pub fn prev_difficulty(&mut self) {
        self.select_difficulty(
            (self.selected_difficulty + DIFFICULTIES.len() - 1) % DIFFICULTIES.len(),
        );
    }

    fn select_difficulty(&mut self, index: usize) {
        self.clear_reset_pending();
        self.selected_difficulty = index;
        let generated = self.mode == Mode::Personal && self.ensure_personal_board();
        self.message = self.board_message();
        if generated {
            self.save_async();
        }
    }

    /// Two-press reset confirm. Refused outright on a solved daily: the win
    /// is already banked, so re-scrambling would only write the scramble
    /// over the solved row.
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
            "Press n again for a new personal network.",
        )
    }

    pub fn clear_reset_pending(&mut self) {
        if self.reset_pending.take().is_some() {
            self.message = idle_message(self.mode).to_string();
        }
    }

    /// Back to the board's own scramble: every tile as generated, no moves,
    /// no locks.
    pub fn reset(&mut self) {
        let difficulty = self.difficulty();
        let seed = self.active_board().seed;
        *self.active_board_mut() = fresh_board(difficulty, seed);
        self.reset_pending = None;
        self.message = match self.mode {
            Mode::Daily => "Today's scramble restored.".to_string(),
            Mode::Personal => "Personal scramble restored.".to_string(),
        };
        self.save_async();
    }

    /// Step the cursor one tile; it stops at the edge.
    pub fn move_cursor(&mut self, side: Side) -> bool {
        self.clear_reset_pending();
        let dimension = self.dimension();
        let board = self.active_board_mut();
        match neighbor(board.cursor, side, dimension) {
            Some(next) => {
                board.cursor = next;
                true
            }
            None => false,
        }
    }

    pub fn rotate_at_cursor(&mut self, turn: Turn) -> bool {
        self.rotate_tile(self.cursor(), turn)
    }

    /// A click: the cursor jumps to the tile and turns it clockwise.
    pub fn click(&mut self, index: usize) -> bool {
        if index >= self.board().len() {
            return false;
        }
        self.active_board_mut().cursor = index;
        self.rotate_tile(index, Turn::Clockwise)
    }

    /// `f`: a tile the player is sure of stops turning until unlocked.
    pub fn toggle_lock(&mut self) {
        self.clear_reset_pending();
        let board = self.active_board_mut();
        let cursor = board.cursor;
        board.locked[cursor] = !board.locked[cursor];
        let locked = board.locked[cursor];
        self.message = if locked {
            "Tile locked; f again unlocks it.".to_string()
        } else {
            "Tile unlocked.".to_string()
        };
    }

    fn rotate_tile(&mut self, index: usize, turn: Turn) -> bool {
        self.clear_reset_pending();
        if self.is_solved() {
            return false;
        }
        let dimension = self.dimension();
        let board = self.active_board_mut();
        if board.locked[index] {
            self.message = "That tile is locked; f unlocks it.".to_string();
            return false;
        }
        let current = board.tiles[index];
        let turned = rotate(current, turn);
        if turned == current {
            self.message = "That tile looks the same every way round.".to_string();
            return false;
        }
        board.tiles[index] = turned;
        board.moves = board.moves.saturating_add(1);
        let moves = board.moves;
        let solved = is_solved(&board.tiles, dimension);
        if solved {
            board.win_reported = true;
        }

        if solved {
            let moves = moves.min(i32::MAX as u32) as i32;
            let mode = match self.mode {
                Mode::Daily => ArcadeMode::Daily,
                Mode::Personal => ArcadeMode::Personal,
            };
            let difficulty = self.difficulty();
            let difficulty_label = match difficulty {
                Difficulty::Easy => ArcadeDifficulty::Easy,
                Difficulty::Medium => ArcadeDifficulty::Medium,
                Difficulty::Hard => ArcadeDifficulty::Hard,
            };
            self.svc
                .record_finish(mode, difficulty_label, ArcadeFinish::Won);
            match self.mode {
                Mode::Daily => {
                    self.message = format!("Connected in {moves} moves.");
                    self.complete_async(difficulty, moves);
                }
                Mode::Personal => {
                    self.message =
                        format!("Connected in {moves} moves. Personal boards have no reward.");
                    self.save_async();
                }
            }
        } else {
            self.message = format!("Move {moves}.");
            self.save_async();
        }
        true
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
        let board = self.active_board();
        GameParams {
            user_id: self.user_id,
            mode: self.mode.as_str().to_string(),
            difficulty_key: self.difficulty().key().to_string(),
            puzzle_date: (self.mode == Mode::Daily).then_some(self.puzzle_date),
            puzzle_seed: board.seed as i64,
            tiles: board.tiles.iter().copied().map(i32::from).collect(),
            moves: board.moves.min(i32::MAX as u32) as i32,
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
            self.ensure_personal_board();
        }
        let board = self.active_board_mut();
        board.locked = vec![false; tiles.len()];
        board.tiles = tiles;
        board.moves = moves;
        board.win_reported = false;
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

    fn ensure_personal_board(&mut self) -> bool {
        let index = self.selected_difficulty;
        if self.personal_boards[index].is_some() {
            return false;
        }
        self.personal_boards[index] = Some(fresh_board(self.difficulty(), OsRng.next_u64()));
        true
    }

    fn active_board(&self) -> &Board {
        let index = self.selected_difficulty;
        match self.mode {
            Mode::Daily => &self.daily_boards[index],
            Mode::Personal => self.personal_boards[index]
                .as_ref()
                .expect("personal board is generated before activation"),
        }
    }

    fn active_board_mut(&mut self) -> &mut Board {
        let index = self.selected_difficulty;
        match self.mode {
            Mode::Daily => &mut self.daily_boards[index],
            Mode::Personal => self.personal_boards[index]
                .as_mut()
                .expect("personal board is generated before activation"),
        }
    }
}

fn idle_message(mode: Mode) -> &'static str {
    match mode {
        Mode::Daily => "Turn tiles until every one is lit: Space turns, f locks.",
        Mode::Personal => "Personal board. No reward; n twice starts a new network.",
    }
}

fn fresh_board(difficulty: Difficulty, seed: u64) -> Board {
    let dimension = board_dimension(difficulty);
    Board {
        seed,
        tiles: generate(difficulty, seed).scrambled,
        moves: 0,
        win_reported: false,
        cursor: source_index(dimension),
        locked: vec![false; dimension * dimension],
    }
}

/// A saved row, or `None` when its tiles are not this seed's network turned
/// tile by tile: turning is the only move, so anything else is corrupt.
fn board_from_game(game: &Game, difficulty: Difficulty) -> Option<Board> {
    let dimension = board_dimension(difficulty);
    let seed = game.puzzle_seed as u64;
    let tiles = valid_tiles(&game.tiles, &generate(difficulty, seed).solution)?;
    let win_reported = is_solved(&tiles, dimension);
    Some(Board {
        seed,
        tiles,
        moves: game.moves.max(0) as u32,
        win_reported,
        cursor: source_index(dimension),
        locked: vec![false; dimension * dimension],
    })
}

fn valid_tiles(values: &[i32], solution: &[u8]) -> Option<Vec<u8>> {
    if values.len() != solution.len() {
        return None;
    }
    let mut tiles = Vec::with_capacity(values.len());
    for (&value, &mask) in values.iter().zip(solution) {
        let value = u8::try_from(value).ok()?;
        let mut turned = mask;
        let mut matches = false;
        for _ in 0..4 {
            if turned == value {
                matches = true;
            }
            turned = rotate(turned, Turn::Clockwise);
        }
        if !matches {
            return None;
        }
        tiles.push(value);
    }
    Some(tiles)
}

pub const fn board_dimension(difficulty: Difficulty) -> usize {
    match difficulty {
        Difficulty::Easy => 5,
        Difficulty::Medium => 7,
        Difficulty::Hard => 9,
    }
}

/// The source, the tile that powers the network: the centre of the board.
pub const fn source_index(dimension: usize) -> usize {
    (dimension / 2) * dimension + dimension / 2
}

/// The tile across `side` of `index`, or `None` at the board's edge. The
/// board does not wrap.
pub fn neighbor(index: usize, side: Side, dimension: usize) -> Option<usize> {
    let row = index / dimension;
    let column = index % dimension;
    match side {
        Side::North if row > 0 => Some(index - dimension),
        Side::South if row + 1 < dimension => Some(index + dimension),
        Side::West if column > 0 => Some(index - 1),
        Side::East if column + 1 < dimension => Some(index + 1),
        Side::North | Side::East | Side::South | Side::West => None,
    }
}

/// Which tiles the source reaches through connections that meet a matching
/// connection on the neighbouring tile.
pub fn lit_tiles(tiles: &[u8], dimension: usize) -> Vec<bool> {
    let source = source_index(dimension);
    let mut lit = vec![false; tiles.len()];
    lit[source] = true;
    let mut pending = vec![source];
    while let Some(index) = pending.pop() {
        for side in Side::ALL {
            if tiles[index] & side.bit() == 0 {
                continue;
            }
            let Some(next) = neighbor(index, side, dimension) else {
                continue;
            };
            if tiles[next] & side.opposite().bit() == 0 || lit[next] {
                continue;
            }
            lit[next] = true;
            pending.push(next);
        }
    }
    lit
}

/// Every tile lit. That is also "no connection left hanging": every board
/// is a spanning tree's tiles turned in place, so it has exactly 2(n-1)
/// connection ends, and lighting all n tiles takes n-1 matched pairs,
/// which uses every end.
pub fn is_solved(tiles: &[u8], dimension: usize) -> bool {
    !lit_tiles(tiles, dimension).contains(&false)
}

pub(crate) fn daily_seed(puzzle_date: NaiveDate, difficulty: Difficulty) -> u64 {
    let mut seed = 0xcbf2_9ce4_8422_2325u64;
    for byte in b"late-sh-netwalk-daily-v1"
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

/// The network for `seed` and its scramble. Deterministic: the daily board
/// is the same for everyone and is rebuilt from the seed on every load, so
/// changing anything here changes every saved board.
pub fn generate(difficulty: Difficulty, seed: u64) -> Puzzle {
    let dimension = board_dimension(difficulty);
    let mut rng = seed;
    let solution = spanning_tree(dimension, &mut rng);
    loop {
        let mut scrambled = Vec::with_capacity(solution.len());
        for &mask in &solution {
            let turns = next_seed(&mut rng) as usize % 4;
            let mut turned = mask;
            for _ in 0..turns {
                turned = rotate(turned, Turn::Clockwise);
            }
            scrambled.push(turned);
        }
        if !is_solved(&scrambled, dimension) {
            return Puzzle {
                solution,
                scrambled,
            };
        }
    }
}

/// A random spanning tree of the grid grown from the source (randomized
/// Prim): each step joins one random not-yet-joined tile across a random
/// edge of the tree's frontier. Every tile ends up on the network.
fn spanning_tree(dimension: usize, rng: &mut u64) -> Vec<u8> {
    let source = source_index(dimension);
    let mut masks = vec![0u8; dimension * dimension];
    let mut joined = vec![false; dimension * dimension];
    joined[source] = true;
    let mut frontier: Vec<(usize, Side)> = Side::ALL.iter().map(|side| (source, *side)).collect();
    while !frontier.is_empty() {
        let pick = next_seed(rng) as usize % frontier.len();
        let (from, side) = frontier.swap_remove(pick);
        let Some(to) = neighbor(from, side, dimension) else {
            continue;
        };
        if joined[to] {
            continue;
        }
        masks[from] |= side.bit();
        masks[to] |= side.opposite().bit();
        joined[to] = true;
        frontier.extend(Side::ALL.iter().map(|side| (to, *side)));
    }
    masks
}

/// One LCG step, returned through a splitmix64 finalizer, as Sliding
/// Puzzle's generator does: the raw state's low bits are periodic, and the
/// callers index with `%`.
fn next_seed(seed: &mut u64) -> u64 {
    *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
    let mut mixed = *seed;
    mixed = (mixed ^ (mixed >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    mixed = (mixed ^ (mixed >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    mixed ^ (mixed >> 31)
}

#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;
