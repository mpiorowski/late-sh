use rand::{SeedableRng, rngs::StdRng};
use uuid::Uuid;

use super::{
    AMBIENT, Bench, CAREFUL, Climb, Day, Drinks, Event, Hand, Journal, KEEN, Ledger, NEGLECTFUL,
    Player, RECKLESS, Start, Threat, builds, climb, date, odds_rival, play_turn, summary,
};
use crate::app::deadchannel::fight::cards::DRAFTS;
use crate::app::deadchannel::fight::data::RULES;
use crate::app::deadchannel::fight::state::{Applied, Command, Drink, Pick, Prey, Rival, Sheet};

/// Seeded climbs per player: enough for a stable median, few enough to
/// stay a unit test.
const SEEDS: u64 = 40;
/// Far past the target, so a slow ladder shows where it stalls instead
/// of vanishing into "never".
const MAX_DAYS: u32 = 120;

/// The balance target (GAME.md, "The daily ration loop"): the first mark
/// is two to three weeks of a road a day. A runner who reads the hand
/// lands at the two-week end, one on the `Auto` key at the three-week
/// end, and one who is careless about it past that. Days are medians
/// over the seeds.
const CAREFUL_MARK_DAYS: std::ops::RangeInclusive<u32> = 14..=18;
const AMBIENT_MARK_DAYS: std::ops::RangeInclusive<u32> = 16..=22;
const RECKLESS_MARK_DAYS: std::ops::RangeInclusive<u32> = 18..=28;

fn climbs(player: Player) -> Vec<Climb> {
    climbs_from(player, Start::FRESH)
}

fn climbs_from(player: Player, start: Start) -> Vec<Climb> {
    let mut bench = Bench::live();
    (0..SEEDS)
        .map(|seed| climb(player, start, seed, MAX_DAYS, &mut bench, &mut Journal::Off))
        .collect()
}

fn marked(climbs: &[Climb]) -> u32 {
    super::median(climbs.iter().map(|climb| climb.marked_on), MAX_DAYS)
}

/// A change to the curves, the glyph tiers, the cards, the road, the Old
/// Signal, or the fight rules moves this. Read the ladder in the message
/// before retuning the target: the target is the design, the numbers are
/// what to fix.
#[test]
fn a_careful_runner_marks_in_about_two_weeks() {
    let climbs = climbs(CAREFUL);
    let day = marked(&climbs);
    assert!(
        CAREFUL_MARK_DAYS.contains(&day),
        "a careful runner's first mark lands on day {day}, target {CAREFUL_MARK_DAYS:?}\n{}",
        summary(&climbs, MAX_DAYS)
    );
}

/// The `Auto` key is the floor, and the floor finishes: one button a
/// fight still gets a runner to the mark inside three weeks.
#[test]
fn a_runner_on_the_auto_key_marks_in_about_three_weeks() {
    let climbs = climbs(AMBIENT);
    let day = marked(&climbs);
    assert!(
        AMBIENT_MARK_DAYS.contains(&day),
        "a runner on the auto key marks on day {day}, target {AMBIENT_MARK_DAYS:?}\n{}",
        summary(&climbs, MAX_DAYS)
    );
}

#[test]
fn a_reckless_runner_marks_late_and_drops_a_few_times() {
    let climbs = climbs(RECKLESS);
    let day = marked(&climbs);
    let deaths = super::median(climbs.iter().map(|climb| Some(climb.deaths)), MAX_DAYS);
    assert!(
        RECKLESS_MARK_DAYS.contains(&day),
        "a reckless runner's first mark lands on day {day}, target {RECKLESS_MARK_DAYS:?}\n{}",
        summary(&climbs, MAX_DAYS)
    );
    assert!(
        (1..=8).contains(&deaths),
        "a reckless runner drops {deaths} times on the way up, one to a few expected\n{}",
        summary(&climbs, MAX_DAYS)
    );
}

