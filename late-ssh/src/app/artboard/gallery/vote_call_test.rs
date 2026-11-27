use chrono::{NaiveDate, TimeZone, Utc};
use late_core::models::artboard_piece::MonthStandings;

use super::{DueCall, vote_call_body, vote_call_due};

fn at(year: i32, month: u32, day: u32, hour: u32, minute: u32) -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(year, month, day, hour, minute, 0)
        .single()
        .expect("a real instant")
}

fn month(year: i32, month: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(year, month, 1).expect("a real month")
}

#[test]
fn a_call_is_due_from_the_hour_on_its_three_days_and_no_other() {
    let due = |day, hour, minute| vote_call_due(at(2026, 11, day, hour, minute));
    let november = |days_left| {
        Some(DueCall {
            period_month: month(2026, 11),
            days_left,
        })
    };
    assert_eq!(due(24, 17, 59), None, "before the hour");
    assert_eq!(due(24, 18, 0), november(7));
    assert_eq!(due(25, 20, 0), None, "six days left is no call day");
    assert_eq!(due(28, 18, 5), november(3));
    assert_eq!(due(30, 23, 59), november(1));
    assert_eq!(due(1, 18, 0), None);

    // The count follows the month's own length.
    assert_eq!(
        vote_call_due(at(2027, 2, 22, 18, 0)),
        Some(DueCall {
            period_month: month(2027, 2),
            days_left: 7,
        })
    );
    assert_eq!(
        vote_call_due(at(2026, 12, 31, 18, 0)),
        Some(DueCall {
            period_month: month(2026, 12),
            days_left: 1,
        })
    );
}

#[test]
fn the_line_says_when_the_wall_closes_and_where_the_month_stands() {
    assert_eq!(
        vote_call_body(
            DueCall {
                period_month: month(2026, 11),
                days_left: 7,
            },
            MonthStandings {
                pieces: 14,
                top_applause: 6,
                one_short: 4,
            },
        ),
        "🎨 November's wall closes in 7 days.\n\
         14 pieces hang and the leader has 6 applause. 4 are one applause short of counting.\n\
         Enter on this message shows them one by one: v applauds, → is the next piece.\n\
         1st takes ART1 and 40,000 chips, 2nd 15,000, 3rd 10,000."
    );
    assert_eq!(
        vote_call_body(
            DueCall {
                period_month: month(2026, 11),
                days_left: 1,
            },
            MonthStandings {
                pieces: 1,
                top_applause: 2,
                one_short: 1,
            },
        ),
        "🎨 November's wall closes at midnight UTC.\n\
         1 piece hangs and none has the 3 applause a prize takes yet. 1 is one applause short of counting.\n\
         Enter on this message shows them one by one: v applauds, → is the next piece.\n\
         1st takes ART1 and 40,000 chips, 2nd 15,000, 3rd 10,000."
    );
}
