use super::{Reading, publish, report, sweep};
use crate::app::deadchannel::fight::data::{RULES, Rules};

/// The candidates `make deadchannel-sweep` reads side by side. Edit this
/// list for the question at hand; the first row is always the live game.
const SWEEP: &[(&str, Rules)] = &[
    ("live", RULES),
    (
        "prices 200%",
        Rules {
            price_percent: 200,
            ..RULES
        },
    ),
    (
        "prices 250%",
        Rules {
            price_percent: 250,
            ..RULES
        },
    ),
    (
        "patch 100%",
        Rules {
            patch_percent: 100,
            ..RULES
        },
    ),
    (
        "the old game: prices 100%, patch 100%",
        Rules {
            price_percent: 100,
            patch_percent: 100,
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
