//! A runner played by a rule, through the real machine (`state.rs`), so
//! the balance is a number the tests can read: how many days the climb
//! from a fresh row to the first mark takes. GAME.md sets the target
//! (three weeks for a runner who never drops, four with a few drops) and
//! `sim_test.rs` pins it; every change to the curves, the glyph tiers,
//! the Old Signal, or the rules goes through that test. Pure: seeded
//! dice, a day counter instead of a clock, no I/O. Callable from a REPL
//! for a look at the whole ladder.

use chrono::{Days, NaiveDate};
use rand::{SeedableRng, rngs::StdRng};
use uuid::Uuid;

use super::data::MAX_LEVEL;
use super::state::{Applied, Command, MAX_TIER, Sheet, Slot};

/// How the simulated runner plays. Every field is spelled out: a player
/// is a named rule, not a default.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Player {
    /// Run from a glyph once the signal is under this fraction of the
    /// max. Never from the Old Signal: that fight is always to the end.
    pub run_under: f64,
    /// Patch, when the bits cover it, once the signal is under this
    /// fraction of the max. `1.0` patches any missing point.
    pub patch_under: f64,
}

/// Plays it safe: runs early, patches every point. The three-week runner.
pub const CAREFUL: Player = Player {
    run_under: 0.4,
    patch_under: 1.0,
};

/// Pushes it: runs late, patches only when half gone. Drops a few times
/// on the way up. The four-week runner.
pub const RECKLESS: Player = Player {
    run_under: 0.15,
    patch_under: 0.5,
};

/// One climb from a fresh row, day 1 to the first mark or to `max_days`.
/// Days count from 1; `None` is "not within `max_days`".
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Climb {
    /// The day each level was first reached, indexed by level; entries 0
    /// and 1 are unused.
    pub level_on: [Option<u32>; MAX_LEVEL as usize + 1],
    /// The day the Old Signal first heard the runner (the gate open).
    pub heard_on: Option<u32>,
    /// The day the Old Signal was put down.
    pub marked_on: Option<u32>,
    /// Dropped signals along the way.
    pub deaths: u32,
    /// Steps into the static that met the Old Signal.
    pub boss_tries: u32,
}

/// Play `player` from a fresh row with `seed` for at most `max_days`.
/// Each day: settle the roll, then until the rations are gone or the
/// signal is down: buy the best affordable upgrade per slot, patch by the
/// rule, step in, and fight by the rule.
pub fn climb(player: Player, seed: u64, max_days: u32) -> Climb {
    let mut rng = StdRng::seed_from_u64(seed);
    let mut sheet = Sheet::fresh(Uuid::nil(), date(1));
    let mut report = Climb {
        level_on: [None; MAX_LEVEL as usize + 1],
        heard_on: None,
        marked_on: None,
        deaths: 0,
        boss_tries: 0,
    };
    report.level_on[1] = Some(1);
    for day in 1..=max_days {
        sheet.settle(date(day));
        loop {
            outfit(&mut sheet, &mut rng);
            let patch_line = (f64::from(sheet.max_signal()) * player.patch_under).ceil() as i32;
            if sheet.signal < patch_line && sheet.bits >= sheet.patch_price() {
                sheet.apply(Command::Patch, &mut rng);
            }
            let boss = sheet.signal_hears();
            if boss && report.heard_on.is_none() {
                report.heard_on = Some(day);
            }
            match sheet.apply(Command::Start, &mut rng).applied {
                Applied::Started => {}
                Applied::Refused(_) => break,
                other => panic!("start answered {other:?}"),
            }
            if boss {
                report.boss_tries += 1;
            }
            let run_line = (f64::from(sheet.max_signal()) * player.run_under).ceil() as i32;
            loop {
                let command = match !boss && sheet.signal < run_line {
                    true => Command::Run,
                    false => Command::Attack,
                };
                match sheet.apply(command, &mut rng).applied {
                    Applied::Round => {}
                    Applied::Escaped => break,
                    Applied::Won { leveled, .. } => {
                        if let Some(level) = leveled {
                            report.level_on[level as usize].get_or_insert(day);
                        }
                        break;
                    }
                    Applied::Lost { .. } => {
                        report.deaths += 1;
                        break;
                    }
                    Applied::Slain { .. } => {
                        report.marked_on = Some(day);
                        return report;
                    }
                    other => panic!("a fight command answered {other:?}"),
                }
            }
        }
    }
    report
}

/// The best tier the bits cover for each slot, bought; the armorer
/// refuses what is not an upgrade or not affordable, so this walks the
/// wall from the top and takes the first sale.
fn outfit(sheet: &mut Sheet, rng: &mut StdRng) {
    for slot in [Slot::Weapon, Slot::Armor] {
        for tier in (1..=MAX_TIER).rev() {
            if let Applied::Outfitted { .. } = sheet.apply(Command::Outfit { slot, tier }, rng).applied
            {
                break;
            }
        }
    }
}

/// The median of `days`, a climb that never got there counting as one
/// day past `max_days` so it drags the median out instead of vanishing.
pub fn median(days: impl Iterator<Item = Option<u32>>, max_days: u32) -> u32 {
    let mut sorted: Vec<u32> = days.map(|day| day.unwrap_or(max_days + 1)).collect();
    sorted.sort_unstable();
    sorted[sorted.len() / 2]
}

/// The ladder at a glance: the median day of every level, the gate, and
/// the mark across `climbs`. For the failing test's message and the REPL.
pub fn summary(climbs: &[Climb], max_days: u32) -> String {
    let mut lines = Vec::new();
    for level in 2..=MAX_LEVEL as usize {
        let day = median(climbs.iter().map(|climb| climb.level_on[level]), max_days);
        lines.push(format!("level {level:>2} on day {day}"));
    }
    lines.push(format!(
        "heard on day {}",
        median(climbs.iter().map(|climb| climb.heard_on), max_days)
    ));
    lines.push(format!(
        "marked on day {}",
        median(climbs.iter().map(|climb| climb.marked_on), max_days)
    ));
    lines.push(format!(
        "deaths {} (median), boss tries {} (median)",
        median(climbs.iter().map(|climb| Some(climb.deaths)), max_days),
        median(climbs.iter().map(|climb| Some(climb.boss_tries)), max_days)
    ));
    lines.join("\n")
}

fn date(day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 1, 1)
        .expect("a real date")
        .checked_add_days(Days::new(u64::from(day)))
        .expect("a day within the calendar")
}

#[cfg(test)]
#[path = "sim_test.rs"]
mod sim_test;
