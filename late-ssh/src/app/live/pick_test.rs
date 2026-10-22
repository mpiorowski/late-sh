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
/// sees it at each moment: each source goes up in the order it happened,
/// keeps its minimum, and the last one up comes down `LIVE_MAX_UP` after it
/// went up.
#[test]
fn the_queue_hands_over_at_each_minimum_oldest_first() {
    let track = LiveSource::BoothTrack(Uuid::from_u128(1));
    let chess = LiveSource::DailyMatch(Uuid::from_u128(2));
    let pool = LiveSource::DailyMatch(Uuid::from_u128(3));
    let candidates = [
        candidate(track, 0),
        candidate(chess, 30),
        candidate(pool, 100),
    ];
    let pick = |secs| pick_queued(None, &candidates, at(secs));

    assert_eq!(pick(-1), None, "nothing has happened yet");
    assert_eq!(pick(0), up(track, 0));
    assert_eq!(pick(119), up(track, 0), "the track keeps its two minutes");
    assert_eq!(pick(120), up(chess, 120), "then the move, oldest first");
    assert_eq!(pick(179), up(chess, 120), "which keeps its minute");
    assert_eq!(pick(180), up(pool, 180));
    assert_eq!(pick(479), up(pool, 180), "nothing waiting: it stays");
    assert_eq!(pick(480), None, "five minutes up, and the strip comes down");
}

/// A stream that goes live waits its turn in the queue like a booth
/// track, then keeps its own two minutes before handing over.
#[test]
fn a_stream_going_live_queues_like_a_track_and_keeps_two_minutes() {
    let track = LiveSource::BoothTrack(Uuid::from_u128(1));
    let stream = LiveSource::Stream(Uuid::from_u128(2));
    let chess = LiveSource::DailyMatch(Uuid::from_u128(3));
    let candidates = [
        candidate(track, 0),
        candidate(stream, 10),
        candidate(chess, 20),
    ];
    let pick = |secs| pick_queued(None, &candidates, at(secs));

    assert_eq!(pick(119), up(track, 0), "the track keeps its two minutes");
    assert_eq!(pick(120), up(stream, 120), "the stream went live first");
    assert_eq!(pick(239), up(stream, 120), "and keeps its two minutes");
    assert_eq!(pick(240), up(chess, 240));
}

/// A burst of booth tracks: each gets its two minutes in turn, and one that
/// would have waited past `LIVE_MAX_WAIT` is dropped rather than shown late.
#[test]
fn an_entry_that_waited_too_long_is_dropped() {
    let track = |n: i64| LiveSource::BoothTrack(Uuid::from_u128(n as u128));
    let candidates: Vec<LiveCandidate> = (0..7)
        .map(|n| candidate(track(n), n * 10))
        .collect();
    let pick = |secs| pick_queued(None, &candidates, at(secs));

    assert_eq!(pick(120), up(track(1), 120));
    assert_eq!(
        pick(600),
        up(track(5), 600),
        "waited 550s, under ten minutes"
    );
    assert_eq!(
        pick(720),
        up(track(5), 600),
        "the seventh waited past ten minutes: dropped, so nothing takes over"
    );
    assert_eq!(pick(900), None);
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

/// A cue being lined up is drawn over whatever is up, or over an empty
/// strip, the freshest aim first.
#[test]
fn an_aim_draws_over_whatever_is_up() {
    let start = Instant::now();
    let pool = LiveSource::DailyMatch(Uuid::from_u128(2));
    let chess = LiveSource::DailyMatch(Uuid::from_u128(1));
    let candidates = [
        candidate(chess, 0),
        LiveCandidate {
            source: pool,
            updated: at(-600),
            aimed_at: Some(start),
        },
    ];

    assert_eq!(aim_overlay(&candidates, start), Some(pool));
    assert_eq!(
        aim_overlay(&candidates, start + LIVE_AIM_WINDOW),
        None,
        "an aim past its window is a player who stopped"
    );
}
