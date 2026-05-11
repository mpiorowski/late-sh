use rand_core::RngCore;
use serde_json::Value;
use uuid::Uuid;

use super::svc::TwentyFortyEightService;

/// Represents the 4x4 grid. 0 means empty.
pub type Grid = [[u32; 4]; 4];

pub struct State {
    pub user_id: Uuid,
    pub score: i32,
    pub best_score: i32,
    pub grid: Grid,
    pub is_game_over: bool,
    pub svc: TwentyFortyEightService,
    // We use a simple PRNG to spawn new tiles (2 or 4)
    rng: rand_core::OsRng,
}

impl State {
    pub fn new(user_id: Uuid, svc: TwentyFortyEightService, best_score: i32) -> Self {
        let mut state = Self {
            user_id,
            score: 0,
            best_score,
            grid: [[0; 4]; 4],
            is_game_over: false,
            svc,
            rng: rand_core::OsRng,
        };
        // 2048 starts with two tiles
        state.spawn_tile();
        state.spawn_tile();
        state
    }

    /// Rebuilds the game state from the DB model (JSONB grid)
    pub fn restore(
        user_id: Uuid,
        svc: TwentyFortyEightService,
        score: i32,
        best_score: i32,
        grid_val: Value,
        is_game_over: bool,
    ) -> Self {
        let mut grid = [[0; 4]; 4];
        if let Some(arr) = grid_val.as_array() {
            for (r, row_val) in arr.iter().enumerate().take(4) {
                if let Some(row_arr) = row_val.as_array() {
                    for (c, cell_val) in row_arr.iter().enumerate().take(4) {
                        grid[r][c] = cell_val.as_u64().unwrap_or(0) as u32;
                    }
                }
            }
        }

        Self {
            user_id,
            score,
            best_score: best_score.max(score),
            grid,
            is_game_over,
            svc,
            rng: rand_core::OsRng,
        }
    }

    /// Serializes the grid for the DB
    pub fn grid_to_value(&self) -> Value {
        serde_json::to_value(self.grid).unwrap_or(serde_json::json!([]))
    }

    /// Start a completely new game, saving over the old one
    pub fn reset(&mut self) {
        self.score = 0;
        self.grid = [[0; 4]; 4];
        self.is_game_over = false;
        self.spawn_tile();
        self.spawn_tile();

        self.svc.save_game_task(
            self.user_id,
            self.score,
            self.grid_to_value(),
            self.is_game_over,
        );
    }

    fn spawn_tile(&mut self) {
        let mut empty_cells = Vec::new();
        for r in 0..4 {
            for c in 0..4 {
                if self.grid[r][c] == 0 {
                    empty_cells.push((r, c));
                }
            }
        }

        if empty_cells.is_empty() {
            return;
        }

        let idx = (self.rng.next_u32() as usize) % empty_cells.len();
        let (r, c) = empty_cells[idx];

        // 10% chance of a 4, 90% chance of a 2
        let value = if self.rng.next_u32().is_multiple_of(10) {
            4
        } else {
            2
        };
        self.grid[r][c] = value;
    }

    fn check_game_over(&mut self) {
        // Any empty spaces?
        for r in 0..4 {
            for c in 0..4 {
                if self.grid[r][c] == 0 {
                    return;
                }
            }
        }

        // Any adjacent matching spaces?
        for r in 0..4 {
            for c in 0..4 {
                let val = self.grid[r][c];
                if (r < 3 && self.grid[r + 1][c] == val) || (c < 3 && self.grid[r][c + 1] == val) {
                    return;
                }
            }
        }

        self.is_game_over = true;
        self.svc.submit_score_task(self.user_id, self.score, true);
    }

    // --- Movement Logic ---

    pub fn move_up(&mut self) -> bool {
        self.shift(|state| {
            let mut moved = false;
            for c in 0..4 {
                let mut col = [
                    state.grid[0][c],
                    state.grid[1][c],
                    state.grid[2][c],
                    state.grid[3][c],
                ];
                if shift_and_merge(&mut col, &mut state.score) {
                    moved = true;
                    for (r, value) in col.iter().enumerate() {
                        state.grid[r][c] = *value;
                    }
                }
            }
            moved
        })
    }

    pub fn move_down(&mut self) -> bool {
        self.shift(|state| {
            let mut moved = false;
            for c in 0..4 {
                let mut col = [
                    state.grid[3][c],
                    state.grid[2][c],
                    state.grid[1][c],
                    state.grid[0][c],
                ];
                if shift_and_merge(&mut col, &mut state.score) {
                    moved = true;
                    state.grid[3][c] = col[0];
                    state.grid[2][c] = col[1];
                    state.grid[1][c] = col[2];
                    state.grid[0][c] = col[3];
                }
            }
            moved
        })
    }

