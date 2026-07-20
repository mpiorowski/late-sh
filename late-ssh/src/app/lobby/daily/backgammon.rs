//! Backgammon rules for daily correspondence matches. Pure state + logic, no
//! I/O: the service persists `DailyBackgammonState` as the match's `state`
//! JSON the same way the other daily games persist theirs.
//!
//! Backgammon is the first roster game where "state = move history" is not
//! enough: the dice are server-rolled, so every turn is stored as its roll
//! plus the hops played with it, and `next_roll` carries the roll the current
//! mover has not played yet. The board, pip counts, and whose turn it is are
//! still derived by replaying the history; the turn is plain parity (white
//! moves first) because forced passes are recorded as turns with no hops.
//! There is no doubling cube in v1 (the payout is a fixed reward template).
//!
//! Rules enforced here: bar checkers must re-enter first, a maximal number of
//! dice must be played (and the higher die when only one can be), landing on
//! a lone opposing checker sends it to the bar, and bearing off requires the
//! whole side home (a die larger than the farthest checker bears off from the
//! farthest point). First to bear off all fifteen wins. Backgammon cannot
//! really draw, but a defensive stall cap turns an endless mutual blockage
//! into one instead of wedging the match forever.

use anyhow::{Context, Result, ensure};
use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

pub const POINTS: usize = 24;
/// `from` sentinel: the bar (both colors; a hop knows its mover).
pub const BAR: u8 = 24;
/// `to` sentinel: borne off.
pub const OFF: u8 = 25;
const CHECKERS_PER_SIDE: u8 = 15;
const STATE_VERSION: u8 = 1;
/// Consecutive forced passes before the match is declared a draw. Mutual
/// full blockage is not known to be reachable, but a stuck match must still
/// end; a hundred recorded passes in a row is unambiguously stuck.
const STALL_PASSES: usize = 100;

/// One hop of one checker with one die: `(from, to)` point indices, with
/// `BAR`/`OFF` as the sentinels. A turn is up to four hops (doubles).
pub type Hop = (u8, u8);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Color {
    White,
    Red,
}

impl Color {
    pub const fn label(self) -> &'static str {
        match self {
            Self::White => "white",
            Self::Red => "red",
        }
    }

    pub const fn other(self) -> Self {
        match self {
            Self::White => Self::Red,
            Self::Red => Self::White,
        }
    }

    const fn idx(self) -> usize {
        match self {
            Self::White => 0,
            Self::Red => 1,
        }
    }
}

/// The derived position. Point index 0 is white's 1-point: white travels
/// 23 -> 0 and bears off past 0 (home indices 0..=5), red travels 0 -> 23
/// and bears off past 23 (home indices 18..=23).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Board {
    /// Checkers per point: positive white, negative red.
    pub points: [i8; POINTS],
    /// Checkers on the bar, indexed white then red.
    pub bar: [u8; 2],
    /// Checkers borne off, indexed white then red.
    pub off: [u8; 2],
}

impl Board {
    /// The standard opening position, fifteen checkers a side.
    pub fn start() -> Self {
        let mut points = [0i8; POINTS];
        // White: 24-point x2, 13-point x5, 8-point x3, 6-point x5.
        points[23] = 2;
        points[12] = 5;
        points[7] = 3;
        points[5] = 5;
        // Red mirrors it.
        points[0] = -2;
        points[11] = -5;
        points[16] = -3;
        points[18] = -5;
        Self {
            points,
            bar: [0; 2],
            off: [0; 2],
        }
    }

    pub fn count(&self, color: Color, point: usize) -> u8 {
        let n = self.points[point];
        match color {
            Color::White => n.max(0) as u8,
            Color::Red => (-n).max(0) as u8,
        }
    }

    fn set(&mut self, color: Color, point: usize, n: u8) {
        self.points[point] = match color {
            Color::White => n as i8,
            Color::Red => -(n as i8),
        };
    }

