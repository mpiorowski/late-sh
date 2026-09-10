//! Eight-ball, bar rules.
//!
//! Bar rules rather than league rules, because that is the table this game is
//! modelled on. The choices that differ from a rulebook:
//!
//! - **Only the eight is called.** Calling every shot is a whole extra input
//!   step per turn and at bar fidelity it buys very little. The eight is
//!   called because "slop the eight in off three rails and win" is the one
//!   case nobody would accept.
//! - **The eight on the break is spotted** — neither a win nor a loss. At a
//!   bar that is a re-rack; spotting it is the same outcome without throwing
//!   away the break the player just made.
//! - **A scratch on the break gives the kitchen**, not the whole table.
//!
//! The table stays open through the break even if balls drop. That is the
//! rule most often played wrong: breaking two stripes does not make you
//! stripes.

use crate::app::games::pool_core::{
    rules::{
        BallInHand, Foul, GameState, Group, Ruling, Turn, break_was_legal, other_seat, stalled,
    },
    shot::ShotOutcome,
};

pub const EIGHT: u8 = 8;

/// Balls this seat may hit first.
pub fn legal_targets(state: &GameState) -> Vec<u8> {
    match state.group_of(state.turn) {
        // Open table: anything but the eight.
        None => (1..=15)
            .filter(|id| *id != EIGHT && state.on_table(*id))
            .collect(),
        Some(group) => {
            let mine: Vec<u8> = (1..=15)
                .filter(|id| group.contains(*id) && state.on_table(*id))
                .collect();
            // Group cleared: the eight is the only ball left to you.
            if mine.is_empty() { vec![EIGHT] } else { mine }
        }
    }
}

/// A pocket must be named only when the shooter is down to the eight.
pub fn requires_call(state: &GameState) -> bool {
    legal_targets(state) == [EIGHT]
}

pub fn judge(state: &GameState, outcome: &ShotOutcome, called_pocket: Option<u8>) -> Ruling {
    if state.is_break() {
        return judge_break(outcome);
    }

    let shooter = state.turn;
    let opponent = other_seat(shooter);
    let targets = legal_targets(state);
    let on_the_eight = targets == [EIGHT];

    // The eight decides the rack the moment it drops, whatever else happened.
    // Win only if it was the shooter's ball to shoot, hit first, called into
    // the pocket it fell in, and the cue ball stayed up.
    if outcome.was_potted(EIGHT) {
        let legal_hit = outcome
            .first_contact
            .is_some_and(|hit| targets.contains(&hit));
        let called_right = called_pocket == outcome.pocket_of(EIGHT);
        let won = on_the_eight && legal_hit && called_right && !outcome.cue_potted;
        return Ruling::won_by(if won { shooter } else { opponent });
    }

    // Ordinary fouls, in the order a referee would call them.
    if outcome.cue_potted {
        return Ruling::foul(Foul::Scratch);
    }
    let Some(hit) = outcome.first_contact else {
        return Ruling::foul(Foul::NoContact);
    };
    if !targets.contains(&hit) {
        return Ruling::foul(Foul::WrongBallFirst);
    }
    if stalled(outcome) {
        return Ruling::foul(Foul::NoRail);
    }

    let potted: Vec<u8> = outcome.potted_object_balls().collect();
    if potted.is_empty() {
        return Ruling::pass();
    }

    if state.groups.is_none() {
        // The first ball down decides. A shot dropping both a solid and a
        // stripe takes the group of whichever fell first: the shooter aimed at
        // one of them, and the other is a bonus either way.
        let group = if Group::Solids.contains(potted[0]) {
            Group::Solids
        } else {
            Group::Stripes
        };
        let mut assignment = [Group::Solids; 2];
        assignment[shooter as usize] = group;
        assignment[opponent as usize] = group.other();
        return Ruling {
            group_assignment: Some(assignment),
            ..Ruling::keep()
        };
    }

    // Groups already set: you keep the table only if one of yours went down.
    let mine = state.group_of(shooter).expect("groups are set");
    if potted.iter().any(|id| mine.contains(*id)) {
        Ruling::keep()
    } else {
        Ruling::pass()
    }
}

/// The break. Nothing here can win or lose the rack, and nothing here assigns
/// groups.
fn judge_break(outcome: &ShotOutcome) -> Ruling {
    let spot = if outcome.was_potted(EIGHT) {
        vec![EIGHT]
    } else {
        Vec::new()
    };

    let penalty = if outcome.cue_potted {
        Some(Foul::Scratch)
    } else if !break_was_legal(outcome) {
        Some(Foul::IllegalBreak)
    } else {
        None
    };

    if let Some(foul) = penalty {
        return Ruling {
            foul: Some(foul),
            ball_in_hand: Some(BallInHand::Kitchen),
            balls_to_spot: spot,
            ..Ruling::pass()
        };
    }

    // A legal break keeps the table if a ball other than the eight stayed
    // down. The eight is going back up, so it does not count as a pot.
    let kept = outcome.potted_object_balls().any(|id| id != EIGHT);
    Ruling {
        turn: if kept { Turn::Keep } else { Turn::Pass },
        balls_to_spot: spot,
        ..Ruling::pass()
    }
}
