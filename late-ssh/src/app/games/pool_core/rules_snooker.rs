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
//! correspondence game has nobody to ask. Nothing is *added* either: snooker
//! has no rule that a ball must reach a cushion after contact, so a roll-up
//! that touches a red and stops is the legal safety it is on a real table.
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
    // Straight after a red, any colour still up. **Asked before the reds**,
    // because the last red is followed by a colour of choice exactly like
    // every other red — the order only starts once *that* colour is down.
    // Asking about the reds first made the final red the one red in the frame
    // that put you on the yellow.
    if state.on_colour {
        return COLOURS
            .into_iter()
            .filter(|id| state.on_table(*id))
            .collect();
    }
    let reds: Vec<u8> = (RED_FIRST..=RED_LAST)
        .filter(|id| state.on_table(*id))
        .collect();
    if !reds.is_empty() {
        return reds;
    }
    // Reds gone and the colour of choice taken: the colours come back in
    // order and stay down.
    COLOURS
        .into_iter()
        .find(|id| state.on_table(*id))
        .into_iter()
        .collect()
}

/// Judge one shot.
///
/// Whether the incoming player is left *snookered* is not answered here: it
/// needs the table geometry this layer deliberately never sees, and it needs
/// it after `balls_to_spot` has gone back up. A foul says a free ball is owed
/// (`free_ball_if_snookered`) and the caller says whether the table agrees.
pub fn judge(state: &GameState, outcome: &ShotOutcome) -> Ruling {
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

    // The ball on is one ball, and so is its value: a red while the reds are
    // up, the lowest colour once they are gone, and — in the one phase where
    // the striker has a choice — the colour they nominated by hitting it. A
    // shot that touched nothing legal nominated nothing and pays the floor.
    // Taking the dearest ball on the table instead would bill every foul in
    // the frame's commonest position at seven.
    let choosing = !on_a_red && real_on.len() > 1;
    let ball_on = match choosing {
        true => outcome.first_contact.filter(|id| real_on.contains(id)),
        false => real_on.first().copied(),
    };

    // Everything the shot did wrong, in the order a referee calls it. The
    // penalty is the value of the ball on or of the ball at fault, whichever
    // is higher, and never less than four.
    let ball_on_value = ball_on.map(value).unwrap_or(0);
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
    // Only reds go down in multiples. Everywhere else there is exactly one
    // ball on, so a second ball in the same stroke is a foul at the dearer of
    // them, however well the rest of the shot went.
    if fault.is_none() && !on_a_red && potted.len() > 1 {
        fault = Some(Foul::MultiplePotted);
        at_fault = at_fault.max(potted.iter().copied().map(value).max().unwrap_or(0));
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
            // Owed. Whether the table actually hides them is the caller's
            // question, and it is asked of the re-spotted table.
            free_ball_if_snookered: true,
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
    // comes next: the striker is on a colour. True of the last red as well as
    // of every other — that colour of choice is the end of the red phase, not
    // the start of the sequence.
    let potted_a_red = on_a_red && !potted.is_empty();
    let reds_left = (RED_FIRST..=RED_LAST).any(|id| state.on_table(id) && !potted.contains(&id));

    // Colours go back up while a red is still on the table, and stay down
    // once the reds are gone. That single line is the shape of a frame — with
    // one more: the colour taken after the *last* red is re-spotted too, and
    // only then do the colours start staying down. `state.on_colour` is what
    // says this shot was a colour of choice rather than one taken in order.
    let mut spot: Vec<u8> = if reds_left || state.on_colour {
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
        next_on_colour: potted_a_red,
        free_ball_if_snookered: false,
        frame_over: nothing_left,
        winner: None,
    }
}

/// What a snooker ball is called. A pool ball wears its number; these wear
/// their colour, and a red is any red.
pub fn name(id: u8) -> &'static str {
    match id {
        YELLOW => "yellow",
        GREEN => "green",
        BROWN => "brown",
        BLUE => "blue",
        PINK => "pink",
        BLACK => "black",
        id if is_red(id) => "red",
        _ => unreachable!("not a snooker ball: {id}"),
    }
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

/// The most the striker could still score off the table as it stands: every
/// red with a black after it, the colours, a black for the colour they are on
/// after a red, and the extra a free ball buys.
///
/// The figure a snooker scoreboard calls "remaining". It ignores fouls, which
/// is what makes it the line a frame is conceded on: past it, the player
/// behind can only win by being given points, and a correspondence game with
/// no referee to call a miss has no way to make that happen on purpose.
pub fn points_remaining(state: &GameState) -> i32 {
    let reds = (RED_FIRST..=RED_LAST)
        .filter(|id| state.on_table(*id))
        .count() as i32;
    let colours: i32 = COLOURS
        .into_iter()
        .filter(|id| state.on_table(*id))
        .map(value)
        .sum();
    let mut left = reds * (value(RED_FIRST) + value(BLACK)) + colours;
    if state.on_colour {
        left += value(BLACK);
    }
    if state.free_ball {
        // The free ball scores as the ball on and then the striker is on
        // what follows it: a red and a black while reds are up, otherwise the
        // lowest colour a second time.
        left += if reds > 0 {
            value(RED_FIRST) + value(BLACK)
        } else {
            COLOURS
                .into_iter()
                .find(|id| state.on_table(*id))
                .map_or(0, value)
        };
    }
    left
}

/// Nothing but the black is left: no red, no other colour.
pub fn only_black_left(state: &GameState) -> bool {
    state.on_table(BLACK)
        && !(RED_FIRST..=RED_LAST).any(|id| state.on_table(id))
        && COLOURS
            .into_iter()
            .filter(|id| *id != BLACK)
            .all(|id| !state.on_table(id))
}

/// One seat leads by more than `remaining`, and returns which.
///
/// Only ever *acted on* with the black alone on the table, where it is the
/// real rule: more than seven in it and the frame is over. Anywhere earlier
/// the player behind is left to play for snookers, which is the best part of
/// the end of a frame (`snookers_required` says how many).
pub fn out_of_reach(scores: [i32; 2], remaining: i32) -> Option<u8> {
    let lead = scores[0] - scores[1];
    if lead > remaining {
        Some(0)
    } else if -lead > remaining {
        Some(1)
    } else {
        None
    }
}

/// Someone is behind by more than is left on the table.
pub fn needs_snookers(scores: [i32; 2], remaining: i32) -> bool {
    out_of_reach(scores, remaining).is_some()
}

/// How many snookers the player `deficit` behind needs to *win*, with
/// `remaining` on the table (`points_remaining`). Nought while clearing the
/// table would do.
///
/// Counted to win rather than to tie, because a level frame here is a draw
/// rather than a re-spotted black. Each snooker is worth the least a foul on
/// the ball on can pay: four while the reds are up, the lowest colour's value
/// (but never under four) once they are gone.
pub fn snookers_required(state: &GameState, deficit: i32, remaining: i32) -> i32 {
    if deficit < remaining {
        return 0;
    }
    let reds = (RED_FIRST..=RED_LAST).any(|id| state.on_table(id));
    let per_snooker = if reds {
        MIN_PENALTY
    } else {
        COLOURS
            .into_iter()
            .find(|id| state.on_table(*id))
            .map_or(MIN_PENALTY, |id| value(id).max(MIN_PENALTY))
    };
    let short = deficit - remaining + 1;
    (short + per_snooker - 1) / per_snooker
}

/// The shot failed to hit a ball that was on, first: nothing at all, or the
/// wrong ball. The half of a miss the table can answer.
pub fn missed_ball_on(state: &GameState, outcome: &ShotOutcome) -> bool {
    let targets = legal_targets(state);
    outcome
        .first_contact
        .is_none_or(|hit| !targets.contains(&hit))
}

/// Foul **and a miss**, as near as a board with no referee can call it.
///
/// The real call is the referee's opinion that the striker did not make their
/// best attempt to hit the ball on, and nothing here can read intent. What the
/// rule book *does* say plainly is when a miss is never called, and that is
/// the half this keeps: not with the black alone on the table, and not when
/// either player needs snookers before or after the stroke — a player who
/// needs snookers is entitled to lay them, and putting the balls back for
/// them would undo the very thing the end of a frame is played for. Every
/// other failure to hit the ball on is called, snookered or not, which is how
/// the professional game tends to play it anyway.
///
/// The miss runs out by itself: each one pays the other player, so a player
/// who keeps missing soon needs snookers, and then it is no longer called.
pub fn is_miss(
    before: &GameState,
    outcome: &ShotOutcome,
    scores_before: [i32; 2],
    scores_after: [i32; 2],
    remaining_after: i32,
) -> bool {
    missed_ball_on(before, outcome)
        && !only_black_left(before)
        && !needs_snookers(scores_before, points_remaining(before))
        && !needs_snookers(scores_after, remaining_after)
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
