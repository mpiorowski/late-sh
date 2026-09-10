//! The rules layer: what a shot meant.
//!
//! A closed enum with exhaustive matches, not trait objects — the same shape
//! the daily roster and the house tables use. Adding snooker later is a third
//! variant plus a third `rules_*.rs`; the compiler then walks every match here
//! and refuses to build until each is answered.
//!
//! **This layer never sees a physics type.** It reads `GameState` (positions
//! plus whose turn it is) and `ShotOutcome` (what happened), and returns a
//! `Ruling`. That is what makes every ruleset testable from struct literals
//! with no table, no simulation, and no floating point.
//!
//! `Ruling` carries `points` and `balls_to_spot` from the start even though
//! neither v1 game scores points and only nine-ball spots anything. Snooker
//! needs both, and adding a field to a struct every ruleset already returns is
//! a much smaller change than adding one to a shape they do not.

use serde::{Deserialize, Serialize};

use crate::app::games::pool_core::{
    ball::CUE,
    rack::RackKind,
    rules_eight, rules_nine, rules_snooker,
    shot::{RackState, ShotOutcome},
    table::{Geometry, TableSpec},
};

/// Which of the two players is at the table. Matches are always two-handed.
pub type Seat = u8;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Group {
    Solids,
    Stripes,
}

impl Group {
    pub fn other(self) -> Self {
        match self {
            Self::Solids => Self::Stripes,
            Self::Stripes => Self::Solids,
        }
    }

    pub fn contains(self, id: u8) -> bool {
        match self {
            Self::Solids => (1..=7).contains(&id),
            Self::Stripes => (9..=15).contains(&id),
        }
    }
}

/// Where the incoming player may place the cue ball.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BallInHand {
    /// Anywhere on the table. The ordinary penalty for a foul.
    Anywhere,
    /// Behind the head string only — bar rules after a scratch on the break.
    Kitchen,
    /// Snooker: in hand from the D, which is a semicircle on the baulk line
    /// rather than the whole of the area behind it.
    TheD,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Turn {
    Keep,
    Pass,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Foul {
    /// Cue ball potted.
    Scratch,
    /// Cue ball touched nothing.
    NoContact,
    /// Cue ball's first contact was not a legal target.
    WrongBallFirst,
    /// Nothing potted and no ball reached a rail after contact.
    NoRail,
    /// The break did not put a ball down or send four to the rails.
    IllegalBreak,
    /// The cue ball was placed somewhere it may not be.
    BadPlacement,
}

impl Foul {
    /// A short phrase for the board's status line.
    pub fn label(self) -> &'static str {
        match self {
            Self::Scratch => "scratch",
            Self::NoContact => "no contact",
            Self::WrongBallFirst => "wrong ball first",
            Self::NoRail => "no rail after contact",
            Self::IllegalBreak => "illegal break",
            Self::BadPlacement => "illegal cue ball placement",
        }
    }
}

/// The rules layer's verdict on one shot.
#[derive(Clone, Debug, PartialEq)]
pub struct Ruling {
    pub turn: Turn,
    pub foul: Option<Foul>,
    pub ball_in_hand: Option<BallInHand>,
    /// Balls returned to the table, by id. The caller re-spots them.
    pub balls_to_spot: Vec<u8>,
    /// Points to the striker. Snooker's alone: the pool games score nothing.
    pub points: i32,
    /// Points to the *other* seat, for a foul. Kept apart from `points` rather
    /// than signed, because a snooker foul can both score the opponent and
    /// leave balls to re-spot, and a single number could not say which.
    pub penalty: i32,
    /// Snooker: the striker has just potted a red and is on a colour.
    pub next_on_colour: bool,
    /// Snooker: the incoming player was left snookered by a foul and may treat
    /// any ball as the ball on.
    pub free_ball: bool,
    /// Every ball is gone. Who won is then a matter of the score, which this
    /// layer does not keep.
    pub frame_over: bool,
    /// Set on the shot that opens the table in eight-ball. Indexed by seat.
    pub group_assignment: Option<[Group; 2]>,
    pub winner: Option<Seat>,
}

