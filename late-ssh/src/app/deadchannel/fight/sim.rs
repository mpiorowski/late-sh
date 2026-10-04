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
use super::data::{DRINK_CRYSTALS, RULES, Rules};
use super::state::{Applied, Command, Drink, MAX_TIER, Pick, Sheet, Slot};

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
    /// Step in against the bright glyph, on a step one waits behind,
    /// whenever it reads this or better at full signal; `None` never
    /// does.
    pub takes_bright: Option<Threat>,
    /// The glass bought before a step in whenever a crystal covers it and
    /// none is poured yet; `None` never drinks.
    pub drinks: Option<Drink>,
    /// Buys the blade shop's piece, the weaker slot first, whenever the
    /// crystals cover it.
    pub carts: bool,
    /// What the runner holds back from the armorer.
    pub purse: Purse,
}

/// How a simulated runner splits its bits between the armorer and patch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Purse {
    /// Every bit goes on the best piece it covers; patch gets what is
    /// left.
    SpendAll,
    /// Keeps the price of one full patch at its level back from the
    /// armorer: gear never leaves it unable to heal.
    KeepAPatch,
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
    takes_bright: None,
    drinks: None,
    carts: false,
    purse: Purse::KeepAPatch,
};

/// Pushes it: runs late, patches only when half gone. Drops a few times
/// on the way up. The four-week runner.
pub const RECKLESS: Player = Player {
    run_under: 0.15,
    patch_under: 0.5,
    step_down_at: None,
    heeds: Heeds::Always,
    takes_bright: None,
    drinks: None,
    carts: false,
    purse: Purse::SpendAll,
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
    takes_bright: None,
    drinks: None,
    carts: false,
    purse: Purse::KeepAPatch,
};

/// The careful runner who also plays the crystal pass: takes the bright
/// glyph when it reads even or better, buys the blade shop's pieces, and
/// drinks static on ice with what is left. What the crystals are worth,
/// measured against [`CAREFUL`].
pub const KEEN: Player = Player {
    run_under: 0.4,
    patch_under: 1.0,
    step_down_at: None,
    heeds: Heeds::Always,
    takes_bright: Some(Threat::Even),
    drinks: Some(Drink::StaticOnIce),
    carts: true,
    purse: Purse::KeepAPatch,
};

/// The players the balance is read from, by name: the report and the
/// sweep print every one.
pub const PLAYERS: [(&str, Player); 4] = [
    ("careful", CAREFUL),
    ("reckless", RECKLESS),
    ("neglectful", NEGLECTFUL),
    ("keen", KEEN),
];

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
    /// The weapon and armor tiers carried on leaving each level (at the
    /// kill that gained the next one, or at the mark for the last),
    /// indexed by level like `level_on`: the kit the level was fought in.
    pub kit_on: [Option<(i32, i32)>; MAX_LEVEL as usize + 1],
    /// Where the bits and the crystals went.
    pub ledger: Ledger,
}

/// One climb's money, summed: what the glyphs paid and where it went.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Ledger {
    /// Bits the glyphs paid, before the machine's garnish.
    pub earned: i64,
    /// Bits paid at the armorer, net of trade-ins.
    pub gear: i64,
    /// Bits paid at patch.
    pub patched: i64,
    /// Bits the street took with a dropped signal.
    pub dropped: i64,
    pub crystals_found: i32,
    pub crystals_spent: i32,
    /// Steps in against a bright glyph, and how many put it down.
    pub bright_tries: u32,
    pub bright_kills: u32,
}

/// The bench a batch of climbs is played at: the rules, and what each
/// pick reads at full signal under them, by level, gear, marks, and glass,
/// remembered. The step-down and bright rules ask before every fight, the
/// answer only moves when one of those does, and it is the same answer on
/// every seed (the odds run on fixed dice), so one bench serves a whole
/// batch. One bench per set of rules: the memo is only true under its own.
pub struct Bench {
    pub rules: Rules,
    threats: HashMap<(Pick, i32, i32, i32, i32, Option<Drink>), Threat>,
}

