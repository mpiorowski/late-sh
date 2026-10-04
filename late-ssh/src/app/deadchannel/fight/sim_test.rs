use super::{Bench, CAREFUL, Climb, KEEN, Ledger, NEGLECTFUL, Player, RECKLESS, climb, summary};

/// Seeded climbs per player: enough for a stable median, few enough to
/// stay a unit test.
const SEEDS: u64 = 40;
/// Far past the target, so a slow ladder shows where it stalls instead
/// of vanishing into "never".
const MAX_DAYS: u32 = 120;

/// The balance target (GAME.md, "The daily ration loop"): a runner who
/// never drops puts the Old Signal down in about three weeks, one who
/// drops a few times in about four. Days are medians over the seeds.
const CAREFUL_MARK_DAYS: std::ops::RangeInclusive<u32> = 18..=24;
const RECKLESS_MARK_DAYS: std::ops::RangeInclusive<u32> = 25..=31;

fn climbs(player: Player) -> Vec<Climb> {
    let mut bench = Bench::live();
    (0..SEEDS)
        .map(|seed| climb(player, seed, MAX_DAYS, &mut bench))
        .collect()
}

fn marked(climbs: &[Climb]) -> u32 {
    super::median(climbs.iter().map(|climb| climb.marked_on), MAX_DAYS)
}

/// A change to the curves, the glyph tiers, the Old Signal, or the fight
/// rules moves this. Read the ladder in the message before retuning the
/// target: the target is the design, the numbers are what to fix.
#[test]
fn a_careful_runner_marks_in_three_weeks() {
    let climbs = climbs(CAREFUL);
    let day = marked(&climbs);
    assert!(
        CAREFUL_MARK_DAYS.contains(&day),
        "a careful runner's first mark lands on day {day}, target {CAREFUL_MARK_DAYS:?}\n{}",
        summary(&climbs, MAX_DAYS)
    );
}

#[test]
fn a_reckless_runner_marks_in_four_weeks_and_drops_a_few_times() {
    let climbs = climbs(RECKLESS);
    let day = marked(&climbs);
    let deaths = super::median(climbs.iter().map(|climb| Some(climb.deaths)), MAX_DAYS);
    assert!(
        RECKLESS_MARK_DAYS.contains(&day),
        "a reckless runner's first mark lands on day {day}, target {RECKLESS_MARK_DAYS:?}\n{}",
        summary(&climbs, MAX_DAYS)
    );
    assert!(
        (2..=8).contains(&deaths),
        "a reckless runner drops {deaths} times on the way up, a few expected\n{}",
        summary(&climbs, MAX_DAYS)
    );
}

/// The careful runner is the faster one: the safe rule must never lose
/// to the reckless one, or the game rewards not caring.
#[test]
fn care_is_the_faster_climb() {
    let careful = marked(&climbs(CAREFUL));
    let reckless = marked(&climbs(RECKLESS));
    assert!(
        careful <= reckless,
        "careful marks on day {careful}, reckless on day {reckless}"
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
/// most rules (the bright glyph, the glass, the blade shop, the purse). When it
/// moves, read which fields moved and why before re-blessing.
#[test]
fn one_keen_climb_holds_still() {
    let climb = climb(KEEN, 7, MAX_DAYS, &mut Bench::live());
    assert_eq!(
        climb,
        Climb {
            level_on: [
                None,
                Some(1),
                Some(1),
                Some(1),
                Some(2),
                Some(3),
                Some(4),
                Some(5),
                Some(6),
                Some(7),
                Some(8),
                Some(9),
                Some(10),
                Some(12),
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
                Some((4, 4)),
                Some((6, 5)),
                Some((7, 6)),
                Some((8, 7)),
                Some((9, 8)),
                Some((11, 10)),
                Some((12, 12)),
                Some((13, 13)),
                Some((14, 14)),
                Some((15, 15)),
                Some((15, 15)),
                Some((15, 15)),
            ],
            ledger: Ledger {
                earned: 155_886,
                gear: 79_236,
                patched: 24_783,
                dropped: 0,
                crystals_found: 26,
                crystals_spent: 26,
                bright_tries: 30,
                bright_kills: 16,
            },
        }
    );
}
