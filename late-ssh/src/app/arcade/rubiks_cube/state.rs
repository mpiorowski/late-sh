use chrono::{NaiveDate, Utc};

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
    stickers: [[Sticker; 9]; 6],
    history: Vec<CubeMove>,
    redo: Vec<CubeMove>,
    view: CubeView,
    puzzle_date: NaiveDate,
    solved_reported: bool,
    solved_event_pending: bool,
    message: String,
}

impl State {
    pub fn new() -> Self {
        Self::new_for_date(Utc::now().date_naive())
    }

    fn new_for_date(puzzle_date: NaiveDate) -> Self {
        let mut state = Self {
            stickers: solved_stickers(),
            history: Vec::new(),
            redo: Vec::new(),
            view: CubeView::default(),
            puzzle_date,
            solved_reported: false,
            solved_event_pending: false,
            message: String::new(),
        };
        state.apply_daily_scramble();
        state.message = format!("Daily cube {}. Solve it from here.", state.daily_label());
        state
    }

    pub fn stickers(&self) -> &[[Sticker; 9]; 6] {
        &self.stickers
    }

    pub fn has_started(&self) -> bool {
        !self.history.is_empty()
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

    pub fn is_solved(&self) -> bool {
        self.stickers
            .iter()
            .all(|face| face.iter().all(|sticker| *sticker == face[0]))
    }

    pub fn reset(&mut self) {
        self.apply_daily_scramble();
        self.solved_event_pending = false;
        self.message = format!("Daily cube {} reset.", self.daily_label());
    }

    pub fn scramble(&mut self) {
        self.reset();
    }

    pub fn ensure_current_daily(&mut self) {
        let today = Utc::now().date_naive();
        if self.puzzle_date == today {
            return;
        }
        *self = Self::new_for_date(today);
    }

    fn apply_daily_scramble(&mut self) {
        self.stickers = solved_stickers();
        self.history.clear();
        self.redo.clear();
        self.solved_event_pending = false;
        for cube_move in daily_scramble(self.puzzle_date) {
            self.apply_move_internal(cube_move);
        }
    }

    pub fn turn_view(&mut self, turn: ViewTurn) {
        self.view = self.view.turned(turn);
        self.message = format!("View: {}", self.view.label());
    }

    pub fn apply_move(&mut self, cube_move: CubeMove) {
        self.apply_move_internal(cube_move);
        self.history.push(cube_move);
        self.redo.clear();
        self.message = if self.is_solved() {
            self.mark_solved_event_pending();
            "Solved.".to_string()
        } else {
            format!("Move {}", cube_move.label())
        };
    }

    pub fn undo(&mut self) {
        let Some(cube_move) = self.history.pop() else {
            self.message = "Nothing to undo.".to_string();
            return;
        };
        self.apply_move_internal(cube_move.inverse());
        self.redo.push(cube_move);
        self.message = format!("Undid {}.", cube_move.label());
    }

    pub fn redo(&mut self) {
        let Some(cube_move) = self.redo.pop() else {
            self.message = "Nothing to redo.".to_string();
            return;
        };
        self.apply_move_internal(cube_move);
        self.history.push(cube_move);
        self.message = if self.is_solved() {
            self.mark_solved_event_pending();
            "Solved.".to_string()
        } else {
            format!("Redid {}.", cube_move.label())
        };
    }

    pub fn take_solved_event_pending(&mut self) -> bool {
        std::mem::take(&mut self.solved_event_pending)
    }

    fn mark_solved_event_pending(&mut self) {
        if self.solved_reported || self.history.is_empty() {
            return;
        }
        self.solved_reported = true;
        self.solved_event_pending = true;
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

impl Default for State {
    fn default() -> Self {
        Self::new()
    }
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
    let normal = face_normal(face);
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
mod tests {
    use super::*;

    fn solved_state() -> State {
        State {
            stickers: solved_stickers(),
            history: Vec::new(),
            redo: Vec::new(),
            view: CubeView::default(),
            puzzle_date: NaiveDate::from_ymd_opt(2026, 6, 18).unwrap(),
            solved_reported: false,
            solved_event_pending: false,
            message: String::new(),
        }
    }

    #[test]
    fn four_turns_restore_cube() {
        for face in FACES {
            let mut state = solved_state();
            for _ in 0..4 {
                state.apply_move(CubeMove {
                    face,
                    inverse: false,
                });
            }
            assert!(state.is_solved(), "{face:?} did not restore");
        }
    }

    #[test]
    fn move_and_inverse_restore_cube() {
        for face in FACES {
            let mut state = solved_state();
            state.apply_move(CubeMove {
                face,
                inverse: false,
            });
            state.apply_move(CubeMove {
                face,
                inverse: true,
            });
            assert!(state.is_solved(), "{face:?} inverse did not restore");
        }
    }

    #[test]
    fn view_arrows_rotate_in_requested_direction() {
        let view = CubeView::default();
        assert_eq!(
            view.turned(ViewTurn::Right).visible_faces(),
            (Face::Up, Face::Right, Face::Back)
        );
        assert_eq!(
            view.turned(ViewTurn::Left).visible_faces(),
            (Face::Up, Face::Left, Face::Front)
        );
        assert_eq!(
            view.turned(ViewTurn::Up).visible_faces(),
            (Face::Back, Face::Up, Face::Right)
        );
        assert_eq!(
            view.turned(ViewTurn::Down).visible_faces(),
            (Face::Front, Face::Down, Face::Right)
        );
    }

    #[test]
    fn opposite_view_turns_restore_orientation() {
        for (first, second) in [
            (ViewTurn::Right, ViewTurn::Left),
            (ViewTurn::Left, ViewTurn::Right),
            (ViewTurn::Up, ViewTurn::Down),
            (ViewTurn::Down, ViewTurn::Up),
        ] {
            let view = CubeView::default().turned(first).turned(second);
            assert_eq!(view, CubeView::default());
        }
    }
}
