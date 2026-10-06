use super::{Reading, publish, report, runs, sweep};
use crate::app::deadchannel::fight::data::{FoeTier, RULES, Rules};

/// The candidates `make deadchannel-sweep` reads side by side. Edit this
/// list for the question at hand; the first row is always the live game.
const SWEEP: &[(&str, Rules)] = &[
    ("live", RULES),
    (
        "boss290",
        Rules {
            old_signal: boss(290, 39),
            ..RULES
        },
    ),
    (
        "boss320",
        Rules {
            old_signal: boss(320, 39),
            ..RULES
        },
    ),
    (
        "boss320 jab75",
        Rules {
            old_signal: boss(320, 39),
            jab_percent: 75,
            ..RULES
        },
    ),
    (
        "boss320 jab75 burn2",
        Rules {
            old_signal: boss(320, 39),
            jab_percent: 75,
            burn_energy: 2,
            ..RULES
        },
    ),
    (
        "boss350 jab75",
        Rules {
            old_signal: boss(350, 39),
            jab_percent: 75,
            ..RULES
        },
    ),
    (
        "boss320 jab75 hits185",
        Rules {
            old_signal: boss(320, 39),
            jab_percent: 75,
            hit_percent: 185,
            ..RULES
        },
    ),
    (
        "boss320 att44 jab75",
        Rules {
            old_signal: boss(320, 44),
            jab_percent: 75,
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
