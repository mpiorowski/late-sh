//! The balance arena: a test-only harness that answers balance questions
//! by measuring instead of modelling (the shape of Lateania's arena,
//! `door/lateania/arena.rs`). Everything runs through the real machine
//! (`state.rs`) on seeded dice, under any set of `data::Rules`, so a
//! candidate number is tried by building a `Rules` with it, not by
//! editing a constant and recompiling. `fight/BALANCE.md` is the manual:
//! the targets, the knobs, and how to run a pass.
//!
//! The instruments, smallest to largest:
//!
//! - [`matchup`]: one runner, built by a [`Recipe`] (level, kit, glass,
//!   marks), against one pick, fought to the end a few hundred times: the
//!   odds, how long it takes, what is left of the signal, what the patch
//!   after it costs, what it pays.
//! - [`level_economy`]: one level as arithmetic, no dice: the kills it
//!   takes, the bits it pays, what the next pair of pieces costs.
//! - [`climbs`]: a `sim::Player` climbed from a fresh row over many seeds
//!   (`sim::climb`): the day and the kit of every level, and the ledger.
//! - [`Reading`]: everything the targets are stated in, for one set of
//!   rules, in one value, and [`Reading::misses`], the targets it fails.
//!   The contract asserts the live reading misses none; the sweep prints
//!   one per candidate, side by side, with what each misses.
//!
//! `arena_test.rs` holds the contract that runs with the suite, and two
//! `#[ignore]` prints: the report (`make deadchannel-arena`, every table,
//! written to `late-ssh/target/deadchannel-arena.md`) and the sweep
//! (`make deadchannel-sweep`, the candidates in `arena_test.rs::SWEEP`).

use chrono::NaiveDate;
use rand::{SeedableRng, rngs::StdRng};
use uuid::Uuid;

use super::data::{self, MAX_LEVEL, RATIONS_PER_DAY, Rules, bright_steps};
use super::sim::{self, Bench, Climb, Player};
use super::state::{Applied, Command, Drink, MAX_TIER, Pick, Sheet};

/// What the runner carries into a matchup, both slots alike.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kit {
    /// Bare hands and street clothes: the runner who walked past the
    /// armorer.
    Bare,
    /// This many tiers over the runner's level (negative: behind it),
    /// clamped to the wall.
    Lead(i32),
}

impl Kit {
    fn tier(self, level: i32) -> i32 {
        match self {
            Kit::Bare => 0,
            Kit::Lead(lead) => (level + lead).clamp(0, MAX_TIER),
        }
    }
}

/// Everything that moves a matchup. The arena pins the rest: a full
/// signal, a fixed day, fixed dice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Recipe {
    level: i32,
    kit: Kit,
    drink: Option<Drink>,
    marks: i32,
}

/// The arena's one day: every matchup is fought on it, so a recipe always
/// reads the same.
fn arena_day() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 1, 1).expect("a real date")
}

impl Recipe {
    /// The recipe as a sheet at full signal, standing on a step of the
    /// arena's day a bright glyph waits behind, so every pick is open.
    fn sheet(self) -> Sheet {
        let mut sheet = Sheet::fresh(Uuid::nil(), arena_day());
        sheet.level = self.level;
        sheet.peak_level = self.level;
        sheet.marks = self.marks;
        sheet.weapon_tier = self.kit.tier(self.level);
        sheet.armor_tier = self.kit.tier(self.level);
        sheet.drink = self.drink;
        sheet.signal = sheet.max_signal();
        sheet.rations_left = RATIONS_PER_DAY - (bright_steps(sheet.day)[0] - 1);
        sheet
    }
}

/// One recipe against one pick, over many fights to the end.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Matchup {
    /// The share of fights won.
    odds: f64,
    /// Exchanges per fight, won or lost.
    rounds: f64,
    /// The signal left after a win, as a share of the max.
    signal_left: f64,
    /// What patch charges after a win, in bits.
    patch: f64,
    /// What a win pays, in bits.
    pays: i64,
}

