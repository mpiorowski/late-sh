use chrono::{Duration, NaiveDate, TimeZone, Utc};
use late_core::models::referral::{ActivityDay, InviteCodeOwner};
use uuid::Uuid;

use super::state::*;

#[test]
fn only_an_invite_login_carries_a_code() {
    assert_eq!(
        ssh_invite_code("invite-AbCd2345"),
        Some("abcd2345".to_string())
    );
    // Plain `ssh late.sh` sends the local $USER: never a code, whatever it is.
    assert_eq!(ssh_invite_code("abcd2345"), None);
    assert_eq!(ssh_invite_code("mat"), None);
    assert_eq!(ssh_invite_code("invite-"), None);
    assert_eq!(ssh_invite_code("invite-abcd234"), None);
}

#[test]
fn a_typed_code_can_be_pasted_in_any_shape_a_friend_sends_it() {
    let expected = Some("abcd2345".to_string());
    assert_eq!(typed_invite_code("abcd2345"), expected);
    assert_eq!(typed_invite_code(" ABCD2345 "), expected);
    assert_eq!(typed_invite_code("invite-abcd2345"), expected);
    assert_eq!(typed_invite_code("invite-abcd2345@late.sh"), expected);
    assert_eq!(typed_invite_code("ssh invite-abcd2345@late.sh"), expected);
    assert_eq!(typed_invite_code(&ssh_invite_command("abcd2345")), expected);
    assert_eq!(typed_invite_code("nope"), None);
}

#[test]
fn attach_refuses_self_late_and_newer_inviters() {
    let now = Utc.with_ymd_and_hms(2026, 10, 2, 12, 0, 0).unwrap();
    let invitee_id = Uuid::now_v7();
    let fresh = now - Duration::hours(1);
    let inviter = |created| InviteCodeOwner {
        user_id: Uuid::now_v7(),
        username: "mat".to_string(),
        created,
    };
    let veteran = inviter(now - Duration::days(400));

    assert_eq!(attach_check(invitee_id, fresh, &veteran, now), Ok(()));
    assert_eq!(
        attach_check(
            invitee_id,
            fresh,
            &InviteCodeOwner {
                user_id: invitee_id,
                ..veteran.clone()
            },
            now
        ),
        Err(AttachRefusal::OwnCode)
    );
    assert_eq!(
        attach_check(
            invitee_id,
            now - Duration::days(ATTACH_WINDOW_DAYS) - Duration::seconds(1),
            &veteran,
            now
        ),
        Err(AttachRefusal::WindowClosed)
    );
    assert_eq!(
        attach_check(invitee_id, fresh, &inviter(fresh), now),
        Err(AttachRefusal::InviterNewer)
    );
}

fn days(count: usize, minutes: i32) -> Vec<ActivityDay> {
    let start = NaiveDate::from_ymd_opt(2026, 9, 1).unwrap();
    (0..count)
        .map(|offset| ActivityDay {
            day: start + Duration::days(offset as i64),
            active_minutes: minutes,
        })
        .collect()
}

/// The bar sits exactly on its two thresholds: meeting both qualifies,
/// missing either by the smallest step does not.
#[test]
fn the_bar_needs_both_days_and_minutes() {
    // Rounded up: the fewest whole minutes a day that reach the total.
    let days_needed = MIN_ACTIVE_DAYS as i64;
    let minutes_per_day = ((MIN_TOTAL_ACTIVE_MINUTES + days_needed - 1) / days_needed) as i32;
    assert!(minutes_per_day >= MIN_DAY_ACTIVE_MINUTES);
    assert!(qualifies(&days(MIN_ACTIVE_DAYS, minutes_per_day)));

    // One counting day short.
    assert!(!qualifies(&days(MIN_ACTIVE_DAYS - 1, minutes_per_day * 2)));
    // Enough days, too few minutes overall.
    assert!(!qualifies(&days(MIN_ACTIVE_DAYS, MIN_DAY_ACTIVE_MINUTES)));
    // Days just under the per-day floor do not count, whatever their sum.
    assert!(!qualifies(&days(
        MIN_ACTIVE_DAYS * 10,
        MIN_DAY_ACTIVE_MINUTES - 1
    )));
}

#[test]
fn a_window_that_closes_short_expires() {
    let until = Utc.with_ymd_and_hms(2026, 11, 1, 0, 0, 0).unwrap();
    let before = until - Duration::seconds(1);
    let met = days(MIN_ACTIVE_DAYS, 200);
    let short = days(2, 200);

    assert_eq!(verdict(&met, until, before), Verdict::Qualified);
    // Clearing the bar on the last sweep still counts after the window.
    assert_eq!(verdict(&met, until, until), Verdict::Qualified);
    assert_eq!(verdict(&short, until, before), Verdict::StillJudging);
    assert_eq!(verdict(&short, until, until), Verdict::Expired);
}

/// Driven through a fixed run of keystrokes and ticks: a burst inside one
/// minute reports once, the next minute reports again, a tick with no new
/// input reports nothing, and nothing reports once the window has closed.
#[test]
fn the_clock_reports_each_typed_minute_once_until_the_window_closes() {
    let created = Utc.with_ymd_and_hms(2026, 10, 1, 0, 0, 0).unwrap();
    let t = Utc.with_ymd_and_hms(2026, 10, 2, 9, 15, 10).unwrap();
    let end = judged_until(created);
    let mut clock = NewcomerClock::new(created, t);

    let mut reported = Vec::new();
    clock.note_input(t);
    clock.note_input(t + Duration::seconds(5));
    reported.push(clock.take_minute());
    clock.note_input(t + Duration::seconds(20));
    reported.push(clock.take_minute());
    reported.push(clock.take_minute());
    clock.note_input(t + Duration::seconds(55));
    reported.push(clock.take_minute());
    clock.note_input(end - Duration::seconds(1));
    reported.push(clock.take_minute());
    clock.note_input(end);
    reported.push(clock.take_minute());
    clock.note_input(end + Duration::minutes(5));
    reported.push(clock.take_minute());

    assert_eq!(
        reported,
        vec![
            Some(t + Duration::seconds(5)),
            None,
            None,
            Some(t + Duration::seconds(55)),
            Some(end - Duration::seconds(1)),
            None,
            None,
        ]
    );
    assert_eq!(
        NewcomerClock::new(created, end),
        NewcomerClock::inert(),
        "an account past its window starts inert"
    );
}