/// Reading the hand is never the slower climb, and care is never slower
/// than carelessness: or the game rewards not playing it.
#[test]
fn the_hand_and_care_are_each_worth_days() {
    let careful = marked(&climbs(CAREFUL));
    let ambient = marked(&climbs(AMBIENT));
    let reckless = marked(&climbs(RECKLESS));
    assert!(
        careful <= ambient && ambient <= reckless,
        "careful marks on day {careful}, ambient on day {ambient}, reckless on day {reckless}"
    );
}

/// The lock a runner found: they walked past the armorer, levelled on
/// exp into a glyph their bare hands cannot beat, and the street took
/// every bit. The game must leave a way back that does not wait on luck:
/// after that first drop the runner gains a level within a week.
#[test]
fn a_runner_who_skipped_the_armorer_climbs_again_within_a_week_of_the_drop() {
    let climbs = climbs(NEGLECTFUL);
    let dropped = climbs
        .iter()
        .filter(|climb| climb.first_drop_on.is_some())
        .count() as u64;
    assert!(
        dropped * 4 >= SEEDS * 3,
        "the neglectful runner is meant to find the drop; {dropped} of {SEEDS} did"
    );
    let stuck = super::median(
        climbs.iter().filter_map(|climb| {
            let dropped = climb.first_drop_on?;
            Some(climb.recovered_on.map(|day| day - dropped))
        }),
        MAX_DAYS,
    );
    assert!(
        stuck <= 7,
        "after the first drop the neglectful runner waits {stuck} days for a level\n{}",
        summary(&climbs, MAX_DAYS)
    );
}

/// One seeded climb, whole. The bands above hold the medians; this holds
/// one runner's every day and every bit, so a rule change that shifts the
/// climb inside a band still shows up, as a diff to read. KEEN touches the
/// most rules (the road's route, the hand read sharp, the bright glyph,
/// the glass, the blade shop, the purse). When it moves, read which
/// fields moved and why before re-blessing.
#[test]
fn one_keen_climb_holds_still() {
    let climb = climb(
        KEEN,
        Start::FRESH,
        7,
        MAX_DAYS,
        &mut Bench::live(),
        &mut Journal::Off,
    );
    assert_eq!(climb, pinned_keen_climb());
}