/// Fights per matchup: enough that a contract's band holds still.
const MATCHUP_FIGHTS: u32 = 400;
const MATCHUP_SEED: u64 = 0xa4e7a;
/// A fight still going after this many exchanges counts as lost.
const MATCHUP_ROUNDS: u32 = 400;

/// `recipe` against `pick` under `rules`, fought to the end
/// [`MATCHUP_FIGHTS`] times. `None` when the step in starts nothing
/// (nothing below the flicker).
fn matchup(rules: &Rules, recipe: Recipe, pick: Pick) -> Option<Matchup> {
    let mut rng = StdRng::seed_from_u64(MATCHUP_SEED);
    let sheet = recipe.sheet();
    let (mut wins, mut rounds, mut left, mut patch, mut pays) = (0u32, 0u64, 0.0f64, 0i64, 0i64);
    for _ in 0..MATCHUP_FIGHTS {
        let mut trial = sheet.clone();
        match trial
            .apply_under(rules, Command::Start { pick }, &mut rng)
            .applied
        {
            Applied::Started { .. } => {}
            Applied::Refused(_) => return None,
            other => panic!("start answered {other:?}"),
        }
        for _ in 0..MATCHUP_ROUNDS {
            rounds += 1;
            match trial.apply_under(rules, Command::Attack, &mut rng).applied {
                Applied::Round => continue,
                Applied::Won { bits, .. } => {
                    wins += 1;
                    pays = bits;
                    // Against the level the fight was won at: a kill that
                    // levels the runner must not read as a wound.
                    let kept = trial.signal.min(sheet.max_signal());
                    left += f64::from(kept) / f64::from(sheet.max_signal());
                    let mut wounded = sheet.clone();
                    wounded.signal = kept;
                    patch += wounded.patch_price_under(rules);
                    break;
                }
                Applied::Slain { .. } => {
                    wins += 1;
                    break;
                }
                Applied::Lost { .. } => break,
                other => panic!("an attack answered {other:?}"),
            }
        }
    }
    let fights = f64::from(MATCHUP_FIGHTS);
    let won = f64::from(wins.max(1));
    Some(Matchup {
        odds: f64::from(wins) / fights,
        rounds: rounds as f64 / fights,
        signal_left: left / won,
        patch: patch as f64 / won,
        pays,
    })
}

/// The Old Signal from a kit level with the runner at the top, by marks
/// and glass.
fn old_signal_odds(rules: &Rules, marks: i32, drink: Option<Drink>) -> f64 {
    let mut sheet = Recipe {
        level: MAX_LEVEL,
        kit: Kit::Lead(0),
        drink,
        marks,
    }
    .sheet();
    sheet.exp = data::exp_to_seek(marks);
    sim::odds_under(rules, &sheet, Pick::Fair, MATCHUP_FIGHTS).expect("the gate is open")
}

/// One level as arithmetic: what it takes and pays with no dice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct LevelEconomy {
    /// Kills of the glyph of the level to leave it.
    kills: i64,
    /// What those kills pay, in bits.
    pays: i64,
    /// The price of the next tier in both slots, handing back this
    /// level's: what keeping the kit level with the runner costs.
    next_pair: i64,
}

fn level_economy(rules: &Rules, level: i32) -> LevelEconomy {
    let (_, _, foe) = rules.foe(level);
    let from = match level {
        1 => 0,
        level => data::exp_to_advance(level - 1, 0).expect("a level under the top"),
    };
    let to = data::exp_to_advance(level, 0).unwrap_or_else(|| data::exp_to_seek(0));
    let kills = (to - from + foe.exp - 1) / foe.exp;
    let next = (level + 1).min(MAX_TIER);
    LevelEconomy {
        kills,
        pays: kills * foe.bits,
        next_pair: 2 * (rules.price(next) - rules.trade_in(next - 1)),
    }
}

