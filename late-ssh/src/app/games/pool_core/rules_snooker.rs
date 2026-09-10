//! Snooker: fifteen reds, six colours, and the only ruleset here that scores.
//!
//! ## What is real and what is not
//!
//! The frame is the real one — fifteen reds, colours re-spotted while a red
//! remains, then the six in order, and the frame decided on points rather than
//! on who potted the last ball. Fouls carry the real penalties, the fouled
//! player may make the offender play again, and a foul that leaves them
//! snookered awards a free ball.
//!
//! What is left out is the part that needs a referee's judgement rather than a
//! rule: the miss rule, and touching balls. Both turn on intent, and a
//! correspondence game has nobody to ask.
//!
//! ## Nomination is inferred, not asked for
//!
//! Real snooker has the striker *declare* which colour they are on, and a free
//! ball nominates too. Asking for that would be a second input on every other
//! shot, so instead **the ball you hit first is the one you nominated**. That
//! is how the game is played among friends, it is unambiguous after the fact,
//! and it makes the free ball rule free of new input: with a free ball,
//! whatever you strike first *is* the ball on.
//!
//! ## Ball numbering
//!
//! Snooker's balls take their own id range so nothing is ambiguous across
//! rulesets: reds are 16..=30 and the colours 31..=36, ascending by value. A
//! pool ball's id is its printed number and always will be; a snooker ball has
//! no number, so its id is free to be an index.

use crate::app::games::pool_core::{
    ball::CUE,
    rules::{BallInHand, Foul, GameState, Ruling, Turn, other_seat},
    shot::{RackState, ShotOutcome},
    table::{Geometry, TableSpec},
};

/// The fifteen reds.
pub const RED_FIRST: u8 = 16;
pub const RED_LAST: u8 = 30;
/// The six colours, ascending by value, which is also ascending by id.
pub const YELLOW: u8 = 31;
pub const GREEN: u8 = 32;
pub const BROWN: u8 = 33;
pub const BLUE: u8 = 34;
pub const PINK: u8 = 35;
pub const BLACK: u8 = 36;
/// The colours in the order they are taken once the reds are gone.
pub const COLOURS: [u8; 6] = [YELLOW, GREEN, BROWN, BLUE, PINK, BLACK];

/// The minimum any foul costs, whatever it was.
const MIN_PENALTY: i32 = 4;

pub fn is_red(id: u8) -> bool {
    (RED_FIRST..=RED_LAST).contains(&id)
}

pub fn is_colour(id: u8) -> bool {
    COLOURS.contains(&id)
}

/// What a ball is worth. A red is one; a colour is its position in the order
/// plus one, which is the same as saying yellow 2 through black 7.
pub fn value(id: u8) -> i32 {
    if is_red(id) {
        return 1;
    }
    match COLOURS.iter().position(|c| *c == id) {
        Some(index) => index as i32 + 2,
        None => 0,
    }
}

/// Balls the striker may legally hit first.
///
/// Three cases, in the order the frame goes through them: on a red while any
/// red is up, on a colour of your choosing straight after potting one, and on
/// the lowest remaining colour once the reds are gone. A free ball overrides
/// all of it — anything on the table is the ball on.
pub fn legal_targets(state: &GameState) -> Vec<u8> {
    if state.free_ball {
        return state
            .rack
            .on_table()
            .map(|ball| ball.id)
            .filter(|id| *id != CUE)
            .collect();
    }
    let reds: Vec<u8> = (RED_FIRST..=RED_LAST)
        .filter(|id| state.on_table(*id))
        .collect();
    if !reds.is_empty() {
        return if state.on_colour {
            COLOURS
                .into_iter()
                .filter(|id| state.on_table(*id))
                .collect()
        } else {
            reds
        };
    }
    // Reds gone: the colours come back in order and stay down.
    COLOURS
        .into_iter()
        .find(|id| state.on_table(*id))
        .into_iter()
        .collect()
}

