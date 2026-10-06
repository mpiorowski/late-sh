use super::{Reading, publish, report, runs, sweep};
use crate::app::deadchannel::fight::data::{RULES, Rules};

/// The candidates `make deadchannel-sweep` reads side by side. Edit this
/// list for the question at hand; the first row is always the live game.
const SWEEP: &[(&str, Rules)] = &[
    ("live", RULES),
    (
        "hits 150%",
        Rules {
            hit_percent: 150,
            ..RULES
        },
    ),
    (
        "hits 190%",
        Rules {
            hit_percent: 190,
            ..RULES
        },
    ),
    (
        "blocks 50%",
        Rules {
            block_percent: 50,
            ..RULES
        },
    ),
    (
        "blocks 70%",
        Rules {
            block_percent: 70,
            ..RULES
        },
    ),
    (
        "glyph signal 115%",
        Rules {
            foe_signal_percent: 115,
            ..RULES
        },
    ),
    (
        "patch 50%",
        Rules {
            patch_percent: 50,
            ..RULES
        },
    ),
];

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