/// `player` climbed from a fresh row on `seeds` seeds under `rules`.
fn climbs(player: Player, rules: Rules, seeds: u64, max_days: u32) -> Vec<Climb> {
    let mut bench = Bench::under(rules);
    (0..seeds)
        .map(|seed| sim::climb(player, seed, max_days, &mut bench))
        .collect()
}

/// The median of one number across climbs.
fn median_of(climbs: &[Climb], read: impl Fn(&Climb) -> i64) -> i64 {
    let mut values: Vec<i64> = climbs.iter().map(read).collect();
    values.sort_unstable();
    values[values.len() / 2]
}

/// How far the kit `level` was fought in ran ahead of it, in half tiers
/// (weapon plus armor, less twice the level), at the median climb; `None`
/// when most climbs never left that level.
fn kit_lead(climbs: &[Climb], level: i32) -> Option<i64> {
    let lead = median_of(climbs, |climb| match climb.kit_on[level as usize] {
        Some((weapon, armor)) => i64::from(weapon + armor - 2 * level),
        None => i64::MIN,
    });
    match lead {
        i64::MIN => None,
        lead => Some(lead),
    }
}

/// Everything the targets are stated in (`fight/BALANCE.md`), for one set
/// of rules. Days are medians over the seeds; shares are of the careful
/// runner's bits earned.
#[derive(Clone, Debug, PartialEq)]
struct Reading {
    /// The day of the first mark, by player.
    careful_day: u32,
    reckless_day: u32,
    keen_day: u32,
    reckless_drops: i64,
    /// The careful runner's kit against the level it was fought at, in
    /// half tiers, at its lowest and highest over levels 3 to 15.
    lead_low: i64,
    lead_high: i64,
    /// The first level the careful runner fought in the whole top kit.
    top_kit_at: Option<i32>,
    /// Where the careful runner's bits went, as shares of what it earned.
    gear_share: f64,
    patch_share: f64,
    dropped_share: f64,
    /// What it earned and never spent or lost.
    idle_share: f64,
    /// The fair fight from a kit level with the runner, at its hardest
    /// over levels 2 to 15.
    fair_low: f64,
    /// The bright glyph from a kit level with the runner, lowest and
    /// highest over levels 4 to 15.
    bright_low: f64,
    bright_high: f64,
    /// The Old Signal at no marks, dry and with static on ice.
    boss_dry: f64,
    boss_glass: f64,
    /// Crystals the keen runner found a day, and how many it spent.
    crystals_a_day: f64,
    keen_crystals_spent: i64,
}

/// Seeds per player in a reading.
const SEEDS: u64 = 80;
const MAX_DAYS: u32 = 120;