/// Judge one shot.
///
/// `snookered` is whether the *incoming* player would be left unable to hit
/// any ball on — the caller works it out, because it needs the table geometry
/// and this layer deliberately never sees any.
pub fn judge(state: &GameState, outcome: &ShotOutcome, snookered: bool) -> Ruling {
    let targets = legal_targets(state);

    // What would have been on *without* the free ball. A free ball makes every
    // ball strikeable, but it is still worth — and still counts as — the ball
    // it stands in for, so potting the black off a free ball while on a red
    // scores one and not seven.
    let real_on = {
        let mut plain = state.clone();
        plain.free_ball = false;
        legal_targets(&plain)
    };
    let on_a_red = real_on.first().copied().is_some_and(is_red);

    // Everything the shot did wrong, in the order a referee calls it. The
    // penalty is the value of the ball on or of the ball at fault, whichever
    // is higher, and never less than four.
    let ball_on_value = real_on.iter().copied().map(value).max().unwrap_or(0);
    let mut fault = None;
    let mut at_fault = ball_on_value;

    if outcome.cue_potted {
        fault = Some(Foul::Scratch);
    } else if let Some(hit) = outcome.first_contact {
        if !targets.contains(&hit) {
            fault = Some(Foul::WrongBallFirst);
            at_fault = at_fault.max(value(hit));
        }
    } else {
        fault = Some(Foul::NoContact);
    }

    // Potting a ball that was not on is a foul at that ball's value — which is
    // how a snooker foul gets expensive, since the black is worth seven.
    let potted: Vec<u8> = outcome.potted_object_balls().collect();
    if fault.is_none() {
        for id in &potted {
            // With a free ball the ball you struck is the one you nominated,
            // so potting it is potting the ball on.
            let nominated = state.free_ball && outcome.first_contact == Some(*id);
            if !real_on.contains(id) && !nominated {
                fault = Some(Foul::WrongBallFirst);
                at_fault = at_fault.max(value(*id));
            }
        }
    }
    if fault.is_none() && potted.is_empty() && stalled_after_contact(outcome) {
        fault = Some(Foul::NoRail);
    }

    if let Some(foul) = fault {
        // Everything potted goes back up, except reds, which stay down even
        // when the shot was a foul.
        let mut spot: Vec<u8> = potted.iter().copied().filter(|id| !is_red(*id)).collect();
        spot.sort_unstable();
        return Ruling {
            turn: Turn::Pass,
            foul: Some(foul),
            ball_in_hand: outcome.cue_potted.then_some(BallInHand::TheD),
            balls_to_spot: spot,
            points: 0,
            penalty: at_fault.max(MIN_PENALTY),
            group_assignment: None,
            next_on_colour: false,
            free_ball: snookered,
            frame_over: false,
            winner: None,
        };
    }

    if potted.is_empty() {
        return Ruling {
            turn: Turn::Pass,
            next_on_colour: false,
            ..Ruling::pass()
        };
    }

    // A legal pot. Score it, and work out what the striker is on next.
    let scored: i32 = potted
        .iter()
        .map(|id| {
            if real_on.contains(id) {
                value(*id)
            } else {
                // The free ball, worth what it stood in for.
                ball_on_value
            }
        })
        .sum();
    // A free ball potted while on a red *is* a red for the purpose of what
    // comes next: the striker is on a colour.
    let potted_a_red = on_a_red && !potted.is_empty();
    let reds_left = (RED_FIRST..=RED_LAST).any(|id| state.on_table(id) && !potted.contains(&id));

    // Colours go back up while a red is still on the table, and stay down
    // once the reds are gone. That single line is the shape of a frame.
    let mut spot: Vec<u8> = if reds_left {
        potted.iter().copied().filter(|id| !is_red(*id)).collect()
    } else {
        Vec::new()
    };
    spot.sort_unstable();

    // The frame ends when the last ball is gone — which is the black, since
    // the colours come back in order.
    let nothing_left = !reds_left
        && COLOURS
            .into_iter()
            .all(|id| !state.on_table(id) || potted.contains(&id))
        && spot.is_empty();

    Ruling {
        turn: Turn::Keep,
        foul: None,
        ball_in_hand: None,
        balls_to_spot: spot,
        points: scored,
        penalty: 0,
        group_assignment: None,
        next_on_colour: potted_a_red && reds_left,
        free_ball: false,
        frame_over: nothing_left,
        winner: None,
    }
}

