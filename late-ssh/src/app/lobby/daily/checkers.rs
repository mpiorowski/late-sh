//! Checkers (American / English draughts) rules for daily correspondence
//! matches. Pure state + logic, no I/O: the service persists
//! `DailyCheckersState` as the match's `state` JSON the same way chess,
//! battleship, connect four, and reversi persist theirs.
//!
//! Like reversi, the state stores only the move history and derives the board,
//! the piece counts, whose turn it is, and the draw clock by replaying it from
//! the fixed opening. Checkers has no passes (a player with no legal move
//! simply loses), so the turn is plain parity — red moves first. The three
//! rules wrinkles all live in this module: captures are mandatory (a simple
//! move is illegal while any jump exists), a jump chain must be played to
//! completion (a partial chain never matches a legal move), and a man that
//! reaches the far rank is crowned and its turn ends immediately, even mid-jump.
//! Like connect four and reversi, checkers can draw.

use anyhow::{Context, Result, ensure};
use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

pub const SIZE: usize = 8;
pub const CELLS: usize = SIZE * SIZE;
const STATE_VERSION: u8 = 1;
/// Plies (half-moves) with no capture and no man move before the match is
/// declared a draw: the standard forty-move rule, forty per side.
const DRAW_PLIES: usize = 80;

/// Row 0 is red's back rank, row 7 is white's. Column 0 is the `a` file. Play
/// happens on the dark squares only (`(row + col)` odd).
pub type Grid = [[Option<Piece>; SIZE]; SIZE];