impl Reading {
    fn take(rules: Rules) -> Self {
        let careful = climbs(sim::CAREFUL, rules, SEEDS, MAX_DAYS);
        let reckless = climbs(sim::RECKLESS, rules, SEEDS, MAX_DAYS);
        let keen = climbs(sim::KEEN, rules, SEEDS, MAX_DAYS);
        let marked =
            |climbs: &[Climb]| sim::median(climbs.iter().map(|climb| climb.marked_on), MAX_DAYS);
        let leads: Vec<i64> = (3..=MAX_LEVEL)
            .filter_map(|level| kit_lead(&careful, level))
            .collect();
        let earned = median_of(&careful, |climb| climb.ledger.earned).max(1) as f64;
        let share = |read: fn(&Climb) -> i64| median_of(&careful, read) as f64 / earned;
        let gear_share = share(|climb| climb.ledger.gear);
        let patch_share = share(|climb| climb.ledger.patched);
        let dropped_share = share(|climb| climb.ledger.dropped);
        let level_kit = |level, pick| {
            matchup(
                &rules,
                Recipe {
                    level,
                    kit: Kit::Lead(0),
                    drink: None,
                    marks: 0,
                },
                pick,
            )
            .expect("the step in starts a fight")
            .odds
        };
        let bright: Vec<f64> = (4..=MAX_LEVEL)
            .map(|level| level_kit(level, Pick::Bright))
            .collect();
        let keen_day = marked(&keen);
        Self {
            careful_day: marked(&careful),
            reckless_day: marked(&reckless),
            keen_day,
            reckless_drops: median_of(&reckless, |climb| i64::from(climb.deaths)),
            lead_low: leads.iter().copied().min().unwrap_or(i64::MIN),
            lead_high: leads.iter().copied().max().unwrap_or(i64::MAX),
            top_kit_at: (1..=MAX_LEVEL).find(|level| {
                kit_lead(&careful, *level) == Some(i64::from(2 * (MAX_TIER - level)))
            }),
            gear_share,
            patch_share,
            dropped_share,
            idle_share: 1.0 - gear_share - patch_share - dropped_share,
            fair_low: (2..=MAX_LEVEL)
                .map(|level| level_kit(level, Pick::Fair))
                .fold(1.0, f64::min),
            bright_low: bright.iter().copied().fold(1.0, f64::min),
            bright_high: bright.iter().copied().fold(0.0, f64::max),
            boss_dry: old_signal_odds(&rules, 0, None),
            boss_glass: old_signal_odds(&rules, 0, Some(Drink::StaticOnIce)),
            crystals_a_day: median_of(&keen, |climb| i64::from(climb.ledger.crystals_found)) as f64
                / f64::from(keen_day.max(1)),
            keen_crystals_spent: median_of(&keen, |climb| i64::from(climb.ledger.crystals_spent)),
        }
    }

    /// The targets this reading misses, one line each: empty is a balanced
    /// game. The bands are the ones `fight/BALANCE.md` states and argues;
    /// change a band there and here together.
    fn misses(&self) -> Vec<String> {
        let pct = |share: f64| (share * 100.0).round() as i64;
        let checks = [
            (
                (18..=24).contains(&self.careful_day),
                format!("careful marks on day {}, want 18 to 24", self.careful_day),
            ),
            (
                (25..=31).contains(&self.reckless_day),
                format!("reckless marks on day {}, want 25 to 31", self.reckless_day),
            ),
            (
                (2..=8).contains(&self.reckless_drops),
                format!("reckless drops {} times, want 2 to 8", self.reckless_drops),
            ),
            (
                self.keen_day <= self.careful_day && self.keen_day + 7 >= self.careful_day,
                format!(
                    "keen marks on day {} against careful's {}, want 0 to 7 days ahead",
                    self.keen_day, self.careful_day
                ),
            ),
            (
                self.lead_low >= -1 && self.lead_high <= 2,
                format!(
                    "kit lead runs {:+.1} to {:+.1} tiers, want -0.5 to +1.0",
                    self.lead_low as f64 / 2.0,
                    self.lead_high as f64 / 2.0
                ),
            ),
            (
                match self.top_kit_at {
                    Some(level) => level >= 14,
                    None => false,
                },
                match self.top_kit_at {
                    Some(level) => format!("top kit at level {level}, want 14 or 15"),
                    None => "the top kit is never worn, want it at 14 or 15".to_string(),
                },
            ),
            (
                self.gear_share >= 0.45,
                format!(
                    "gear takes {}% of the bits, want 45% or more",
                    pct(self.gear_share)
                ),
            ),
            (
                (0.10..=0.35).contains(&self.patch_share),
                format!(
                    "patch takes {}% of the bits, want 10 to 35%",
                    pct(self.patch_share)
                ),
            ),
            (
                self.dropped_share <= 0.15,
                format!(
                    "careful loses {}% of the bits to drops, want 15% or less",
                    pct(self.dropped_share)
                ),
            ),
            (
                self.idle_share <= 0.25,
                format!(
                    "{}% of the bits sit idle, want 25% or less",
                    pct(self.idle_share)
                ),
            ),
            (
                self.fair_low >= 0.85,
                format!(
                    "the fair fight dips to {}%, want 85% or more",
                    pct(self.fair_low)
                ),
            ),
            (
                self.bright_low >= 0.35 && self.bright_high <= 0.70,
                format!(
                    "the bright glyph runs {} to {}%, want 35 to 70%",
                    pct(self.bright_low),
                    pct(self.bright_high)
                ),
            ),
            (
                (0.30..=0.50).contains(&self.boss_dry),
                format!(
                    "the old signal dry is {}%, want 30 to 50%",
                    pct(self.boss_dry)
                ),
            ),
            (
                self.boss_glass - self.boss_dry >= 0.10 && self.boss_glass <= 0.75,
                format!(
                    "a glass moves the old signal {}% to {}%, want 10 points or more and 75% at most",
                    pct(self.boss_dry),
                    pct(self.boss_glass)
                ),
            ),
            (
                (0.8..=2.0).contains(&self.crystals_a_day),
                format!("{:.1} crystals a day, want 0.8 to 2.0", self.crystals_a_day),
            ),
        ];
        checks
            .into_iter()
            .filter(|(holds, _)| !holds)
            .map(|(_, miss)| miss)
            .collect()
    }