fn pinned_keen_climb() -> Climb {
    Climb {
        level_on: [
            None,
            Some(1),
            Some(1),
            Some(1),
            Some(2),
            Some(3),
            Some(4),
            Some(4),
            Some(5),
            Some(6),
            Some(7),
            Some(9),
            Some(10),
            Some(11),
            Some(13),
            Some(15),
        ],
        heard_on: Some(17),
        marked_on: Some(17),
        deaths: 0,
        first_drop_on: None,
        recovered_on: None,
        boss_tries: 1,
        kit_on: [
            None,
            Some((1, 1)),
            Some((2, 2)),
            Some((3, 3)),
            Some((5, 4)),
            Some((5, 5)),
            Some((6, 6)),
            Some((7, 7)),
            Some((9, 8)),
            Some((10, 9)),
            Some((10, 10)),
            Some((12, 11)),
            Some((12, 12)),
            Some((14, 13)),
            Some((15, 14)),
            Some((15, 15)),
        ],
        ledger: Ledger {
            earned: 171562,
            cached: 53638,
            gear: 141251,
            patched: 7585,
            patch_days: 13,
            dropped: 0,
            borrowed: 0,
            loan_fees: 0,
            crystals_found: 16,
            crystals_spent: 16,
            glasses: 13,
            poured_bits: 0,
            carted: 1,
            inked: 0,
            bright_tries: 16,
            bright_kills: 13,
        },
        days: vec![
            Day {
                level: 3,
                signal: 26,
                weapon_tier: 3,
                armor_tier: 2,
                bits: 890,
                crystals: 1,
                drafted: 1,
                kills: 5,
                bright_kills: 2,
                runs: 0,
                dropped: false,
                earned: 2861,
                gear: 2003,
                patched: 18,
                steps: 10,
            },
            Day {
                level: 4,
                signal: 40,
                weapon_tier: 4,
                armor_tier: 4,
                bits: 2673,
                crystals: 1,
                drafted: 1,
                kills: 5,
                bright_kills: 1,
                runs: 0,
                dropped: false,
                earned: 5272,
                gear: 3417,
                patched: 72,
                steps: 10,
            },
            Day {
                level: 5,
                signal: 49,
                weapon_tier: 5,
                armor_tier: 5,
                bits: 3088,
                crystals: 1,
                drafted: 1,
                kills: 5,
                bright_kills: 1,
                runs: 0,
                dropped: false,
                earned: 6062,
                gear: 5517,
                patched: 130,
                steps: 10,
            },
            Day {
                level: 7,
                signal: 62,
                weapon_tier: 6,
                armor_tier: 6,
                bits: 4985,
                crystals: 2,
                drafted: 2,
                kills: 5,
                bright_kills: 2,
                runs: 0,
                dropped: false,
                earned: 6947,
                gear: 4810,
                patched: 240,
                steps: 10,
            },
            Day {
                level: 8,
                signal: 56,
                weapon_tier: 7,
                armor_tier: 7,
                bits: 7312,
                crystals: 1,
                drafted: 2,
                kills: 5,
                bright_kills: 0,
                runs: 0,
                dropped: false,
                earned: 7289,
                gear: 4962,
                patched: 0,
                steps: 10,
            },
            Day {
                level: 9,
                signal: 53,
                weapon_tier: 9,
                armor_tier: 8,
                bits: 3020,
                crystals: 2,
                drafted: 2,
                kills: 5,
                bright_kills: 2,
                runs: 0,
                dropped: false,
                earned: 10628,
                gear: 14480,
                patched: 440,
                steps: 10,
            },
            Day {
                level: 10,
                signal: 89,
                weapon_tier: 10,
                armor_tier: 9,
                bits: 2227,
                crystals: 3,
                drafted: 3,
                kills: 5,
                bright_kills: 2,
                runs: 0,
                dropped: false,
                earned: 13438,
                gear: 13619,
                patched: 612,
                steps: 10,
            },
            Day {
                level: 10,
                signal: 42,
                weapon_tier: 10,
                armor_tier: 10,
                bits: 11842,
                crystals: 1,
                drafted: 3,
                kills: 5,
                bright_kills: 0,
                runs: 0,
                dropped: false,
                earned: 10035,
                gear: 0,
                patched: 420,
                steps: 10,
            },
            Day {
                level: 11,
                signal: 79,
                weapon_tier: 11,
                armor_tier: 11,
                bits: 14342,
                crystals: 1,
                drafted: 3,
                kills: 5,
                bright_kills: 1,
                runs: 0,
                dropped: false,
                earned: 12408,
                gear: 9314,
                patched: 594,
                steps: 10,
            },
            Day {
                level: 12,
                signal: 102,
                weapon_tier: 12,
                armor_tier: 12,
                bits: 6038,
                crystals: 1,
                drafted: 4,
                kills: 4,
                bright_kills: 1,
                runs: 1,
                dropped: false,
                earned: 11408,
                gear: 18733,
                patched: 979,
                steps: 10,
            },
            Day {
                level: 13,
                signal: 85,
                weapon_tier: 13,
                armor_tier: 13,
                bits: 5810,
                crystals: 2,
                drafted: 4,
                kills: 5,
                bright_kills: 1,
                runs: 0,
                dropped: false,
                earned: 12732,
                gear: 12960,
                patched: 0,
                steps: 10,
            },
            Day {
                level: 13,
                signal: 130,
                weapon_tier: 14,
                armor_tier: 13,
                bits: 2942,
                crystals: 1,
                drafted: 4,
                kills: 5,
                bright_kills: 0,
                runs: 0,
                dropped: false,
                earned: 14940,
                gear: 16859,
                patched: 949,
                steps: 10,
            },
            Day {
                level: 14,
                signal: 54,
                weapon_tier: 14,
                armor_tier: 14,
                bits: 11051,
                crystals: 0,
                drafted: 4,
                kills: 5,
                bright_kills: 0,
                runs: 0,
                dropped: false,
                earned: 15837,
                gear: 6734,
                patched: 994,
                steps: 10,
            },
            Day {
                level: 14,
                signal: 26,
                weapon_tier: 15,
                armor_tier: 14,
                bits: 333,
                crystals: 0,
                drafted: 4,
                kills: 2,
                bright_kills: 0,
                runs: 3,
                dropped: false,
                earned: 9977,
                gear: 19743,
                patched: 952,
                steps: 10,
            },
            Day {
                level: 15,
                signal: 103,
                weapon_tier: 15,
                armor_tier: 15,
                bits: 9095,
                crystals: 0,
                drafted: 4,
                kills: 5,
                bright_kills: 0,
                runs: 0,
                dropped: false,
                earned: 16862,
                gear: 8100,
                patched: 0,
                steps: 10,
            },
            Day {
                level: 15,
                signal: 26,
                weapon_tier: 15,
                armor_tier: 15,
                bits: 19378,
                crystals: 0,
                drafted: 4,
                kills: 3,
                bright_kills: 0,
                runs: 2,
                dropped: false,
                earned: 11468,
                gear: 0,
                patched: 1185,
                steps: 10,
            },
            Day {
                level: 15,
                signal: 10,
                weapon_tier: 15,
                armor_tier: 15,
                bits: 50,
                crystals: 0,
                drafted: 4,
                kills: 2,
                bright_kills: 0,
                runs: 0,
                dropped: false,
                earned: 3398,
                gear: 0,
                patched: 0,
                steps: 3,
            },
        ],
        turns: 240,
        fights: 82,
    }
}

