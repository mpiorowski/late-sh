use std::time::{Duration, Instant};

use chrono::{TimeZone, Utc};
use uuid::Uuid;

use super::*;
use crate::app::live::pick::LIVE_MAX_UP;

/// One evening on the strip, from the stamps alone: a booth track and a
/// daily move take turns in the order they happened, each for its minimum,
/// the match's result joins the queue when it ends and opens nothing, a
/// shared link goes up at the next handover for its five minutes, and the
/// strip comes down when nothing is left.
#[test]
fn a_track_a_match_and_its_result_take_turns_and_a_link_cuts_in() {
    let start = Instant::now();
    let t0 = Utc.with_ymd_and_hms(2026, 9, 29, 21, 0, 0).unwrap();
    let track = LiveSource::BoothTrack(Uuid::from_u128(1));
    let chess = LiveSource::DailyMatch(Uuid::from_u128(2));
    let result = LiveSource::DailyResult(Uuid::from_u128(2));
    let article = LiveSource::NewsArticle(Uuid::from_u128(3));
    let candidate = |source, secs: i64| LiveCandidate {
        source,
        updated: t0 + chrono::Duration::seconds(secs),
        aimed_at: None,
    };
    let playing = [candidate(chess, 20), candidate(track, 0)];
    let finished = [candidate(result, 150), candidate(track, 0)];
    let shared = [
        candidate(result, 150),
        candidate(track, 0),
        candidate(article, 230),
    ];
    let mut live = LiveState::new();
    let mut step = |candidates: &[LiveCandidate], secs: u64| {
        let changed = live.refresh(
            candidates,
            start + Duration::from_secs(secs),
            t0 + chrono::Duration::seconds(secs as i64),
            false,
        );
        (changed, live.showing(), live.opens())
    };

    assert_eq!(
        step(&playing, 5),
        (true, Some(track), Some(track)),
        "the track somebody queued goes up"
    );
    assert_eq!(
        step(&playing, 119),
        (false, Some(track), Some(track)),
        "the chess move waits out the track's two minutes"
    );
    assert_eq!(
        step(&playing, 120),
        (true, Some(chess), Some(chess)),
        "then takes the strip"
    );
    assert_eq!(
        step(&finished, 150),
        (true, Some(result), None),
        "the match ends: its result takes its place and opens nothing"
    );
    assert_eq!(step(&finished, 209), (false, Some(result), None));
    assert_eq!(
        step(&shared, 230),
        (true, Some(article), Some(article)),
        "a shared link goes up at the next handover"
    );
    assert_eq!(step(&shared, 529), (false, Some(article), Some(article)));
    assert_eq!(
        step(&shared, 530),
        (true, None, None),
        "the link's five minutes are up and nothing is waiting"
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

    assert!(!live.refresh(&candidates, start, at(1), true));
    assert_eq!(live.showing(), None, "it does not go up under a selection");
    assert!(live.refresh(&candidates, start, at(2), false));
    assert_eq!(live.showing(), Some(track));

    let stale = LIVE_MAX_UP.as_secs() as i64;
    assert!(!live.refresh(&candidates, start, at(stale), true));
    assert_eq!(live.showing(), Some(track), "nor come down");

    // The track left the booth: there is nothing left to hold.
    assert!(live.refresh(&[], start, at(stale), true));
    assert_eq!(live.showing(), None);
}
