use std::borrow::Cow;
use std::time::{Duration, Instant};

use crate::terminal_probe::*;

const SECOND: Duration = Duration::from_secs(1);

#[test]
fn strip_replies_removes_every_reply_and_keeps_the_input_in_order() {
    let (input, replies) = strip_replies(b"jk");
    assert!(matches!(input, Cow::Borrowed(b"jk")));
    assert_eq!(replies, 0);

    let (input, replies) = strip_replies(b"\x1b[0nq\x1b[A\x1b[0n");
    assert_eq!((input.as_ref(), replies), (&b"q\x1b[A"[..], 2));

    // Other terminal reports are input the app parses itself.
    let (input, replies) = strip_replies(b"\x1b[?62;4c\x1b[12;40R");
    assert_eq!(
        (input.as_ref(), replies),
        (&b"\x1b[?62;4c\x1b[12;40R"[..], 0)
    );
}

#[test]
fn probe_backlog_buckets_the_bytes_written_ahead() {
    let buckets = [0, 1, 4096, 4097, 32768, 32769].map(ProbeBacklog::from_bytes);
    assert_eq!(
        buckets,
        [
            ProbeBacklog::None,
            ProbeBacklog::Small,
            ProbeBacklog::Small,
            ProbeBacklog::Medium,
            ProbeBacklog::Medium,
            ProbeBacklog::Large,
        ]
    );
}

#[test]
fn a_session_holds_one_probe_at_a_time_on_a_ten_second_cadence() {
    let start = Instant::now();
    let probe = TerminalProbe::new(start);

    assert!(!probe.in_flight());
    assert!(probe.try_start(start, 900), "first pass is probed");
    assert!(probe.in_flight());
    assert!(!probe.try_start(start + SECOND, 0), "one probe in flight");
    assert_eq!(
        probe.answered(start + Duration::from_millis(80)),
        Some(ProbeSample {
            rtt: Duration::from_millis(80),
            backlog: ProbeBacklog::Small,
        })
    );
    assert!(!probe.in_flight());
    assert_eq!(probe.answered(start + SECOND), None, "a stray reply");

    assert!(!probe.try_start(start + 9 * SECOND, 0));
    assert!(probe.try_start(start + 11 * SECOND, 0));
    // A query that never left frees the slot, and the next waits an interval.
    probe.cancel(start + 11 * SECOND);
    assert_eq!(probe.answered(start + 12 * SECOND), None);
    assert!(
        !probe.try_start(start + 12 * SECOND, 40_000),
        "a large pass waits the interval too"
    );
    assert!(!probe.try_start(start + 20 * SECOND, 0));
    assert!(probe.try_start(start + 21 * SECOND, 0));
}

#[test]
fn a_large_frame_is_probed_two_seconds_after_the_last_probe() {
    let start = Instant::now();
    let probe = TerminalProbe::new(start);
    assert!(probe.try_start(start, 0));
    assert!(probe.answered(start + Duration::from_millis(50)).is_some());

    assert!(!probe.try_start(start + SECOND, 40_000), "inside the gap");
    assert!(!probe.try_start(start + 3 * SECOND, 20_000), "not large");
    assert!(probe.try_start(start + 3 * SECOND, 40_000));
    assert_eq!(
        probe.answered(start + 3 * SECOND + Duration::from_millis(400)),
        Some(ProbeSample {
            rtt: Duration::from_millis(400),
            backlog: ProbeBacklog::Large,
        })
    );
}

