//! Per-session city view state: where your runner stands, the animation
//! clock, the shop panel that is open, and the last thing the street said
//! to you. Pure: no I/O, no clock reads (the tick is handed in).
//!
//! The city is the wallet (GAME.md, "The three surfaces"): nothing happens
//! here that you could miss, so there is no shared lobby and no crowd. One
//! runner, one street, the doors you walk up to.

use super::map::{self, Landmark};

/// How long a street line (a vendor's remark) stays pinned, in animation
/// ticks (~7.5 per second).
const LINE_TICKS: u64 = 60;
/// How far one run goes.
pub const RUN_STEPS: u16 = 6;

/// What Enter does at the landmark within reach.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Enter {
    /// Opens the landmark's panel (the shops and the board).
    Panel(Landmark),
    /// The street answers with a line (carts, the screen, the stairs).
    Line(Landmark),
    /// Back up the wire: the way out of the city.
    Leave,
}

impl Landmark {
    /// What Enter does here. Shops open; the street talks; the wire leaves.
    pub fn on_enter(self) -> Enter {
        match self {
            Landmark::Armorer
            | Landmark::Tailor
            | Landmark::Lockers
            | Landmark::Bands
            | Landmark::Bar
            | Landmark::Repairs
            | Landmark::Board
            | Landmark::Bits => Enter::Panel(self),
            Landmark::Screen
            | Landmark::Noodles
            | Landmark::Umbrellas
            | Landmark::Blades
            | Landmark::Reader
            | Landmark::Stairs => Enter::Line(self),
            Landmark::Wire => Enter::Leave,
        }
    }
}

#[derive(Debug, Clone)]
pub struct State {
    pub player_x: u16,
    pub player_y: u16,
    /// Wall-synced animation clock (`marquee_tick`), mirrored on each tick.
    pub anim_tick: u64,
    panel: Option<Landmark>,
    /// A line the street said, and the tick it stops showing.
    line: Option<(Landmark, usize, u64)>,
}

impl Default for State {
    fn default() -> Self {
        Self::new()
    }
}

impl State {
    pub fn new() -> Self {
        Self {
            player_x: map::SPAWN.0,
            player_y: map::SPAWN.1,
            anim_tick: 0,
            panel: None,
            line: None,
        }
    }

    /// Advance the animation clock; expire a street line whose time is up.
    pub fn tick(&mut self, wall_tick: u64) {
        self.anim_tick = wall_tick;
        if let Some((_, _, until)) = self.line
            && wall_tick >= until
        {
            self.line = None;
        }
    }

    /// One step. Blocked cells (walls, facades, carts, the drop) hold the
    /// runner where it is; returns whether it moved.
    pub fn walk(&mut self, dx: i32, dy: i32) -> bool {
        let nx = self.player_x.saturating_add_signed(dx as i16);
        let ny = self.player_y.saturating_add_signed(dy as i16);
        if !map::walkable(nx, ny) {
            return false;
        }
        self.player_x = nx;
        self.player_y = ny;
        true
    }

    /// Several steps in one go (Shift+arrow, `HJKL`): up to `RUN_STEPS`,
    /// stopping at anything solid and at the first landmark that comes
    /// within reach, so a run never overshoots a door. Returns the steps
    /// taken.
    pub fn run(&mut self, dx: i32, dy: i32) -> u16 {
        let mut steps = 0;
        while steps < RUN_STEPS {
            if !self.walk(dx, dy) {
                break;
            }
            steps += 1;
            if self.nearby().is_some() {
                break;
            }
        }
        steps
    }

    /// The landmark within reach of the runner, if any.
    pub fn nearby(&self) -> Option<Landmark> {
        map::nearest_landmark(self.player_x, self.player_y)
    }

    pub fn panel(&self) -> Option<Landmark> {
        self.panel
    }

    pub fn open_panel(&mut self, landmark: Landmark) {
        self.panel = Some(landmark);
    }

    pub fn close_panel(&mut self) {
        self.panel = None;
    }

    /// The street says something: which landmark spoke and which line of
    /// its pool. The pool index is picked by the caller from the pool size,
    /// so this state stays free of the copy.
    pub fn say(&mut self, landmark: Landmark, line_index: usize) {
        self.line = Some((landmark, line_index, self.anim_tick + LINE_TICKS));
    }

    /// The pinned street line, if one is showing.
    pub fn line(&self) -> Option<(Landmark, usize)> {
        self.line.map(|(landmark, index, _)| (landmark, index))
    }
}

#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;
