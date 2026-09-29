use std::time::{Duration, Instant};

use chrono::{TimeZone, Utc};
use uuid::Uuid;

use super::*;
use crate::app::live::pick::LIVE_STRIP_LINGER;

/// One evening on the strip, from the stamps alone: a booth track and a
/// daily move each get their turn in the order they happened, a result cuts
/// in for its minute, and the strip comes down when the news is old.
#[test]
fn a_track_and_a_match_take_turns_and_a_result_cuts_in() {
    let start = Instant::now();
    let t0 = Utc.with_ymd_and_hms(2026, 9, 29, 21, 0, 0).unwrap();
    let track = LiveSource::BoothTrack(Uuid::from_u128(1));
    let chess = LiveSource::DailyMatch(Uuid::from_u128(2));
    let candidates = [
        LiveCandidate {
            source: chess,
            updated: t0 + chrono::Duration::seconds(20),
            aimed_at: None,
        },
        LiveCandidate {
            source: track,
            updated: t0,
            aimed_at: None,
        },
    ];
    let mut live = LiveState::new();
    let step = |live: &mut LiveState, secs: u64, finished_at: Option<Instant>| {
        let changed = live.refresh(
            &candidates,
            finished_at,
            start + Duration::from_secs(secs),
            t0 + chrono::Duration::seconds(secs as i64),
            false,
        );
        (changed, live.showing(), live.opens())
    };

    assert_eq!(
        step(&mut live, 5, None),
        (true, Some(Showing::Featured(track)), Some(track)),
        "the track somebody queued goes up"
    );
    assert_eq!(
        step(&mut live, 30, None),
        (false, Some(Showing::Featured(track)), Some(track)),
        "the chess move waits for the track's minute"
    );
    assert_eq!(
        step(&mut live, 60, None),
        (true, Some(Showing::Featured(chess)), Some(chess)),
        "then takes the strip"
    );

    // A match ends at 70: the result holds the strip for a minute and
    // opens nothing, then the strip goes back to what is featured.
    let finished_at = Some(start + Duration::from_secs(70));
    assert_eq!(
        step(&mut live, 70, finished_at),
        (true, Some(Showing::DailyFinish), None)
    );
    assert_eq!(
        step(&mut live, 129, finished_at),
        (false, Some(Showing::DailyFinish), None)
    );
    assert_eq!(
        step(&mut live, 130, finished_at),
        (true, Some(Showing::Featured(chess)), Some(chess))
    );

    let stale = 20 + LIVE_STRIP_LINGER.as_secs();
    assert_eq!(
        step(&mut live, stale - 1, finished_at),
        (false, Some(Showing::Featured(chess)), Some(chess))
    );
    assert_eq!(
        step(&mut live, stale, finished_at),
        (true, None, None),
        "nothing happened for five minutes: the strip comes down"
    );
}

/// Going up or coming down would shift the messages under a selection, so
/// both wait for the viewer to stop reading.
#[test]
fn the_strip_holds_its_height_while_the_viewer_reads() {
    let start = Instant::now();
    let t0 = Utc.with_ymd_and_hms(2026, 9, 29, 21, 0, 0).unwrap();
    let track = LiveSource::BoothTrack(Uuid::from_u128(1));
    let candidates = [LiveCandidate {
        source: track,
        updated: t0,
        aimed_at: None,
    }];
    let mut live = LiveState::new();
    let at = |secs: i64| t0 + chrono::Duration::seconds(secs);

    assert!(!live.refresh(&candidates, None, start, at(1), true));
    assert_eq!(live.showing(), None, "it does not go up under a selection");
    assert!(live.refresh(&candidates, None, start, at(2), false));
    assert_eq!(live.showing(), Some(Showing::Featured(track)));

    let stale = LIVE_STRIP_LINGER.as_secs() as i64;
    assert!(!live.refresh(&candidates, None, start, at(stale), true));
    assert_eq!(
        live.showing(),
        Some(Showing::Featured(track)),
        "nor come down"
    );

    // The track left the booth: there is nothing left to hold.
    assert!(live.refresh(&[], None, start, at(stale), true));
    assert_eq!(live.showing(), None);
}
