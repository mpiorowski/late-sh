//! Eight-ball rules, exercised entirely from struct literals.
//!
//! No table, no simulation, no floating point in any assertion. That is the
//! point of keeping `ShotOutcome` free of physics types: every rule below is
//! stated as "this happened, therefore that", which is how the rule reads in
//! a rulebook and how a bug in it will be reported.

use crate::app::games::pool_core::{
    ball::{Ball, CUE},
    rules::{BallInHand, Foul, GameState, Group, PoolRules, Ruling, Turn},
    rules_eight::EIGHT,
    shot::{Pot, RackState, ShotOutcome},
};

const RULES: PoolRules = PoolRules::EightBall;

/// A rack with the given object balls still up, laid out on a line. Positions
/// are irrelevant to every rule here — only which balls remain matters.
fn rack(on_table: &[u8]) -> RackState {
    let mut balls = vec![Ball::resting(CUE, [0.5, 0.5])];
    for (i, id) in on_table.iter().enumerate() {
        balls.push(Ball::resting(*id, [0.2 + i as f64 * 0.1, 0.3]));
    }
    // Everything else is already down.
    for id in 1..=15u8 {
        if !on_table.contains(&id) {
            let mut ball = Ball::resting(id, [0.0, 0.0]);
            ball.potted = Some(0);
            balls.push(ball);
        }
    }
    RackState { balls }
}

fn state(on_table: &[u8], groups: Option<[Group; 2]>, shots_taken: u32) -> GameState {
    GameState {
        rack: rack(on_table),
        turn: 0,
        groups,
        shots_taken,
        ball_in_hand: None,
        on_colour: false,
        free_ball: false,
    }
}

/// A shot that hit `first` and potted `potted`, each into pocket 0 unless the
/// tuple says otherwise, and reached a rail so it is never a stall.
fn shot(first: Option<u8>, potted: &[(u8, u8)]) -> ShotOutcome {
    ShotOutcome {
        first_contact: first,
        potted: potted
            .iter()
            .map(|(ball, pocket)| Pot {
                ball: *ball,
                pocket: *pocket,
            })
            .collect(),
        cushion_after_contact: true,
        balls_to_rail: vec![1, 2, 3, 4],
        cue_potted: potted.iter().any(|(b, _)| *b == CUE),
        duration: 3.0,
        truncated: false,
    }
}

fn judge(state: &GameState, outcome: &ShotOutcome, call: Option<u8>) -> Ruling {
    RULES.judge(state, outcome, call, false)
}

// ── The break ─────────────────────────────────────────────────────────

#[test]
fn a_break_that_pots_keeps_the_table_but_leaves_it_open() {
    let s = state(&(1..=15).collect::<Vec<_>>(), None, 0);
    let r = judge(&s, &shot(Some(1), &[(3, 0)]), None);
    assert_eq!(r.turn, Turn::Keep);
    assert_eq!(r.foul, None);
    assert_eq!(
        r.group_assignment, None,
        "breaking a solid must not make you solids"
    );
}

#[test]
fn a_break_that_pots_nothing_but_reaches_the_rails_is_legal() {
    let s = state(&(1..=15).collect::<Vec<_>>(), None, 0);
    let r = judge(&s, &shot(Some(1), &[]), None);
    assert_eq!(r.foul, None);
    assert_eq!(r.turn, Turn::Pass);
}

#[test]
fn a_break_that_barely_moves_is_a_foul() {
    let s = state(&(1..=15).collect::<Vec<_>>(), None, 0);
    let mut outcome = shot(Some(1), &[]);
    outcome.balls_to_rail = vec![1, 2];
    let r = judge(&s, &outcome, None);
    assert_eq!(r.foul, Some(Foul::IllegalBreak));
    assert_eq!(r.ball_in_hand, Some(BallInHand::Kitchen));
}

#[test]
fn a_scratch_on_the_break_gives_the_kitchen_not_the_table() {
    let s = state(&(1..=15).collect::<Vec<_>>(), None, 0);
    let r = judge(&s, &shot(Some(1), &[(3, 0), (CUE, 2)]), None);
    assert_eq!(r.foul, Some(Foul::Scratch));
    assert_eq!(r.ball_in_hand, Some(BallInHand::Kitchen));
}

#[test]
fn the_eight_on_the_break_is_spotted_and_nobody_wins() {
    let s = state(&(1..=15).collect::<Vec<_>>(), None, 0);
    let r = judge(&s, &shot(Some(1), &[(EIGHT, 0)]), None);
    assert_eq!(r.winner, None, "the eight on the break is not a result");
    assert_eq!(r.balls_to_spot, vec![EIGHT]);
    assert_eq!(
        r.turn,
        Turn::Pass,
        "the eight is going back up, so it did not earn another shot"
    );
}

// ── The open table ────────────────────────────────────────────────────

#[test]
fn the_first_pot_after_the_break_assigns_both_groups() {
    let s = state(&(1..=15).collect::<Vec<_>>(), None, 1);
    let r = judge(&s, &shot(Some(11), &[(11, 0)]), None);
    assert_eq!(r.turn, Turn::Keep);
    let groups = r.group_assignment.expect("groups should be assigned");
    assert_eq!(groups[0], Group::Stripes, "potted a stripe");
    assert_eq!(
        groups[1],
        Group::Solids,
        "the opponent gets the other group"
    );
}

