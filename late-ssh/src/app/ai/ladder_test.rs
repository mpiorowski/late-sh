use std::time::{Duration, Instant};

use uuid::Uuid;

use super::{Decision, LadderBot, Ladders};

fn secs(s: u64) -> Duration {
    Duration::from_secs(s)
}

#[test]
fn first_mention_answers_then_cooldown_throttles() {
    let mut ladders = Ladders::default();
    let user = Uuid::now_v7();
    let room = Uuid::now_v7();
    let t0 = Instant::now();

    assert_eq!(
        ladders.check_and_step(LadderBot::Bot, user, room, t0),
        Decision::Answer
    );
    assert_eq!(
        ladders.check_and_step(LadderBot::Bot, user, room, t0 + secs(10)),
        Decision::Throttled {
            remaining: secs(20)
        }
    );
    assert_eq!(
        ladders.check_and_step(LadderBot::Bot, user, room, t0 + secs(30)),
        Decision::Answer
    );
}

#[test]
fn ladder_escalates_and_caps_at_top_rung() {
    let mut ladders = Ladders::default();
    let user = Uuid::now_v7();
    let room = Uuid::now_v7();
    let t0 = Instant::now();

    // Answer at each rung the moment the previous cooldown expires; the
    // cooldown in force afterwards must follow 30s, 60s, 120s, 300s, 300s.
    let mut now = t0;
    for expected_cooldown in [30, 60, 120, 300, 300] {
        assert_eq!(
            ladders.check_and_step(LadderBot::Bot, user, room, now),
            Decision::Answer
        );
        assert_eq!(
            ladders.remaining(LadderBot::Bot, user, room, now + secs(1)),
            Some(secs(expected_cooldown - 1))
        );
        now += secs(expected_cooldown);
    }
}

#[test]
fn quiet_window_resets_the_ladder() {
    let mut ladders = Ladders::default();
    let user = Uuid::now_v7();
    let room = Uuid::now_v7();
    let t0 = Instant::now();

    // Climb two rungs, then go quiet past the reset window.
    assert_eq!(
        ladders.check_and_step(LadderBot::Bot, user, room, t0),
        Decision::Answer
    );
    assert_eq!(
        ladders.check_and_step(LadderBot::Bot, user, room, t0 + secs(30)),
        Decision::Answer
    );
    let after_quiet = t0 + secs(30) + secs(15 * 60);
    assert_eq!(
        ladders.check_and_step(LadderBot::Bot, user, room, after_quiet),
        Decision::Answer
    );
    // Back on the first rung: 30s again, not the 120s of rung three.
    assert_eq!(
        ladders.remaining(LadderBot::Bot, user, room, after_quiet + secs(1)),
        Some(secs(29))
    );
}

#[test]
fn throttled_attempts_do_not_step_or_extend() {
    let mut ladders = Ladders::default();
    let user = Uuid::now_v7();
    let room = Uuid::now_v7();
    let t0 = Instant::now();

    assert_eq!(
        ladders.check_and_step(LadderBot::Bot, user, room, t0),
        Decision::Answer
    );
    // Hammering during the cooldown changes nothing.
    for s in [5, 10, 15] {
        assert!(matches!(
            ladders.check_and_step(LadderBot::Bot, user, room, t0 + secs(s)),
            Decision::Throttled { .. }
        ));
    }
    assert_eq!(
        ladders.check_and_step(LadderBot::Bot, user, room, t0 + secs(30)),
        Decision::Answer
    );
    // That answer was rung two, so the cooldown in force is 60s.
    assert_eq!(
        ladders.remaining(LadderBot::Bot, user, room, t0 + secs(31)),
        Some(secs(59))
    );
}

#[test]
fn users_rooms_and_bots_are_isolated() {
    let mut ladders = Ladders::default();
    let user = Uuid::now_v7();
    let other_user = Uuid::now_v7();
    let room = Uuid::now_v7();
    let other_room = Uuid::now_v7();
    let t0 = Instant::now();

    assert_eq!(
        ladders.check_and_step(LadderBot::Bot, user, room, t0),
        Decision::Answer
    );
    // Same user, hot in `room`: another user, another room, and another bot
    // are all unaffected.
    assert_eq!(
        ladders.check_and_step(LadderBot::Bot, other_user, room, t0 + secs(1)),
        Decision::Answer
    );
    assert_eq!(
        ladders.check_and_step(LadderBot::Bot, user, other_room, t0 + secs(1)),
        Decision::Answer
    );
    assert_eq!(
        ladders.check_and_step(LadderBot::Bartender, user, room, t0 + secs(1)),
        Decision::Answer
    );
    assert!(matches!(
        ladders.check_and_step(LadderBot::Bot, user, room, t0 + secs(1)),
        Decision::Throttled { .. }
    ));
}

#[test]
fn bartender_ladder_is_gentler_and_caps_at_sixty() {
    let mut ladders = Ladders::default();
    let user = Uuid::now_v7();
    let room = Uuid::now_v7();
    let t0 = Instant::now();

    let mut now = t0;
    for expected_cooldown in [15, 30, 60, 60] {
        assert_eq!(
            ladders.check_and_step(LadderBot::Bartender, user, room, now),
            Decision::Answer
        );
        assert_eq!(
            ladders.remaining(LadderBot::Bartender, user, room, now + secs(1)),
            Some(secs(expected_cooldown - 1))
        );
        now += secs(expected_cooldown);
    }
}

#[test]
fn remaining_is_read_only_and_clears_when_ready() {
    let mut ladders = Ladders::default();
    let user = Uuid::now_v7();
    let room = Uuid::now_v7();
    let t0 = Instant::now();

    assert_eq!(ladders.remaining(LadderBot::Bot, user, room, t0), None);
    assert_eq!(
        ladders.check_and_step(LadderBot::Bot, user, room, t0),
        Decision::Answer
    );
    // Peeking many times never steps the ladder.
    for _ in 0..3 {
        assert_eq!(
            ladders.remaining(LadderBot::Bot, user, room, t0 + secs(10)),
            Some(secs(20))
        );
    }
    assert_eq!(
        ladders.remaining(LadderBot::Bot, user, room, t0 + secs(30)),
        None
    );
}
