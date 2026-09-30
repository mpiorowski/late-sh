use std::time::Instant;

use chrono::{DateTime, TimeZone, Utc};
use uuid::Uuid;

use super::*;

fn t0() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 9, 28, 14, 0, 0).unwrap()
}

fn at(secs: i64) -> DateTime<Utc> {
    t0() + chrono::Duration::seconds(secs)
}

fn candidate(source: LiveSource, secs: i64) -> LiveCandidate {
    LiveCandidate {
        source,
        updated: at(secs),
        aimed_at: None,
    }
}

fn up(source: LiveSource, secs: i64) -> Option<Featured> {
    Some(Featured {
        source,
        since: at(secs),
    })
}

/// One busy stretch from the stamps alone, as a session that just connected
/// sees it at each moment: a link jumps the Rest lane at the next handover,
/// every source keeps its minimum, and the last one up comes down
/// `LIVE_MAX_UP` after it went up.
#[test]
fn the_lanes_hand_over_at_each_minimum_and_news_goes_first() {
    let track = LiveSource::BoothTrack(Uuid::from_u128(1));
    let chess = LiveSource::DailyMatch(Uuid::from_u128(2));
    let pool = LiveSource::DailyMatch(Uuid::from_u128(3));
    let link = LiveSource::NewsArticle(Uuid::from_u128(4));
    let candidates = [
        candidate(track, 0),
        candidate(chess, 30),
        candidate(link, 50),
        candidate(pool, 100),
    ];
    let pick = |secs| pick_queued(None, &candidates, at(secs));

    assert_eq!(pick(-1), None, "nothing has happened yet");
    assert_eq!(pick(0), up(track, 0));
    assert_eq!(pick(119), up(track, 0), "the track keeps its two minutes");
    assert_eq!(pick(120), up(link, 120), "the link goes ahead of the move");
    assert_eq!(pick(419), up(link, 120), "and stays its five minutes");
    assert_eq!(pick(420), up(chess, 420), "then the rest, oldest first");
    assert_eq!(pick(480), up(pool, 480));
    assert_eq!(pick(779), up(pool, 480), "nothing waiting: it stays");
    assert_eq!(pick(780), None, "five minutes up, and the strip comes down");
}

/// A burst of links: each gets its five minutes in turn, and one that would
/// have waited past `LIVE_MAX_WAIT` is dropped rather than shown late.
#[test]
fn an_entry_that_waited_too_long_is_dropped() {
    let link = |n: u128| LiveSource::NewsArticle(Uuid::from_u128(n));
    let candidates = [
        candidate(link(1), 0),
        candidate(link(2), 10),
        candidate(link(3), 20),
        candidate(link(4), 30),
    ];
    let pick = |secs| pick_queued(None, &candidates, at(secs));

    assert_eq!(pick(300), up(link(2), 300));
    assert_eq!(
        pick(600),
        up(link(3), 600),
        "waited 580s, under ten minutes"
    );
    assert_eq!(
        pick(900),
        None,
        "the fourth waited past ten minutes: dropped"
    );
}

/// The replay reads only each candidate's latest stamp, so a match that
/// moves again rewrites the history. A session keeps what it has up for its
/// minimum, then follows the replay.
#[test]
fn a_session_keeps_its_minimum_when_a_match_moves_again() {
    let (a, b) = (
        LiveSource::DailyMatch(Uuid::from_u128(1)),
        LiveSource::DailyMatch(Uuid::from_u128(2)),
    );
    let before = [candidate(a, 0), candidate(b, 10)];
    let showing = pick_queued(None, &before, at(5));
    assert_eq!(showing, up(a, 0));

    // A moves again at 30: the replay now reads B from 10, then A.
    let after = [candidate(a, 30), candidate(b, 10)];
    assert_eq!(pick_queued(None, &after, at(30)), up(b, 10));
    assert_eq!(
        pick_queued(showing, &after, at(59)),
        showing,
        "the session keeps A its minute"
    );
    assert_eq!(
        pick_queued(showing, &after, at(60)),
        up(b, 60),
        "then follows the replay"
    );

    // What the session had up is gone (the track was skipped): no hold.
    assert_eq!(pick_queued(showing, &[candidate(b, 10)], at(20)), up(b, 20));
}

/// A cue being lined up is drawn over the Rest lane or an empty strip, the
/// freshest aim first, but never over a link.
#[test]
fn an_aim_draws_over_everything_but_a_link() {
    let start = Instant::now();
    let (chess, pool) = (
        LiveSource::DailyMatch(Uuid::from_u128(1)),
        LiveSource::DailyMatch(Uuid::from_u128(2)),
    );
    let link = LiveSource::NewsArticle(Uuid::from_u128(3));
    let candidates = [
        candidate(chess, 0),
        LiveCandidate {
            source: pool,
            updated: at(-600),
            aimed_at: Some(start),
        },
    ];

    assert_eq!(
        aim_overlay(up(chess, 0), &candidates, start),
        Some(pool),
        "over a move"
    );
    assert_eq!(
        aim_overlay(None, &candidates, start),
        Some(pool),
        "over an empty strip"
    );
    assert_eq!(aim_overlay(up(link, 0), &candidates, start), None);
    assert_eq!(
        aim_overlay(up(chess, 0), &candidates, start + LIVE_AIM_WINDOW),
        None,
        "an aim past its window is a player who stopped"
    );
}
