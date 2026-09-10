//! Snooker rules, tested from struct literals with no table and no physics —
//! the same trick the other two rulesets use, and the reason `ShotOutcome` is
//! free of physics types.
//!
//! What is worth testing here is the *sequence*: a frame is a machine that
//! alternates reds and colours, then stops alternating, and the ways it can go
//! wrong are all about which ball is on when.

use crate::app::games::pool_core::{ball::Ball, rack, table::SNOOKER_12FT};
use crate::app::games::pool_core::{
    rules::{BallInHand, Foul, GameState, PoolRules, Turn},
    rules_snooker::{BLACK, BLUE, BROWN, COLOURS, GREEN, PINK, RED_FIRST, RED_LAST, YELLOW, value},
    shot::{Pot, RackState, ShotOutcome},
    table::BAR_BOX_7FT,
};

const RULES: PoolRules = PoolRules::Snooker;

fn frame() -> GameState {
    GameState {
        rack: rack::build(&SNOOKER_12FT, rack::RackKind::Snooker, 7),
        turn: 0,
        groups: None,
        shots_taken: 1,
        ball_in_hand: None,
        on_colour: false,
        free_ball: false,
    }
}

/// A shot that hit `hit` first, potted `potted`, and found a cushion.
fn shot(hit: Option<u8>, potted: &[u8]) -> ShotOutcome {
    ShotOutcome {
        first_contact: hit,
        potted: potted
            .iter()
            .map(|id| Pot {
                ball: *id,
                pocket: 0,
            })
            .collect(),
        cushion_after_contact: true,
        ..ShotOutcome::default()
    }
}

fn pot(state: &mut GameState, ids: &[u8]) {
    for id in ids {
        if let Some(ball) = state.rack.balls.iter_mut().find(|b| b.id == *id) {
            ball.potted = Some(0);
        }
    }
}

#[test]
fn a_frame_alternates_reds_and_colours_then_stops() {
    let mut state = frame();

    // Off a red, every red is on and no colour is.
    let on = RULES.legal_targets(&state);
    assert!(on.iter().all(|id| (RED_FIRST..=RED_LAST).contains(id)));
    assert_eq!(on.len(), 15);

    // Pot one and the colours are on — any of them, which is what "nominate"
    // means when the nomination is the ball you hit.
    let ruling = RULES.judge(&state, &shot(Some(RED_FIRST), &[RED_FIRST]), None, false);
    assert_eq!(ruling.turn, Turn::Keep);
    assert_eq!(ruling.points, 1);
    assert!(ruling.next_on_colour);

    state.on_colour = true;
    pot(&mut state, &[RED_FIRST]);
    let on = RULES.legal_targets(&state);
    assert_eq!(on, COLOURS.to_vec(), "any colour, and only a colour");

    // A colour potted off a red goes straight back on its spot.
    let ruling = RULES.judge(&state, &shot(Some(BLACK), &[BLACK]), None, false);
    assert_eq!(ruling.points, 7);
    assert_eq!(ruling.balls_to_spot, vec![BLACK]);
    assert!(!ruling.next_on_colour, "back on a red");

    // With the reds gone the colours come back in order and stay down.
    pot(&mut state, &(RED_FIRST..=RED_LAST).collect::<Vec<_>>());
    state.on_colour = false;
    assert_eq!(RULES.legal_targets(&state), vec![YELLOW]);
    let ruling = RULES.judge(&state, &shot(Some(YELLOW), &[YELLOW]), None, false);
    assert_eq!(ruling.points, 2);
    assert!(
        ruling.balls_to_spot.is_empty(),
        "a colour potted after the reds stays down"
    );

    pot(&mut state, &[YELLOW]);
    assert_eq!(RULES.legal_targets(&state), vec![GREEN]);
}

#[test]
fn the_frame_ends_when_the_last_ball_goes() {
    let mut state = frame();
    pot(&mut state, &(RED_FIRST..=RED_LAST).collect::<Vec<_>>());
    pot(&mut state, &[YELLOW, GREEN, BROWN, BLUE, PINK]);
    assert_eq!(RULES.legal_targets(&state), vec![BLACK]);

    let ruling = RULES.judge(&state, &shot(Some(BLACK), &[BLACK]), None, false);
    assert!(ruling.frame_over, "nothing left to pot");
    assert_eq!(ruling.points, 7);
    assert!(
        ruling.winner.is_none(),
        "who won is a matter of the score, which this layer does not keep"
    );
}

