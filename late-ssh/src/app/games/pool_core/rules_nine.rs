//! Nine-ball.
//!
//! Simpler than eight-ball and simpler still than the tournament game: hit the
//! lowest ball first, pot anything to keep shooting, pot the nine to win.
//! Combinations are legal and always were — banking the 1 into the 9 wins the
//! rack, which is most of nine-ball's character.
//!
//! Deliberately left out of v1, both cheap to add later:
//!
//! - **Push-out** after the break. It needs a second decision step from the
//!   incoming player, which is a whole extra round trip on a correspondence
//!   clock.
//! - **Three-foul loss.** It needs a per-player foul counter carried across
//!   turns and a warning surfaced in the UI, and it almost never fires.
//!
//! Nothing here is called: in nine-ball there is only ever one legal target,
//! so a called pocket would add input for no decision.

use crate::app::games::pool_core::{
    rules::{Foul, GameState, Ruling, break_was_legal, stalled},
    shot::ShotOutcome,
};

pub const NINE: u8 = 9;

/// The lowest ball on the table, and nothing else.
pub fn legal_targets(state: &GameState) -> Vec<u8> {
    state.lowest_on_table().into_iter().collect()
}

pub fn judge(state: &GameState, outcome: &ShotOutcome) -> Ruling {
    let shooter = state.turn;

    // The nine going down ends the rack — unless the shot was a foul, in which
    // case it is spotted and play carries on. That single rule is why a scratch
    // on the winning shot is so painful.
    let nine_down = outcome.was_potted(NINE);

    let foul = if outcome.cue_potted {
        Some(Foul::Scratch)
    } else if state.is_break() && !break_was_legal(outcome) {
        Some(Foul::IllegalBreak)
    } else {
        match outcome.first_contact {
            None => Some(Foul::NoContact),
            Some(hit) if !legal_targets(state).contains(&hit) => Some(Foul::WrongBallFirst),
            Some(_) if stalled(outcome) => Some(Foul::NoRail),
            Some(_) => None,
        }
    };

    if let Some(foul) = foul {
        let mut ruling = Ruling::foul(foul);
        if nine_down {
            ruling.balls_to_spot.push(NINE);
        }
        return ruling;
    }

    if nine_down {
        return Ruling::won_by(shooter);
    }

    if outcome.potted_object_balls().next().is_some() {
        Ruling::keep()
    } else {
        Ruling::pass()
    }
}
