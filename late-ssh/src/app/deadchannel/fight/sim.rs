//! A runner played by a rule, through the real machine (`state.rs`), so
//! the balance is a number the tests can read: how many days the climb
//! from a fresh row to the first mark takes. GAME.md sets the target
//! (three weeks for a runner who never drops, four with a few drops) and
//! `sim_test.rs` pins it; every change to the curves, the glyph tiers,
//! the Old Signal, or the rules goes through that test. Pure: seeded
//! dice, a day counter instead of a clock, no I/O. Callable from a REPL
//! for a look at the whole ladder.
//!
//! Also the odds of one fight (`odds`, `Threat`): the same machine played
//! to the end a few hundred times from the sheet as it stands, so the
//! threat word the picker prints is the sim's number, not a guess from
//! the stat gap.

use std::collections::HashMap;

use chrono::{Days, NaiveDate};
use rand::{SeedableRng, rngs::StdRng};
use uuid::Uuid;

use super::data::MAX_LEVEL;
use super::state::{Applied, Command, MAX_TIER, Pick, Sheet, Slot};

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
    /// Step down to the glyph a level below whenever the glyph of your
    /// level reads this or worse at full signal; `None` never steps down.
    /// Never at the gate: the Old Signal is not a glyph to avoid.
    pub step_down_at: Option<Threat>,
    /// When the runner starts heeding the armorer and the picker.
    pub heeds: Heeds,
}

/// When a simulated runner starts playing by its rule for gear and for
/// the step down.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Heeds {
    /// From the first step in: the best tier the bits cover, per slot,
    /// before every fight, and the step down by the rule.
    Always,
    /// Not until the first dropped signal teaches it; from then on,
    /// always. The runner who walked past the armorer.
    AfterFirstDrop,
}

/// Plays it safe: runs early, patches every point. The three-week runner.
pub const CAREFUL: Player = Player {
    run_under: 0.4,
    patch_under: 1.0,
    step_down_at: None,
    heeds: Heeds::Always,
};

/// Pushes it: runs late, patches only when half gone. Drops a few times
/// on the way up. The four-week runner.
pub const RECKLESS: Player = Player {
    run_under: 0.15,
    patch_under: 0.5,
    step_down_at: None,
    heeds: Heeds::Always,
};

/// Walks past the armorer and the picker's warning until the street takes
/// everything, then plays carefully and steps down from a grim fight: the
/// runner who found the lock (bare hands at level 2, no bits, the hiss in
/// front of them).
pub const NEGLECTFUL: Player = Player {
    run_under: 0.4,
    patch_under: 1.0,
    step_down_at: Some(Threat::Grim),
    heeds: Heeds::AfterFirstDrop,
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
    /// The day of the first dropped signal.
    pub first_drop_on: Option<u32>,
    /// The first day after the first drop that a level was gained: how
    /// long the drop kept the runner stuck.
    pub recovered_on: Option<u32>,
    /// Steps into the static that met the Old Signal.
    pub boss_tries: u32,
}

