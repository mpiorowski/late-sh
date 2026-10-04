use super::{Bench, CAREFUL, Climb, NEGLECTFUL, Player, RECKLESS, climb, summary};

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
