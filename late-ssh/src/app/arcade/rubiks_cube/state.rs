use chrono::{NaiveDate, Utc};
use late_core::models::rubiks_cube::{Game, GameParams};
use uuid::Uuid;

use super::svc::RubiksCubeService;

pub const DAILY_WIN_REWARD_CHIPS: i64 = 500;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Sticker {
    White,
    Yellow,
    Orange,
    Red,
    Green,
    Blue,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Face {
    Up,
    Down,
    Left,
    Right,
    Front,
    Back,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CubeMove {
    pub face: Face,
    pub inverse: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ViewTurn {
    Up,
    Down,
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CubeView {
    top: Face,
    front: Face,
}

#[derive(Clone)]
pub struct State {
    user_id: Uuid,
    stickers: [[Sticker; 9]; 6],
    user_moves: u32,
    view: CubeView,
    puzzle_date: NaiveDate,
    solved_reported: bool,
    reset_pending: bool,
    message: String,
    svc: RubiksCubeService,
}

impl State {
    pub fn new(user_id: Uuid, svc: RubiksCubeService, saved_game: Option<Game>) -> Self {
        let today = Utc::now().date_naive();
        let mut state = Self::new_for_date(user_id, svc, today);
        // Restore a saved cube only for today's scramble: stale rows are from
        // an older daily and the fresh scramble already replaced them.
        if let Some(game) = saved_game
            && game.puzzle_date == today
            && let Some(stickers) = stickers_from_string(&game.stickers)
        {
            state.stickers = stickers;
            state.user_moves = game.user_moves.max(0) as u32;
            state.solved_reported = state.is_solved();
            if state.has_started() {
                state.message = format!("Daily cube {} restored.", state.daily_label());
            }
        }
        state
    }

    fn new_for_date(user_id: Uuid, svc: RubiksCubeService, puzzle_date: NaiveDate) -> Self {
        let mut state = Self {
            user_id,
            stickers: solved_stickers(),
            user_moves: 0,
            view: CubeView::default(),
            puzzle_date,
            solved_reported: false,
            reset_pending: false,
            message: String::new(),
            svc,
        };
        state.apply_daily_scramble();
        state.message = format!("Daily cube {}. Solve it from here.", state.daily_label());
        state
    }

    pub fn stickers(&self) -> &[[Sticker; 9]; 6] {
        &self.stickers
    }

    pub fn has_started(&self) -> bool {
        self.user_moves > 0
    }

    pub fn daily_label(&self) -> String {
        self.puzzle_date.format("%Y-%m-%d").to_string()
    }

    pub fn view(&self) -> CubeView {
        self.view
    }

    pub fn view_label(&self) -> String {
        self.view.label()
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn reset_pending(&self) -> bool {
        self.reset_pending
    }

    pub fn request_reset(&mut self) -> bool {
        if self.reset_pending {
            self.reset_pending = false;
            return true;
        }
        self.reset_pending = true;
        self.message = "Press reset again to reset today's cube.".to_string();
        false
    }

    pub fn clear_reset_pending(&mut self) {
        self.reset_pending = false;
    }

    pub fn is_solved(&self) -> bool {
        self.stickers
            .iter()
            .all(|face| face.iter().all(|sticker| *sticker == face[0]))
    }

    /// Today's cube has at least one player move on it and is not yet solved.
    pub fn has_unfinished_daily(&self) -> bool {
        self.puzzle_date == Utc::now().date_naive() && self.has_started() && !self.is_solved()
    }

    pub fn reset(&mut self) {
        self.reset_pending = false;
        self.apply_daily_scramble();
        self.message = format!("Daily cube {} reset.", self.daily_label());
        self.save_async();
    }

    pub fn ensure_current_daily(&mut self) {
        let today = Utc::now().date_naive();
        if self.puzzle_date == today {
            return;
        }
        *self = Self::new_for_date(self.user_id, self.svc.clone(), today);
    }

    fn apply_daily_scramble(&mut self) {
        self.stickers = solved_stickers();
        self.user_moves = 0;
        for cube_move in daily_scramble(self.puzzle_date) {
            self.apply_move_internal(cube_move);
        }
    }

    pub fn turn_view(&mut self, turn: ViewTurn) {
        self.clear_reset_pending();
        self.view = self.view.turned(turn);
        self.message = format!("View: {}", self.view.label());
    }

    pub fn apply_move(&mut self, cube_move: CubeMove) {
        let label = cube_move.label();
        self.apply_move_labeled(cube_move, label);
    }

    /// Apply a move expressed relative to the current view: `slot` is the
    /// on-screen face the player means (always F for front, U for up, ...) and
    /// it is resolved to the absolute face currently occupying that slot.
    pub fn apply_relative_move(&mut self, slot: Face, inverse: bool) {
        let cube_move = CubeMove {
            face: self.view.resolve_face(slot),
            inverse,
        };
        let label = CubeMove {
            face: slot,
            inverse,
        }
        .label();
        self.apply_move_labeled(cube_move, label);
    }

    fn apply_move_labeled(&mut self, cube_move: CubeMove, label: String) {
        self.clear_reset_pending();
        self.apply_move_internal(cube_move);
        self.user_moves = self.user_moves.saturating_add(1);
        self.message = if self.is_solved() {
            self.record_solved();
            "Solved.".to_string()
        } else {
            format!("Move {label}")
        };
        self.save_async();
    }

    fn save_async(&self) {
        // Pure unit tests drive moves without a tokio runtime; skip the
        // fire-and-forget save there instead of panicking in spawn.
        if tokio::runtime::Handle::try_current().is_err() {
            return;
        }
        self.svc.save_game_task(GameParams {
            user_id: self.user_id,
            puzzle_date: self.puzzle_date,
            stickers: stickers_to_string(&self.stickers),
            user_moves: self.user_moves.min(i32::MAX as u32) as i32,
        });
    }

    fn record_solved(&mut self) {
        if self.solved_reported || !self.has_started() {
            return;
        }
        self.solved_reported = true;
        self.svc.record_win_task(self.user_id, self.puzzle_date);
    }

    fn apply_move_internal(&mut self, cube_move: CubeMove) {
        let (axis, layer, normal_sign) = move_axis(cube_move.face);
        let mut quarter_turns = if cube_move.inverse {
            normal_sign
        } else {
            -normal_sign
        };
        while quarter_turns < 0 {
            quarter_turns += 4;
        }
        for _ in 0..quarter_turns {
            self.rotate_layer_positive(axis, layer);
        }
    }

    fn rotate_layer_positive(&mut self, axis: Axis, layer: i8) {
        let old = self.stickers;
        let mut next = old;
        for face in FACES {
            for row in 0..3 {
                for col in 0..3 {
                    let (position, normal) = sticker_coord(face, row, col);
                    if coord_axis(position, axis) != layer {
                        continue;
                    }
                    let new_position = rotate_coord_positive(position, axis);
                    let new_normal = rotate_coord_positive(normal, axis);
                    let (new_face, new_row, new_col) = face_row_col(new_normal, new_position);
                    next[new_face.index()][new_row * 3 + new_col] =
                        old[face.index()][row * 3 + col];
                }
            }
        }
        self.stickers = next;
    }
}

/// Serialize the cube as 54 face chars (U D L R F B faces in `Face::index`
/// order, 9 stickers each), the format stored in `rubiks_cube_games`.
fn stickers_to_string(stickers: &[[Sticker; 9]; 6]) -> String {
    stickers
        .iter()
        .flat_map(|face| face.iter().map(|sticker| sticker.as_char()))
        .collect()
}

fn stickers_from_string(value: &str) -> Option<[[Sticker; 9]; 6]> {
    let mut chars = value.chars();
    let mut stickers = solved_stickers();
    for face in &mut stickers {
        for slot in face.iter_mut() {
            *slot = Sticker::from_char(chars.next()?)?;
        }
    }
    chars.next().is_none().then_some(stickers)
}

impl Sticker {
    fn as_char(self) -> char {
        match self {
            Sticker::White => 'W',
            Sticker::Yellow => 'Y',
            Sticker::Orange => 'O',
            Sticker::Red => 'R',
            Sticker::Green => 'G',
            Sticker::Blue => 'B',
        }
    }

    fn from_char(value: char) -> Option<Self> {
        match value {
            'W' => Some(Sticker::White),
            'Y' => Some(Sticker::Yellow),
            'O' => Some(Sticker::Orange),
            'R' => Some(Sticker::Red),
            'G' => Some(Sticker::Green),
            'B' => Some(Sticker::Blue),
            _ => None,
        }
    }
}

fn daily_scramble(puzzle_date: NaiveDate) -> Vec<CubeMove> {
    let mut seed = stable_daily_seed(puzzle_date);
    let faces = [
        Face::Up,
        Face::Down,
        Face::Left,
        Face::Right,
        Face::Front,
        Face::Back,
    ];
    let mut previous = None;
    let mut moves = Vec::with_capacity(24);
    for _ in 0..24 {
        let mut face = faces[(next_seed(&mut seed) as usize) % faces.len()];
        while Some(face) == previous {
            face = faces[(next_seed(&mut seed) as usize) % faces.len()];
        }
        let inverse = next_seed(&mut seed).is_multiple_of(2);
        moves.push(CubeMove { face, inverse });
        previous = Some(face);
    }
    moves
}

fn stable_daily_seed(puzzle_date: NaiveDate) -> u64 {
    let mut seed = 0xcbf2_9ce4_8422_2325u64;
    for byte in b"late-sh-rubiks-cube-daily-v1" {
        seed ^= u64::from(*byte);
        seed = seed.wrapping_mul(0x0000_0100_0000_01b3);
    }
    for byte in puzzle_date.format("%Y-%m-%d").to_string().bytes() {
        seed ^= u64::from(byte);
        seed = seed.wrapping_mul(0x0000_0100_0000_01b3);
    }
    seed
}

fn next_seed(seed: &mut u64) -> u64 {
    *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
    *seed
}

impl CubeMove {
    pub fn inverse(self) -> Self {
        Self {
            face: self.face,
            inverse: !self.inverse,
        }
    }

    pub fn label(self) -> String {
        let face = match self.face {
            Face::Up => "U",
            Face::Down => "D",
            Face::Left => "L",
            Face::Right => "R",
            Face::Front => "F",
            Face::Back => "B",
        };
        if self.inverse {
            format!("{face}'")
        } else {
            face.to_string()
        }
    }
}

impl Face {
    pub fn index(self) -> usize {
        match self {
            Face::Up => 0,
            Face::Down => 1,
            Face::Left => 2,
            Face::Right => 3,
            Face::Front => 4,
            Face::Back => 5,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Face::Up => "U",
            Face::Down => "D",
            Face::Left => "L",
            Face::Right => "R",
            Face::Front => "F",
            Face::Back => "B",
        }
    }
}

impl Default for CubeView {
    fn default() -> Self {
        Self {
            top: Face::Up,
            front: Face::Front,
        }
    }
}

impl CubeView {
    pub fn label(self) -> String {
        let (_, front, right) = self.visible_faces();
        format!(
            "{}/{}",
            front.label().to_ascii_lowercase(),
            right.label().to_ascii_lowercase()
        )
    }

    pub fn visible_faces(self) -> (Face, Face, Face) {
        let right = face_from_normal(cross(face_normal(self.top), face_normal(self.front)));
        (self.top, self.front, right)
    }

    /// Map a viewer-relative slot (the up/down/left/right/front/back the player
    /// currently sees) to the absolute cube face occupying it, so controls and
    /// labels stay pinned to the screen instead of the underlying face.
    pub fn resolve_face(self, slot: Face) -> Face {
        let (top, front, right) = self.visible_faces();
        match slot {
            Face::Up => top,
            Face::Down => opposite(top),
            Face::Left => opposite(right),
            Face::Right => right,
            Face::Front => front,
            Face::Back => opposite(front),
        }
    }

    fn turned(self, turn: ViewTurn) -> Self {
        let (top, front, right) = self.visible_faces();
        match turn {
            ViewTurn::Up => Self {
                top: opposite(front),
                front: top,
            },
            ViewTurn::Down => Self {
                top: front,
                front: opposite(top),
            },
            ViewTurn::Left => Self {
                top,
                front: opposite(right),
            },
            ViewTurn::Right => Self { top, front: right },
        }
    }
}

const FACES: [Face; 6] = [
    Face::Up,
    Face::Down,
    Face::Left,
    Face::Right,
    Face::Front,
    Face::Back,
];

fn solved_stickers() -> [[Sticker; 9]; 6] {
    [
        [Sticker::White; 9],
        [Sticker::Yellow; 9],
        [Sticker::Orange; 9],
        [Sticker::Red; 9],
        [Sticker::Green; 9],
        [Sticker::Blue; 9],
    ]
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Axis {
    X,
    Y,
    Z,
}

type Coord = (i8, i8, i8);

fn move_axis(face: Face) -> (Axis, i8, i8) {
    match face {
        Face::Up => (Axis::Y, 1, 1),
        Face::Down => (Axis::Y, -1, -1),
        Face::Left => (Axis::X, -1, -1),
        Face::Right => (Axis::X, 1, 1),
        Face::Front => (Axis::Z, 1, 1),
        Face::Back => (Axis::Z, -1, -1),
    }
}

fn coord_axis(coord: Coord, axis: Axis) -> i8 {
    match axis {
        Axis::X => coord.0,
        Axis::Y => coord.1,
        Axis::Z => coord.2,
    }
}

fn rotate_coord_positive((x, y, z): Coord, axis: Axis) -> Coord {
    match axis {
        Axis::X => (x, -z, y),
        Axis::Y => (z, y, -x),
        Axis::Z => (-y, x, z),
    }
}

pub fn face_for_view(view: CubeView) -> (Face, Face, Face) {
    view.visible_faces()
}

pub fn oriented_face(
    stickers: &[[Sticker; 9]; 6],
    face: Face,
    view: CubeView,
) -> [[Sticker; 3]; 3] {
    let (top, front, right) = face_for_view(view);
    let top_normal = face_normal(top);
    let front_normal = face_normal(front);
    let right_normal = face_normal(right);
    let (screen_right, screen_up) = if face == top {
        (right_normal, negate(front_normal))
    } else if face == front {
        (right_normal, top_normal)
    } else if face == right {
        (negate(front_normal), top_normal)
    } else {
        (right_normal, top_normal)
    };
    oriented_grid(stickers, face, screen_right, screen_up)
}

fn oriented_grid(
    stickers: &[[Sticker; 9]; 6],
    face: Face,
    screen_right: Coord,
    screen_up: Coord,
) -> [[Sticker; 3]; 3] {
    let normal = face_normal(face);
    let mut grid = [[Sticker::White; 3]; 3];
    for row in 0..3 {
        for col in 0..3 {
            let (position, sticker_normal) = sticker_coord(face, row, col);
            if sticker_normal != normal {
                continue;
            }
            let x = dot(position, screen_right);
            let y = dot(position, screen_up);
            grid[(1 - y) as usize][(x + 1) as usize] = stickers[face.index()][row * 3 + col];
        }
    }
    grid
}

/// One face as it appears unfolded from the current viewpoint: the absolute
/// face sitting in a viewer-relative slot, plus its stickers oriented to match
/// what you would see if you turned that face toward you. `slot` is the
/// viewer-relative label (U/D/L/R/F/B) that stays pinned to the on-screen
/// position regardless of which absolute face has been rotated into it.
#[derive(Clone, Copy)]
pub struct NetTile {
    pub face: Face,
    pub slot: &'static str,
    pub grid: [[Sticker; 3]; 3],
}

/// The whole cube unfolded relative to the current view: `up` is what is above
/// you, `left` is your actual left, `down` is your bottom, and so on. Each tile
/// is oriented so no mental rotation is needed to read a hidden side.
pub struct NetView {
    pub up: NetTile,
    pub left: NetTile,
    pub front: NetTile,
    pub right: NetTile,
    pub back: NetTile,
    pub down: NetTile,
}

pub fn net_view(stickers: &[[Sticker; 9]; 6], view: CubeView) -> NetView {
    let (top, front, right) = view.visible_faces();
    let top_normal = face_normal(top);
    let front_normal = face_normal(front);
    let right_normal = face_normal(right);
    let tile = |face: Face, slot: &'static str, screen_right: Coord, screen_up: Coord| NetTile {
        face,
        slot,
        grid: oriented_grid(stickers, face, screen_right, screen_up),
    };
    NetView {
        up: tile(top, "U", right_normal, negate(front_normal)),
        left: tile(opposite(right), "L", front_normal, top_normal),
        front: tile(front, "F", right_normal, top_normal),
        right: tile(right, "R", negate(front_normal), top_normal),
        back: tile(opposite(front), "B", negate(right_normal), top_normal),
        down: tile(opposite(top), "D", right_normal, front_normal),
    }
}

fn sticker_coord(face: Face, row: usize, col: usize) -> (Coord, Coord) {
    let r = row as i8;
    let c = col as i8;
    match face {
        Face::Up => ((c - 1, 1, r - 1), (0, 1, 0)),
        Face::Down => ((c - 1, -1, 1 - r), (0, -1, 0)),
        Face::Left => ((-1, 1 - r, c - 1), (-1, 0, 0)),
        Face::Right => ((1, 1 - r, 1 - c), (1, 0, 0)),
        Face::Front => ((c - 1, 1 - r, 1), (0, 0, 1)),
        Face::Back => ((1 - c, 1 - r, -1), (0, 0, -1)),
    }
}

fn face_row_col(normal: Coord, position: Coord) -> (Face, usize, usize) {
    let (x, y, z) = position;
    match normal {
        (0, 1, 0) => (Face::Up, (z + 1) as usize, (x + 1) as usize),
        (0, -1, 0) => (Face::Down, (1 - z) as usize, (x + 1) as usize),
        (-1, 0, 0) => (Face::Left, (1 - y) as usize, (z + 1) as usize),
        (1, 0, 0) => (Face::Right, (1 - y) as usize, (1 - z) as usize),
        (0, 0, 1) => (Face::Front, (1 - y) as usize, (x + 1) as usize),
        (0, 0, -1) => (Face::Back, (1 - y) as usize, (1 - x) as usize),
        _ => unreachable!("invalid sticker normal"),
    }
}

fn face_normal(face: Face) -> Coord {
    match face {
        Face::Up => (0, 1, 0),
        Face::Down => (0, -1, 0),
        Face::Left => (-1, 0, 0),
        Face::Right => (1, 0, 0),
        Face::Front => (0, 0, 1),
        Face::Back => (0, 0, -1),
    }
}

fn face_from_normal(normal: Coord) -> Face {
    match normal {
        (0, 1, 0) => Face::Up,
        (0, -1, 0) => Face::Down,
        (-1, 0, 0) => Face::Left,
        (1, 0, 0) => Face::Right,
        (0, 0, 1) => Face::Front,
        (0, 0, -1) => Face::Back,
        _ => unreachable!("invalid face normal"),
    }
}

fn opposite(face: Face) -> Face {
    face_from_normal(negate(face_normal(face)))
}

fn negate((x, y, z): Coord) -> Coord {
    (-x, -y, -z)
}

fn cross(a: Coord, b: Coord) -> Coord {
    (
        a.1 * b.2 - a.2 * b.1,
        a.2 * b.0 - a.0 * b.2,
        a.0 * b.1 - a.1 * b.0,
    )
}

fn dot(a: Coord, b: Coord) -> i8 {
    a.0 * b.0 + a.1 * b.1 + a.2 * b.2
}

#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;