#[test]
fn the_backlog_is_the_second_of_output_before_the_probe_not_one_pass() {
    let ms = Duration::from_millis;
    let start = Instant::now();
    let probe = TerminalProbe::new(start);
    assert!(probe.try_start(start, 0));
    assert!(probe.answered(start + ms(50)).is_some());

    // An animation pass writes 40 KiB inside the large-frame gap, then a
    // clean pass runs 700 ms later with those bytes still in the pipe.
    assert!(!probe.try_start(start + ms(1500), 40_000), "inside the gap");
    assert!(
        probe.try_start(start + ms(2200), 0),
        "the clean pass is probed behind the large backlog"
    );
    assert_eq!(
        probe.answered(start + ms(2500)),
        Some(ProbeSample {
            rtt: ms(300),
            backlog: ProbeBacklog::Large,
        })
    );

    // Passes inside the window add up; output older than the window is gone.
    assert!(!probe.try_start(start + ms(12_400), 3_000), "not due yet");
    assert!(probe.try_start(start + ms(13_000), 2_000));
    assert_eq!(
        probe.answered(start + ms(13_100)),
        Some(ProbeSample {
            rtt: ms(100),
            backlog: ProbeBacklog::Medium,
        })
    );
}

#[test]
fn a_terminal_that_never_answers_retires_the_probe_after_three_losses() {
    let start = Instant::now();
    let probe = TerminalProbe::new(start);
    let timeout = 30 * SECOND;

    assert!(probe.try_start(start, 0));
    assert_eq!(probe.expire(start + timeout - SECOND), None);
    assert_eq!(probe.expire(start + timeout), Some(ProbeExpiry::Unanswered));

    // The next probe waits a full interval, so a reply this slow lands on
    // nothing instead of being credited to the next probe.
    assert!(!probe.try_start(start + timeout, 0), "quiet after a loss");
    assert!(
        !probe.try_start(start + timeout + SECOND, 40_000),
        "a large backlog waits the interval after a loss too"
    );
    assert_eq!(
        probe.answered(start + timeout + 5 * SECOND),
        None,
        "a late reply"
    );

    // An answer in between clears the streak.
    assert!(probe.try_start(start + timeout + 10 * SECOND, 0));
    assert!(probe.answered(start + timeout + 11 * SECOND).is_some());

    let mut now = start + timeout + 21 * SECOND;
    let mut expiries = Vec::new();
    for _ in 0..3 {
        assert!(probe.try_start(now, 0));
        now += timeout;
        expiries.push(probe.expire(now));
        now += 10 * SECOND;
    }
    assert_eq!(
        expiries,
        [
            Some(ProbeExpiry::Unanswered),
            Some(ProbeExpiry::Unanswered),
            Some(ProbeExpiry::Retired),
        ]
    );
    assert!(!probe.try_start(now + 60 * SECOND, 0), "retired for good");
    assert_eq!(probe.answered(now + 61 * SECOND), None);
    let tally = probe.tally();
    assert_eq!((tally.lost, tally.retired, tally.answered()), (4, true, 1));
}

#[test]
fn a_session_tallies_its_trips_by_backlog_and_counts_its_losses() {
    let ms = Duration::from_millis;
    let start = Instant::now();
    let probe = TerminalProbe::new(start);

    assert!(probe.try_start(start, 0));
    probe.answered(start + ms(40));
    assert!(probe.try_start(start + ms(3_000), 40_000));
    probe.answered(start + ms(3_400));
    assert!(probe.try_start(start + ms(14_000), 0));
    probe.answered(start + ms(14_060));
    assert!(probe.try_start(start + ms(25_000), 0));
    assert_eq!(
        probe.expire(start + ms(55_000)),
        Some(ProbeExpiry::Unanswered)
    );

    let tally = probe.tally();
    assert_eq!(
        tally,
        ProbeTally {
            none: TripTally {
                count: 2,
                fastest: ms(40),
                slowest: ms(60),
                total: ms(100),
            },
            large: TripTally {
                count: 1,
                fastest: ms(400),
                slowest: ms(400),
                total: ms(400),
            },
            lost: 1,
            ..ProbeTally::default()
        }
    );
    assert_eq!(tally.answered(), 3);
    assert_eq!(tally.fastest(), Some(ms(40)));
    assert_eq!(tally.none.mean(), Some(ms(50)));
    assert_eq!(tally.small.mean(), None);
}
