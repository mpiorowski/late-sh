use chrono::NaiveDate;
use late_core::models::user::ComebackMark;

use super::*;

fn day(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).unwrap()
}

const PLAIN_SSH: Standing = Standing { hears: false };
const CLI: Standing = Standing { hears: true };

fn mark(moment: Moment, on: NaiveDate) -> ComebackMark {
    ComebackMark {
        step: moment.step(),
        day: on,
    }
}

/// The series runs one moment per day from the day after the account's
/// first: nothing on the tour's day, nothing twice in a day, nothing past
/// the last, nothing on a mark the series no longer knows.
#[test]
fn a_login_is_owed_one_moment_per_later_day_until_the_series_closes() {
    let created = day(2026, 11, 7);
    let (d2, d3, d4, d5) = (
        day(2026, 11, 8),
        day(2026, 11, 9),
        day(2026, 11, 10),
        day(2026, 11, 11),
    );
    assert_eq!(due_moment(None, created, created, PLAIN_SSH), None);
    assert_eq!(
        due_moment(None, created, d2, PLAIN_SSH),
        Some(Moment::HearIt)
    );
    assert_eq!(
        due_moment(None, created, day(2027, 3, 1), PLAIN_SSH),
        Some(Moment::HearIt),
        "an old account with no mark is owed the series too"
    );
    assert_eq!(
        due_moment(Some(mark(Moment::HearIt, d2)), created, d2, PLAIN_SSH),
        None,
        "nothing twice in a day"
    );
    assert_eq!(
        due_moment(Some(mark(Moment::HearIt, d2)), created, d3, PLAIN_SSH),
        Some(Moment::BigGames)
    );
    assert_eq!(
        due_moment(Some(mark(Moment::BigGames, d3)), created, d4, PLAIN_SSH),
        Some(Moment::People)
    );
    assert_eq!(
        due_moment(Some(mark(Moment::People, d4)), created, d5, PLAIN_SSH),
        None,
        "the series is spent"
    );
    let unknown = ComebackMark { step: 9, day: d2 };
    assert_eq!(due_moment(Some(unknown), created, d3, PLAIN_SSH), None);
}

/// A moment with nothing to sell this session is passed over, not spent:
/// a session that hears the radio gets the games first, then the people,
/// and the series ends there.
#[test]
fn a_moment_with_nothing_to_say_is_passed_over() {
    let created = day(2026, 11, 7);
    let (d2, d3, d4) = (day(2026, 11, 8), day(2026, 11, 9), day(2026, 11, 10));
    assert_eq!(due_moment(None, created, d2, CLI), Some(Moment::BigGames));
    assert_eq!(
        due_moment(Some(mark(Moment::BigGames, d2)), created, d3, CLI),
        Some(Moment::People)
    );
    assert_eq!(
        due_moment(Some(mark(Moment::People, d3)), created, d4, CLI),
        None
    );
}

#[test]
fn slash_comeback_takes_a_moments_number_and_nothing_else() {
    assert_eq!(parse_command("/comeback 1"), Some(Some(Moment::HearIt)));
    assert_eq!(parse_command("/comeback 2"), Some(Some(Moment::BigGames)));
    assert_eq!(
        parse_command("  /comeback   3 "),
        Some(Some(Moment::People))
    );
    assert_eq!(parse_command("/comeback"), Some(None));
    assert_eq!(parse_command("/comeback 7"), Some(None));
    assert_eq!(parse_command("/comeback one"), Some(None));
    assert_eq!(parse_command("/comeback 1 2"), Some(None));
    assert_eq!(parse_command("/comebacks"), None);
    assert_eq!(parse_command("/onboard"), None);
}

#[test]
fn the_steps_round_trip_and_the_last_closes_the_series() {
    for moment in [Moment::HearIt, Moment::BigGames, Moment::People] {
        assert_eq!(Moment::from_step(moment.step()), Some(moment));
    }
    assert_eq!(Moment::from_step(0), None);
    assert_eq!(Moment::from_step(Moment::LAST.step() + 1), None);
}
