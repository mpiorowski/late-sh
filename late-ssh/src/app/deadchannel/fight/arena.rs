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
//!   marks), against one pick, the cards played by one `Hand` (the `Auto`
//!   key, or a runner reading them), fought to the end a few hundred
//!   times: the odds, how many turns it takes, what is left of the signal,
//!   the static it leaves in the deck, what the patch after it costs, what
//!   it pays.
//! - [`level_economy`]: one level as arithmetic, no dice: the kills it
//!   takes, the bits it pays, what the next pair of pieces costs.
//! - [`climbs`]: a `sim::Player` climbed from a fresh row over many seeds
//!   (`sim::climb`), a road a day: the day and the kit of every level, the
//!   level at the end of every day, and the ledger.
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

use super::cards::{Card, DRAFTS};
use super::data::{self, Intent, MAX_LEVEL, Rules};
use super::road::{FIGHT_STEPS, LANES, Node, STEPS};
use super::sim::{self, Bench, Build, Climb, Day, Event, Hand, Journal, Player, Snapshot};
use super::state::{Applied, Drink, MAX_TIER, Pick, Sheet, Slot, gear_name};

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
#[derive(Clone, Debug, PartialEq, Eq)]
struct Recipe {
    level: i32,
    kit: Kit,
    drink: Option<Drink>,
    marks: i32,
    /// The cards drafted, in the order of the drafts.
    cards: Vec<Card>,
}

/// The cards a runner on `build` holds at `level`: a pick for every
/// draft the level has reached.
fn drafted(build: &Build, level: i32) -> Vec<Card> {
    DRAFTS
        .iter()
        .zip(build)
        .filter(|(draft, _)| level >= draft.level)
        .map(|(_, card)| *card)
        .collect()
}

/// A build as the tables name it.
fn build_name(cards: &[Card]) -> String {
    match cards.is_empty() {
        true => "the starting deck".to_string(),
        false => cards
            .iter()
            .map(|card| card.name())
            .collect::<Vec<_>>()
            .join(" · "),
    }
}

/// `work` over `items`, a thread each: the readings are many climbs that
/// share nothing, and a pass should not wait on them one at a time.
fn fan<I: Send, T: Send>(items: Vec<I>, work: impl Fn(I) -> T + Sync) -> Vec<T> {
    std::thread::scope(|scope| {
        let work = &work;
        let handles: Vec<_> = items
            .into_iter()
            .map(|item| scope.spawn(move || work(item)))
            .collect();
        handles
            .into_iter()
            .map(|handle| handle.join().expect("an arena thread panicked"))
            .collect()
    })
}

/// The arena's one day: every matchup is fought on it, so a recipe always
/// reads the same.
fn arena_day() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 1, 1).expect("a real date")
}

impl Recipe {
    /// A runner of `level` on `build`, the kit level with it, dry, no
    /// marks: the runner most tables are read from.
    fn level_kit(level: i32, build: &Build) -> Self {
        Self {
            level,
            kit: Kit::Lead(0),
            drink: None,
            marks: 0,
            cards: drafted(build, level),
        }
    }

    /// The recipe as a sheet at full signal with a clean deck and the
    /// day's road ahead of it.
    fn sheet(&self) -> Sheet {
        let mut sheet = Sheet::fresh(Uuid::nil(), arena_day());
        sheet.level = self.level;
        sheet.peak_level = self.level;
        sheet.marks = self.marks;
        sheet.weapon_tier = self.kit.tier(self.level);
        sheet.armor_tier = self.kit.tier(self.level);
        sheet.drink = self.drink;
        sheet.cards = self.cards.clone();
        sheet.signal = sheet.max_signal();
        sheet
    }
}

/// One recipe against one pick, over many fights to the end.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Matchup {
    /// The share of fights won.
    odds: f64,
    /// Turns of cards per fight, won or lost.
    turns: f64,
    /// The signal left after a win, as a share of the max.
    signal_left: f64,
    /// Static cards left in the deck after a win.
    static_left: f64,
    /// What patch charges after a win, in bits.
    patch: f64,
    /// What a win pays, in bits.
    pays: i64,
}

/// Fights per matchup: enough that a contract's band holds still.
const MATCHUP_FIGHTS: u32 = 400;
const MATCHUP_SEED: u64 = 0xa4e7a;
/// A fight still going after this many turns counts as lost.
const MATCHUP_TURNS: u32 = 80;