    const HEADER: &str = "| rules | careful | reckless (drops) | keen | kit lead | top kit at | gear | patch | dropped | idle | fair low | bright | boss dry / glass | crystals a day | misses |\n|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|\n";

    /// The reading as one row of the sweep's table.
    fn row(&self, name: &str) -> String {
        let half = |lead: i64| format!("{:+.1}", lead as f64 / 2.0);
        format!(
            "| {name} | {} | {} ({}) | {} | {} to {} | {} | {:.0}% | {:.0}% | {:.0}% | {:.0}% | {:.0}% | {:.0} to {:.0}% | {:.0}% / {:.0}% | {:.1} | {} |\n",
            self.careful_day,
            self.reckless_day,
            self.reckless_drops,
            self.keen_day,
            half(self.lead_low),
            half(self.lead_high),
            match self.top_kit_at {
                Some(level) => format!("lv {level}"),
                None => "never".to_string(),
            },
            self.gear_share * 100.0,
            self.patch_share * 100.0,
            self.dropped_share * 100.0,
            self.idle_share * 100.0,
            self.fair_low * 100.0,
            self.bright_low * 100.0,
            self.bright_high * 100.0,
            self.boss_dry * 100.0,
            self.boss_glass * 100.0,
            self.crystals_a_day,
            self.misses().len(),
        )
    }
}

fn pct(matchup: Option<Matchup>) -> String {
    match matchup {
        Some(matchup) => format!("{:.0}%", matchup.odds * 100.0),
        None => "-".to_string(),
    }
}

/// What gear is worth: the fair fight at every level from every kit, bare
/// hands to four tiers over.
fn gear_table(rules: &Rules) -> String {
    const LEADS: [i32; 8] = [-4, -3, -2, -1, 0, 1, 2, 4];
    let mut out = String::from("| lv | bare |");
    for lead in LEADS {
        out.push_str(&format!(" {lead:+} |"));
    }
    out.push_str(" rounds | signal left |\n|---|---|");
    out.push_str(&"---|".repeat(LEADS.len() + 2));
    out.push('\n');
    for level in 1..=MAX_LEVEL {
        let recipe = |kit| Recipe {
            level,
            kit,
            drink: None,
            marks: 0,
        };
        out.push_str(&format!(
            "| {level} | {} |",
            pct(matchup(rules, recipe(Kit::Bare), Pick::Fair))
        ));
        for lead in LEADS {
            out.push_str(&format!(
                " {} |",
                pct(matchup(rules, recipe(Kit::Lead(lead)), Pick::Fair))
            ));
        }
        let level_kit =
            matchup(rules, recipe(Kit::Lead(0)), Pick::Fair).expect("the fair fight starts");
        out.push_str(&format!(
            " {:.1} | {:.0}% |\n",
            level_kit.rounds,
            level_kit.signal_left * 100.0
        ));
    }
    out
}

