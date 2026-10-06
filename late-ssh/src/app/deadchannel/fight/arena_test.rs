use super::{Reading, publish, report, runs, sweep};
use crate::app::deadchannel::fight::data::{FoeTier, RULES, Rules};

/// The candidates `make deadchannel-sweep` reads side by side. Edit this
/// list for the question at hand; the first row is always the live game.
const SWEEP: &[(&str, Rules)] = &[
    ("live", RULES),
    (
        "boss 280",
        Rules {
            old_signal: boss(280, 39),
            ..RULES
        },
    ),
    (
        "boss 300",
        Rules {
            old_signal: boss(300, 39),
            ..RULES
        },
    ),
    (
        "hits 175%",
        Rules {
            hit_percent: 175,
            ..RULES
        },
    ),
    (
        "hits 195%",
        Rules {
            hit_percent: 195,
            ..RULES
        },
    ),
    (
        "blocks 55%",
        Rules {
            block_percent: 55,
            ..RULES
        },
    ),
    (
        "blocks 65%",
        Rules {
            block_percent: 65,
            ..RULES
        },
    ),
    (
        "bright 135%",
        Rules {
            bright_signal_percent: 135,
            ..RULES
        },
    ),
    (
        "jab 50%",
        Rules {
            jab_percent: 50,
            ..RULES
        },
    ),
    (
        "sever 200%",
        Rules {
            sever_percent: 200,
            ..RULES
        },
    ),
];

/// The Old Signal with `signal` and `attack`, its defense the live one.
const fn boss(signal: i32, attack: u32) -> FoeTier {
    FoeTier {
        signal,
        attack,
        ..RULES.old_signal
    }
}

/// Every table for the live rules. `make deadchannel-arena`.
#[test]
#[ignore = "the report: run it with make deadchannel-arena"]
fn arena_report() {
    publish("deadchannel-arena.md", &report(RULES));
}

/// Whole runs, start to finish, for the live rules. `make deadchannel-run`.
#[test]
#[ignore = "the printed runs: run it with make deadchannel-run"]
fn arena_run() {
    publish("deadchannel-run.md", &runs(RULES));
}

/// One reading per candidate in [`SWEEP`]. `make deadchannel-sweep`.
#[test]
#[ignore = "the sweep: run it with make deadchannel-sweep"]
fn arena_sweep() {
    publish("deadchannel-sweep.md", &sweep(SWEEP));
}

/// The contract: the live rules miss no target (`fight/BALANCE.md`). A
/// failure names every band the change walked out of.
#[test]
fn the_live_rules_miss_no_target() {
    let reading = Reading::take(RULES);
    assert_eq!(reading.misses(), Vec::<String>::new(), "{reading:?}");
}