/// Play `player` from a fresh row with `seed` for at most `max_days`.
/// Each day: settle the roll, then until the rations are gone or the
/// signal is down: buy the best affordable upgrade per slot, patch by the
/// rule, pick the fight by the rule, step in, and fight by the rule.
pub fn climb(player: Player, seed: u64, max_days: u32) -> Climb {
    let mut rng = StdRng::seed_from_u64(seed);
    let mut sheet = Sheet::fresh(Uuid::nil(), date(1));
    let mut report = Climb {
        level_on: [None; MAX_LEVEL as usize + 1],
        heard_on: None,
        marked_on: None,
        deaths: 0,
        first_drop_on: None,
        recovered_on: None,
        boss_tries: 0,
    };
    // The fair glyph's threat at full signal, by level, gear, and marks:
    // the step-down rule asks it before every fight, and it only moves
    // when one of those does.
    let mut threats: HashMap<(i32, i32, i32, i32), Threat> = HashMap::new();
    report.level_on[1] = Some(1);
    for day in 1..=max_days {
        sheet.settle(date(day));
        loop {
            let heeding = match player.heeds {
                Heeds::Always => true,
                Heeds::AfterFirstDrop => report.first_drop_on.is_some(),
            };
            if heeding {
                outfit(&mut sheet, &mut rng);
            }
            let patch_line = (f64::from(sheet.max_signal()) * player.patch_under).ceil() as i32;
            if sheet.signal < patch_line && sheet.bits >= sheet.patch_price() {
                sheet.apply(Command::Patch, &mut rng);
            }
            let gate = sheet.signal_hears();
            if gate && report.heard_on.is_none() {
                report.heard_on = Some(day);
            }
            let pick = match (heeding, gate, player.step_down_at) {
                (true, false, Some(line)) => {
                    let key = (
                        sheet.level,
                        sheet.weapon_tier,
                        sheet.armor_tier,
                        sheet.marks,
                    );
                    let threat = *threats.entry(key).or_insert_with(|| fresh_threat(&sheet));
                    match threat >= line && sheet.level > 1 {
                        true => Pick::Lower,
                        false => Pick::Fair,
                    }
                }
                (true, true, _) | (true, false, None) | (false, _, _) => Pick::Fair,
            };
            match sheet.apply(Command::Start { pick }, &mut rng).applied {
                Applied::Started { .. } => {}
                Applied::Refused(_) => break,
                other => panic!("start answered {other:?}"),
            }
            let boss = gate && pick == Pick::Fair;
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
                            if report.first_drop_on.is_some() {
                                report.recovered_on.get_or_insert(day);
                            }
                        }
                        break;
                    }
                    Applied::Lost { .. } => {
                        report.deaths += 1;
                        report.first_drop_on.get_or_insert(day);
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

/// The fair glyph's threat for `sheet`'s runner at full signal, the
/// rations topped up so the estimate is about the fight, not the day.
fn fresh_threat(sheet: &Sheet) -> Threat {
    let mut rested = sheet.clone();
    rested.signal = rested.max_signal();
    rested.rations_left = rested.rations_left.max(1);
    let odds = odds(&rested, Pick::Fair, ODDS_FIGHTS).expect("a rested runner can step in");
    Threat::of(odds)
}

/// The best tier the bits cover for each slot, bought; the armorer
/// refuses what is not an upgrade or not affordable, so this walks the
/// wall from the top and takes the first sale.
fn outfit(sheet: &mut Sheet, rng: &mut StdRng) {
    for slot in [Slot::Weapon, Slot::Armor] {
        for tier in (1..=MAX_TIER).rev() {
            if let Applied::Outfitted { .. } =
                sheet.apply(Command::Outfit { slot, tier }, rng).applied
            {
                break;
            }
        }
    }
}

/// How a fight reads before stepping in, worst last, from the chance of
/// winning it fought to the end.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Threat {
    /// Nine in ten or better.
    Easy,
    /// Six in ten or better.
    Even,
    /// Three in ten or better.
    Risky,
    /// Worse than three in ten.
    Grim,
}

impl Threat {
    pub fn of(odds: f64) -> Self {
        match odds {
            o if o >= 0.9 => Threat::Easy,
            o if o >= 0.6 => Threat::Even,
            o if o >= 0.3 => Threat::Risky,
            _ => Threat::Grim,
        }
    }

    pub fn word(self) -> &'static str {
        match self {
            Threat::Easy => "easy",
            Threat::Even => "even",
            Threat::Risky => "risky",
            Threat::Grim => "grim",
        }
    }
}

/// Fights the odds are read from: enough that the word holds still across
/// the threshold noise, few enough to read on a key press.
pub const ODDS_FIGHTS: u32 = 200;
/// Fixed dice for the odds: the same sheet always reads the same word.
const ODDS_SEED: u64 = 0x5eed_0dd5;
/// A fight that has not ended in this many exchanges counts as lost: a
/// run of glances can feed a glyph a long time.
const ODDS_ROUNDS: u32 = 200;

/// The chance of winning `pick`, fought to the end without running, from
/// `sheet` as it stands (its signal, its gear), over `fights` seeded
/// fights through the real machine. `None` when the step in would not
/// start a fight: the signal is down, the rations are spent, a fight is
/// already waiting, or there is nothing below the flicker.
pub fn odds(sheet: &Sheet, pick: Pick, fights: u32) -> Option<f64> {
    let mut rng = StdRng::seed_from_u64(ODDS_SEED);
    let mut wins = 0u32;
    for _ in 0..fights {
        let mut trial = sheet.clone();
        match trial.apply(Command::Start { pick }, &mut rng).applied {
            Applied::Started { .. } => {}
            Applied::Refused(_) | Applied::Resumed => return None,
            other => panic!("start answered {other:?}"),
        }
        for _ in 0..ODDS_ROUNDS {
            match trial.apply(Command::Attack, &mut rng).applied {
                Applied::Round => continue,
                Applied::Won { .. } | Applied::Slain { .. } => {
                    wins += 1;
                    break;
                }
                Applied::Lost { .. } => break,
                other => panic!("an attack answered {other:?}"),
            }
        }
    }
    Some(f64::from(wins) / f64::from(fights))
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