impl Ruling {
    /// The common case: a clean shot that hands the table over.
    pub fn pass() -> Self {
        Self {
            turn: Turn::Pass,
            foul: None,
            ball_in_hand: None,
            balls_to_spot: Vec::new(),
            points: 0,
            penalty: 0,
            next_on_colour: false,
            free_ball: false,
            frame_over: false,
            group_assignment: None,
            winner: None,
        }
    }

    /// A clean shot that potted something, so the shooter stays at the table.
    pub fn keep() -> Self {
        Self {
            turn: Turn::Keep,
            ..Self::pass()
        }
    }

    /// An ordinary foul: turn over, cue ball in hand anywhere.
    pub fn foul(foul: Foul) -> Self {
        Self {
            foul: Some(foul),
            ball_in_hand: Some(BallInHand::Anywhere),
            ..Self::pass()
        }
    }

    pub fn won_by(seat: Seat) -> Self {
        Self {
            winner: Some(seat),
            ..Self::pass()
        }
    }
}

/// Everything the rules need to know before a shot.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GameState {
    pub rack: RackState,
    pub turn: Seat,
    /// Eight-ball group per seat. `None` while the table is open, and always
    /// `None` in nine-ball.
    pub groups: Option<[Group; 2]>,
    /// Shots played so far. Zero means the break is still to come.
    pub shots_taken: u32,
    pub ball_in_hand: Option<BallInHand>,
    /// Snooker: the striker potted a red and is now on a colour of their
    /// choosing. Always false in the pool games.
    pub on_colour: bool,
    /// Snooker: this striker was left snookered by a foul and may treat any
    /// ball as the ball on.
    pub free_ball: bool,
}

impl GameState {
    pub fn is_break(&self) -> bool {
        self.shots_taken == 0
    }

    pub fn group_of(&self, seat: Seat) -> Option<Group> {
        self.groups.map(|g| g[seat as usize])
    }

    pub fn on_table(&self, id: u8) -> bool {
        self.rack.get(id).is_some_and(|b| b.potted.is_none())
    }

    /// Lowest-numbered object ball still up. Nine-ball's legal target, and
    /// eight-ball uses it for nothing.
    pub fn lowest_on_table(&self) -> Option<u8> {
        self.rack
            .on_table()
            .map(|b| b.id)
            .filter(|id| *id != CUE)
            .min()
    }
}

pub fn other_seat(seat: Seat) -> Seat {
    1 - seat
}

/// The roster of rulesets. Exhaustive matches only — no `_ =>` arms.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PoolRules {
    EightBall,
    NineBall,
    Snooker,
}

impl PoolRules {
    pub const ALL: [Self; 3] = [Self::EightBall, Self::NineBall, Self::Snooker];

    pub fn rack_kind(self) -> RackKind {
        match self {
            Self::EightBall => RackKind::EightBall,
            Self::NineBall => RackKind::NineBall,
            Self::Snooker => RackKind::Snooker,
        }
    }

    /// Balls the cue ball may legally strike first.
    pub fn legal_targets(self, state: &GameState) -> Vec<u8> {
        match self {
            Self::EightBall => rules_eight::legal_targets(state),
            Self::NineBall => rules_nine::legal_targets(state),
            Self::Snooker => rules_snooker::legal_targets(state),
        }
    }

    /// Whether the shooter must name a pocket for this shot. Only the eight
    /// is ever called — see `rules_eight`.
    pub fn requires_call(self, state: &GameState) -> bool {
        match self {
            Self::EightBall => rules_eight::requires_call(state),
            Self::NineBall | Self::Snooker => false,
        }
    }

    pub fn judge(
        self,
        state: &GameState,
        outcome: &ShotOutcome,
        called_pocket: Option<u8>,
        snookered: bool,
    ) -> Ruling {
        match self {
            Self::EightBall => rules_eight::judge(state, outcome, called_pocket),
            Self::NineBall => rules_nine::judge(state, outcome),
            Self::Snooker => rules_snooker::judge(state, outcome, snookered),
        }
    }

    /// Whether this ruleset keeps a score. Only snooker does, and the board
    /// asks before finding room for two numbers it would otherwise leave at
    /// nought.
    pub fn scores(self) -> bool {
        matches!(self, Self::Snooker)
    }
}

/// Balls that must reach a rail for a break to count when nothing is potted.
pub const BREAK_RAIL_COUNT: usize = 4;