/// Sixteen builds: one option of every draft, none twice.
#[test]
fn the_builds_are_every_way_through_the_drafts() {
    let builds = builds();
    assert_eq!(builds.len(), 1 << DRAFTS.len());
    for (index, build) in builds.iter().enumerate() {
        assert!(!builds[..index].contains(build), "{build:?} twice");
        for (draft, card) in DRAFTS.iter().zip(build) {
            assert!(draft.options.contains(card), "{build:?}");
        }
    }
}

/// The sim plays the key's turn a card at a time so the journal can name
/// the cards; the game plays it with one command. They are the same turn:
/// the same fight on the same dice lands on the same sheet either way.
#[test]
fn the_sims_auto_turn_is_the_auto_keys_turn() {
    let mut sheet = Sheet::fresh(Uuid::nil(), date(1));
    sheet.level = 9;
    sheet.signal = sheet.max_signal();
    sheet.weapon_tier = 8;
    sheet.armor_tier = 8;
    sheet.cards = vec![
        DRAFTS[0].options[0],
        DRAFTS[1].options[0],
        DRAFTS[2].options[0],
    ];
    sheet.road.static_cards = 2;
    let (mut by_key, mut by_sim) = (sheet.clone(), sheet);
    let (mut key_rng, mut sim_rng) = (StdRng::seed_from_u64(11), StdRng::seed_from_u64(11));
    by_key.engage(&RULES, Pick::Bright, &mut key_rng);
    by_sim.engage(&RULES, Pick::Bright, &mut sim_rng);
    for turn in 0..40 {
        let keyed = by_key.apply(Command::Auto, &mut key_rng).applied;
        let simmed = play_turn(&mut by_sim, &RULES, Hand::Auto, &mut sim_rng).applied;
        assert_eq!(keyed, simmed, "turn {turn}");
        assert_eq!(by_key, by_sim, "turn {turn}");
        if keyed != Applied::Round {
            return;
        }
    }
    panic!("a fight always ends");
}

/// The saver's glass goes only on a day with a hard fight in it: never
/// more glasses than bright steps and gate steps, and at least one for
/// the gate.
#[test]
fn the_saver_spends_a_glass_only_on_a_hard_day() {
    let saver = Player {
        hand: Hand::Auto,
        takes_bright: Some(Threat::Even),
        drinks: Drinks::ForTheHard(Drink::StaticOnIce),
        ..CAREFUL
    };
    let climb = climb(
        saver,
        Start::FRESH,
        3,
        MAX_DAYS,
        &mut Bench::live(),
        &mut Journal::Off,
    );
    let hard_days = climb.ledger.bright_tries + climb.boss_tries;
    assert!(
        climb.ledger.glasses <= hard_days,
        "{} glasses over {hard_days} hard fights",
        climb.ledger.glasses
    );
    assert!(climb.ledger.glasses >= 1, "{climb:?}");
}