#[test]
fn hitting_the_eight_first_on_an_open_table_is_a_foul() {
    let s = state(&(1..=15).collect::<Vec<_>>(), None, 1);
    let r = judge(&s, &shot(Some(EIGHT), &[]), None);
    assert_eq!(r.foul, Some(Foul::WrongBallFirst));
    assert_eq!(r.ball_in_hand, Some(BallInHand::Anywhere));
}

// ── Ordinary play ─────────────────────────────────────────────────────

#[test]
fn potting_your_own_group_keeps_the_table() {
    let s = state(
        &[1, 2, 9, 10, EIGHT],
        Some([Group::Solids, Group::Stripes]),
        4,
    );
    let r = judge(&s, &shot(Some(1), &[(1, 0)]), None);
    assert_eq!(r.turn, Turn::Keep);
    assert_eq!(r.foul, None);
}

#[test]
fn potting_only_the_opponents_ball_passes_the_table_without_a_foul() {
    // Legal hit, their ball dropped. Not a foul, but you are done.
    let s = state(
        &[1, 2, 9, 10, EIGHT],
        Some([Group::Solids, Group::Stripes]),
        4,
    );
    let r = judge(&s, &shot(Some(1), &[(9, 0)]), None);
    assert_eq!(r.turn, Turn::Pass);
    assert_eq!(r.foul, None);
}

#[test]
fn hitting_the_opponents_group_first_is_a_foul() {
    let s = state(
        &[1, 2, 9, 10, EIGHT],
        Some([Group::Solids, Group::Stripes]),
        4,
    );
    let r = judge(&s, &shot(Some(9), &[]), None);
    assert_eq!(r.foul, Some(Foul::WrongBallFirst));
}

#[test]
fn touching_nothing_is_a_foul() {
    let s = state(&[1, 9, EIGHT], Some([Group::Solids, Group::Stripes]), 4);
    let r = judge(&s, &shot(None, &[]), None);
    assert_eq!(r.foul, Some(Foul::NoContact));
}

#[test]
fn a_legal_hit_that_goes_nowhere_is_a_foul() {
    let s = state(&[1, 9, EIGHT], Some([Group::Solids, Group::Stripes]), 4);
    let mut outcome = shot(Some(1), &[]);
    outcome.cushion_after_contact = false;
    let r = judge(&s, &outcome, None);
    assert_eq!(r.foul, Some(Foul::NoRail));
}

#[test]
fn a_scratch_gives_the_opponent_the_whole_table() {
    let s = state(&[1, 9, EIGHT], Some([Group::Solids, Group::Stripes]), 4);
    let r = judge(&s, &shot(Some(1), &[(1, 0), (CUE, 3)]), None);
    assert_eq!(r.foul, Some(Foul::Scratch));
    assert_eq!(r.ball_in_hand, Some(BallInHand::Anywhere));
    assert_eq!(r.turn, Turn::Pass);
}

// ── The eight ─────────────────────────────────────────────────────────

/// Group cleared, so the eight is the only legal target.
fn on_the_eight() -> GameState {
    state(&[9, 10, EIGHT], Some([Group::Solids, Group::Stripes]), 8)
}

#[test]
fn the_eight_must_be_called_only_once_the_group_is_cleared() {
    assert!(RULES.requires_call(&on_the_eight()));
    let mid_rack = state(&[1, 9, EIGHT], Some([Group::Solids, Group::Stripes]), 4);
    assert!(!RULES.requires_call(&mid_rack));
}

#[test]
fn potting_the_called_eight_wins() {
    let r = judge(&on_the_eight(), &shot(Some(EIGHT), &[(EIGHT, 2)]), Some(2));
    assert_eq!(r.winner, Some(0));
}

#[test]
fn the_eight_in_the_wrong_pocket_loses() {
    let r = judge(&on_the_eight(), &shot(Some(EIGHT), &[(EIGHT, 5)]), Some(2));
    assert_eq!(r.winner, Some(1), "called pocket 2, it went down 5");
}

#[test]
fn an_uncalled_eight_loses() {
    let r = judge(&on_the_eight(), &shot(Some(EIGHT), &[(EIGHT, 2)]), None);
    assert_eq!(r.winner, Some(1));
}

#[test]
fn scratching_on_the_winning_eight_loses() {
    let r = judge(
        &on_the_eight(),
        &shot(Some(EIGHT), &[(EIGHT, 2), (CUE, 4)]),
        Some(2),
    );
    assert_eq!(
        r.winner,
        Some(1),
        "the eight dropped but so did the cue ball"
    );
}

#[test]
fn potting_the_eight_early_loses() {
    // Still has solids on the table; the eight goes down anyway.
    let s = state(&[1, 2, 9, EIGHT], Some([Group::Solids, Group::Stripes]), 4);
    let r = judge(&s, &shot(Some(1), &[(EIGHT, 0)]), None);
    assert_eq!(r.winner, Some(1));
}

#[test]
fn legal_targets_narrow_to_the_eight_when_the_group_is_gone() {
    assert_eq!(RULES.legal_targets(&on_the_eight()), vec![EIGHT]);
    let open = state(&[1, 2, 9, EIGHT], None, 1);
    let mut targets = RULES.legal_targets(&open);
    targets.sort_unstable();
    assert_eq!(
        targets,
        vec![1, 2, 9],
        "open table is anything but the eight"
    );
}