/// Nothing potted and nothing reached a cushion: a foul in snooker as much as
/// in pool, and for the same reason — a shot that does nothing is a shot that
/// refuses to play.
fn stalled_after_contact(outcome: &ShotOutcome) -> bool {
    !outcome.cushion_after_contact
}

/// Can the striker hit any ball that is on?
///
/// The real rule asks whether *both extreme edges* of every ball on are
/// obstructed, so this tests the two grazing paths rather than the line
/// between centres: a ball you can only clip is still a ball you can hit, and
/// awarding a free ball there would be handing out points for a shot that
/// exists.
pub fn is_snookered(spec: &TableSpec, _geom: &Geometry, rack: &RackState, targets: &[u8]) -> bool {
    let Some(cue) = rack.get(CUE).filter(|b| b.potted.is_none()) else {
        return false;
    };
    let r = spec.ball_radius;
    targets.iter().all(|id| {
        let Some(target) = rack.get(*id).filter(|b| b.potted.is_none()) else {
            return true;
        };
        let to = [target.pos[0] - cue.pos[0], target.pos[1] - cue.pos[1]];
        let len = to[0].hypot(to[1]);
        if len < 1e-9 {
            return false;
        }
        // The two ghost-ball centres for a grazing hit on either edge.
        let perp = [-to[1] / len, to[0] / len];
        [-1.0f64, 1.0].iter().all(|side| {
            let ghost = [
                target.pos[0] + perp[0] * side * 2.0 * r,
                target.pos[1] + perp[1] * side * 2.0 * r,
            ];
            blocked(rack, cue.pos, ghost, *id, r)
        })
    })
}

/// Is the cue ball's path from `from` to the ghost centre `to` obstructed by
/// anything other than the ball being aimed at?
fn blocked(rack: &RackState, from: [f64; 2], to: [f64; 2], target: u8, r: f64) -> bool {
    let d = [to[0] - from[0], to[1] - from[1]];
    let len2 = d[0] * d[0] + d[1] * d[1];
    if len2 < 1e-12 {
        return false;
    }
    rack.on_table()
        .filter(|ball| ball.id != CUE && ball.id != target)
        .any(|ball| {
            let ap = [ball.pos[0] - from[0], ball.pos[1] - from[1]];
            let t = ((ap[0] * d[0] + ap[1] * d[1]) / len2).clamp(0.0, 1.0);
            let closest = [from[0] + d[0] * t, from[1] + d[1] * t];
            (ball.pos[0] - closest[0]).hypot(ball.pos[1] - closest[1]) < 2.0 * r
        })
}

/// Who has won a finished frame: the higher score, or nobody on a tie.
///
/// A real tie re-spots the black and plays for it. That is a whole extra frame
/// state for an outcome that turns up once in a very long while, and the daily
/// domain already knows how to record a draw.
pub fn frame_winner(scores: [i32; 2]) -> Option<u8> {
    match scores[0].cmp(&scores[1]) {
        std::cmp::Ordering::Greater => Some(0),
        std::cmp::Ordering::Less => Some(1),
        std::cmp::Ordering::Equal => None,
    }
}

/// The penalty for a foul, handed to the other seat.
pub fn award(seat: u8, penalty: i32) -> (u8, i32) {
    (other_seat(seat), penalty)
}
