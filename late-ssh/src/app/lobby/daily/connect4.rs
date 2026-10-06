//! Connect Four rules for daily correspondence matches. Pure state + logic,
//! no I/O: the service persists `DailyConnect4State` as the match's `state`
//! JSON the same way chess and battleship persist theirs.
//!
//! The state stores only the drop history; the grid, whose turn it is, and
//! the move count are all derived from it, so the state can never
//! self-contradict. Red is decided at claim time and always moves first.
//! Unlike battleship, connect four can draw: 42 drops with no line.

use anyhow::{Context, Result, bail, ensure};
use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

pub const COLS: usize = 7;
pub const ROWS: usize = 6;
pub const CELLS: usize = COLS * ROWS;
const STATE_VERSION: u8 = 1;

/// Row 0 is the bottom of the board.
pub type Grid = [[Option<Disc>; COLS]; ROWS];

/// `0 -> a`, `6 -> g`: chess-file style column names.
pub fn column_label(column: usize) -> char {
    (b'a' + column as u8) as char
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Disc {
    Red,
    Yellow,
}

impl Disc {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Red => "red",
            Self::Yellow => "yellow",
        }
    }

    pub const fn other(self) -> Self {
        match self {
            Self::Red => Self::Yellow,
            Self::Yellow => Self::Red,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DropOutcome {
    /// Row the disc landed in (0 = bottom); the renderer highlights it.
    pub row: usize,
    pub disc: Disc,
    /// Four in a row: the dropper wins.
    pub connected: bool,
    /// Board full with no line: nobody wins.
    pub draw: bool,
}

impl DropOutcome {
    /// `d3` / `d3, four in a row` / `g6, board full` — the move-feed label.
    pub fn label(&self, column: usize) -> String {
        let spot = format!("{}{}", column_label(column), self.row + 1);
        if self.connected {
            format!("{spot}, four in a row")
        } else if self.draw {
            format!("{spot}, board full")
        } else {
            spot
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DailyConnect4State {
    pub version: u8,
    #[serde(default)]
    pub revision: u64,
    /// Red moves first; colors are assigned randomly at claim time.
    pub red: Uuid,
    pub yellow: Uuid,
    /// Columns in play order. Even indices are red's drops.
    pub drops: Vec<u8>,
}

impl DailyConnect4State {
    pub fn new(challenger: Uuid, claimer: Uuid) -> Self {
        let (red, yellow) = if rand::thread_rng().gen_bool(0.5) {
            (challenger, claimer)
        } else {
            (claimer, challenger)
        };
        Self {
            version: STATE_VERSION,
            revision: 0,
            red,
            yellow,
            drops: Vec::new(),
        }
    }

    pub fn parse(value: &Value) -> Result<Self> {
        let state: Self =
            serde_json::from_value(value.clone()).context("corrupt daily match state")?;
        ensure!(
            state.version == STATE_VERSION,
            "unsupported daily connect four state version: {}",
            state.version
        );
        Ok(state)
    }

    /// Whose disc drops next.
    pub fn turn(&self) -> Disc {
        if self.drops.len().is_multiple_of(2) {
            Disc::Red
        } else {
            Disc::Yellow
        }
    }

    pub fn user_of(&self, disc: Disc) -> Uuid {
        match disc {
            Disc::Red => self.red,
            Disc::Yellow => self.yellow,
        }
    }

    pub fn disc_of(&self, user_id: Uuid) -> Option<Disc> {
        if user_id == self.red {
            Some(Disc::Red)
        } else if user_id == self.yellow {
            Some(Disc::Yellow)
        } else {
            None
        }
    }

    /// Rebuild the board from the drop history.
    pub fn grid(&self) -> Grid {
        let mut grid = [[None; COLS]; ROWS];
        let mut heights = [0usize; COLS];
        for (index, &column) in self.drops.iter().enumerate() {
            let column = column as usize;
            let disc = if index.is_multiple_of(2) {
                Disc::Red
            } else {
                Disc::Yellow
            };
            grid[heights[column]][column] = Some(disc);
            heights[column] += 1;
        }
        grid
    }

    /// `(row, column)` of the most recent drop, for highlighting.
    pub fn last_drop(&self) -> Option<(usize, usize)> {
        let &column = self.drops.last()?;
        let row = self
            .drops
            .iter()
            .filter(|&&c| c == column)
            .count()
            .saturating_sub(1);
        Some((row, column as usize))
    }

    pub fn move_count(&self) -> usize {
        self.drops.len()
    }

    /// The cells of the winning run through the last drop, if any — the
    /// renderer lights them up once the match ends.
    pub fn winning_line(&self) -> Option<Vec<(usize, usize)>> {
        let (row, column) = self.last_drop()?;
        let grid = self.grid();
        let disc = grid[row][column]?;
        DIRECTIONS.into_iter().find_map(|(dr, dc)| {
            let mut cells = run_cells(&grid, row, column, -dr, -dc, disc);
            cells.reverse();
            cells.push((row, column));
            cells.extend(run_cells(&grid, row, column, dr, dc, disc));
            (cells.len() >= 4).then_some(cells)
        })
    }

    /// Drop the current player's disc. Validates bounds and full columns;
    /// the caller owns turn order and match status.
    pub fn apply_drop(&mut self, column: usize) -> Result<DropOutcome> {
        ensure!(column < COLS, "that column is off the board");
        let mut grid = self.grid();
        let Some(row) = (0..ROWS).find(|&row| grid[row][column].is_none()) else {
            bail!("column {} is full", column_label(column));
        };
        let disc = self.turn();
        grid[row][column] = Some(disc);
        self.drops.push(column as u8);
        let connected = connects_four(&grid, row, column, disc);
        Ok(DropOutcome {
            row,
            disc,
            connected,
            draw: !connected && self.drops.len() == CELLS,
        })
    }
}

const DIRECTIONS: [(isize, isize); 4] = [(0, 1), (1, 0), (1, 1), (1, -1)];

/// A winning line must pass through the last drop, so scan the four
/// directions outward from it.
fn connects_four(grid: &Grid, row: usize, column: usize, disc: Disc) -> bool {
    DIRECTIONS.into_iter().any(|(dr, dc)| {
        let run = 1
            + run_cells(grid, row, column, dr, dc, disc).len()
            + run_cells(grid, row, column, -dr, -dc, disc).len();
        run >= 4
    })
}

/// Same-disc cells walking from (row, column) exclusive, in walk order.
fn run_cells(
    grid: &Grid,
    row: usize,
    column: usize,
    dr: isize,
    dc: isize,
    disc: Disc,
) -> Vec<(usize, usize)> {
    let mut cells = Vec::new();
    let (mut row, mut column) = (row as isize, column as isize);
    loop {
        row += dr;
        column += dc;
        if !(0..ROWS as isize).contains(&row) || !(0..COLS as isize).contains(&column) {
            return cells;
        }
        if grid[row as usize][column as usize] != Some(disc) {
            return cells;
        }
        cells.push((row as usize, column as usize));
    }
}

#[cfg(test)]
#[path = "connect4_test.rs"]
mod connect4_test;