/// Shared break test: a break is legal if it puts a ball down or drives four
/// balls to a rail. Both v1 games use it; snooker has no break rule at all.
pub fn break_was_legal(outcome: &ShotOutcome) -> bool {
    outcome.potted_object_balls().next().is_some()
        || outcome.balls_to_rail.len() >= BREAK_RAIL_COUNT
}

/// Shared "did the shooter do anything at all" test, applied after the legal
/// target has already been checked.
pub fn stalled(outcome: &ShotOutcome) -> bool {
    outcome.potted.is_empty() && !outcome.cushion_after_contact
}

/// Whether the cue ball may be placed at `at`.
///
/// Three ways to be wrong: off the table, on top of another ball, or outside
/// the kitchen when the placement is restricted to it. The cushion test uses
/// the same `Geometry` the simulator does, so a spot that passes here cannot
/// be one the physics immediately rejects.
pub fn placement_ok(
    spec: &TableSpec,
    geom: &Geometry,
    rack: &RackState,
    at: [f64; 2],
    zone: BallInHand,
) -> bool {
    if !at[0].is_finite() || !at[1].is_finite() {
        return false;
    }
    let r = spec.ball_radius;

    // Inside every cushion, and not through a pocket mouth.
    for cushion in &geom.cushions {
        let (d, _) = Geometry::cushion_separation(cushion, at);
        if d < r {
            return false;
        }
    }
    for pocket in &geom.pockets {
        if Geometry::pocket_depth(pocket, at) > 0.0 {
            return false;
        }
    }
    // A pocket mouth is a gap in the rails, so a point beyond the table that
    // happens to line up with one clears both loops above. Bound it outright.
    if at[0] < r || at[0] > spec.length - r || at[1] < r || at[1] > spec.width - r {
        return false;
    }

    for ball in rack.on_table() {
        if ball.id == CUE {
            continue;
        }
        let dx = ball.pos[0] - at[0];
        let dy = ball.pos[1] - at[1];
        if (dx * dx + dy * dy).sqrt() < 2.0 * r {
            return false;
        }
    }

    match zone {
        BallInHand::Anywhere => true,
        BallInHand::Kitchen => at[0] < head_string(spec),
        BallInHand::TheD => {
            let head = head_string(spec);
            at[0] <= head && (at[0] - head).hypot(at[1] - spec.width / 2.0) <= spec.d_radius
        }
    }
}

/// The head string: the line the kitchen sits behind. Carried on the spec,
/// because a snooker baulk line is not a quarter of the table.
pub fn head_string(spec: &TableSpec) -> f64 {
    spec.head_string
}

/// A legal resting place at or near `preferred`.
///
/// Needed in two places that both have to cope with the obvious spot being
/// taken: re-spotting a ball the rules sent back up (`Ruling::balls_to_spot`),
/// and placing the cue ball for a player who has ball in hand. Pool's own rule
/// is "on the spot, or as near behind it as possible on the long string", so
/// the search walks the spot line first — down-table, then up-table — and only
/// then spirals out.
pub fn free_spot(
    spec: &TableSpec,
    geom: &Geometry,
    rack: &RackState,
    preferred: [f64; 2],
    zone: BallInHand,
) -> Option<[f64; 2]> {
    let step = spec.ball_radius * 0.5;
    let ok = |at: [f64; 2]| placement_ok(spec, geom, rack, at, zone);

    if ok(preferred) {
        return Some(preferred);
    }
    // Along the long string, behind the spot first (the actual rule), then in
    // front of it.
    for direction in [1.0, -1.0] {
        let mut at = preferred;
        for _ in 0..((spec.length / step) as u32) {
            at[0] += direction * step;
            if at[0] < 0.0 || at[0] > spec.length {
                break;
            }
            if ok(at) {
                return Some(at);
            }
        }
    }
    // Last resort: sweep the table. Only reachable on a table so crowded that
    // the whole spot line is blocked, which a fifteen-ball rack can manage.
    let mut y = spec.ball_radius;
    while y < spec.width {
        let mut x = spec.ball_radius;
        while x < spec.length {
            if ok([x, y]) {
                return Some([x, y]);
            }
            x += step;
        }
        y += step;
    }
    None
}
