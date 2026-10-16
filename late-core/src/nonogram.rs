use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use std::hash::{Hash, Hasher};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NonogramPuzzle {
    pub id: String,
    pub width: u16,
    pub height: u16,
    pub row_clues: Vec<Vec<u8>>,
    pub col_clues: Vec<Vec<u8>>,
    pub solution: Vec<Vec<u8>>,
    pub difficulty: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NonogramPack {
    pub size_key: String,
    pub width: u16,
    pub height: u16,
    pub puzzles: Vec<NonogramPuzzle>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NonogramPackIndexEntry {
    pub size_key: String,
    pub width: u16,
    pub height: u16,
    pub puzzle_count: usize,
    pub path: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NonogramPackIndex {
    pub version: u32,
    pub packs: Vec<NonogramPackIndexEntry>,
}

impl NonogramPuzzle {
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.solution.len() != usize::from(self.height) {
            anyhow::bail!(
                "puzzle {} height mismatch: expected {}, got {}",
                self.id,
                self.height,
                self.solution.len()
            );
        }

        for row in &self.solution {
            if row.len() != usize::from(self.width) {
                anyhow::bail!(
                    "puzzle {} width mismatch: expected {}, got {}",
                    self.id,
                    self.width,
                    row.len()
                );
            }

            if row.iter().any(|cell| *cell > 1) {
                anyhow::bail!("puzzle {} contains non-binary solution values", self.id);
            }
        }

        let (row_clues, col_clues) = derive_clues(&self.solution);
        if row_clues != self.row_clues {
            anyhow::bail!("puzzle {} row clues do not match solution", self.id);
        }
        if col_clues != self.col_clues {
            anyhow::bail!("puzzle {} column clues do not match solution", self.id);
        }

        Ok(())
    }
}

impl NonogramPack {
    pub fn validate(&self) -> anyhow::Result<()> {
        for puzzle in &self.puzzles {
            if puzzle.width != self.width || puzzle.height != self.height {
                anyhow::bail!(
                    "puzzle {} dimensions do not match pack {}",
                    puzzle.id,
                    self.size_key
                );
            }
            puzzle.validate()?;
        }
        Ok(())
    }

    pub fn select_for_date(&self, date: NaiveDate) -> Option<&NonogramPuzzle> {
        if self.puzzles.is_empty() {
            return None;
        }

        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.size_key.hash(&mut hasher);
        date.format("%Y-%m-%d").to_string().hash(&mut hasher);
        "late-sh-nonogram-daily".hash(&mut hasher);
        let idx = (hasher.finish() as usize) % self.puzzles.len();
        self.puzzles.get(idx)
    }
}

pub fn derive_clues(solution: &[Vec<u8>]) -> (Vec<Vec<u8>>, Vec<Vec<u8>>) {
    if solution.is_empty() {
        return (Vec::new(), Vec::new());
    }

    let row_clues = solution
        .iter()
        .map(|row| derive_line_clues(row.iter().copied()))
        .collect();

    let width = solution[0].len();
    let mut col_clues = Vec::with_capacity(width);
    for col in 0..width {
        let line = solution.iter().map(|row| row[col]);
        col_clues.push(derive_line_clues(line));
    }

    (row_clues, col_clues)
}

fn derive_line_clues<I>(cells: I) -> Vec<u8>
where
    I: IntoIterator<Item = u8>,
{
    let mut clues = Vec::new();
    let mut run = 0u8;

    for cell in cells {
        if cell == 1 {
            run = run.saturating_add(1);
        } else if run > 0 {
            clues.push(run);
            run = 0;
        }
    }

    if run > 0 {
        clues.push(run);
    }

    clues
}

/// A cell as line logic sees it: the solver's working grid, or a player's
/// marks mapped at the call site.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cell {
    Unknown,
    Filled,
    Empty,
}

/// Solves a puzzle by line logic alone: each row and column on its own is
/// checked against every placement of its clue that fits the cells known so
/// far, and a cell all those placements agree on is fixed. Passes repeat
/// until nothing moves. Returns the grid when that fixes every cell; since
/// each cell was forced, it is then also the only answer. `None` when the
/// clues contradict themselves, or the logic stalls with cells still open,
/// so that finishing would take a guess.
pub fn solve_by_lines(row_clues: &[Vec<u8>], col_clues: &[Vec<u8>]) -> Option<Vec<Vec<u8>>> {
    let width = col_clues.len();
    let mut grid = vec![vec![Cell::Unknown; width]; row_clues.len()];
    loop {
        let mut moved = false;
        for (row, clues) in row_clues.iter().enumerate() {
            let line = grid[row].clone();
            let solved = solve_line(clues, &line)?;
            if solved != line {
                grid[row] = solved;
                moved = true;
            }
        }
        for (col, clues) in col_clues.iter().enumerate() {
            let line: Vec<Cell> = grid.iter().map(|row| row[col]).collect();
            let solved = solve_line(clues, &line)?;
            if solved != line {
                for (row, cell) in solved.into_iter().enumerate() {
                    grid[row][col] = cell;
                }
                moved = true;
            }
        }
        if !moved {
            break;
        }
    }
    grid.into_iter()
        .map(|row| {
            row.into_iter()
                .map(|cell| match cell {
                    Cell::Filled => Some(1),
                    Cell::Empty => Some(0),
                    Cell::Unknown => None,
                })
                .collect()
        })
        .collect()
}

/// Whether some placement of `clues` fits the known cells of `line`, so the
/// line can still be finished from here.
pub fn line_fits(clues: &[u8], line: &[Cell]) -> bool {
    solve_line(clues, line).is_some()
}

/// Every placement of `clues` that fits the known cells of `line`, folded
/// into what they all agree on: a cell filled in every placement is filled,
/// empty in every placement is empty, anything else stays unknown. `None`
/// when no placement fits.
fn solve_line(clues: &[u8], line: &[Cell]) -> Option<Vec<Cell>> {
    let n = line.len();
    let k = clues.len();
    let blocks: Vec<usize> = clues.iter().map(|&clue| usize::from(clue)).collect();
    let can_empty = |i: usize| line[i] != Cell::Filled;
    let mut empties_before = vec![0usize; n + 1];
    for i in 0..n {
        empties_before[i + 1] = empties_before[i] + usize::from(line[i] == Cell::Empty);
    }
    // Block `j` can sit on cells `start..start + len` when none is known empty.
    let block_fits = |start: usize, len: usize| {
        start + len <= n && empties_before[start + len] == empties_before[start]
    };

    // head[i][j]: cells `..i` hold exactly blocks `..j`.
    let mut head = vec![vec![false; k + 1]; n + 1];
    head[0][0] = true;
    for i in 1..=n {
        for j in 0..=k {
            let gap = can_empty(i - 1) && head[i - 1][j];
            let block_ends_here = j > 0 && {
                let len = blocks[j - 1];
                i >= len && block_fits(i - len, len) && {
                    let start = i - len;
                    match start {
                        0 => j == 1,
                        _ => can_empty(start - 1) && head[start - 1][j - 1],
                    }
                }
            };
            head[i][j] = gap || block_ends_here;
        }
    }
    if !head[n][k] {
        return None;
    }

    // tail[i][j]: cells `i..` hold exactly blocks `j..`.
    let mut tail = vec![vec![false; k + 1]; n + 1];
    tail[n][k] = true;
    for i in (0..n).rev() {
        for j in 0..=k {
            let gap = can_empty(i) && tail[i + 1][j];
            let block_starts_here = j < k && {
                let end = i + blocks[j];
                block_fits(i, blocks[j])
                    && match end == n {
                        true => j + 1 == k,
                        false => can_empty(end) && tail[end + 1][j + 1],
                    }
            };
            tail[i][j] = gap || block_starts_here;
        }
    }

    let mut can_fill = vec![false; n];
    let mut may_be_empty = vec![false; n];
    for i in 0..n {
        may_be_empty[i] = can_empty(i) && (0..=k).any(|j| head[i][j] && tail[i + 1][j]);
    }
    for (j, &len) in blocks.iter().enumerate() {
        for start in 0..n {
            if !block_fits(start, len) {
                continue;
            }
            let before = match start {
                0 => j == 0,
                _ => can_empty(start - 1) && head[start - 1][j],
            };
            let end = start + len;
            let after = match end == n {
                true => j + 1 == k,
                false => can_empty(end) && tail[end + 1][j + 1],
            };
            if before && after {
                can_fill[start..end].fill(true);
            }
        }
    }

    Some(
        (0..n)
            .map(|i| match (can_fill[i], may_be_empty[i]) {
                (true, true) => Cell::Unknown,
                (true, false) => Cell::Filled,
                (false, true) => Cell::Empty,
                (false, false) => unreachable!("a line with a placement gives every cell a value"),
            })
            .collect(),
    )
}

#[cfg(test)]
#[path = "nonogram_test.rs"]
mod nonogram_test;