/// `recipe` against `pick` under `rules`, the cards played by `hand`,
/// fought to the end [`MATCHUP_FIGHTS`] times. `None` when no fight
/// starts (nothing below the flicker).
fn matchup(rules: &Rules, recipe: &Recipe, pick: Pick, hand: Hand) -> Option<Matchup> {
    let mut rng = StdRng::seed_from_u64(MATCHUP_SEED);
    let sheet = recipe.sheet();
    let (mut wins, mut turns, mut left, mut statics, mut patch, mut pays) =
        (0u32, 0u64, 0.0f64, 0u64, 0i64, 0i64);
    for _ in 0..MATCHUP_FIGHTS {
        let mut trial = sheet.clone();
        match trial.engage(rules, pick, &mut rng).applied {
            Applied::Started { .. } => {}
            Applied::Refused(_) => return None,
            other => panic!("a fight started with {other:?}"),
        }
        for _ in 0..MATCHUP_TURNS {
            turns += 1;
            match sim::play_turn(&mut trial, rules, hand, &mut rng).applied {
                Applied::Round => continue,
                Applied::Won { bits, .. } => {
                    wins += 1;
                    pays = bits;
                    // Against the level the fight was won at: a kill that
                    // levels the runner must not read as a wound.
                    let kept = trial.signal.min(sheet.max_signal());
                    left += f64::from(kept) / f64::from(sheet.max_signal());
                    statics += u64::from(trial.road.static_cards);
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
                other => panic!("a turn answered {other:?}"),
            }
        }
    }
    let fights = f64::from(MATCHUP_FIGHTS);
    let won = f64::from(wins.max(1));
    Some(Matchup {
        odds: f64::from(wins) / fights,
        turns: turns as f64 / fights,
        signal_left: left / won,
        static_left: statics as f64 / won,
        patch: patch as f64 / won,
        pays,
    })
}

/// The Old Signal from a kit level with the runner at the top, by marks,
/// glass, and build, the cards played by `hand`.
fn old_signal(
    rules: &Rules,
    marks: i32,
    drink: Option<Drink>,
    hand: Hand,
    build: &Build,
) -> Matchup {
    let mut recipe = Recipe {
        level: MAX_LEVEL,
        kit: Kit::Lead(0),
        drink,
        marks,
        cards: build.to_vec(),
    }
    .sheet();
    recipe.exp = data::exp_to_seek(marks);
    let mut rng = StdRng::seed_from_u64(MATCHUP_SEED);
    let (mut wins, mut turns) = (0u32, 0u64);
    for _ in 0..MATCHUP_FIGHTS {
        let mut trial = recipe.clone();
        trial.engage(rules, Pick::Fair, &mut rng);
        for _ in 0..MATCHUP_TURNS {
            turns += 1;
            match sim::play_turn(&mut trial, rules, hand, &mut rng).applied {
                Applied::Round => continue,
                Applied::Slain { .. } => {
                    wins += 1;
                    break;
                }
                Applied::Lost { .. } => break,
                other => panic!("a turn against the old signal answered {other:?}"),
            }
        }
    }
    let fights = f64::from(MATCHUP_FIGHTS);
    Matchup {
        odds: f64::from(wins) / fights,
        turns: turns as f64 / fights,
        signal_left: 0.0,
        static_left: 0.0,
        patch: 0.0,
        pays: 0,
    }
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
        .map(|seed| sim::climb(player, seed, max_days, &mut bench, &mut Journal::Off))
        .collect()
}

/// The level a climb stood at when `day` (from 1) ended; a climb that has
/// marked stays at the top of the curve.
fn level_after(climb: &Climb, day: usize) -> i64 {
    match (climb.days.get(day - 1), climb.marked_on) {
        (Some(day), _) => i64::from(day.level),
        (None, Some(_)) => i64::from(MAX_LEVEL),
        (None, None) => 0,
    }
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
/// runner's bits earned. "Auto" is the `Auto` key every turn; "sharp" is a
/// runner reading the hand.
#[derive(Clone, Debug, PartialEq)]
struct Reading {
    /// The day of the first mark, by player.
    careful_day: u32,
    ambient_day: u32,
    reckless_day: u32,
    keen_day: u32,
    ambient_drops: i64,
    reckless_drops: i64,
    /// The careful runner's level at the end of day one and day seven.
    day_one_level: i64,
    day_seven_level: i64,
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
    /// The fair fight on auto from a kit level with the runner, at its
    /// hardest over levels 2 to 15.
    fair_low: f64,
    /// Turns a fair fight takes on auto, at its shortest and longest over
    /// levels 2 to 15.
    turns_low: f64,
    turns_high: f64,
    /// The signal a fair fight leaves, as a share of the max, averaged
    /// over levels 2 to 15: on auto, and read sharp.
    left_auto: f64,
    left_sharp: f64,
    /// The bright glyph from a kit level with the runner, lowest and
    /// highest over levels 4 to 15, on auto and read sharp.
    bright_low: f64,
    bright_high: f64,
    bright_sharp_low: f64,
    bright_sharp_high: f64,
    /// The signal a bright glyph leaves a runner on auto who puts it down,
    /// as a share of the max, averaged over levels 4 to 15: what it costs.
    bright_left: f64,
    /// The Old Signal at no marks: on auto, read sharp, and on auto with
    /// static on ice.
    boss_auto: f64,
    boss_sharp: f64,
    boss_glass: f64,
    boss_turns: f64,
    /// Crystals the keen runner found a day, and how many it spent.
    crystals_a_day: f64,
    keen_crystals_spent: i64,
    /// Every build there is, read on its own: a draft is only a choice if
    /// no option of it is a trap and none is the answer.
    builds: Vec<BuildReading>,
}

/// One build's numbers. The named players carry `sim::HOUSE_BUILD`; this
/// is the same careful and ambient runner on any other.
#[derive(Clone, Debug, PartialEq)]
struct BuildReading {
    build: Build,
    careful_day: u32,
    ambient_day: u32,
    ambient_drops: i64,
    /// The fair fight and the bright one on auto, from a kit level with
    /// the runner and the cards drafted by then, at their hardest over
    /// the levels the reading holds them at.
    fair_low: f64,
    bright_low: f64,
    /// The signal a fair fight leaves on auto, averaged over the levels.
    left_auto: f64,
    boss_auto: f64,
    boss_sharp: f64,
}

/// The bands every build is held to (`fight/BALANCE.md`, "The drafts"):
/// wider than the house build's, since they hold the best deck and the
/// worst at once.
const BUILD_CAREFUL_DAYS: std::ops::RangeInclusive<u32> = 14..=18;
const BUILD_AMBIENT_DAYS: std::ops::RangeInclusive<u32> = 15..=22;
const BUILD_BOSS_AUTO: (f64, f64) = (0.30, 0.80);
/// How far apart a draft's two options may land, averaged over the
/// builds that carry each: days to the mark on the key, and the Old
/// Signal on the key.
const DRAFT_DAYS_APART: f64 = 1.5;
const DRAFT_BOSS_APART: f64 = 0.25;

/// Seeds per player per build: half a reading's, sixteen times over.
const BUILD_SEEDS: u64 = 40;

impl BuildReading {
    fn take(rules: Rules, build: Build) -> Self {
        let marked =
            |climbs: &[Climb]| sim::median(climbs.iter().map(|climb| climb.marked_on), MAX_DAYS);
        let on = |player: Player| climbs(Player { build, ..player }, rules, BUILD_SEEDS, MAX_DAYS);
        let ambient = on(sim::AMBIENT);
        let fight = |level, pick| {
            matchup(&rules, &Recipe::level_kit(level, &build), pick, Hand::Auto)
                .expect("the fight starts")
        };
        let fair: Vec<Matchup> = (2..=MAX_LEVEL)
            .map(|level| fight(level, Pick::Fair))
            .collect();
        let low = |odds: &mut dyn Iterator<Item = f64>| odds.fold(f64::MAX, f64::min);
        Self {
            build,
            careful_day: marked(&on(sim::CAREFUL)),
            ambient_day: marked(&ambient),
            ambient_drops: median_of(&ambient, |climb| i64::from(climb.deaths)),
            fair_low: low(&mut fair.iter().map(|fight| fight.odds)),
            bright_low: low(&mut (4..=MAX_LEVEL).map(|level| fight(level, Pick::Bright).odds)),
            left_auto: fair.iter().map(|fight| fight.signal_left).sum::<f64>() / fair.len() as f64,
            boss_auto: old_signal(&rules, 0, None, Hand::Auto, &build).odds,
            boss_sharp: old_signal(&rules, 0, None, Hand::Sharp, &build).odds,
        }
    }
}

/// How far apart a draft's two options land, each averaged over every
/// build that carries it: the days to the mark on the key, and the Old
/// Signal on the key, in points.
#[derive(Clone, Copy, Debug, PartialEq)]
struct DraftSpread {
    options: [Card; 2],
    days: [f64; 2],
    boss: [f64; 2],
}

fn draft_spreads(builds: &[BuildReading]) -> Vec<DraftSpread> {
    DRAFTS
        .iter()
        .enumerate()
        .map(|(index, draft)| {
            let mean = |option: Card, read: fn(&BuildReading) -> f64| {
                let with: Vec<f64> = builds
                    .iter()
                    .filter(|reading| reading.build[index] == option)
                    .map(read)
                    .collect();
                with.iter().sum::<f64>() / with.len() as f64
            };
            DraftSpread {
                options: draft.options,
                days: draft
                    .options
                    .map(|option| mean(option, |reading| f64::from(reading.ambient_day))),
                boss: draft
                    .options
                    .map(|option| mean(option, |reading| reading.boss_auto)),
            }
        })
        .collect()
}

/// Seeds per player in a reading.
const SEEDS: u64 = 80;
const MAX_DAYS: u32 = 120;

impl Reading {
    fn take(rules: Rules) -> Self {
        let house = sim::HOUSE_BUILD;
        let mut batches = fan(
            vec![sim::CAREFUL, sim::AMBIENT, sim::RECKLESS, sim::KEEN],
            |player| climbs(player, rules, SEEDS, MAX_DAYS),
        )
        .into_iter();
        let mut batch = || batches.next().expect("a batch a player");
        let (careful, ambient, reckless, keen) = (batch(), batch(), batch(), batch());
        let builds = fan(sim::builds(), |build| BuildReading::take(rules, build));
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
        let level_kit = |level, pick, hand| {
            matchup(&rules, &Recipe::level_kit(level, &house), pick, hand)
                .expect("the fight starts")
        };
        let fair_auto: Vec<Matchup> = (2..=MAX_LEVEL)
            .map(|level| level_kit(level, Pick::Fair, Hand::Auto))
            .collect();
        let fair_sharp: Vec<Matchup> = (2..=MAX_LEVEL)
            .map(|level| level_kit(level, Pick::Fair, Hand::Sharp))
            .collect();
        let mean = |fights: &[Matchup], read: fn(&Matchup) -> f64| {
            fights.iter().map(read).sum::<f64>() / fights.len() as f64
        };
        let low = |values: &[f64]| values.iter().copied().fold(f64::MAX, f64::min);
        let high = |values: &[f64]| values.iter().copied().fold(f64::MIN, f64::max);
        let bright = |hand| -> Vec<Matchup> {
            (4..=MAX_LEVEL)
                .map(|level| level_kit(level, Pick::Bright, hand))
                .collect()
        };
        let bright_fights = bright(Hand::Auto);
        let bright_auto: Vec<f64> = bright_fights.iter().map(|fight| fight.odds).collect();
        let bright_sharp: Vec<f64> = bright(Hand::Sharp).iter().map(|fight| fight.odds).collect();
        let turns: Vec<f64> = fair_auto.iter().map(|fight| fight.turns).collect();
        let fair_odds: Vec<f64> = fair_auto.iter().map(|fight| fight.odds).collect();
        let keen_day = marked(&keen);
        let boss = old_signal(&rules, 0, None, Hand::Sharp, &house);
        Self {
            careful_day: marked(&careful),
            ambient_day: marked(&ambient),
            reckless_day: marked(&reckless),
            keen_day,
            ambient_drops: median_of(&ambient, |climb| i64::from(climb.deaths)),
            reckless_drops: median_of(&reckless, |climb| i64::from(climb.deaths)),
            day_one_level: median_of(&careful, |climb| level_after(climb, 1)),
            day_seven_level: median_of(&careful, |climb| level_after(climb, 7)),
            lead_low: leads.iter().copied().min().unwrap_or(i64::MIN),
            lead_high: leads.iter().copied().max().unwrap_or(i64::MAX),
            top_kit_at: (1..=MAX_LEVEL).find(|level| {
                kit_lead(&careful, *level) == Some(i64::from(2 * (MAX_TIER - level)))
            }),
            gear_share,
            patch_share,
            dropped_share,
            idle_share: 1.0 - gear_share - patch_share - dropped_share,
            fair_low: low(&fair_odds),
            turns_low: low(&turns),
            turns_high: high(&turns),
            left_auto: mean(&fair_auto, |fight| fight.signal_left),
            left_sharp: mean(&fair_sharp, |fight| fight.signal_left),
            bright_low: low(&bright_auto),
            bright_high: high(&bright_auto),
            bright_sharp_low: low(&bright_sharp),
            bright_sharp_high: high(&bright_sharp),
            bright_left: mean(&bright_fights, |fight| fight.signal_left),
            boss_auto: old_signal(&rules, 0, None, Hand::Auto, &house).odds,
            boss_sharp: boss.odds,
            boss_glass: old_signal(&rules, 0, Some(Drink::StaticOnIce), Hand::Auto, &house).odds,
            boss_turns: boss.turns,
            crystals_a_day: median_of(&keen, |climb| i64::from(climb.ledger.crystals_found)) as f64
                / f64::from(keen_day.max(1)),
            keen_crystals_spent: median_of(&keen, |climb| i64::from(climb.ledger.crystals_spent)),
            builds,
        }
    }

    /// The targets this reading misses, one line each: empty is a balanced
    /// game. The bands are the ones `fight/BALANCE.md` states and argues;
    /// change a band there and here together.
    fn misses(&self) -> Vec<String> {
        let pct = |share: f64| (share * 100.0).round() as i64;
        let mut checks = vec![
            (
                (14..=18).contains(&self.careful_day),
                format!("careful marks on day {}, want 14 to 18", self.careful_day),
            ),
            (
                (16..=21).contains(&self.ambient_day) && self.ambient_day >= self.careful_day,
                format!(
                    "ambient marks on day {} against careful's {}, want 16 to 21 and never ahead",
                    self.ambient_day, self.careful_day
                ),
            ),
            (
                self.ambient_drops <= 3,
                format!(
                    "ambient drops {} times, want 3 or fewer",
                    self.ambient_drops
                ),
            ),
            (
                (20..=28).contains(&self.reckless_day),
                format!("reckless marks on day {}, want 20 to 28", self.reckless_day),
            ),
            (
                (2..=8).contains(&self.reckless_drops),
                format!("reckless drops {} times, want 2 to 8", self.reckless_drops),
            ),
            (
                self.keen_day <= self.careful_day && self.keen_day + 5 >= self.careful_day,
                format!(
                    "keen marks on day {} against careful's {}, want 0 to 5 days ahead",
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
                (0.05..=0.30).contains(&self.patch_share),
                format!(
                    "patch takes {}% of the bits, want 5 to 30%",
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
                self.idle_share <= 0.30,
                format!(
                    "{}% of the bits sit idle, want 30% or less",
                    pct(self.idle_share)
                ),
            ),
            (
                self.fair_low >= 0.85,
                format!(
                    "the fair fight on auto dips to {}%, want 85% or more",
                    pct(self.fair_low)
                ),
            ),
            (
                self.turns_low >= 2.0 && self.turns_high <= 5.0,
                format!(
                    "a fair fight runs {:.1} to {:.1} turns, want 2 to 5",
                    self.turns_low, self.turns_high
                ),
            ),
            (
                self.left_sharp - self.left_auto >= 0.08,
                format!(
                    "reading the hand leaves {}% of the signal against auto's {}%, want 8 points or more",
                    pct(self.left_sharp),
                    pct(self.left_auto)
                ),
            ),
            (
                self.bright_low >= 0.40,
                format!(
                    "the bright glyph on auto dips to {}%, want 40% or more",
                    pct(self.bright_low)
                ),
            ),
            (
                self.bright_left <= 0.45,
                format!(
                    "a bright glyph put down on auto leaves {}% of the signal, want 45% or less",
                    pct(self.bright_left)
                ),
            ),
            (
                self.bright_sharp_low >= 0.85 && self.bright_sharp_low >= self.bright_low + 0.10,
                format!(
                    "the bright glyph read sharp dips to {}% against auto's {}%, want 85% or more and 10 points over",
                    pct(self.bright_sharp_low),
                    pct(self.bright_low)
                ),
            ),
            (
                (0.35..=0.60).contains(&self.boss_auto),
                format!(
                    "the old signal on auto is {}%, want 35 to 60%",
                    pct(self.boss_auto)
                ),
            ),
            (
                self.boss_sharp >= self.boss_auto + 0.15 && self.boss_sharp <= 0.95,
                format!(
                    "the old signal read sharp is {}% against auto's {}%, want 15 points over and 95% at most",
                    pct(self.boss_sharp),
                    pct(self.boss_auto)
                ),
            ),
            (
                self.boss_glass >= self.boss_auto + 0.10,
                format!(
                    "a glass moves the old signal on auto {}% to {}%, want 10 points or more",
                    pct(self.boss_auto),
                    pct(self.boss_glass)
                ),
            ),
            (
                (0.8..=2.3).contains(&self.crystals_a_day),
                format!("{:.1} crystals a day, want 0.8 to 2.3", self.crystals_a_day),
            ),
        ];
        // Every build, on its own: none is a trap.
        for reading in &self.builds {
            let name = build_name(&reading.build);
            checks.extend([
                (
                    BUILD_CAREFUL_DAYS.contains(&reading.careful_day),
                    format!(
                        "{name}: careful marks on day {}, want {BUILD_CAREFUL_DAYS:?}",
                        reading.careful_day
                    ),
                ),
                (
                    BUILD_AMBIENT_DAYS.contains(&reading.ambient_day)
                        && reading.ambient_drops <= 3,
                    format!(
                        "{name}: ambient marks on day {} with {} drops, want {BUILD_AMBIENT_DAYS:?} and 3 drops at most",
                        reading.ambient_day, reading.ambient_drops
                    ),
                ),
                (
                    reading.fair_low >= 0.85,
                    format!(
                        "{name}: the fair fight on auto dips to {}%, want 85% or more",
                        pct(reading.fair_low)
                    ),
                ),
                (
                    reading.bright_low >= 0.40,
                    format!(
                        "{name}: the bright glyph on auto dips to {}%, want 40% or more",
                        pct(reading.bright_low)
                    ),
                ),
                (
                    (BUILD_BOSS_AUTO.0..=BUILD_BOSS_AUTO.1).contains(&reading.boss_auto),
                    format!(
                        "{name}: the old signal on auto is {}%, want {} to {}%",
                        pct(reading.boss_auto),
                        pct(BUILD_BOSS_AUTO.0),
                        pct(BUILD_BOSS_AUTO.1)
                    ),
                ),
                (
                    reading.boss_sharp >= reading.boss_auto,
                    format!(
                        "{name}: the old signal read sharp is {}% against auto's {}%, want it no worse",
                        pct(reading.boss_sharp),
                        pct(reading.boss_auto)
                    ),
                ),
            ]);
        }
        // Every draft: neither option is the answer.
        for spread in draft_spreads(&self.builds) {
            let [first, second] = spread.options.map(Card::name);
            checks.extend([
                (
                    (spread.days[0] - spread.days[1]).abs() <= DRAFT_DAYS_APART,
                    format!(
                        "{first} marks on day {:.1} and {second} on day {:.1} on the key, want them within {DRAFT_DAYS_APART} days",
                        spread.days[0], spread.days[1]
                    ),
                ),
                (
                    (spread.boss[0] - spread.boss[1]).abs() <= DRAFT_BOSS_APART,
                    format!(
                        "{first} reads the old signal {}% and {second} {}% on the key, want them within {} points",
                        pct(spread.boss[0]),
                        pct(spread.boss[1]),
                        pct(DRAFT_BOSS_APART)
                    ),
                ),
            ]);
        }
        checks
            .into_iter()
            .filter(|(holds, _)| !holds)
            .map(|(_, miss)| miss)
            .collect()
    }

    const HEADER: &str = "| rules | careful | ambient (drops) | reckless (drops) | keen | lv day 1 / 7 | kit lead | top kit | gear | patch | dropped | idle | fair low | turns | left auto / sharp | bright auto (left) | bright sharp | boss auto / sharp / auto with a glass (turns) | crystals a day | builds: ambient day, boss auto | misses |\n|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|\n";

    /// The reading as one row of the sweep's table.
    fn row(&self, name: &str) -> String {
        let half = |lead: i64| format!("{:+.1}", lead as f64 / 2.0);
        format!(
            "| {name} | {} | {} ({}) | {} ({}) | {} | {} / {} | {} to {} | {} | {:.0}% | {:.0}% | {:.0}% | {:.0}% | {:.0}% | {:.1} to {:.1} | {:.0}% / {:.0}% | {:.0} to {:.0}% ({:.0}%) | {:.0} to {:.0}% | {:.0}% / {:.0}% / {:.0}% ({:.1}) | {:.1} | {} to {}, {:.0} to {:.0}% | {} |\n",
            self.careful_day,
            self.ambient_day,
            self.ambient_drops,
            self.reckless_day,
            self.reckless_drops,
            self.keen_day,
            self.day_one_level,
            self.day_seven_level,
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
            self.turns_low,
            self.turns_high,
            self.left_auto * 100.0,
            self.left_sharp * 100.0,
            self.bright_low * 100.0,
            self.bright_high * 100.0,
            self.bright_left * 100.0,
            self.bright_sharp_low * 100.0,
            self.bright_sharp_high * 100.0,
            self.boss_auto * 100.0,
            self.boss_sharp * 100.0,
            self.boss_glass * 100.0,
            self.boss_turns,
            self.crystals_a_day,
            self.builds
                .iter()
                .map(|reading| reading.ambient_day)
                .min()
                .unwrap_or(0),
            self.builds
                .iter()
                .map(|reading| reading.ambient_day)
                .max()
                .unwrap_or(0),
            self.builds
                .iter()
                .map(|reading| reading.boss_auto)
                .fold(f64::MAX, f64::min)
                * 100.0,
            self.builds
                .iter()
                .map(|reading| reading.boss_auto)
                .fold(f64::MIN, f64::max)
                * 100.0,
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

/// What gear is worth: the fair fight on auto at every level from every
/// kit, bare hands to four tiers over.
fn gear_table(rules: &Rules) -> String {
    const LEADS: [i32; 8] = [-4, -3, -2, -1, 0, 1, 2, 4];
    let mut out = String::from("| lv | bare |");
    for lead in LEADS {
        out.push_str(&format!(" {lead:+} |"));
    }
    out.push('\n');
    out.push_str("|---|---|");
    out.push_str(&"---|".repeat(LEADS.len()));
    out.push('\n');
    for level in 1..=MAX_LEVEL {
        let recipe = |kit| Recipe {
            kit,
            ..Recipe::level_kit(level, &sim::HOUSE_BUILD)
        };
        out.push_str(&format!(
            "| {level} | {} |",
            pct(matchup(rules, &recipe(Kit::Bare), Pick::Fair, Hand::Auto))
        ));
        for lead in LEADS {
            out.push_str(&format!(
                " {} |",
                pct(matchup(
                    rules,
                    &recipe(Kit::Lead(lead)),
                    Pick::Fair,
                    Hand::Auto
                ))
            ));
        }
        out.push('\n');
    }
    out
}

/// What reading the hand is worth: the fair fight and the bright one at
/// every level from a kit level with the runner, on the `Auto` key and
/// read sharp, with the numbers printed on the cards.
fn hands_table(rules: &Rules) -> String {
    let mut out = String::from(
        "| lv | deck | strike / block / hit | glyph signal | fair auto: odds, turns, signal left, static | fair sharp | bright auto | bright sharp |\n|---|---|---|---|---|---|---|---|\n",
    );
    for level in 1..=MAX_LEVEL {
        let recipe = Recipe::level_kit(level, &sim::HOUSE_BUILD);
        let sheet = recipe.sheet();
        let (_, _, foe) = rules.foe(level);
        let strike = rules.strike(sheet.attack(), foe.defense);
        let cell = |pick, hand| match matchup(rules, &recipe, pick, hand) {
            Some(fight) => format!(
                "{:.0}%, {:.1}, {:.0}%, {:.1}",
                fight.odds * 100.0,
                fight.turns,
                fight.signal_left * 100.0,
                fight.static_left
            ),
            None => "-".to_string(),
        };
        out.push_str(&format!(
            "| {level} | {} | {strike} / {} / {} | {} | {} | {} | {} | {} |\n",
            build_name(&recipe.cards),
            rules.block(sheet.defense()),
            rules.hit(foe.attack, sheet.defense()),
            foe.signal,
            cell(Pick::Fair, Hand::Auto),
            cell(Pick::Fair, Hand::Sharp),
            cell(Pick::Bright, Hand::Auto),
            cell(Pick::Bright, Hand::Sharp),
        ));
    }
    out
}

/// The bright glyph and the step down at every level, on auto: from a kit
/// level with the runner, a tier either side, and with each glass.
fn bright_table(rules: &Rules) -> String {
    let mut out = String::from(
        "| lv | lower | bright -1 | bright 0 | bright +1 | +ice | +neat | +pattern | bright pays |\n|---|---|---|---|---|---|---|---|---|\n",
    );
    for level in 1..=MAX_LEVEL {
        let recipe = |lead, drink| Recipe {
            kit: Kit::Lead(lead),
            drink,
            ..Recipe::level_kit(level, &sim::HOUSE_BUILD)
        };
        let bright = |lead, drink| matchup(rules, &recipe(lead, drink), Pick::Bright, Hand::Auto);
        out.push_str(&format!(
            "| {level} | {} | {} | {} | {} | {} | {} | {} | {} |\n",
            pct(matchup(rules, &recipe(0, None), Pick::Lower, Hand::Auto)),
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

/// The Old Signal from a kit level with the runner, by marks and by
/// glass, read sharp, and on auto with no glass.
fn old_signal_table(rules: &Rules) -> String {
    let mut out = String::from(
        "| marks | auto | sharp | static on ice | dead air, neat | test pattern | turns |\n|---|---|---|---|---|---|---|\n",
    );
    for marks in 0..=5 {
        let house = sim::HOUSE_BUILD;
        let odds = |drink| old_signal(rules, marks, drink, Hand::Sharp, &house).odds * 100.0;
        out.push_str(&format!(
            "| {marks} | {:.0}% | {:.0}% | {:.0}% | {:.0}% | {:.0}% | {:.1} |\n",
            old_signal(rules, marks, None, Hand::Auto, &house).odds * 100.0,
            odds(None),
            odds(Some(Drink::StaticOnIce)),
            odds(Some(Drink::DeadAirNeat)),
            odds(Some(Drink::TestPattern)),
            old_signal(rules, marks, None, Hand::Sharp, &house).turns,
        ));
    }
    out
}

/// Every level as arithmetic, beside what the fight costs in patch: where
/// a level's bits have to go.
fn economy_table(rules: &Rules) -> String {
    let mut out = String::from(
        "| lv | kills | days of fights | level pays | next pair | pair / pay | patch a kill | patch / pay | wall price | cache |\n|---|---|---|---|---|---|---|---|---|---|\n",
    );
    for level in 1..=MAX_LEVEL {
        let economy = level_economy(rules, level);
        let fight = matchup(
            rules,
            &Recipe::level_kit(level, &sim::HOUSE_BUILD),
            Pick::Fair,
            Hand::Auto,
        )
        .expect("the fair fight starts");
        out.push_str(&format!(
            "| {level} | {} | {:.1} | {} | {} | {:.0}% | {:.0} | {:.0}% | {} | {} |\n",
            economy.kills,
            economy.kills as f64 / FIGHT_STEPS as f64,
            economy.pays,
            economy.next_pair,
            economy.next_pair as f64 / economy.pays as f64 * 100.0,
            fight.patch,
            fight.patch / fight.pays.max(1) as f64 * 100.0,
            rules.price(level),
            rules.cache(level),
        ));
    }
    out
}

/// The climb as a curve: the median level every player stands at when
/// each day ends, a road a day from a fresh row.
fn days_table(batches: &[(&str, Vec<Climb>)], days: usize) -> String {
    let mut out = String::from("| day |");
    for (name, _) in batches {
        out.push_str(&format!(" {name} |"));
    }
    out.push_str("\n|---|");
    out.push_str(&"---|".repeat(batches.len()));
    out.push('\n');
    for day in 0..days {
        out.push_str(&format!("| {} |", day + 1));
        for (_, climbs) in batches {
            let level = median_of(climbs, |climb| level_after(climb, day + 1));
            let marked = climbs
                .iter()
                .filter(|climb| climb.marked_on.is_some_and(|on| on as usize <= day + 1))
                .count();
            out.push_str(&match marked * 2 > climbs.len() {
                true => " marked |".to_string(),
                false => format!(" {level} |"),
            });
        }
        out.push('\n');
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
        "\nheard on day {}, marked on day {}. drops {} ({}), boss tries {}. {:.1} turns a fight.\n",
        days(|climb| climb.heard_on),
        days(|climb| climb.marked_on),
        median_of(climbs, |climb| i64::from(climb.deaths)),
        match days(|climb| climb.first_drop_on) {
            day if day > max_days => "most climbs never drop".to_string(),
            day => format!("the first on day {day}"),
        },
        median_of(climbs, |climb| i64::from(climb.boss_tries)),
        climbs
            .iter()
            .map(|climb| f64::from(climb.turns))
            .sum::<f64>()
            / climbs
                .iter()
                .map(|climb| f64::from(climb.fights))
                .sum::<f64>()
                .max(1.0),
    ));
    out.push_str(&format!(
        "bits: earned {} ({} from caches), gear {}, patch {}, dropped {}, borrowed {} (fees {}). crystals: found {}, spent {}. bright: {} tries, {} kills.\n\n",
        median_of(climbs, |climb| climb.ledger.earned),
        median_of(climbs, |climb| climb.ledger.cached),
        median_of(climbs, |climb| climb.ledger.gear),
        median_of(climbs, |climb| climb.ledger.patched),
        median_of(climbs, |climb| climb.ledger.dropped),
        median_of(climbs, |climb| climb.ledger.borrowed),
        median_of(climbs, |climb| climb.ledger.loan_fees),
        median_of(climbs, |climb| i64::from(climb.ledger.crystals_found)),
        median_of(climbs, |climb| i64::from(climb.ledger.crystals_spent)),
        median_of(climbs, |climb| i64::from(climb.ledger.bright_tries)),
        median_of(climbs, |climb| i64::from(climb.ledger.bright_kills)),
    ));
    out
}

/// One player's climb a day at a time: where the runner stood at dusk
/// and what the day held, the median over the seeds still climbing, the
/// level also at its lowest and highest. Stops once most climbs have
/// marked.
fn day_table(name: &str, climbs: &[Climb], days: usize) -> String {
    let mut out = format!(
        "### {name}\n\n| day | level: low, median, high | kit | cards | kills (bright) | runs | dropped | earned | gear | patch | bits at dusk | crystals |\n|---|---|---|---|---|---|---|---|---|---|---|---|\n"
    );
    for day in 0..days {
        let on: Vec<&Day> = climbs
            .iter()
            .filter_map(|climb| climb.days.get(day))
            .collect();
        if on.len() * 2 < climbs.len() {
            break;
        }
        let sorted = |read: fn(&Day) -> i64| {
            let mut values: Vec<i64> = on.iter().map(|day| read(day)).collect();
            values.sort_unstable();
            values
        };
        let median = |read: fn(&Day) -> i64| sorted(read)[on.len() / 2];
        let levels = sorted(|day| i64::from(day.level));
        out.push_str(&format!(
            "| {} | {}, {}, {} | {} / {} | {} | {} ({}) | {} | {:.0}% | {} | {} | {} | {} | {} |\n",
            day + 1,
            levels[0],
            levels[levels.len() / 2],
            levels[levels.len() - 1],
            median(|day| i64::from(day.weapon_tier)),
            median(|day| i64::from(day.armor_tier)),
            median(|day| day.drafted as i64),
            median(|day| i64::from(day.kills)),
            median(|day| i64::from(day.bright_kills)),
            median(|day| i64::from(day.runs)),
            on.iter().filter(|day| day.dropped).count() as f64 / on.len() as f64 * 100.0,
            median(|day| day.earned),
            median(|day| day.gear),
            median(|day| day.patched),
            median(|day| day.bits),
            median(|day| i64::from(day.crystals)),
        ));
    }
    out.push('\n');
    out
}

/// What each draft's cards are worth where they are drafted: the runner
/// at the draft's level, kit level with it, holding the house picks of
/// the drafts before, with the card it replaces kept and with each
/// option.
fn cards_table(rules: &Rules) -> String {
    let mut out = String::from(
        "| at | card | fair auto: odds, turns, signal left | fair sharp | bright auto | bright sharp |\n|---|---|---|---|---|---|\n",
    );
    for (index, draft) in DRAFTS.iter().enumerate() {
        let before = &sim::HOUSE_BUILD[..index];
        let picks: [Option<Card>; 3] = [None, Some(draft.options[0]), Some(draft.options[1])];
        for pick in picks {
            let mut cards = before.to_vec();
            cards.extend(pick);
            let recipe = Recipe {
                level: draft.level,
                kit: Kit::Lead(0),
                drink: None,
                marks: 0,
                cards,
            };
            let cell = |pick, hand| match matchup(rules, &recipe, pick, hand) {
                Some(fight) => format!(
                    "{:.0}%, {:.1}, {:.0}%",
                    fight.odds * 100.0,
                    fight.turns,
                    fight.signal_left * 100.0
                ),
                None => "-".to_string(),
            };
            out.push_str(&format!(
                "| lv {} | {} | {} | {} | {} | {} |\n",
                draft.level,
                match pick {
                    Some(card) => card.name().to_string(),
                    None => format!("(the {} kept)", draft.replaces.name()),
                },
                cell(Pick::Fair, Hand::Auto),
                cell(Pick::Fair, Hand::Sharp),
                cell(Pick::Bright, Hand::Auto),
                cell(Pick::Bright, Hand::Sharp),
            ));
        }
    }
    out
}

/// Every build there is, a row each, then every draft's two options side
/// by side, each averaged over the builds that carry it.
fn builds_table(builds: &[BuildReading]) -> String {
    let mut out = String::from(
        "| build | careful | ambient (drops) | fair low | signal left, key | bright low | boss key | boss sharp |\n|---|---|---|---|---|---|---|---|\n",
    );
    for reading in builds {
        out.push_str(&format!(
            "| {} | {} | {} ({}) | {:.0}% | {:.0}% | {:.0}% | {:.0}% | {:.0}% |\n",
            build_name(&reading.build),
            reading.careful_day,
            reading.ambient_day,
            reading.ambient_drops,
            reading.fair_low * 100.0,
            reading.left_auto * 100.0,
            reading.bright_low * 100.0,
            reading.boss_auto * 100.0,
            reading.boss_sharp * 100.0,
        ));
    }
    out.push_str(
        "\n| draft | card | marks on the key (mean day) | boss on the key |\n|---|---|---|---|\n",
    );
    for (draft, spread) in DRAFTS.iter().zip(draft_spreads(builds)) {
        for option in 0..spread.options.len() {
            out.push_str(&format!(
                "| lv {} | {} | {:.1} | {:.0}% |\n",
                draft.level,
                spread.options[option].name(),
                spread.days[option],
                spread.boss[option] * 100.0,
            ));
        }
    }
    out
}

/// Days of a printed run that show every turn of every fight. Past them
/// a fight is one line, unless it is the Old Signal's or it was lost.
const RUN_DETAIL_DAYS: u32 = 2;

fn kit_words(weapon_tier: i32, armor_tier: i32) -> String {
    format!(
        "{} (tier {weapon_tier}) / {} (tier {armor_tier})",
        gear_name(Slot::Weapon, weapon_tier).unwrap_or("bare hands"),
        gear_name(Slot::Armor, armor_tier).unwrap_or("street clothes"),
    )
}

fn sheet_words(sheet: &Snapshot) -> String {
    let mut words = format!(
        "lv {} · exp {} · signal {}/{} · {} · {} bits · {} crystals",
        sheet.level,
        sheet.exp,
        sheet.signal,
        sheet.max_signal,
        kit_words(sheet.weapon_tier, sheet.armor_tier),
        sheet.bits,
        sheet.crystals
    );
    if sheet.debt > 0 {
        words.push_str(&format!(" · owes {}", sheet.debt));
    }
    if sheet.static_cards > 0 {
        words.push_str(&format!(" · {} static in the deck", sheet.static_cards));
    }
    words
}

fn card_names(cards: &[Card]) -> String {
    match cards.is_empty() {
        true => "nothing".to_string(),
        false => cards
            .iter()
            .map(|card| card.name())
            .collect::<Vec<_>>()
            .join(", "),
    }
}

/// The day's road as three rows of ten: `g` a glyph, `B` a bright one,
/// `+` a rest, `$` a cache.
fn road_words(day: u32) -> String {
    let road = super::road::road_for(sim::date(day));
    (0..LANES)
        .map(|lane| {
            let row: String = (0..STEPS)
                .map(|step| match road.steps[step][lane] {
                    Node::Glyph => 'g',
                    Node::Bright => 'B',
                    Node::Rest => '+',
                    Node::Cache => '$',
                })
                .collect();
            format!("`{row}`")
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// A journal as a run to read: every day from the first dawn to the mark,
/// every shop visit and every step, and every card of every turn for the
/// first days, the Old Signal, and any fight that was lost.
fn run_words(events: &[Event]) -> String {
    let mut out = String::new();
    let mut day = 0;
    // The fight being read: its header, its turns, and whether it is one
    // to print whole.
    let mut fight: Option<(String, Vec<String>, bool, i32)> = None;
    let mut step_no = 0;
    for event in events {
        match event {
            Event::Dawn { day: dawn, sheet } => {
                day = *dawn;
                out.push_str(&format!(
                    "\n### day {day}\n\ndawn: {}\n\ndeck: {}\n\nroad: {}\n\n",
                    sheet_words(sheet),
                    card_names(&sheet.deck),
                    road_words(day)
                ));
            }
            Event::Drafted { card, over } => out.push_str(&format!(
                "- **new card: {}**, in place of a {}. {}\n",
                card.name(),
                over.name(),
                card.rule()
            )),
            Event::Patched { restored, paid } => {
                out.push_str(&format!("- patch: +{restored} signal for {paid} bits\n"));
            }
            Event::Borrowed { amount, fee } => out.push_str(&format!(
                "- the bits machine: {amount} bits lent, {fee} more on the debt\n"
            )),
            Event::Outfitted { slot, tier, paid } => out.push_str(&format!(
                "- the armorer: {} ({} tier {tier}) for {paid} bits\n",
                gear_name(*slot, *tier).expect("a tier on the wall"),
                match slot {
                    Slot::Weapon => "weapon",
                    Slot::Armor => "armor",
                }
            )),
            Event::Carted {
                slot,
                tier,
                crystals,
            } => out.push_str(&format!(
                "- the blade shop: {} ({} tier {tier}) for {crystals} crystals\n",
                gear_name(*slot, *tier).expect("a tier on the wall"),
                match slot {
                    Slot::Weapon => "weapon",
                    Slot::Armor => "armor",
                }
            )),
            Event::Drank { drink } => {
                out.push_str(&format!("- dead air: a glass of {}\n", drink.name()));
            }
            Event::Step { step, lane, node } => {
                step_no = *step;
                out.push_str(&format!(
                    "- **step {step}**, lane {}: {}",
                    lane + 1,
                    match node {
                        Node::Glyph => "a glyph",
                        Node::Bright => "a bright glyph",
                        Node::Rest => "a rest",
                        Node::Cache => "a cache",
                    }
                ));
            }
            Event::Mended { restored, signal } => {
                out.push_str(&format!(". mended +{restored}, signal {signal}\n"));
            }
            Event::Cleared { cards } => {
                out.push_str(&format!(". {cards} static shaken out of the deck\n"));
            }
            Event::Cached { bits } => out.push_str(&format!(". +{bits} bits\n")),
            Event::Met {
                foe,
                pick,
                boss,
                foe_signal,
                hit,
                signal,
                static_cards,
            } => {
                let mut header = format!(
                    ". the {foe}{} ({foe_signal} signal, hits for {hit}), you at {signal}",
                    match pick {
                        Pick::Lower => ", a step down",
                        Pick::Fair | Pick::Bright => "",
                    }
                );
                if *static_cards > 0 {
                    header.push_str(&format!(" with {static_cards} static"));
                }
                fight = Some((header, Vec::new(), *boss || day <= RUN_DETAIL_DAYS, *hit));
            }
            Event::Turn {
                intent,
                hand,
                played,
                foe_signal,
                block,
                signal,
            } => {
                let (_, turns, _, hit) = fight.as_mut().expect("a turn is part of a fight");
                let shows = match intent {
                    Intent::Hit => format!("it hits for {hit}"),
                    Intent::Heavy => format!("it comes down for {}", *hit * 2),
                    Intent::Charge => "it gathers".to_string(),
                    Intent::Noise => "it throws noise".to_string(),
                };
                turns.push(format!(
                    "    - turn {}: {shows}. hand: {}. played: {}. glyph at {foe_signal}, block {block}, you at {signal}\n",
                    turns.len() + 1,
                    card_names(hand),
                    card_names(played),
                ));
            }
            Event::Ran { signal } => {
                let (header, turns, whole, _) = fight.take().expect("a run ends a fight");
                out.push_str(&format!("{header}. **ran**, signal {signal}\n"));
                if whole {
                    out.push_str(&turns.concat());
                }
            }
            Event::Won {
                turns: count,
                signal,
                bits,
                exp,
                crystals,
                leveled,
                static_cards,
            } => {
                let (header, turns, whole, _) = fight.take().expect("a win ends a fight");
                out.push_str(&format!(
                    "{header}. won in {count} turns, signal {signal}, +{bits} bits, +{exp} exp"
                ));
                if *crystals > 0 {
                    out.push_str(", a crystal");
                }
                if *static_cards > 0 {
                    out.push_str(&format!(", {static_cards} static in the deck"));
                }
                if let Some(level) = leveled {
                    out.push_str(&format!(". **level {level}**"));
                }
                out.push('\n');
                if whole {
                    out.push_str(&turns.concat());
                }
            }
            Event::Fell { turns: count, bits_lost } => {
                let (header, turns, _, _) = fight.take().expect("a drop ends a fight");
                out.push_str(&format!(
                    "{header}. **the signal dropped** on turn {count} of step {step_no}. the street took {bits_lost} bits\n"
                ));
                out.push_str(&turns.concat());
            }
            Event::Slain { turns: count } => {
                let (header, turns, _, _) = fight.take().expect("the kill ends the fight");
                out.push_str(&format!(
                    "{header}. **the Old Signal is down** in {count} turns. a mark\n"
                ));
                out.push_str(&turns.concat());
            }
            Event::Dusk { sheet, today, .. } => out.push_str(&format!(
                "\ndusk: {}\n\ntoday: {} steps, {} glyphs down ({} bright), {} runs, earned {}, gear {}, patch {}\n",
                sheet_words(sheet),
                today.steps,
                today.kills,
                today.bright_kills,
                today.runs,
                today.earned,
                today.gear,
                today.patched,
            )),
        }
    }
    out
}

/// One climb of `player` on `seed`, journaled and printed whole.
fn run(name: &str, player: Player, seed: u64, rules: Rules) -> String {
    let mut journal = Journal::On(Vec::new());
    let climb = sim::climb(
        player,
        seed,
        MAX_DAYS,
        &mut Bench::under(rules),
        &mut journal,
    );
    let Journal::On(events) = journal else {
        unreachable!("the journal was on");
    };
    let mut out = format!(
        "## {name}, seed {seed}\n\n{}, {} drops, {} tries at the Old Signal, {} fights, {} turns. build: {}.\n",
        match climb.marked_on {
            Some(day) => format!("marked on day {day}"),
            None => format!("no mark in {MAX_DAYS} days"),
        },
        climb.deaths,
        climb.boss_tries,
        climb.fights,
        climb.turns,
        build_name(&player.build),
    );
    out.push_str(&run_words(&events));
    out.push('\n');
    out
}

/// The seed the printed runs are played on.
const RUN_SEED: u64 = 7;

/// A whole run, start to finish, for the players a pass reads first: the
/// runner on the key, the one reading the hand, and the one playing the
/// crystal pass.
fn runs(rules: Rules) -> String {
    let mut out = String::from(
        "# deadchannel: whole runs\n\nOne seeded climb a player, from a fresh row to the first mark, through the real machine. Every shop visit and every step is here; every card of every turn for the first days, for the Old Signal, and for any fight that was lost.\n\n",
    );
    let printed = fan(
        vec![
            ("ambient", sim::AMBIENT),
            ("careful", sim::CAREFUL),
            ("keen", sim::KEEN),
        ],
        |(name, player)| run(name, player, RUN_SEED, rules),
    );
    out.push_str(&printed.concat());
    out
}

/// Days the first-days tables print.
const FIRST_DAYS: usize = 30;

/// Days the report's curve prints.
const CURVE_DAYS: usize = 28;

/// Every table for one set of rules: the whole report.
fn report(rules: Rules) -> String {
    let mut out = String::from("# deadchannel arena\n\n## The reading\n\n");
    out.push_str(Reading::HEADER);
    let reading = Reading::take(rules);
    out.push_str(&reading.row("live"));
    for miss in reading.misses() {
        out.push_str(&format!("\nMissed: {miss}\n"));
    }
    let batches = fan(sim::PLAYERS.to_vec(), |(name, player)| {
        (name, climbs(player, rules, SEEDS, MAX_DAYS))
    });
    out.push_str("\n## The climb, day by day: the level at the end of each day\n\n");
    out.push_str(&days_table(&batches, CURVE_DAYS));
    out.push_str("\n## A day at a time: where each player stands at dusk\n\n");
    for (name, climbs) in &batches {
        out.push_str(&day_table(name, climbs, FIRST_DAYS));
    }
    out.push_str("\n## The drafts: what each card is worth where it is picked\n\n");
    out.push_str(&cards_table(&rules));
    out.push_str("\n## Every build\n\n");
    out.push_str(&builds_table(&reading.builds));
    out.push_str("\n## What reading the hand is worth\n\n");
    out.push_str(&hands_table(&rules));
    out.push_str("\n## What gear is worth: the fair fight on auto by kit lead\n\n");
    out.push_str(&gear_table(&rules));
    out.push_str("\n## Where a level's bits go\n\n");
    out.push_str(&economy_table(&rules));
    out.push_str("\n## The bright glyph and the step down\n\n");
    out.push_str(&bright_table(&rules));
    out.push_str("\n## The Old Signal\n\n");
    out.push_str(&old_signal_table(&rules));
    out.push_str("\n## Climbs\n\n");
    for (name, climbs) in &batches {
        out.push_str(&climb_table(name, climbs, MAX_DAYS));
    }
    out
}

/// One reading per candidate, side by side.
fn sweep(candidates: &[(&str, Rules)]) -> String {
    let mut out = String::from(Reading::HEADER);
    let mut missed = String::new();
    let mut drafts = String::new();
    for (name, rules) in candidates {
        let reading = Reading::take(*rules);
        out.push_str(&reading.row(name));
        for miss in reading.misses() {
            missed.push_str(&format!("- {name}: {miss}\n"));
        }
        drafts.push_str(&format!("| {name} |"));
        for spread in draft_spreads(&reading.builds) {
            for option in 0..spread.options.len() {
                drafts.push_str(&format!(
                    " {:.1}, {:.0}% |",
                    spread.days[option],
                    spread.boss[option] * 100.0
                ));
            }
        }
        drafts.push('\n');
    }
    out.push_str("\nEvery draft's options on the key, each over the builds that carry it: the mean day of the mark, and the Old Signal.\n\n| rules |");
    for draft in &DRAFTS {
        for option in draft.options {
            out.push_str(&format!(" {} |", option.name()));
        }
    }
    out.push_str("\n|---|");
    out.push_str(&"---|".repeat(DRAFTS.len() * 2));
    out.push('\n');
    out.push_str(&drafts);
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