#[test]
fn a_foul_pays_the_ball_on_or_the_ball_at_fault() {
    let state = frame();

    // Missing everything is the cheapest foul there is: four.
    let ruling = RULES.judge(&state, &shot(None, &[]), None, false);
    assert_eq!(ruling.foul, Some(Foul::NoContact));
    assert_eq!(ruling.penalty, 4);
    assert_eq!(ruling.points, 0);
    assert_eq!(ruling.turn, Turn::Pass);

    // Hitting the black when you are on a red costs seven, not four: the
    // penalty is the value of whichever ball is higher.
    let ruling = RULES.judge(&state, &shot(Some(BLACK), &[]), None, false);
    assert_eq!(ruling.foul, Some(Foul::WrongBallFirst));
    assert_eq!(ruling.penalty, 7);

    // And potting the cue ball hands over the D as well as the points.
    let mut scratched = shot(Some(RED_FIRST), &[]);
    scratched.cue_potted = true;
    let ruling = RULES.judge(&state, &scratched, None, false);
    assert_eq!(ruling.foul, Some(Foul::Scratch));
    assert_eq!(ruling.ball_in_hand, Some(BallInHand::TheD));
}

#[test]
fn a_red_potted_on_a_foul_stays_down_and_a_colour_goes_back_up() {
    // The asymmetry is real snooker and it is the reason `balls_to_spot`
    // exists: reds are gone for good however they went, colours always come
    // back while there is a red on the table.
    let state = frame();
    let mut fouled = shot(Some(BLACK), &[BLACK, RED_FIRST]);
    fouled.cue_potted = false;
    let ruling = RULES.judge(&state, &fouled, None, false);

    assert!(ruling.foul.is_some());
    assert_eq!(ruling.balls_to_spot, vec![BLACK]);
    assert!(!ruling.balls_to_spot.contains(&RED_FIRST));
}

#[test]
fn a_free_ball_makes_anything_the_ball_on_and_scores_it_as_the_ball_on() {
    let mut state = frame();
    state.free_ball = true;

    let on = RULES.legal_targets(&state);
    assert!(
        on.len() > 15,
        "with a free ball every ball on the table is on: {}",
        on.len()
    );

    // Potting the black as a free ball while on a red scores *one*, not seven
    // — the free ball is worth the value of the ball it stands in for.
    let ruling = RULES.judge(&state, &shot(Some(BLACK), &[BLACK]), None, false);
    assert!(ruling.foul.is_none(), "a free ball is not a foul");
    assert_eq!(ruling.points, 1, "worth the red it stood in for");
    assert_eq!(ruling.turn, Turn::Keep);
}

#[test]
fn a_foul_that_leaves_them_snookered_awards_a_free_ball() {
    let state = frame();
    let clear = RULES.judge(&state, &shot(None, &[]), None, false);
    assert!(!clear.free_ball, "a plain foul is only a foul");

    let snookered = RULES.judge(&state, &shot(None, &[]), None, true);
    assert!(snookered.free_ball, "and one that hides them is worse");
}

#[test]
fn every_ball_is_worth_what_it_is_worth() {
    assert_eq!(value(RED_FIRST), 1);
    assert_eq!(value(RED_LAST), 1);
    for (index, id) in COLOURS.into_iter().enumerate() {
        assert_eq!(value(id), index as i32 + 2, "{id}");
    }
    assert_eq!(value(BLACK), 7);
    // A pool ball has no value here, and asking must not panic or lie.
    assert_eq!(value(8), 0);
}

