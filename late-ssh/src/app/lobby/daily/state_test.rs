use super::*;
use chrono::TimeZone;

#[test]
fn format_deadline_scales_units() {
    let now = Utc.with_ymd_and_hms(2026, 7, 8, 12, 0, 0).unwrap();
    assert_eq!(
        format_deadline(now + chrono::Duration::hours(50), now),
        "2d 2h"
    );
    assert_eq!(
        format_deadline(now + chrono::Duration::minutes(90), now),
        "1h 30m"
    );
    assert_eq!(
        format_deadline(now + chrono::Duration::minutes(41), now),
        "41m"
    );
    assert_eq!(format_deadline(now - chrono::Duration::hours(1), now), "0m");
}

#[test]
fn fresh_turn_edges_notifies_each_became_my_turn_edge_once() {
    let a = Uuid::from_u128(1);
    let b = Uuid::from_u128(2);
    let mut notified = HashSet::from([a]);

    // Already-notified id stays quiet; a new my-turn match is an edge.
    assert_eq!(fresh_turn_edges(&mut notified, &[a, b]), vec![b]);
    assert_eq!(fresh_turn_edges(&mut notified, &[a, b]), Vec::<Uuid>::new());

    // Turn passes to the opponent and comes back: a fresh edge.
    assert_eq!(fresh_turn_edges(&mut notified, &[b]), Vec::<Uuid>::new());
    assert_eq!(fresh_turn_edges(&mut notified, &[a, b]), vec![a]);

    // Finished matches fall out of the set.
    assert_eq!(fresh_turn_edges(&mut notified, &[]), Vec::<Uuid>::new());
    assert!(notified.is_empty());
}
