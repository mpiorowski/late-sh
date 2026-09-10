//! Nine-ball rules, exercised entirely from struct literals.

use crate::app::games::pool_core::{
    ball::{Ball, CUE},
    rules::{BallInHand, Foul, GameState, PoolRules, Ruling, Turn},
    rules_nine::NINE,
    shot::{Pot, RackState, ShotOutcome},
};

const RULES: PoolRules = PoolRules::NineBall;

fn rack(on_table: &[u8]) -> RackState {
    let mut balls = vec![Ball::resting(CUE, [0.5, 0.5])];
    for (i, id) in on_table.iter().enumerate() {
        balls.push(Ball::resting(*id, [0.2 + i as f64 * 0.1, 0.3]));
    }
    for id in 1..=9u8 {
        if !on_table.contains(&id) {
            let mut ball = Ball::resting(id, [0.0, 0.0]);
            ball.potted = Some(0);
            balls.push(ball);
        }
    }
    RackState { balls }
}

fn state(on_table: &[u8], shots_taken: u32) -> GameState {
    GameState {
        rack: rack(on_table),
        turn: 0,
        groups: None,
        shots_taken,
        ball_in_hand: None,
        on_colour: false,
        free_ball: false,
    }
}

fn shot(first: Option<u8>, potted: &[u8]) -> ShotOutcome {
    ShotOutcome {
        first_contact: first,
        potted: potted
            .iter()
            .map(|ball| Pot {
                ball: *ball,
                pocket: 0,
            })
            .collect(),
        cushion_after_contact: true,
        balls_to_rail: vec![1, 2, 3, 4],
        cue_potted: potted.contains(&CUE),
        duration: 3.0,
        truncated: false,
    }
}

fn judge(state: &GameState, outcome: &ShotOutcome) -> Ruling {
    RULES.judge(state, outcome, None, false)
}

#[test]
fn the_lowest_ball_is_the_only_legal_target() {
    let s = state(&[3, 5, 7, NINE], 4);
    assert_eq!(RULES.legal_targets(&s), vec![3]);
}

#[test]
fn nine_ball_never_asks_for_a_called_pocket() {
    assert!(!RULES.requires_call(&state(&[3, NINE], 4)));
}

#[test]
fn hitting_a_higher_ball_first_is_a_foul() {
    let s = state(&[3, 5, NINE], 4);
    let r = judge(&s, &shot(Some(5), &[5]));
    assert_eq!(r.foul, Some(Foul::WrongBallFirst));
    assert_eq!(r.ball_in_hand, Some(BallInHand::Anywhere));
}

#[test]
fn potting_anything_off_a_legal_hit_keeps_the_table() {
    // The 3 is legal, and the 7 dropping off it is a perfectly good shot.
    let s = state(&[3, 5, 7, NINE], 4);
    let r = judge(&s, &shot(Some(3), &[7]));
    assert_eq!(r.turn, Turn::Keep);
    assert_eq!(r.foul, None);
}

#[test]
fn a_legal_miss_passes_the_table() {
    let s = state(&[3, NINE], 4);
    let r = judge(&s, &shot(Some(3), &[]));
    assert_eq!(r.turn, Turn::Pass);
    assert_eq!(r.foul, None);
}

#[test]
fn a_legal_combination_into_the_nine_wins() {
    // Hit the 3, the 3 knocks in the 9. This is nine-ball's whole character.
    let s = state(&[3, 5, NINE], 4);
    let r = judge(&s, &shot(Some(3), &[NINE]));
    assert_eq!(r.winner, Some(0));
}

#[test]
fn the_nine_on_the_break_wins() {
    let s = state(&(1..=9).collect::<Vec<_>>(), 0);
    let r = judge(&s, &shot(Some(1), &[NINE]));
    assert_eq!(r.winner, Some(0), "a golden break is a win");
}

#[test]
fn scratching_on_the_nine_spots_it_instead_of_winning() {
    // The rule that makes nine-ball hurt: the nine was down and the rack was
    // over, right up until the cue ball followed it in.
    let s = state(&[3, NINE], 4);
    let r = judge(&s, &shot(Some(3), &[NINE, CUE]));
    assert_eq!(r.winner, None);
    assert_eq!(r.foul, Some(Foul::Scratch));
    assert_eq!(r.balls_to_spot, vec![NINE]);
    assert_eq!(r.ball_in_hand, Some(BallInHand::Anywhere));
}

#[test]
fn an_illegal_break_fouls() {
    let s = state(&(1..=9).collect::<Vec<_>>(), 0);
    let mut outcome = shot(Some(1), &[]);
    outcome.balls_to_rail = vec![1];
    let r = judge(&s, &outcome);
    assert_eq!(r.foul, Some(Foul::IllegalBreak));
}

#[test]
fn touching_nothing_is_a_foul() {
    let s = state(&[3, NINE], 4);
    let r = judge(&s, &shot(None, &[]));
    assert_eq!(r.foul, Some(Foul::NoContact));
}

#[test]
fn a_legal_hit_that_reaches_no_rail_is_a_foul() {
    let s = state(&[3, NINE], 4);
    let mut outcome = shot(Some(3), &[]);
    outcome.cushion_after_contact = false;
    let r = judge(&s, &outcome);
    assert_eq!(r.foul, Some(Foul::NoRail));
}