    /// Every checker home (or off) and none on the bar: allowed to bear off.
    pub fn all_home(&self, color: Color) -> bool {
        if self.bar[color.idx()] > 0 {
            return false;
        }
        let outside = match color {
            Color::White => 6..POINTS,
            Color::Red => 0..18,
        };
        outside.into_iter().all(|p| self.count(color, p) == 0)
    }

    /// Total pips left to bear everything off; the classic race score.
    pub fn pip_count(&self, color: Color) -> u32 {
        let mut pips = self.bar[color.idx()] as u32 * 25;
        for p in 0..POINTS {
            let distance = match color {
                Color::White => p as u32 + 1,
                Color::Red => POINTS as u32 - p as u32,
            };
            pips += self.count(color, p) as u32 * distance;
        }
        pips
    }
}

/// A finished match's verdict, derived by replay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackgammonStatus {
    Ongoing,
    Win(Color),
    Draw,
}

/// One recorded turn: the roll the server produced and the hops played with
/// it. No hops is a forced pass (the mover had no legal play).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Turn {
    pub roll: [u8; 2],
    pub hops: Vec<Hop>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TurnOutcome {
    /// Who moved.
    pub color: Color,
    /// The roll the hops were played with.
    pub roll: [u8; 2],
    /// Per hop: whether it landed on (and barred) a lone opposing checker.
    pub hits: Vec<bool>,
    /// The match is now over (all fifteen borne off).
    pub finished: bool,
    /// The winner when decisive; `None` while running.
    pub winner: Option<Color>,
}

impl TurnOutcome {
    /// Standard notation from the mover's seat: `63: 24/18 13/10`, hits
    /// starred (`24/18*`), `bar`/`off` spelled out, and the result appended
    /// on the finishing turn.
    pub fn label(&self, hops: &[Hop]) -> String {
        let dice = format!("{}{}", self.roll[0], self.roll[1]);
        let mut out = if hops.is_empty() {
            format!("{dice}: no play")
        } else {
            let parts: Vec<String> = hops
                .iter()
                .enumerate()
                .map(|(i, &(from, to))| {
                    let star = if self.hits.get(i).copied().unwrap_or(false) {
                        "*"
                    } else {
                        ""
                    };
                    format!(
                        "{}/{}{star}",
                        point_name(self.color, from),
                        point_name(self.color, to)
                    )
                })
                .collect();
            format!("{dice}: {}", parts.join(" "))
        };
        if self.finished {
            match self.winner {
                Some(color) => out.push_str(&format!(", {} wins", color.label())),
                None => out.push_str(", draw"),
            }
        }
        out
    }
}

