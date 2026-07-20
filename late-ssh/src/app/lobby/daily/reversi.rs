//! Reversi (Othello) rules for daily correspondence matches. Pure state +
//! logic, no I/O: the service persists `DailyReversiState` as the match's
//! `state` JSON the same way chess, battleship, and connect four persist
//! theirs.
//!
//! The state stores only the placement history; the board, the disc counts,
//! and whose turn it is are all derived by replaying it from the fixed
//! four-disc opening, so the state can never self-contradict. Forced passes
//! (a player with no legal move) are NOT stored — they fall out of the replay,
//! so there is exactly one way to represent a position. Black is decided at
//! claim time and always moves first. Like connect four, reversi can draw: a
//! finished board with an equal disc count belongs to nobody.

use anyhow::{Context, Result, ensure};
use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::cmp::Ordering;
use uuid::Uuid;

pub const SIZE: usize = 8;
pub const CELLS: usize = SIZE * SIZE;
const STATE_VERSION: u8 = 1;

/// Row 0 is the bottom rank ("1"), column 0 is the `a` file.
pub type Grid = [[Option<Disc>; SIZE]; SIZE];

/// `(0, 0) -> a1`, `(7, 7) -> h8`: chess-style square names.
pub fn cell_label(row: usize, col: usize) -> String {
    format!("{}{}", (b'a' + col as u8) as char, row + 1)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Disc {
    Black,
    White,
}

impl Disc {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Black => "black",
            Self::White => "white",
        }
    }

    pub const fn other(self) -> Self {
        match self {
            Self::Black => Self::White,
            Self::White => Self::Black,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MoveOutcome {
    /// Whose disc was placed (the player who was actually on the clock — a
    /// pending forced pass is already resolved by the time this fires).
    pub disc: Disc,
    /// Opponent discs turned over by this move; the renderer highlights them.
    pub flipped: Vec<(usize, usize)>,
    /// Neither side can move afterward (board full or double-blocked): the
    /// match ends.
    pub finished: bool,
    /// Finished with an equal disc count: nobody wins, nobody is paid.
    pub draw: bool,
    /// Majority holder when finished; `None` while running or on a draw.
    pub winner: Option<Disc>,
}

impl MoveOutcome {
    /// `d3` / `d3, black wins` / `c1, draw` — the move-feed label, matching the
    /// terse connect-four style (plain square while running, result on finish).
    pub fn label(&self, row: usize, col: usize) -> String {
        let spot = cell_label(row, col);
        if !self.finished {
            return spot;
        }
        match self.winner {
            Some(disc) => format!("{spot}, {} wins", disc.label()),
            None => format!("{spot}, draw"),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DailyReversiState {
    pub version: u8,
    #[serde(default)]
    pub revision: u64,
    /// Black moves first; colors are assigned randomly at claim time.
    pub black: Uuid,
    pub white: Uuid,
    /// Placed squares in play order (`row * 8 + col`). Forced passes are never
    /// recorded — they are recovered during replay.
    pub moves: Vec<u8>,
}

impl DailyReversiState {
    pub fn new(challenger: Uuid, claimer: Uuid) -> Self {
        let (black, white) = if rand::thread_rng().gen_bool(0.5) {
            (challenger, claimer)
        } else {
            (claimer, challenger)
        };
        Self {
            version: STATE_VERSION,
            revision: 0,
            black,
            white,
            moves: Vec::new(),
        }
    }

    pub fn parse(value: &Value) -> Result<Self> {
        let state: Self =
            serde_json::from_value(value.clone()).context("corrupt daily match state")?;
        ensure!(
            state.version == STATE_VERSION,
            "unsupported daily reversi state version: {}",
            state.version
        );
        Ok(state)
    }

    pub fn user_of(&self, disc: Disc) -> Uuid {
        match disc {
            Disc::Black => self.black,
            Disc::White => self.white,
        }
    }

    pub fn disc_of(&self, user_id: Uuid) -> Option<Disc> {
        if user_id == self.black {
            Some(Disc::Black)
        } else if user_id == self.white {
            Some(Disc::White)
        } else {
            None
        }
    }

    /// Rebuild the board from the placement history, applying each move's flips
    /// and skipping a player who had no legal move at that point.
    pub fn grid(&self) -> Grid {
        self.replay().0
    }

    /// Whose disc drops next, accounting for a forced pass. Meaningful only
    /// while the match runs; callers gate on [`Self::is_finished`] first.
    pub fn turn(&self) -> Disc {
        self.replay().1
    }

    /// True once neither player can move (board full or double-blocked).
    pub fn is_finished(&self) -> bool {
        let grid = self.grid();
        legal_cells(&grid, Disc::Black).is_empty() && legal_cells(&grid, Disc::White).is_empty()
    }

    /// `(black, white)` disc counts on the current board.
    pub fn disc_counts(&self) -> (usize, usize) {
        count(&self.grid())
    }

    /// The squares the given disc may legally play right now.
    pub fn legal_moves(&self, disc: Disc) -> Vec<(usize, usize)> {
        legal_cells(&self.grid(), disc)
    }

    /// The discs a hypothetical move would flip — for the renderer's ghost
    /// preview under the cursor. Empty if the square is taken or illegal.
    pub fn preview_flips(&self, row: usize, col: usize, disc: Disc) -> Vec<(usize, usize)> {
        let grid = self.grid();
        if row >= SIZE || col >= SIZE || grid[row][col].is_some() {
            return Vec::new();
        }
        flips_for(&grid, row, col, disc)
    }

    /// `(row, col)` of the most recent placement, for highlighting.
    pub fn last_move(&self) -> Option<(usize, usize)> {
        let &cell = self.moves.last()?;
        Some((cell as usize / SIZE, cell as usize % SIZE))
    }

    pub fn move_count(&self) -> usize {
        self.moves.len()
    }

    /// Place the current player's disc. Validates bounds, occupancy, and that
    /// the move flips at least one disc; the caller owns turn order and match
    /// status. Returns what flipped and whether the match is now over.
    pub fn apply_move(&mut self, row: usize, col: usize) -> Result<MoveOutcome> {
        ensure!(row < SIZE && col < SIZE, "that square is off the board");
        let (mut grid, disc) = self.replay();
        ensure!(
            grid[row][col].is_none(),
            "{} is already taken",
            cell_label(row, col)
        );
        let flipped = flips_for(&grid, row, col, disc);
        ensure!(
            !flipped.is_empty(),
            "{} flips nothing",
            cell_label(row, col)
        );

        grid[row][col] = Some(disc);
        for &(r, c) in &flipped {
            grid[r][c] = Some(disc);
        }
        self.moves.push((row * SIZE + col) as u8);

        let finished = legal_cells(&grid, Disc::Black).is_empty()
            && legal_cells(&grid, Disc::White).is_empty();
        let (winner, draw) = if finished {
            let (black, white) = count(&grid);
            match black.cmp(&white) {
                Ordering::Greater => (Some(Disc::Black), false),
                Ordering::Less => (Some(Disc::White), false),
                Ordering::Equal => (None, true),
            }
        } else {
            (None, false)
        };

        Ok(MoveOutcome {
            disc,
            flipped,
            finished,
            draw,
            winner,
        })
    }

    /// Replay the history into `(board, player-to-move)`. The returned disc is
    /// the side actually on the clock at the current position: if the natural
    /// next player has no move but the other does, the pass is applied here.
    fn replay(&self) -> (Grid, Disc) {
        let mut grid = starting_grid();
        let mut turn = Disc::Black;
        for &cell in &self.moves {
            // A legal history never stores the passing player's non-move, so if
            // the side to move can't play, the stored move is the other side's.
            if legal_cells(&grid, turn).is_empty() {
                turn = turn.other();
            }
            let (row, col) = (cell as usize / SIZE, cell as usize % SIZE);
            for (r, c) in flips_for(&grid, row, col, turn) {
                grid[r][c] = Some(turn);
            }
            grid[row][col] = Some(turn);
            turn = turn.other();
        }
        if legal_cells(&grid, turn).is_empty() && !legal_cells(&grid, turn.other()).is_empty() {
            turn = turn.other();
        }
        (grid, turn)
    }
}

/// The fixed four-disc Othello opening: same-colored discs on each diagonal,
/// black to move.
fn starting_grid() -> Grid {
    let mut grid = [[None; SIZE]; SIZE];
    grid[3][3] = Some(Disc::White);
    grid[3][4] = Some(Disc::Black);
    grid[4][3] = Some(Disc::Black);
    grid[4][4] = Some(Disc::White);
    grid
}

const DIRECTIONS: [(isize, isize); 8] = [
    (-1, -1),
    (-1, 0),
    (-1, 1),
    (0, -1),
    (0, 1),
    (1, -1),
    (1, 0),
    (1, 1),
];

/// The opponent discs that placing `disc` at `(row, col)` would flip. Empty if
/// the square is off the board, occupied, or brackets nothing (an illegal
/// move). A legal move flips at least one disc.
fn flips_for(grid: &Grid, row: usize, col: usize, disc: Disc) -> Vec<(usize, usize)> {
    if row >= SIZE || col >= SIZE || grid[row][col].is_some() {
        return Vec::new();
    }
    let mut flips = Vec::new();
    for (dr, dc) in DIRECTIONS {
        let mut line = Vec::new();
        let (mut r, mut c) = (row as isize + dr, col as isize + dc);
        while (0..SIZE as isize).contains(&r) && (0..SIZE as isize).contains(&c) {
            match grid[r as usize][c as usize] {
                Some(d) if d == disc.other() => line.push((r as usize, c as usize)),
                // Own disc closes the bracket: the run between flips.
                Some(_) => {
                    flips.extend(line);
                    break;
                }
                // Empty square (or edge, handled by the loop guard): no bracket.
                None => break,
            }
            r += dr;
            c += dc;
        }
    }
    flips
}

/// Every square where `disc` has a legal move, in row-major order.
fn legal_cells(grid: &Grid, disc: Disc) -> Vec<(usize, usize)> {
    let mut cells = Vec::new();
    for row in 0..SIZE {
        for col in 0..SIZE {
            if grid[row][col].is_none() && !flips_for(grid, row, col, disc).is_empty() {
                cells.push((row, col));
            }
        }
    }
    cells
}

/// `(black, white)` disc totals on the board.
fn count(grid: &Grid) -> (usize, usize) {
    let (mut black, mut white) = (0, 0);
    for row in grid {
        for cell in row {
            match cell {
                Some(Disc::Black) => black += 1,
                Some(Disc::White) => white += 1,
                None => {}
            }
        }
    }
    (black, white)
}

#[cfg(test)]
#[path = "reversi_test.rs"]
mod reversi_test;