/// The bright glyph and the step down at every level: from a kit level
/// with the runner, a tier either side, and with each glass.
fn bright_table(rules: &Rules) -> String {
    let mut out = String::from(
        "| lv | lower | bright -1 | bright 0 | bright +1 | +ice | +neat | +pattern | bright pays |\n|---|---|---|---|---|---|---|---|---|\n",
    );
    for level in 1..=MAX_LEVEL {
        let recipe = |lead, drink| Recipe {
            level,
            kit: Kit::Lead(lead),
            drink,
            marks: 0,
        };
        let bright = |lead, drink| matchup(rules, recipe(lead, drink), Pick::Bright);
        out.push_str(&format!(
            "| {level} | {} | {} | {} | {} | {} | {} | {} | {} |\n",
            pct(matchup(rules, recipe(0, None), Pick::Lower)),
            pct(bright(-1, None)),
            pct(bright(0, None)),
            pct(bright(1, None)),
            pct(bright(0, Some(Drink::StaticOnIce))),
            pct(bright(0, Some(Drink::DeadAirNeat))),
            pct(bright(0, Some(Drink::TestPattern))),
            rules.bright_foe(level).2.bits,
        ));
    }
    out
}

/// The Old Signal from a kit level with the runner, by marks and by glass.
fn old_signal_table(rules: &Rules) -> String {
    let mut out = String::from(
        "| marks | no glass | static on ice | dead air, neat | test pattern |\n|---|---|---|---|---|\n",
    );
    for marks in 0..=5 {
        let odds = |drink| old_signal_odds(rules, marks, drink) * 100.0;
        out.push_str(&format!(
            "| {marks} | {:.0}% | {:.0}% | {:.0}% | {:.0}% |\n",
            odds(None),
            odds(Some(Drink::StaticOnIce)),
            odds(Some(Drink::DeadAirNeat)),
            odds(Some(Drink::TestPattern)),
        ));
    }
    out
}

/// Every level as arithmetic, beside what the fight costs in patch: where
/// a level's bits have to go.
fn economy_table(rules: &Rules) -> String {
    let mut out = String::from(
        "| lv | kills | level pays | next pair | pair / pay | patch a kill | patch / pay | wall price |\n|---|---|---|---|---|---|---|---|\n",
    );
    for level in 1..=MAX_LEVEL {
        let economy = level_economy(rules, level);
        let fight = matchup(
            rules,
            Recipe {
                level,
                kit: Kit::Lead(0),
                drink: None,
                marks: 0,
            },
            Pick::Fair,
        )
        .expect("the fair fight starts");
        out.push_str(&format!(
            "| {level} | {} | {} | {} | {:.0}% | {:.0} | {:.0}% | {} |\n",
            economy.kills,
            economy.pays,
            economy.next_pair,
            economy.next_pair as f64 / economy.pays as f64 * 100.0,
            fight.patch,
            fight.patch / fight.pays.max(1) as f64 * 100.0,
            rules.price(level),
        ));
    }
    out
}

