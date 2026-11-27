//! The closing week's #lounge lines: when one is due and what it says.
//! Pure. The claim, the post and their logs and metrics are `svc.rs`'s
//! (`GalleryService::run_vote_call`).

use chrono::{DateTime, Datelike, Months, NaiveDate, Timelike, Utc};
use late_core::models::artboard_piece::{GALLERY_AWARD_MIN_APPLAUSE, MonthStandings};
use late_core::models::profile_award::gallery_prize_chips;

use crate::app::common::primitives::thousands;

/// The days a call goes out on, as days left on the month's wall with
/// today counted: the first of its last seven days, three days out, and
/// the last day.
const VOTE_CALL_DAYS_LEFT: [i32; 3] = [7, 3, 1];

/// UTC hour from which a call day's line is posted: the award roll's hour
/// (`leaderboard/svc.rs::AWARD_ROLL_HOUR_UTC`), the overlap of US and
/// European waking hours. On the last day it leaves six hours to applaud.
const VOTE_CALL_HOUR_UTC: u32 = 18;

/// A call whose day and hour have come.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct DueCall {
    pub(super) period_month: NaiveDate,
    pub(super) days_left: i32,
}

/// The call due at `now`, if this is a call day past the posting hour. A
/// call is due on its own day only: one missed (the process was down from
/// the hour to midnight) is skipped, since its line would count the days
/// wrong.
pub(super) fn vote_call_due(now: DateTime<Utc>) -> Option<DueCall> {
    if now.hour() < VOTE_CALL_HOUR_UTC {
        return None;
    }
    let today = now.date_naive();
    let period_month = today.with_day(1).expect("every month has a first day");
    let next_month = period_month
        .checked_add_months(Months::new(1))
        .expect("the month after this one exists");
    let days_left = (next_month - today).num_days() as i32;
    VOTE_CALL_DAYS_LEFT.contains(&days_left).then_some(DueCall {
        period_month,
        days_left,
    })
}

/// The line itself: when the wall closes, where the month stands, what
/// Enter on the line does, and what the places pay.
pub(super) fn vote_call_body(call: DueCall, standings: MonthStandings) -> String {
    let month = call.period_month.format("%B");
    let closes = match call.days_left {
        1 => format!("\u{1F3A8} {month}'s wall closes at midnight UTC."),
        days => format!("\u{1F3A8} {month}'s wall closes in {days} days."),
    };
    let hangs = match standings.pieces {
        1 => "1 piece hangs".to_string(),
        pieces => format!("{pieces} pieces hang"),
    };
    let leader = match standings.top_applause >= GALLERY_AWARD_MIN_APPLAUSE {
        true => format!("the leader has {} applause", standings.top_applause),
        false => format!("none has the {GALLERY_AWARD_MIN_APPLAUSE} applause a prize takes yet"),
    };
    let mut standing = format!("{hangs} and {leader}.");
    match standings.one_short {
        0 => {}
        1 => standing.push_str(" 1 is one applause short of counting."),
        short => standing.push_str(&format!(" {short} are one applause short of counting.")),
    }
    let [first, second, third] = [1, 2, 3].map(|rank| {
        thousands(gallery_prize_chips(rank).expect("every podium place pays a gallery prize"))
    });
    [
        closes,
        standing,
        "Enter on this message shows them one by one: v applauds, \u{2192} is the next piece."
            .to_string(),
        format!("1st takes ART1 and {first} chips, 2nd {second}, 3rd {third}."),
    ]
    .join("\n")
}

#[cfg(test)]
#[path = "vote_call_test.rs"]
mod vote_call_test;