/// A crossing the cards end before the turn does: the sleeper goes down
/// to a played card, and the threat word reads it as a win instead of
/// taking the session down with it.
#[test]
fn a_rival_put_down_by_a_card_reads_as_a_sure_crossing() {
    let mut sheet = Sheet::fresh(Uuid::nil(), date(1));
    sheet.level = 9;
    sheet.signal = sheet.max_signal();
    sheet.weapon_tier = 8;
    sheet.armor_tier = 8;
    let prey = Prey {
        rival: Rival {
            user_id: Uuid::from_u128(7),
            username: "tom".to_string(),
            level: 1,
            ambush_id: Uuid::nil(),
            answer: false,
            portrait: [String::new(), String::new(), String::new()],
        },
        signal: 1,
        attack: 0,
        defense: 0,
        carrying: 100,
        stake: 50,
    };
    assert_eq!(odds_rival(&sheet, &prey, 20), Some(1.0));
}

/// The second climb on the key is never slower than the first, and every
/// draft inked never slows it further: a mark and the needle are paid for
/// in the days a drop costs, and the key is the runner who drops
/// (`fight/BALANCE.md`, "The second climb"). The arena holds the bands;
/// this holds the order, so a change that turns either into a tax fails
/// fast.
#[test]
fn a_mark_and_the_inks_never_slow_the_second_climb() {
    let first = marked(&climbs(AMBIENT));
    let second = marked(&climbs_from(AMBIENT, Start { marks: 1, inked: 0 }));
    let inked = marked(&climbs_from(
        AMBIENT,
        Start {
            marks: 1,
            inked: DRAFTS.len(),
        },
    ));
    assert!(
        second <= first && inked <= second,
        "on the key: first climb day {first}, second day {second}, second with every draft inked day {inked}"
    );
}

/// A journal is the climb it was kept of: keeping one changes nothing,
/// it opens on the first dawn and closes on the mark's dusk, every day
/// has its dawn and its dusk, and the bits it saw paid are the ledger's.
#[test]
fn a_journal_tells_the_climb_it_was_kept_of() {
    let quiet = climb(
        KEEN,
        Start::FRESH,
        7,
        MAX_DAYS,
        &mut Bench::live(),
        &mut Journal::Off,
    );
    let mut journal = Journal::On(Vec::new());
    let told = climb(
        KEEN,
        Start::FRESH,
        7,
        MAX_DAYS,
        &mut Bench::live(),
        &mut journal,
    );
    assert_eq!(told, quiet, "a journal only listens");
    let Journal::On(events) = journal else {
        panic!("the journal was on");
    };

    assert!(matches!(events.first(), Some(Event::Dawn { day: 1, .. })));
    let marked = told.marked_on.expect("a keen runner marks");
    assert!(matches!(
        events.last(),
        Some(Event::Dusk { day, .. }) if *day == marked
    ));
    let count = |is: fn(&Event) -> bool| events.iter().filter(|event| is(event)).count();
    assert_eq!(
        count(|event| matches!(event, Event::Dawn { .. })),
        told.days.len()
    );
    assert_eq!(
        count(|event| matches!(event, Event::Dusk { .. })),
        told.days.len()
    );
    assert_eq!(count(|event| matches!(event, Event::Slain { .. })), 1);
    assert_eq!(
        count(|event| matches!(event, Event::Met { .. })) as u32,
        told.fights
    );
    assert_eq!(
        count(|event| matches!(event, Event::Turn { .. })) as u32,
        told.turns
    );
    assert_eq!(
        count(|event| matches!(event, Event::Drafted { .. })),
        DRAFTS.len(),
        "every draft is taken on the way up"
    );
    let mut paid = 0;
    let mut dusks: Vec<Day> = Vec::new();
    for event in &events {
        if let Event::Won { bits, .. } | Event::Cached { bits } = event {
            paid += bits;
        }
        if let Event::Dusk { today, .. } = event {
            dusks.push(*today);
        }
    }
    assert_eq!(paid, told.ledger.earned);
    assert_eq!(dusks, told.days);
}
