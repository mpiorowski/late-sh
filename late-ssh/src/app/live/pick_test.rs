use std::time::Instant;

use chrono::{TimeZone, Utc};
use uuid::Uuid;

use super::*;

/// Drives the picker through one lobby's afternoon from the writes alone:
/// three moves a few seconds apart each get their minute in order, an aim
/// jumps the queue, and the clock is the only thing a session brings.
#[test]
fn every_session_picks_the_same_match_from_the_writes_alone() {
    let start = Instant::now();
    let t0 = Utc.with_ymd_and_hms(2026, 9, 28, 14, 0, 0).unwrap();
    let at = |secs: i64| t0 + chrono::Duration::seconds(secs);
    let (chess, pool, reversi) = (
        LiveSource::DailyMatch(Uuid::from_u128(10)),
        LiveSource::DailyMatch(Uuid::from_u128(20)),
        LiveSource::DailyMatch(Uuid::from_u128(30)),
    );
    let candidates = |pool_aimed_at: Option<Instant>| {
        vec![
            LiveCandidate {
                source: reversi,
                updated: at(20),
                aimed_at: None,
            },
            LiveCandidate {
                source: chess,
                updated: at(0),
                aimed_at: None,
            },
            LiveCandidate {
                source: pool,
                updated: at(10),
                aimed_at: pool_aimed_at,
            },
        ]
    };
    let pick = |now_secs: i64| {
        pick_featured(None, &candidates(None), at(now_secs), start).map(|featured| featured.source)
    };

    // The first write goes up; the two behind it wait their turn.
    assert_eq!(pick(5), Some(chess));
    assert_eq!(
        pick(30),
        Some(chess),
        "the pool move landed, chess keeps its minute"
    );
    assert_eq!(pick(70), Some(pool), "pool took over when the hold ran out");
    assert_eq!(pick(130), Some(reversi), "then reversi, a minute later");
    assert_eq!(pick(10_000), Some(reversi), "and stays, nothing newer");

    // A cue being lined up takes the strip whatever the clock says, and
    // the replay resumes when it is put down.
    let aimed = candidates(Some(start));
    assert_eq!(
        pick_featured(None, &aimed, at(130), start + LIVE_AIM_WINDOW / 2).map(|f| f.source),
        Some(pool)
    );
    assert_eq!(
        pick_featured(None, &aimed, at(130), start + LIVE_AIM_WINDOW).map(|f| f.source),
        Some(reversi)
    );

    assert_eq!(pick_featured(None, &[], at(130), start), None);
}

/// A match that moves again loses its old place in the replay, so the
/// replay alone would flip the board early. A session holds the match it is
/// showing for its minute, then rejoins the replay.
#[test]
fn a_match_on_the_strip_keeps_its_minute_when_another_moves_again() {
    let start = Instant::now();
    let t0 = Utc.with_ymd_and_hms(2026, 9, 28, 14, 0, 0).unwrap();
    let at = |secs: i64| t0 + chrono::Duration::seconds(secs);
    let (a, b, c) = (
        LiveSource::DailyMatch(Uuid::from_u128(1)),
        LiveSource::DailyMatch(Uuid::from_u128(2)),
        LiveSource::DailyMatch(Uuid::from_u128(3)),
    );
    let candidate = |source, secs| LiveCandidate {
        source,
        updated: at(secs),
        aimed_at: None,
    };

    // B moved first, then A, then C: A takes the strip when B's minute ends.
    let before = [candidate(b, 0), candidate(a, 10), candidate(c, 20)];
    let showing = pick_featured(None, &before, at(60), start);
    assert_eq!(
        showing,
        Some(Featured {
            source: a,
            since: at(60)
        })
    );

    // B moves again twenty seconds into A's minute. The replay alone now
    // reads A, C, B, and would hand the strip to C.
    let after = [candidate(b, 80), candidate(a, 10), candidate(c, 20)];
    let held = pick_featured(showing, &after, at(80), start);
    assert_eq!(held, showing, "A keeps its minute");
    assert_eq!(pick_featured(held, &after, at(119), start), showing);

    // Then the queue moves on, each for a minute from when this session
    // put it up.
    let next = pick_featured(held, &after, at(120), start);
    assert_eq!(
        next,
        Some(Featured {
            source: c,
            since: at(120)
        })
    );
    assert_eq!(pick_featured(next, &after, at(179), start), next);
    assert_eq!(
        pick_featured(next, &after, at(180), start),
        Some(Featured {
            source: b,
            since: at(180)
        })
    );

    // The hold is for a match still in the lobby: one that left it gives
    // the strip up at once.
    let gone = [candidate(b, 80), candidate(c, 20)];
    assert_eq!(
        pick_featured(showing, &gone, at(79), start).map(|f| f.source),
        Some(c)
    );
}

/// A burst queues more matches than the linger has minutes for. One whose
/// turn comes after its own linger ran out is skipped, so it does not hold
/// an empty strip in front of a move that just landed.
#[test]
fn a_queued_match_that_went_stale_waiting_does_not_take_a_turn() {
    let start = Instant::now();
    let t0 = Utc.with_ymd_and_hms(2026, 9, 28, 14, 0, 0).unwrap();
    let at = |secs: i64| t0 + chrono::Duration::seconds(secs);
    let id = |n: u128| LiveSource::DailyMatch(Uuid::from_u128(n));
    // Seven moves ten seconds apart, then an eighth as the queue runs dry.
    let mut candidates: Vec<LiveCandidate> = (0..7)
        .map(|n| LiveCandidate {
            source: id(n + 1),
            updated: at(n as i64 * 10),
            aimed_at: None,
        })
        .collect();
    candidates.push(LiveCandidate {
        source: id(8),
        updated: at(350),
        aimed_at: None,
    });

    // The seventh would go up at 360, five minutes after its move at 60.
    let pick = pick_featured(None, &candidates, at(365), start);
    assert_eq!(
        pick,
        Some(Featured {
            source: id(8),
            since: at(360)
        }),
        "the fresh move follows the sixth, not the stale seventh"
    );
}

/// The strip goes up on a write and comes down `LIVE_STRIP_LINGER` later,
/// unless the shooter is still lining up.
#[test]
fn the_strip_is_fresh_after_a_write_or_under_an_aim() {
    let start = Instant::now();
    let written = Utc.with_ymd_and_hms(2026, 9, 28, 14, 0, 0).unwrap();
    let just_after = written + chrono::Duration::seconds(30);
    let long_after = written + chrono::Duration::from_std(LIVE_STRIP_LINGER).unwrap();

    assert!(strip_is_fresh(written, None, just_after, start));
    assert!(
        !strip_is_fresh(written, None, long_after, start),
        "the linger ran out"
    );
    assert!(
        strip_is_fresh(
            written,
            Some(start),
            long_after,
            start + LIVE_AIM_WINDOW / 2
        ),
        "a fresh aim keeps a stale match up"
    );
    assert!(
        !strip_is_fresh(written, Some(start), long_after, start + LIVE_AIM_WINDOW),
        "an aim past its window is a player who stopped"
    );
}
