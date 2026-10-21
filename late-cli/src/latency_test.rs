use std::time::{Duration, Instant};

use serde_json::json;

use super::*;

fn ms(millis: u64) -> Duration {
    Duration::from_millis(millis)
}

#[test]
fn the_terminal_sample_runs_from_the_query_going_out_to_its_answer() {
    let watch = TerminalProbeWatch::default();
    let start = Instant::now();
    assert_eq!(
        watch.saw_input(b"\x1b[0n", start),
        None,
        "an answer with no query seen"
    );

    watch.saw_output(b"frame bytes\x1b[5n", start);
    assert_eq!(
        watch.saw_input(b"jk", start + ms(5)),
        None,
        "keystrokes are not the answer"
    );
    assert_eq!(
        watch.saw_input(b"j\x1b[0n", start + ms(12)),
        Some(LatencySample {
            kind: LatencyKind::Terminal,
            elapsed: ms(12),
        })
    );
    assert_eq!(
        watch.saw_input(b"\x1b[0n", start + ms(20)),
        None,
        "one answer per query"
    );
}

#[test]
fn a_sample_goes_over_the_wire_as_a_latency_event() {
    let rtt = LatencySample {
        kind: LatencyKind::SshRtt,
        elapsed: Duration::from_micros(42_000),
    };
    assert_eq!(
        rtt.payload(),
        json!({ "event": "latency", "kind": "ssh_rtt", "micros": 42000 })
    );
    let terminal = LatencySample {
        kind: LatencyKind::Terminal,
        elapsed: Duration::from_micros(900),
    };
    assert_eq!(
        terminal.payload(),
        json!({ "event": "latency", "kind": "terminal", "micros": 900 })
    );
}
