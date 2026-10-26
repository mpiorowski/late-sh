use chrono::Utc;
use uuid::Uuid;

use super::DrunkMap;

#[test]
fn drunk_levels_decay_and_prune() {
    let drunk = DrunkMap::new();
    let id = Uuid::from_u128(1);
    let now = Utc::now();

    drunk.record_drink(id, 1_500, now);
    assert_eq!(drunk.levels(now).get(&id), Some(&3));

    // A drink from an hour ago has partially worn off.
    drunk.record_drink(id, 1_500, now - chrono::Duration::hours(1));
    assert_eq!(
        drunk.levels(now).get(&id),
        Some(&2),
        "1h decay of 1500 points should read buzzed"
    );

    // Fully sober entries drop out of the chat-facing map entirely.
    drunk.record_drink(id, 100, now - chrono::Duration::hours(10));
    assert!(drunk.levels(now).is_empty());

    // A seed pass taken after the pour replaces everything.
    drunk.record_drink(id, 2_000, now);
    drunk.set_drunk_states(now + chrono::Duration::seconds(1), Vec::new());
    assert!(drunk.levels(now).is_empty());
}

/// The seed task reads the table, then replaces the map. A pour that lands
/// on this replica between the two is newer than the snapshot and must
/// survive it, whether the snapshot carries an older row for the drinker
/// or none at all (their first drink); otherwise the Nightcap's cutoff
/// reopens for a patron who just hit the ceiling.
#[test]
fn a_pour_newer_than_the_seed_snapshot_survives_the_seed() {
    let drunk = DrunkMap::new();
    let topped_up = Uuid::from_u128(1);
    let first_timer = Uuid::from_u128(2);
    let sobered = Uuid::from_u128(3);
    let seeded_at = Utc::now();
    let before = seeded_at - chrono::Duration::seconds(5);
    let after = seeded_at + chrono::Duration::seconds(1);

    drunk.record_drink(topped_up, 2_000, after);
    drunk.record_drink(first_timer, 500, after);
    drunk.record_drink(sobered, 2_000, before);
    drunk.set_drunk_states(seeded_at, vec![(topped_up, 1_000, before)]);

    assert_eq!(
        drunk.level(topped_up, after),
        4,
        "the fresh pour outranks the seed's row"
    );
    assert_eq!(
        drunk.level(first_timer, after),
        2,
        "a first pour has no row to lose to"
    );
    assert_eq!(
        drunk.level(sobered, after),
        0,
        "a stale entry the table dropped is pruned"
    );
}

#[test]
fn one_drinkers_level_reads_the_same_as_the_map() {
    let drunk = DrunkMap::new();
    let id = Uuid::from_u128(1);
    let now = Utc::now();

    assert_eq!(drunk.level(id, now), 0, "never seen reads sober");
    drunk.record_drink(id, 2_000, now);
    assert_eq!(drunk.level(id, now), 4);
    assert_eq!(drunk.levels(now).get(&id), Some(&4));
}
