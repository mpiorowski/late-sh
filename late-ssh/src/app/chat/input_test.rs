use super::{apply_pomodoro_request, is_next_room_key, is_prev_room_key, leader_reaction_emoji};
use crate::app::chat::state::PomodoroRequest;
use crate::app::common::pomodoro::PomodoroTimer;
use chrono::Utc;

#[test]
fn next_room_keys_include_ctrl_n() {
    assert!(is_next_room_key(b'l'));
    assert!(is_next_room_key(b'L'));
    assert!(is_next_room_key(0x0E));
    assert!(!is_next_room_key(b'h'));
}

#[test]
fn prev_room_keys_include_ctrl_p() {
    assert!(is_prev_room_key(b'h'));
    assert!(is_prev_room_key(b'H'));
    assert!(is_prev_room_key(0x10));
    assert!(!is_prev_room_key(b'l'));
}

#[test]
fn leader_reaction_keys_are_plain_digits_except_custom_zero() {
    assert_eq!(leader_reaction_emoji(b'0'), None);
    assert_eq!(leader_reaction_emoji(b'1'), Some("👍"));
    assert_eq!(leader_reaction_emoji(b'5'), Some("🔥"));
    assert_eq!(leader_reaction_emoji(b'6'), Some("🙌"));
    assert_eq!(leader_reaction_emoji(b'7'), Some("🚀"));
    assert_eq!(leader_reaction_emoji(b'8'), Some("🤔"));
    assert_eq!(leader_reaction_emoji(b'9'), Some("💩"));
    assert_eq!(leader_reaction_emoji(b'!'), None);
}

fn start(minutes: u32, label: &str) -> PomodoroRequest {
    PomodoroRequest::Start {
        minutes,
        label: label.to_string(),
    }
}

#[test]
fn apply_pomodoro_request_starts_a_timer_and_names_it_in_the_banner() {
    let now = Utc::now();
    let mut timer = None;

    let banner = apply_pomodoro_request(&mut timer, start(50, "deep work"), now);
    assert_eq!(banner.message, "started deep work for 50 min");
    let running = timer.expect("start should arm the timer");
    assert_eq!(running.label, "deep work");
    // The duration is measured from the caller's clock, not re-read inside.
    assert_eq!(running.ends_at, now + chrono::Duration::minutes(50));
}

/// A second start replaces the running block instead of being refused, and the
/// banner has to say so: silently restarting a 50 minute timer as a 25 minute
/// one is the kind of thing you only notice at the wrong moment.
#[test]
fn apply_pomodoro_request_restarts_a_running_timer() {
    let now = Utc::now();
    let mut timer = None;
    apply_pomodoro_request(&mut timer, start(50, "deep work"), now);

    let banner = apply_pomodoro_request(&mut timer, start(5, "quick"), now);
    assert_eq!(banner.message, "restarted quick for 5 min");
    let running = timer.expect("restart should leave a timer armed");
    assert_eq!(running.label, "quick");
    assert_eq!(running.ends_at, now + chrono::Duration::minutes(5));
}

#[test]
fn apply_pomodoro_request_stops_a_running_timer() {
    let now = Utc::now();
    let mut timer = Some(PomodoroTimer {
        label: "deep work".to_string(),
        ends_at: now + chrono::Duration::minutes(25),
    });

    let banner = apply_pomodoro_request(&mut timer, PomodoroRequest::Stop, now);
    assert_eq!(banner.message, "stopped deep work");
    assert!(timer.is_none(), "stop should clear the timer");
}

/// Stopping nothing is a user error, not a silent no-op: without the banner
/// there is no feedback at all, because the HUD badge was already absent.
#[test]
fn apply_pomodoro_request_reports_stopping_with_no_timer() {
    let mut timer = None;

    let banner = apply_pomodoro_request(&mut timer, PomodoroRequest::Stop, Utc::now());
    assert!(
        banner.message.contains("no pomodoro running"),
        "expected a usage banner, got: {}",
        banner.message
    );
    assert!(timer.is_none());
}