#[test]
fn the_rack_is_the_one_a_referee_would_set() {
    let spec = SNOOKER_12FT;
    let rack = rack::build(&spec, rack::RackKind::Snooker, 1);
    assert_eq!(rack.balls.len(), 22, "cue, fifteen reds, six colours");
    assert_eq!(
        (RED_FIRST..=RED_LAST)
            .filter(|id| rack.get(*id).is_some())
            .count(),
        15
    );

    // The colours are on their own spots, and the spots are where a referee
    // would find them: brown on the baulk line, blue in the middle, black
    // near the top cushion.
    let at = |id: u8| rack.get(id).expect("on the table").pos;
    assert!((at(BROWN)[0] - spec.head_string).abs() < 1e-9);
    assert!((at(BLUE)[0] - spec.length / 2.0).abs() < 1e-9);
    assert!(at(BLACK)[0] > at(PINK)[0], "black beyond the pink");
    assert!(at(YELLOW)[1] < at(BROWN)[1] && at(GREEN)[1] > at(BROWN)[1]);

    // And the reds sit behind the pink rather than on top of it.
    let apex = (RED_FIRST..=RED_LAST)
        .map(|id| at(id)[0])
        .fold(f64::INFINITY, f64::min);
    assert!(apex > at(PINK)[0], "the triangle starts past the pink");

    // Nothing starts overlapping anything, which the simulator would spend its
    // first steps undoing.
    let balls: Vec<&Ball> = rack.balls.iter().collect();
    for (i, a) in balls.iter().enumerate() {
        for b in balls.iter().skip(i + 1) {
            let gap = (a.pos[0] - b.pos[0]).hypot(a.pos[1] - b.pos[1]);
            assert!(
                gap >= 2.0 * spec.ball_radius - 1e-9,
                "{} and {} start {gap}m apart",
                a.id,
                b.id
            );
        }
    }
}

#[test]
fn the_d_is_a_semicircle_and_not_the_whole_baulk_end() {
    // `BallInHand::Kitchen` is "anywhere behind the line"; snooker's is the D,
    // and putting the cue ball in the corner of the baulk area is not legal.
    let spec = SNOOKER_12FT;
    let geom = spec.geometry();
    let rack = RackState { balls: Vec::new() };
    let ok = |at: [f64; 2]| {
        crate::app::games::pool_core::rules::placement_ok(&spec, &geom, &rack, at, BallInHand::TheD)
    };
    assert!(ok(rack::d_spot(&spec)), "the middle of the D");
    assert!(
        !ok([spec.head_string * 0.2, spec.width * 0.1]),
        "the baulk corner is behind the line but outside the D"
    );
    assert!(
        !ok([spec.head_string + 0.2, spec.width / 2.0]),
        "and past the line is not the D either"
    );
}

#[test]
fn a_frame_opens_with_the_cue_ball_in_hand_in_the_d() {
    // Where you break from decides what the pack does, so it is a rule and not
    // a nicety — and a board that hands you a fixed cue ball for the opening
    // shot has quietly taken a decision away from you.
    use crate::app::lobby::daily::pool::DailyPoolState;
    use uuid::Uuid;

    let state = DailyPoolState::new(PoolRules::Snooker, Uuid::new_v4(), Uuid::new_v4());
    assert_eq!(state.ball_in_hand, Some(BallInHand::TheD));
    assert!(
        !state.must_place(),
        "the cue ball is on the table, it is just yours to move"
    );

    // And the pool games still break from a fixed spot, as they should.
    for rules in [PoolRules::EightBall, PoolRules::NineBall] {
        let pool = DailyPoolState::new(rules, Uuid::new_v4(), Uuid::new_v4());
        assert_eq!(pool.ball_in_hand, None, "{rules:?}");
    }
}

#[test]
fn only_snooker_scores() {
    // The board asks before finding room for two numbers, and the pool games
    // would show a pair of noughts for the whole match.
    assert!(PoolRules::Snooker.scores());
    assert!(!PoolRules::EightBall.scores());
    assert!(!PoolRules::NineBall.scores());
    // And the snooker table is the reason the game is hard: nearly twice the
    // bar box in each direction, with a ball that is smaller.
    let (long, short) = (SNOOKER_12FT.length, BAR_BOX_7FT.length);
    assert!(long > short * 1.7, "{long} against {short}");
    let (small, big) = (SNOOKER_12FT.ball_radius, BAR_BOX_7FT.ball_radius);
    assert!(small < big, "{small} against {big}");
}