/// `(0, 0) -> a1`, `(7, 7) -> h8`: chess-style square names.
pub fn cell_label(row: usize, col: usize) -> String {
    format!("{}{}", (b'a' + col as u8) as char, row + 1)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Color {
    Red,
    White,
}

impl Color {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Red => "red",
            Self::White => "white",
        }
    }

    pub const fn other(self) -> Self {
        match self {
            Self::Red => Self::White,
            Self::White => Self::Red,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Piece {
    pub color: Color,
    pub king: bool,
}

/// A finished match's verdict, derived by replay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckersStatus {
    Ongoing,
    Win(Color),
    Draw,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MoveOutcome {
    /// Who moved.
    pub color: Color,
    /// Squares cleared by this move's jumps (empty for a simple slide); the
    /// renderer fades them out.
    pub captured: Vec<(usize, usize)>,
    /// A man reached the far rank and was crowned this move.
    pub crowned: bool,
    /// The match is now over (opponent has no move, or the draw clock ran out).
    pub finished: bool,
    /// Finished by the forty-move rule: nobody wins, nobody is paid.
    pub draw: bool,
    /// The winner when decisive; `None` while running or on a draw.
    pub winner: Option<Color>,
}

impl MoveOutcome {
    /// `b3-c4` (slide) / `b3xd5` (jump) / `b3xd5xf7` (chain), a `(K)` suffix on
    /// a crowning, and the result appended on the finishing move.
    pub fn label(&self, path: &[(usize, usize)]) -> String {
        let sep = if self.captured.is_empty() { "-" } else { "x" };
        let mut out = path
            .iter()
            .map(|&(row, col)| cell_label(row, col))
            .collect::<Vec<_>>()
            .join(sep);
        if self.crowned {
            out.push_str("(K)");
        }
        if self.finished {
            match self.winner {
                Some(color) => out.push_str(&format!(", {} wins", color.label())),
                None => out.push_str(", draw"),
            }
        }
        out
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DailyCheckersState {
    pub version: u8,
    #[serde(default)]
    pub revision: u64,
    /// Red moves first; colors are assigned randomly at claim time.
    pub red: Uuid,
    pub white: Uuid,
    /// Each turn is a path of squares (`row * 8 + col`) the moving piece
    /// visited: two squares for a slide, three or more for a jump chain.
    pub moves: Vec<Vec<u8>>,
}

impl DailyCheckersState {
    pub fn new(challenger: Uuid, claimer: Uuid) -> Self {
        let (red, white) = if rand::thread_rng().gen_bool(0.5) {
            (challenger, claimer)
        } else {
            (claimer, challenger)
        };
        Self {
            version: STATE_VERSION,
            revision: 0,
            red,
            white,
            moves: Vec::new(),
        }
    }

    pub fn parse(value: &Value) -> Result<Self> {
        let state: Self =
            serde_json::from_value(value.clone()).context("corrupt daily match state")?;
        ensure!(
            state.version == STATE_VERSION,
            "unsupported daily checkers state version: {}",
            state.version
        );
        Ok(state)
    }

    pub fn user_of(&self, color: Color) -> Uuid {
        match color {
            Color::Red => self.red,
            Color::White => self.white,
        }
    }

    pub fn color_of(&self, user_id: Uuid) -> Option<Color> {
        if user_id == self.red {
            Some(Color::Red)
        } else if user_id == self.white {
            Some(Color::White)
        } else {
            None
        }
    }

    /// Rebuild the board from the move history.
    pub fn grid(&self) -> Grid {
        self.replay().0
    }

    /// Whose turn it is. No passes in checkers, so this is plain parity: red on
    /// even move counts, white on odd.
    pub fn turn(&self) -> Color {
        if self.moves.len().is_multiple_of(2) {
            Color::Red
        } else {
            Color::White
        }
    }

    /// `(red, white)` piece counts on the current board.
    pub fn piece_counts(&self) -> (usize, usize) {
        let grid = self.grid();
        let (mut red, mut white) = (0, 0);
        for row in &grid {
            for cell in row {
                match cell {
                    Some(p) if p.color == Color::Red => red += 1,
                    Some(_) => white += 1,
                    None => {}
                }
            }
        }
        (red, white)
    }

    /// The complete legal moves for `color` right now: only jump chains if any
    /// capture exists (mandatory capture), otherwise the simple slides.
    pub fn legal_moves(&self, color: Color) -> Vec<Vec<(usize, usize)>> {
        generate_moves(&self.grid(), color)
    }

    /// `(row, col)` squares of the most recent move, for highlighting.
    pub fn last_move(&self) -> Option<Vec<(usize, usize)>> {
        let raw = self.moves.last()?;
        Some(raw.iter().map(|&i| cell(i)).collect())
    }

    pub fn move_count(&self) -> usize {
        self.moves.len()
    }

    pub fn is_finished(&self) -> bool {
        !matches!(self.status(), CheckersStatus::Ongoing)
    }

    /// The current verdict, derived by replay: the side to move losing when it
    /// has no legal move, or a draw once the forty-move clock runs out.
    pub fn status(&self) -> CheckersStatus {
        let (grid, since_progress) = self.replay();
        let mover = self.turn();
        if generate_moves(&grid, mover).is_empty() {
            CheckersStatus::Win(mover.other())
        } else if since_progress >= DRAW_PLIES {
            CheckersStatus::Draw
        } else {
            CheckersStatus::Ongoing
        }
    }

    /// Play `path` for the side to move. The caller owns turn order and match
    /// status; this validates the path against the legal move set (which is
    /// where mandatory capture, full-chain, and crowning rules are enforced),
    /// applies it, and reports what changed.
    pub fn apply_move(&mut self, path: &[(usize, usize)]) -> Result<MoveOutcome> {
        ensure!(path.len() >= 2, "a move needs a start and a destination");
        let grid = self.grid();
        let color = self.turn();
        ensure!(
            generate_moves(&grid, color)
                .iter()
                .any(|legal| legal.as_slice() == path),
            "that is not a legal move"
        );

        let (start_row, start_col) = path[0];
        let mover = grid[start_row][start_col].expect("a legal move starts on a piece");
        let captured: Vec<(usize, usize)> = path
            .windows(2)
            .filter(|w| w[1].0.abs_diff(w[0].0) == 2)
            .map(|w| ((w[0].0 + w[1].0) / 2, (w[0].1 + w[1].1) / 2))
            .collect();
        let (end_row, _) = *path.last().unwrap();
        let crowned = !mover.king && is_crown_row(color, end_row);

        self.moves
            .push(path.iter().map(|&(r, c)| (r * SIZE + c) as u8).collect());

        let (winner, draw, finished) = match self.status() {
            CheckersStatus::Win(c) => (Some(c), false, true),
            CheckersStatus::Draw => (None, true, true),
            CheckersStatus::Ongoing => (None, false, false),
        };

        Ok(MoveOutcome {
            color,
            captured,
            crowned,
            finished,
            draw,
            winner,
        })
    }

    /// Replay the history into `(board, plies since the last progress move)`. A
    /// capture or a man move is progress and resets the draw clock; a king
    /// shuffling does not.
    fn replay(&self) -> (Grid, usize) {
        let mut grid = starting_grid();
        let mut since_progress = 0usize;
        for raw in &self.moves {
            let path: Vec<(usize, usize)> = raw.iter().map(|&i| cell(i)).collect();
            let (start_row, start_col) = path[0];
            let moved_a_man = grid[start_row][start_col].is_some_and(|p| !p.king);
            let captured = path.windows(2).any(|w| w[1].0.abs_diff(w[0].0) == 2);
            apply_path(&mut grid, &path);
            if captured || moved_a_man {
                since_progress = 0;
            } else {
                since_progress += 1;
            }
        }
        (grid, since_progress)
    }
}

/// The standard opening: three ranks of men each, on the dark squares. Red
/// fills rows 0-2 and advances up the board; white fills rows 5-7 and advances
/// down.
fn starting_grid() -> Grid {
    let mut grid = [[None; SIZE]; SIZE];
    for (row, rank) in grid.iter_mut().enumerate() {
        for (col, square) in rank.iter_mut().enumerate() {
            if (row + col) % 2 != 1 {
                continue;
            }
            if row <= 2 {
                *square = Some(Piece {
                    color: Color::Red,
                    king: false,
                });
            } else if row >= 5 {
                *square = Some(Piece {
                    color: Color::White,
                    king: false,
                });
            }
        }
    }
    grid
}

const RED_DIRS: [(isize, isize); 2] = [(1, -1), (1, 1)];
const WHITE_DIRS: [(isize, isize); 2] = [(-1, -1), (-1, 1)];
const KING_DIRS: [(isize, isize); 4] = [(1, -1), (1, 1), (-1, -1), (-1, 1)];

/// The directions a piece may travel: men forward only, kings all four.
fn dirs(piece: Piece) -> &'static [(isize, isize)] {
    match (piece.color, piece.king) {
        (_, true) => &KING_DIRS,
        (Color::Red, false) => &RED_DIRS,
        (Color::White, false) => &WHITE_DIRS,
    }
}

/// Red is crowned on the top rank, white on the bottom.
fn is_crown_row(color: Color, row: usize) -> bool {
    match color {
        Color::Red => row == SIZE - 1,
        Color::White => row == 0,
    }
}

fn in_bounds(row: isize, col: isize) -> bool {
    (0..SIZE as isize).contains(&row) && (0..SIZE as isize).contains(&col)
}

fn cell(index: u8) -> (usize, usize) {
    (index as usize / SIZE, index as usize % SIZE)
}

/// Every complete legal move for `color`: mandatory-capture means jump chains
/// alone when any capture exists, otherwise every simple slide.
fn generate_moves(grid: &Grid, color: Color) -> Vec<Vec<(usize, usize)>> {
    let mut captures = Vec::new();
    for row in 0..SIZE {
        for col in 0..SIZE {
            if grid[row][col].is_some_and(|p| p.color == color) {
                captures.extend(capture_paths(grid, row, col));
            }
        }
    }
    if !captures.is_empty() {
        return captures;
    }

    let mut slides = Vec::new();
    for row in 0..SIZE {
        for col in 0..SIZE {
            let Some(piece) = grid[row][col] else {
                continue;
            };
            if piece.color != color {
                continue;
            }
            for &(dr, dc) in dirs(piece) {
                let (nr, nc) = (row as isize + dr, col as isize + dc);
                if in_bounds(nr, nc) && grid[nr as usize][nc as usize].is_none() {
                    slides.push(vec![(row, col), (nr as usize, nc as usize)]);
                }
            }
        }
    }
    slides
}

/// Every maximal jump chain starting from the piece at `(row, col)`.
fn capture_paths(grid: &Grid, row: usize, col: usize) -> Vec<Vec<(usize, usize)>> {
    let Some(piece) = grid[row][col] else {
        return Vec::new();
    };
    // The moving piece leaves its start empty; captured pieces stay on the
    // board until the turn ends, so they keep blocking landings and can't be
    // jumped twice (tracked in `captured`).
    let mut work = *grid;
    work[row][col] = None;
    let mut results = Vec::new();
    let mut captured = Vec::new();
    let mut path = vec![(row, col)];
    extend_captures(
        &work,
        piece,
        row,
        col,
        &mut captured,
        &mut path,
        &mut results,
    );
    results
}

fn extend_captures(
    work: &Grid,
    piece: Piece,
    row: usize,
    col: usize,
    captured: &mut Vec<(usize, usize)>,
    path: &mut Vec<(usize, usize)>,
    results: &mut Vec<Vec<(usize, usize)>>,
) {
    for &(dr, dc) in dirs(piece) {
        let (mid_row, mid_col) = (row as isize + dr, col as isize + dc);
        let (land_row, land_col) = (row as isize + 2 * dr, col as isize + 2 * dc);
        if !in_bounds(land_row, land_col) {
            continue;
        }
        let (mid_row, mid_col) = (mid_row as usize, mid_col as usize);
        let (land_row, land_col) = (land_row as usize, land_col as usize);
        if captured.contains(&(mid_row, mid_col)) {
            continue; // already jumped this piece earlier in the chain
        }
        let Some(victim) = work[mid_row][mid_col] else {
            continue;
        };
        if victim.color != piece.color.other() {
            continue;
        }
        if work[land_row][land_col].is_some() {
            continue; // landing blocked (incl. a not-yet-removed captured piece)
        }

        captured.push((mid_row, mid_col));
        path.push((land_row, land_col));
        // Reaching the far rank crowns the man and ends the turn immediately,
        // so the chain stops here even if more jumps look available.
        if !piece.king && is_crown_row(piece.color, land_row) {
            results.push(path.clone());
        } else {
            let before = results.len();
            extend_captures(work, piece, land_row, land_col, captured, path, results);
            if results.len() == before {
                results.push(path.clone()); // no further jump: a terminal chain
            }
        }
        path.pop();
        captured.pop();
    }
}

/// Apply a validated move path in place: slide or jump the piece, remove the
/// jumped pieces, and crown it if it finished on the far rank.
fn apply_path(grid: &mut Grid, path: &[(usize, usize)]) {
    let (start_row, start_col) = path[0];
    let mut piece = grid[start_row][start_col]
        .take()
        .expect("a move path starts on a piece");
    for window in path.windows(2) {
        let ((from_row, from_col), (to_row, to_col)) = (window[0], window[1]);
        if to_row.abs_diff(from_row) == 2 {
            grid[(from_row + to_row) / 2][(from_col + to_col) / 2] = None;
        }
    }
    let (end_row, end_col) = *path.last().unwrap();
    if !piece.king && is_crown_row(piece.color, end_row) {
        piece.king = true;
    }
    grid[end_row][end_col] = Some(piece);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh() -> DailyCheckersState {
        DailyCheckersState {
            version: STATE_VERSION,
            revision: 0,
            red: Uuid::from_u128(1),
            white: Uuid::from_u128(2),
            moves: Vec::new(),
        }
    }

    fn man(color: Color) -> Option<Piece> {
        Some(Piece { color, king: false })
    }

    #[test]
    fn cell_labels_read_like_a_board() {
        assert_eq!(cell_label(0, 0), "a1");
        assert_eq!(cell_label(7, 7), "h8");
        assert_eq!(cell_label(2, 3), "d3");
    }

    #[test]
    fn red_opens_with_seven_slides_and_no_captures() {
        let state = fresh();
        assert_eq!(state.turn(), Color::Red);
        assert_eq!(state.user_of(Color::Red), Uuid::from_u128(1));
        assert_eq!(state.piece_counts(), (12, 12));
        let moves = state.legal_moves(Color::Red);
        // Only the front rank (row 2) can advance into the empty row 3.
        assert_eq!(moves.len(), 7);
        assert!(moves.iter().all(|m| m.len() == 2)); // all slides, no jumps
    }

    #[test]
    fn a_slide_moves_the_piece_and_passes_the_turn() {
        let mut state = fresh();
        let outcome = state.apply_move(&[(2, 1), (3, 0)]).unwrap();
        assert_eq!(outcome.color, Color::Red);
        assert!(outcome.captured.is_empty());
        assert!(!outcome.crowned && !outcome.finished);
        assert_eq!(state.turn(), Color::White);
        assert_eq!(state.piece_counts(), (12, 12));
        assert_eq!(state.last_move(), Some(vec![(2, 1), (3, 0)]));
        assert_eq!(outcome.label(&[(2, 1), (3, 0)]), "b3-a4");
    }

    #[test]
    fn a_capture_is_mandatory_and_removes_the_jumped_piece() {
        // Reach a position where red's only legal reply is a jump.
        let mut state = fresh();
        state.apply_move(&[(2, 1), (3, 2)]).unwrap(); // red advances to c4
        state.apply_move(&[(5, 4), (4, 3)]).unwrap(); // white steps beside it at d5
        // Red at c4 must take d5; a simple slide is now illegal.
        assert!(state.apply_move(&[(2, 3), (3, 4)]).is_err());
        let outcome = state.apply_move(&[(3, 2), (5, 4)]).unwrap();
        assert_eq!(outcome.captured, vec![(4, 3)]);
        assert_eq!(outcome.color, Color::Red);
        assert!(!outcome.finished);
        assert_eq!(state.piece_counts(), (12, 11));
        assert_eq!(outcome.label(&[(3, 2), (5, 4)]), "c4xe6");
    }

    #[test]
    fn a_double_jump_chains_into_one_move() {
        let mut grid = [[None; SIZE]; SIZE];
        grid[2][1] = man(Color::Red);
        grid[3][2] = man(Color::White);
        grid[5][4] = man(Color::White);
        let moves = generate_moves(&grid, Color::Red);
        assert_eq!(moves, vec![vec![(2, 1), (4, 3), (6, 5)]]);

        apply_path(&mut grid, &moves[0]);
        assert_eq!(grid[6][5], man(Color::Red)); // landed, still a man
        assert!(grid[3][2].is_none() && grid[5][4].is_none()); // both taken
        assert!(grid[2][1].is_none());
    }

    #[test]
    fn crowning_ends_the_turn_mid_chain() {
        // A man reaching the back rank is crowned and its turn ends at once:
        // from c6 red jumps over d7 to e8 and stops, even though a king could
        // continue back over f7. So only the single-jump chain is legal.
        let mut grid = [[None; SIZE]; SIZE];
        grid[5][2] = man(Color::Red);
        grid[6][3] = man(Color::White);
        grid[6][5] = man(Color::White);
        assert_eq!(
            generate_moves(&grid, Color::Red),
            vec![vec![(5, 2), (7, 4)]]
        );

        apply_path(&mut grid, &[(5, 2), (7, 4)]);
        assert_eq!(
            grid[7][4],
            Some(Piece {
                color: Color::Red,
                king: true
            })
        );
        assert!(grid[6][3].is_none());
        assert_eq!(grid[6][5], man(Color::White)); // the tempting piece survives
    }

    #[test]
    fn a_blocked_side_has_no_move() {
        // Lone red man at b1, boxed in: both diagonals hold white men and the
        // jump landings are occupied, so red cannot move (and would lose).
        let mut grid = [[None; SIZE]; SIZE];
        grid[0][1] = man(Color::Red);
        grid[1][0] = man(Color::White);
        grid[1][2] = man(Color::White);
        grid[2][3] = man(Color::White); // blocks the b1xd3 landing
        assert!(generate_moves(&grid, Color::Red).is_empty());
        assert!(!generate_moves(&grid, Color::White).is_empty());
    }

    #[test]
    fn state_round_trips_through_json() {
        let mut state = DailyCheckersState {
            version: STATE_VERSION,
            revision: 0,
            red: Uuid::from_u128(7),
            white: Uuid::from_u128(8),
            moves: Vec::new(),
        };
        state.apply_move(&[(2, 1), (3, 2)]).unwrap();
        let value = serde_json::to_value(&state).unwrap();
        let parsed = DailyCheckersState::parse(&value).unwrap();
        assert_eq!(
            parsed.moves,
            vec![vec![(2 * SIZE + 1) as u8, (3 * SIZE + 2) as u8]]
        );
        assert_eq!(parsed.color_of(Uuid::from_u128(7)), Some(Color::Red));
        assert_eq!(parsed.color_of(Uuid::from_u128(9)), None);
        assert_eq!(parsed.turn(), Color::White);
        assert_eq!(parsed.status(), CheckersStatus::Ongoing);
    }
}
