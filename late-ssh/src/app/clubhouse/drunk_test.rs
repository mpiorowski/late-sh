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

    // A drink from hours ago has partially worn off.
    drunk.record_drink(id, 1_500, now - chrono::Duration::hours(2));
    assert_eq!(
        drunk.levels(now).get(&id),
        Some(&2),
        "2h decay of 1500 points should read buzzed"
    );

    // Fully sober entries drop out of the chat-facing map entirely.
    drunk.record_drink(id, 100, now - chrono::Duration::hours(10));
    assert!(drunk.levels(now).is_empty());

    // A seed pass replaces everything.
    drunk.record_drink(id, 2_000, now);
    drunk.set_drunk_states(Vec::new());
    assert!(drunk.levels(now).is_empty());
}