/// One player's climb: the median day of every level and the kit it was
/// fought in, then the ledger.
fn climb_table(name: &str, climbs: &[Climb], max_days: u32) -> String {
    let mut out = format!(
        "### {name}\n\n| level | reached on day | fought in (weapon / armor) |\n|---|---|---|\n"
    );
    for level in 1..=MAX_LEVEL as usize {
        let day = sim::median(climbs.iter().map(|climb| climb.level_on[level]), max_days);
        let kit = |slot: fn((i32, i32)) -> i32| match median_of(climbs, |climb| {
            climb.kit_on[level].map(slot).map(i64::from).unwrap_or(-1)
        }) {
            -1 => "-".to_string(),
            tier => tier.to_string(),
        };
        out.push_str(&format!(
            "| {level} | {} | {} / {} |\n",
            match day > max_days {
                true => "never".to_string(),
                false => day.to_string(),
            },
            kit(|kit| kit.0),
            kit(|kit| kit.1)
        ));
    }
    let days = |read: fn(&Climb) -> Option<u32>| sim::median(climbs.iter().map(read), max_days);
    out.push_str(&format!(
        "\nheard on day {}, marked on day {}. drops {} ({}), boss tries {}.\n",
        days(|climb| climb.heard_on),
        days(|climb| climb.marked_on),
        median_of(climbs, |climb| i64::from(climb.deaths)),
        match days(|climb| climb.first_drop_on) {
            day if day > max_days => "most climbs never drop".to_string(),
            day => format!("the first on day {day}"),
        },
        median_of(climbs, |climb| i64::from(climb.boss_tries)),
    ));
    out.push_str(&format!(
        "bits: earned {}, gear {}, patch {}, dropped {}. crystals: found {}, spent {}. bright: {} tries, {} kills.\n\n",
        median_of(climbs, |climb| climb.ledger.earned),
        median_of(climbs, |climb| climb.ledger.gear),
        median_of(climbs, |climb| climb.ledger.patched),
        median_of(climbs, |climb| climb.ledger.dropped),
        median_of(climbs, |climb| i64::from(climb.ledger.crystals_found)),
        median_of(climbs, |climb| i64::from(climb.ledger.crystals_spent)),
        median_of(climbs, |climb| i64::from(climb.ledger.bright_tries)),
        median_of(climbs, |climb| i64::from(climb.ledger.bright_kills)),
    ));
    out
}

/// Every table for one set of rules: the whole report.
fn report(rules: Rules) -> String {
    let mut out = String::from("# deadchannel arena\n\n## The reading\n\n");
    out.push_str(Reading::HEADER);
    let reading = Reading::take(rules);
    out.push_str(&reading.row("live"));
    for miss in reading.misses() {
        out.push_str(&format!("\nMissed: {miss}\n"));
    }
    out.push_str("\n## What gear is worth: the fair fight by kit lead\n\n");
    out.push_str(&gear_table(&rules));
    out.push_str("\n## Where a level's bits go\n\n");
    out.push_str(&economy_table(&rules));
    out.push_str("\n## The bright glyph and the step down\n\n");
    out.push_str(&bright_table(&rules));
    out.push_str("\n## The Old Signal\n\n");
    out.push_str(&old_signal_table(&rules));
    out.push_str("\n## Climbs\n\n");
    for (name, player) in sim::PLAYERS {
        out.push_str(&climb_table(
            name,
            &climbs(player, rules, SEEDS, MAX_DAYS),
            MAX_DAYS,
        ));
    }
    out
}

/// One reading per candidate, side by side.
fn sweep(candidates: &[(&str, Rules)]) -> String {
    let mut out = String::from(Reading::HEADER);
    let mut missed = String::new();
    for (name, rules) in candidates {
        let reading = Reading::take(*rules);
        out.push_str(&reading.row(name));
        for miss in reading.misses() {
            missed.push_str(&format!("- {name}: {miss}\n"));
        }
    }
    if !missed.is_empty() {
        out.push_str("\nMissed targets:\n\n");
        out.push_str(&missed);
    }
    out
}

/// Print `out` and write it under `late-ssh/target/`.
fn publish(file: &str, out: &str) {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join(file);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).expect("report dir");
    }
    std::fs::write(&path, out).expect("write report");
    eprintln!("{out}\n[arena] written to {}", path.display());
}

#[cfg(test)]
#[path = "arena_test.rs"]
mod arena_test;
