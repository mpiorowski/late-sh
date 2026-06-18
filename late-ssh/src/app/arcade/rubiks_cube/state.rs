use rand_core::{OsRng, RngCore};

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

#[derive(Clone)]
pub struct State {
    stickers: [[Sticker; 9]; 6],
    history: Vec<CubeMove>,
    redo: Vec<CubeMove>,
    view_turns: u8,
    scramble_id: u32,
    message: String,
}

impl State {
    pub fn new() -> Self {
        Self {
            stickers: solved_stickers(),
            history: Vec::new(),
            redo: Vec::new(),
            view_turns: 0,
            scramble_id: 0,
            message: "Scramble with s, turn faces with u/d/l/r/f/b.".to_string(),
        }
    }

    pub fn stickers(&self) -> &[[Sticker; 9]; 6] {
        &self.stickers
    }

    pub fn move_count(&self) -> usize {
        self.history.len()
    }

    pub fn scramble_id(&self) -> u32 {
        self.scramble_id
    }

    pub fn view_turns(&self) -> u8 {
        self.view_turns
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
        self.stickers = solved_stickers();
        self.history.clear();
        self.redo.clear();
        self.scramble_id = 0;
        self.message = "Cube reset.".to_string();
    }

    pub fn scramble(&mut self) {
        self.stickers = solved_stickers();
        self.history.clear();
        self.redo.clear();
        self.scramble_id = self.scramble_id.wrapping_add(1).max(1);

        let mut rng = OsRng;
        let faces = [
            Face::Up,
            Face::Down,
            Face::Left,
            Face::Right,
            Face::Front,
            Face::Back,
        ];
        let mut previous = None;
        for _ in 0..24 {
            let mut face = faces[(rng.next_u32() as usize) % faces.len()];
            while Some(face) == previous {
                face = faces[(rng.next_u32() as usize) % faces.len()];
            }
            let inverse = rng.next_u32().is_multiple_of(2);
            self.apply_move_internal(CubeMove { face, inverse });
            self.history.push(CubeMove { face, inverse });
            previous = Some(face);
        }
        self.history.clear();
        self.message = "Scrambled. Solve it from here.".to_string();
    }

    pub fn turn_view(&mut self) {
        self.view_turns = (self.view_turns + 1) % 4;
        self.message = format!("View: {}", view_label(self.view_turns));
    }

    pub fn apply_move(&mut self, cube_move: CubeMove) {
        self.apply_move_internal(cube_move);
        self.history.push(cube_move);
        self.redo.clear();
        self.message = if self.is_solved() {
            format!("Solved in {} moves.", self.history.len())
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
        self.message = format!("Redid {}.", cube_move.label());
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

pub fn face_for_view(view_turns: u8) -> (Face, Face, Face) {
    match view_turns % 4 {
        0 => (Face::Up, Face::Front, Face::Right),
        1 => (Face::Up, Face::Right, Face::Back),
        2 => (Face::Up, Face::Back, Face::Left),
        _ => (Face::Up, Face::Left, Face::Front),
    }
}

pub fn view_label(view_turns: u8) -> &'static str {
    match view_turns % 4 {
        0 => "front/right",
        1 => "right/back",
        2 => "back/left",
        _ => "left/front",
    }
}

pub fn oriented_face(
    stickers: &[[Sticker; 9]; 6],
    face: Face,
    view_turns: u8,
) -> [[Sticker; 3]; 3] {
    let (_, front, right) = face_for_view(view_turns);
    let normal = face_normal(face);
    let screen_right = match face {
        Face::Up => face_normal(right),
        Face::Down => negate(face_normal(right)),
        _ if face == right => negate(face_normal(front)),
        _ => face_normal(right),
    };
    let screen_up = match face {
        Face::Up => negate(face_normal(front)),
        Face::Down => face_normal(front),
        _ => face_normal(Face::Up),
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

fn negate((x, y, z): Coord) -> Coord {
    (-x, -y, -z)
}

fn dot(a: Coord, b: Coord) -> i8 {
    a.0 * b.0 + a.1 * b.1 + a.2 * b.2
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn four_turns_restore_cube() {
        for face in FACES {
            let mut state = State::new();
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
            let mut state = State::new();
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
}