/// A point in the mover's own 1..24 numbering (each seat counts down toward
/// its home), with the `bar`/`off` sentinels spelled out.
pub fn point_name(color: Color, point: u8) -> String {
    match point {
        BAR => "bar".to_string(),
        OFF => "off".to_string(),
        p => match color {
            Color::White => (p as usize + 1).to_string(),
            Color::Red => (POINTS - p as usize).to_string(),
        },
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DailyBackgammonState {
    pub version: u8,
    #[serde(default)]
    pub revision: u64,
    /// White moves first; colors are assigned randomly at claim time.
    pub white: Uuid,
    pub red: Uuid,
    /// Every completed turn, forced passes included, oldest first.
    pub turns: Vec<Turn>,
    /// The roll waiting on the current mover. Rolled server-side (at claim
    /// for white's opening, then by `roll_next` after every turn); `None`
    /// only once the match is finished or in a client's optimistic state
    /// while the server's roll is still in flight.
    pub next_roll: Option<[u8; 2]>,
}

impl DailyBackgammonState {
    pub fn new(challenger: Uuid, claimer: Uuid) -> Self {
        let mut rng = rand::thread_rng();
        let (white, red) = if rng.gen_bool(0.5) {
            (challenger, claimer)
        } else {
            (claimer, challenger)
        };
        // The opening roll is one die per player, so it is never doubles.
        let opening = loop {
            let roll = [rng.gen_range(1..=6u8), rng.gen_range(1..=6u8)];
            if roll[0] != roll[1] {
                break roll;
            }
        };
        Self {
            version: STATE_VERSION,
            revision: 0,
            white,
            red,
            turns: Vec::new(),
            next_roll: Some(opening),
        }
    }

    pub fn parse(value: &Value) -> Result<Self> {
        let state: Self =
            serde_json::from_value(value.clone()).context("corrupt daily match state")?;
        ensure!(
            state.version == STATE_VERSION,
            "unsupported daily backgammon state version: {}",
            state.version
        );
        Ok(state)
    }

    pub fn user_of(&self, color: Color) -> Uuid {
        match color {
            Color::White => self.white,
            Color::Red => self.red,
        }
    }

    pub fn color_of(&self, user_id: Uuid) -> Option<Color> {
        if user_id == self.white {
            Some(Color::White)
        } else if user_id == self.red {
            Some(Color::Red)
        } else {
            None
        }
    }

    /// Whose turn it is. Forced passes are recorded as turns, so this is
    /// plain parity: white on even turn counts, red on odd.
    pub fn turn(&self) -> Color {
        if self.turns.len().is_multiple_of(2) {
            Color::White
        } else {
            Color::Red
        }
    }

    /// Rebuild the position from the turn history.
    pub fn board(&self) -> Board {
        let mut board = Board::start();
        let mut color = Color::White;
        for turn in &self.turns {
            for &hop in &turn.hops {
                apply_hop(&mut board, color, hop);
            }
            color = color.other();
        }
        board
    }

    pub fn move_count(&self) -> usize {
        self.turns.len()
    }

    pub fn last_turn(&self) -> Option<&Turn> {
        self.turns.last()
    }

    pub fn is_finished(&self) -> bool {
        !matches!(self.status(), BackgammonStatus::Ongoing)
    }

    /// The current verdict: fifteen borne off wins, and the defensive stall
    /// cap (a hundred recorded passes in a row) draws a match that can no
    /// longer move.
    pub fn status(&self) -> BackgammonStatus {
        let board = self.board();
        if board.off[Color::White.idx()] == CHECKERS_PER_SIDE {
            return BackgammonStatus::Win(Color::White);
        }
        if board.off[Color::Red.idx()] == CHECKERS_PER_SIDE {
            return BackgammonStatus::Win(Color::Red);
        }
        let stalled = self
            .turns
            .iter()
            .rev()
            .take_while(|turn| turn.hops.is_empty())
            .count();
        if stalled >= STALL_PASSES {
            BackgammonStatus::Draw
        } else {
            BackgammonStatus::Ongoing
        }
    }

    /// The complete legal turns for the side to move with the pending roll:
    /// every maximal hop sequence. Empty when there is no pending roll (the
    /// server's roll is still in flight) or no hop is playable.
    pub fn legal_turns(&self) -> Vec<Vec<Hop>> {
        let Some(roll) = self.next_roll else {
            return Vec::new();
        };
        legal_turns(&self.board(), self.turn(), roll)
    }

    /// Play a full turn for the side to move. Validates the hops against the
    /// legal turn set (which is where bar entry, maximal-dice, higher-die,
    /// and bear-off rules are enforced), records it, and clears `next_roll`;
    /// the server follows up with `roll_next` (a client applying the same
    /// turn optimistically must not roll, so the split is deliberate).
    pub fn apply_turn(&mut self, hops: &[Hop]) -> Result<TurnOutcome> {
        let roll = self.next_roll.context("no roll to play")?;
        let color = self.turn();
        let board = self.board();
        ensure!(
            legal_turns(&board, color, roll)
                .iter()
                .any(|legal| legal.as_slice() == hops),
            "that is not a legal move"
        );
        let mut work = board;
        let hits: Vec<bool> = hops
            .iter()
            .map(|&hop| apply_hop(&mut work, color, hop))
            .collect();

        self.turns.push(Turn {
            roll,
            hops: hops.to_vec(),
        });
        self.next_roll = None;

        let (winner, finished) = match self.status() {
            BackgammonStatus::Win(c) => (Some(c), true),
            BackgammonStatus::Draw => (None, true),
            BackgammonStatus::Ongoing => (None, false),
        };
        Ok(TurnOutcome {
            color,
            roll,
            hits,
            finished,
            winner,
        })
    }

    /// The position with a partial turn's hops applied on top: what the
    /// mover sees while building a turn hop by hop (the state itself only
    /// changes when the whole turn is played).
    pub fn preview(&self, hops: &[Hop]) -> Board {
        let mut board = self.board();
        let color = self.turn();
        for &hop in hops {
            apply_hop(&mut board, color, hop);
        }
        board
    }

    /// Server-side only: roll for the next mover, recording forced passes
    /// until someone can play (or the stall cap draws the match). Leaves
    /// `next_roll` set for the side `turn()` then points at, or `None` when
    /// the match is over.
    pub fn roll_next(&mut self) {
        let mut rng = rand::thread_rng();
        loop {
            if !matches!(self.status(), BackgammonStatus::Ongoing) {
                self.next_roll = None;
                return;
            }
            let roll = [rng.gen_range(1..=6u8), rng.gen_range(1..=6u8)];
            if !legal_turns(&self.board(), self.turn(), roll).is_empty() {
                self.next_roll = Some(roll);
                return;
            }
            self.turns.push(Turn {
                roll,
                hops: Vec::new(),
            });
        }
    }
}

/// Where one die takes a checker, before the landing is checked: the next
/// point index, `OFF` when it runs past the edge, entry from the bar.
fn hop_dest(color: Color, from: u8, die: u8) -> u8 {
    match color {
        Color::White => {
            if from == BAR {
                POINTS as u8 - die
            } else if from >= die {
                from - die
            } else {
                OFF
            }
        }
        Color::Red => {
            if from == BAR {
                die - 1
            } else if from + die < POINTS as u8 {
                from + die
            } else {
                OFF
            }
        }
    }
}

/// A landing point is open unless the opponent holds it with two or more.
fn landing_open(board: &Board, color: Color, point: u8) -> bool {
    board.count(color.other(), point as usize) <= 1
}

/// Bearing off from `from` with `die`, for a side that is all home: exact
/// distance always works; a bigger die works only from the farthest
/// occupied point (no checker farther from home).
fn can_bear_off(board: &Board, color: Color, from: u8, die: u8) -> bool {
    let distance = match color {
        Color::White => from + 1,
        Color::Red => POINTS as u8 - from,
    };
    if die < distance {
        return false;
    }
    if die == distance {
        return true;
    }
    let farther: Vec<usize> = match color {
        Color::White => (from as usize + 1..6).collect(),
        Color::Red => (18..from as usize).collect(),
    };
    farther.into_iter().all(|p| board.count(color, p) == 0)
}

/// Every playable hop for one die: bar entry alone while any checker waits
/// on the bar, otherwise moves and (when all home) bear-offs.
fn hops_for_die(board: &Board, color: Color, die: u8) -> Vec<Hop> {
    if board.bar[color.idx()] > 0 {
        let entry = hop_dest(color, BAR, die);
        return if landing_open(board, color, entry) {
            vec![(BAR, entry)]
        } else {
            Vec::new()
        };
    }
    let all_home = board.all_home(color);
    let mut hops = Vec::new();
    for from in 0..POINTS as u8 {
        if board.count(color, from as usize) == 0 {
            continue;
        }
        let to = hop_dest(color, from, die);
        if to == OFF {
            if all_home && can_bear_off(board, color, from, die) {
                hops.push((from, OFF));
            }
        } else if landing_open(board, color, to) {
            hops.push((from, to));
        }
    }
    hops
}

/// Apply one hop in place; returns whether it hit a lone opposing checker.
fn apply_hop(board: &mut Board, color: Color, (from, to): Hop) -> bool {
    if from == BAR {
        board.bar[color.idx()] -= 1;
    } else {
        let n = board.count(color, from as usize);
        board.set(color, from as usize, n.saturating_sub(1));
    }
    if to == OFF {
        board.off[color.idx()] += 1;
        return false;
    }
    let hit = board.count(color.other(), to as usize) == 1;
    if hit {
        board.bar[color.other().idx()] += 1;
        board.set(color, to as usize, 0);
    }
    let n = board.count(color, to as usize);
    board.set(color, to as usize, n + 1);
    hit
}

/// Every complete legal turn for `color` with `roll`: all maximal hop
/// sequences (both die orders for a non-double, four dice for a double),
/// with the higher die forced when only a single die can be played. Empty
/// means the turn is a forced pass.
pub fn legal_turns(board: &Board, color: Color, roll: [u8; 2]) -> Vec<Vec<Hop>> {
    let orders: Vec<Vec<u8>> = if roll[0] == roll[1] {
        vec![vec![roll[0]; 4]]
    } else {
        vec![vec![roll[0], roll[1]], vec![roll[1], roll[0]]]
    };
    let mut sequences = Vec::new();
    for dice in &orders {
        extend_turns(board, color, dice, &mut Vec::new(), &mut sequences);
    }
    let max = sequences.iter().map(Vec::len).max().unwrap_or(0);
    if max == 0 {
        return Vec::new();
    }
    sequences.retain(|sequence| sequence.len() == max);
    // Only one die playable: the higher one must be, when it can be.
    if max == 1 && roll[0] != roll[1] {
        let high = hops_for_die(board, color, roll[0].max(roll[1]));
        if !high.is_empty() {
            sequences.retain(|sequence| high.contains(&sequence[0]));
        }
    }
    sequences.sort();
    sequences.dedup();
    sequences
}

/// Depth-first over the dice in `dice` order, recording the path wherever it
/// can go no further (the maximal filter happens in `legal_turns`).
fn extend_turns(
    board: &Board,
    color: Color,
    dice: &[u8],
    path: &mut Vec<Hop>,
    out: &mut Vec<Vec<Hop>>,
) {
    let Some((&die, rest)) = dice.split_first() else {
        out.push(path.clone());
        return;
    };
    let hops = hops_for_die(board, color, die);
    if hops.is_empty() {
        out.push(path.clone());
        return;
    }
    for hop in hops {
        let mut next = *board;
        apply_hop(&mut next, color, hop);
        path.push(hop);
        extend_turns(&next, color, rest, path, out);
        path.pop();
    }
}

// ── Cursor slots ───────────────────────────────────────────────
//
// The board screen's cursor walks a visual 2 x 14 grid: two rows of six
// point columns, the bar column between them, and the off tray at the right
// edge. The mapping to a semantic target depends on which seat the viewer
// has (their home board is drawn bottom-right), so it lives here as pure
// coordinate math the renderer, the input layer, and the tests all share.

pub const SLOT_COLS: usize = 14;
pub const SLOT_ROWS: usize = 2;
pub const BAR_COL: usize = 6;
pub const OFF_COL: usize = 13;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BgTarget {
    Point(usize),
    Bar,
    Off,
}

impl BgTarget {
    /// The `Hop` endpoint encoding: the point index, or the sentinels.
    pub fn code(self) -> u8 {
        match self {
            Self::Point(p) => p as u8,
            Self::Bar => BAR,
            Self::Off => OFF,
        }
    }
}

/// Visual slot (row-major over the 2 x 14 grid) to semantic target, from
/// `seat`'s perspective. Bottom-right is `seat`'s 1..6 home board.
pub fn slot_target(slot: usize, seat: Color) -> Option<BgTarget> {
    if slot >= SLOT_ROWS * SLOT_COLS {
        return None;
    }
    let (row, col) = (slot / SLOT_COLS, slot % SLOT_COLS);
    if col == BAR_COL {
        return Some(BgTarget::Bar);
    }
    if col == OFF_COL {
        return Some(BgTarget::Off);
    }
    let point_col = if col < BAR_COL { col } else { col - 1 };
    // From white's seat: the bottom row reads 12..1 left to right (indices
    // 11..0), the top row 13..24 (indices 12..23).
    let white_point = if row == 1 {
        11 - point_col
    } else {
        12 + point_col
    };
    Some(BgTarget::Point(match seat {
        Color::White => white_point,
        Color::Red => 23 - white_point,
    }))
}

#[cfg(test)]
#[path = "backgammon_test.rs"]
mod backgammon_test;