    pub fn move_left(&mut self) -> bool {
        self.shift(|state| {
            let mut moved = false;
            for r in 0..4 {
                let mut row = state.grid[r];
                if shift_and_merge(&mut row, &mut state.score) {
                    moved = true;
                    state.grid[r] = row;
                }
            }
            moved
        })
    }

    pub fn move_right(&mut self) -> bool {
        self.shift(|state| {
            let mut moved = false;
            for r in 0..4 {
                let mut row = [
                    state.grid[r][3],
                    state.grid[r][2],
                    state.grid[r][1],
                    state.grid[r][0],
                ];
                if shift_and_merge(&mut row, &mut state.score) {
                    moved = true;
                    state.grid[r][3] = row[0];
                    state.grid[r][2] = row[1];
                    state.grid[r][1] = row[2];
                    state.grid[r][0] = row[3];
                }
            }
            moved
        })
    }

    /// Common wrapper for all directional moves. Handles spawning new tiles, game over checks, and saving.
    fn shift<F>(&mut self, shift_fn: F) -> bool
    where
        F: FnOnce(&mut Self) -> bool,
    {
        if self.is_game_over {
            return false;
        }

        let moved = shift_fn(self);

        if moved {
            self.best_score = self.best_score.max(self.score);
            self.spawn_tile();
            self.check_game_over();

            // Fire-and-forget DB save
            self.svc.save_game_task(
                self.user_id,
                self.score,
                self.grid_to_value(),
                self.is_game_over,
            );

            // Also proactively submit score if it went up significantly, though Game Over guarantees it
            if self.score > 0 && self.score % 100 == 0 {
                self.svc.submit_score_task(self.user_id, self.score, false);
            }
        }

        moved
    }
}

/// Helper that shifts non-zeroes to the left, merges identical adjacent values, and updates score.
/// Returns true if the array changed.
fn shift_and_merge(line: &mut [u32; 4], score: &mut i32) -> bool {
    let mut changed = false;

    // 1. Shift non-zeros to the left
    let mut insert_pos = 0;
    for i in 0..4 {
        if line[i] != 0 {
            if i != insert_pos {
                line[insert_pos] = line[i];
                line[i] = 0;
                changed = true;
            }
            insert_pos += 1;
        }
    }

    // 2. Merge adjacent equals
    for i in 0..3 {
        if line[i] != 0 && line[i] == line[i + 1] {
            line[i] *= 2;
            *score += line[i] as i32;
            line[i + 1] = 0;
            changed = true;
        }
    }

    // 3. Shift left again to cover gaps left by merging
    insert_pos = 0;
    for i in 0..4 {
        if line[i] != 0 {
            if i != insert_pos {
                line[insert_pos] = line[i];
                line[i] = 0;
                changed = true;
            }
            insert_pos += 1;
        }
    }

    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shift_and_merge_moves_and_merges_once_per_pair() {
        let mut score = 0;

        // Shift left
        let mut line = [0, 2, 0, 2];
        assert!(shift_and_merge(&mut line, &mut score));
        assert_eq!(line, [4, 0, 0, 0]);
        assert_eq!(score, 4);

        // Merge multiples
        let mut line = [2, 2, 2, 2];
        assert!(shift_and_merge(&mut line, &mut score));
        assert_eq!(line, [4, 4, 0, 0]);

        // Don't merge cascaded
        let mut line = [2, 2, 4, 8];
        assert!(shift_and_merge(&mut line, &mut score));
        assert_eq!(line, [4, 4, 8, 0]);

        // No change
        let mut line = [2, 4, 8, 16];
        assert!(!shift_and_merge(&mut line, &mut score));
        assert_eq!(line, [2, 4, 8, 16]);
    }

    #[test]
    fn shift_and_merge_does_not_chain_merge_in_single_move() {
        let mut score = 0;
        let mut line = [4, 4, 4, 0];

        assert!(shift_and_merge(&mut line, &mut score));
        assert_eq!(line, [8, 4, 0, 0]);
        assert_eq!(score, 8);
    }

    #[test]
    fn shift_and_merge_accumulates_score_for_two_merges() {
        let mut score = 0;
        let mut line = [8, 8, 16, 16];

        assert!(shift_and_merge(&mut line, &mut score));
        assert_eq!(line, [16, 32, 0, 0]);
        assert_eq!(score, 48);
    }
}