impl Bench {
    /// The live game.
    pub fn live() -> Self {
        Self::under(RULES)
    }

    /// A candidate set of rules.
    pub fn under(rules: Rules) -> Self {
        Self {
            rules,
            threats: HashMap::new(),
        }
    }

    fn threat(&mut self, pick: Pick, sheet: &Sheet) -> Threat {
        let key = (
            pick,
            sheet.level,
            sheet.weapon_tier,
            sheet.armor_tier,
            sheet.marks,
            sheet.drink,
        );
        let rules = self.rules;
        *self
            .threats
            .entry(key)
            .or_insert_with(|| fresh_threat(&rules, sheet, pick))
    }
}

/// Play `player` from a fresh row with `seed` for at most `max_days`.
/// Each day: settle the roll, then until the rations are gone or the
/// signal is down: buy the best affordable upgrade per slot, shop the
/// cart and the bar by the rule, patch by the rule, pick the fight by the
/// rule, step in, and fight by the rule.
pub fn climb(player: Player, seed: u64, max_days: u32, bench: &mut Bench) -> Climb {
    let rules = bench.rules;
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
        kit_on: [None; MAX_LEVEL as usize + 1],
        ledger: Ledger::default(),
    };
    report.level_on[1] = Some(1);
    for day in 1..=max_days {
        sheet.settle(date(day));
        loop {
            let heeding = match player.heeds {
                Heeds::Always => true,
                Heeds::AfterFirstDrop => report.first_drop_on.is_some(),
            };
            // Patch first: the heal is what the next fight is fought on,
            // and the armorer only sees what the purse rule leaves.
            let patch_line = (f64::from(sheet.max_signal()) * player.patch_under).ceil() as i32;
            if sheet.signal < patch_line
                && let Applied::Patched { paid, .. } =
                    sheet.apply_under(&rules, Command::Patch, &mut rng).applied
            {
                report.ledger.patched += paid;
            }
            if heeding {
                report.ledger.gear += outfit(&rules, player.purse, &mut sheet, &mut rng);
            }
            if player.carts {
                let slot = match sheet.weapon_tier <= sheet.armor_tier {
                    true => Slot::Weapon,
                    false => Slot::Armor,
                };
                if let Applied::Carted { crystals, .. } = sheet
                    .apply_under(&rules, Command::Cart { slot }, &mut rng)
                    .applied
                {
                    report.ledger.crystals_spent += crystals;
                }
            }
            if let Some(drink) = player.drinks
                && let Applied::Drank { .. } = sheet
                    .apply_under(&rules, Command::Drink { drink }, &mut rng)
                    .applied
            {
                report.ledger.crystals_spent += DRINK_CRYSTALS;
            }
            let gate = sheet.signal_hears();
            if gate && report.heard_on.is_none() {
                report.heard_on = Some(day);
            }
            let bright = match (
                player.takes_bright,
                sheet.bright_waits() && !sheet.is_down(),
            ) {
                (Some(line), true) => bench.threat(Pick::Bright, &sheet) <= line,
                (Some(_), false) | (None, _) => false,
            };
            let pick = match (bright, heeding, gate, player.step_down_at) {
                (true, _, _, _) => Pick::Bright,
                (false, true, false, Some(line)) => {
                    match bench.threat(Pick::Fair, &sheet) >= line && sheet.level > 1 {
                        true => Pick::Lower,
                        false => Pick::Fair,
                    }
                }
                (false, true, true, _) | (false, true, false, None) | (false, false, _, _) => {
                    Pick::Fair
                }
            };
            match sheet
                .apply_under(&rules, Command::Start { pick }, &mut rng)
                .applied
            {
                Applied::Started { .. } => {}
                Applied::Refused(_) => break,
                other => panic!("start answered {other:?}"),
            }
            let kit = (sheet.weapon_tier, sheet.armor_tier);
            let boss = gate && pick == Pick::Fair;
            if boss {
                report.boss_tries += 1;
            }
            if pick == Pick::Bright {
                report.ledger.bright_tries += 1;
            }
            let run_line = (f64::from(sheet.max_signal()) * player.run_under).ceil() as i32;
            loop {
                let command = match !boss && sheet.signal < run_line {
                    true => Command::Run,
                    false => Command::Attack,
                };
                match sheet.apply_under(&rules, command, &mut rng).applied {
                    Applied::Round => {}
                    Applied::Escaped => break,
                    Applied::Won {
                        leveled,
                        bits,
                        bright,
                        crystals,
                        ..
                    } => {
                        report.ledger.earned += bits;
                        report.ledger.crystals_found += crystals;
                        if bright {
                            report.ledger.bright_kills += 1;
                        }
                        if let Some(level) = leveled {
                            report.level_on[level as usize].get_or_insert(day);
                            report.kit_on[level as usize - 1].get_or_insert(kit);
                            if report.first_drop_on.is_some() {
                                report.recovered_on.get_or_insert(day);
                            }
                        }
                        break;
                    }
                    Applied::Lost { bits_lost } => {
                        report.ledger.dropped += bits_lost;
                        report.deaths += 1;
                        report.first_drop_on.get_or_insert(day);
                        break;
                    }
                    Applied::Slain { .. } => {
                        report.kit_on[MAX_LEVEL as usize] = Some(kit);
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

/// `pick`'s threat for `sheet`'s runner at full signal, a spent day's
/// rations topped up so the estimate is about the fight, not the day (a
/// day with a ration left keeps its step: the bright glyph waits behind
/// it).
fn fresh_threat(rules: &Rules, sheet: &Sheet, pick: Pick) -> Threat {
    let mut rested = sheet.clone();
    rested.signal = rested.max_signal();
    rested.rations_left = rested.rations_left.max(1);
    let odds = odds_under(rules, &rested, pick, ODDS_FIGHTS).expect("a rested runner can step in");
    Threat::of(odds)
}

/// The best tier the purse covers, for each slot, the weaker slot first,
/// bought: the wall walked from the top down to the first piece whose
/// price leaves the purse rule's reserve. Returns the bits paid.
fn outfit(rules: &Rules, purse: Purse, sheet: &mut Sheet, rng: &mut StdRng) -> i64 {
    let reserve = match purse {
        Purse::SpendAll => 0,
        Purse::KeepAPatch => {
            i64::from(sheet.max_signal()) * i64::from(sheet.level) * rules.patch_percent / 100
        }
    };
    let slots = match sheet.weapon_tier <= sheet.armor_tier {
        true => [Slot::Weapon, Slot::Armor],
        false => [Slot::Armor, Slot::Weapon],
    };
    let mut spent = 0;
    for slot in slots {
        let affordable = (sheet.tier_of(slot) + 1..=MAX_TIER)
            .rev()
            .find(|tier| sheet.outfit_price_under(rules, slot, *tier) + reserve <= sheet.bits);
        if let Some(tier) = affordable
            && let Applied::Outfitted { paid, .. } = sheet
                .apply_under(rules, Command::Outfit { slot, tier }, rng)
                .applied
        {
            spent += paid;
        }
    }
    spent
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
    odds_under(&RULES, sheet, pick, fights)
}

/// [`odds`] under a candidate set of rules.
pub fn odds_under(rules: &Rules, sheet: &Sheet, pick: Pick, fights: u32) -> Option<f64> {
    let mut rng = StdRng::seed_from_u64(ODDS_SEED);
    let mut wins = 0u32;
    for _ in 0..fights {
        let mut trial = sheet.clone();
        match trial
            .apply_under(rules, Command::Start { pick }, &mut rng)
            .applied
        {
            Applied::Started { .. } => {}
            Applied::Refused(_) | Applied::Resumed => return None,
            other => panic!("start answered {other:?}"),
        }
        for _ in 0..ODDS_ROUNDS {
            match trial.apply_under(rules, Command::Attack, &mut rng).applied {
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
