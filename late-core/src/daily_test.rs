use std::collections::HashMap;

use chrono::{Duration, NaiveDate};

use super::daily::{choose_for_date, fnv1a, splitmix64};

/// Every daily board is rebuilt from these hashes, so they are pinned to the
/// published FNV-1a and splitmix64 reference values.
#[test]
fn the_hashes_match_their_reference_values() {
    assert_eq!(
        [fnv1a(""), fnv1a("a"), fnv1a("foobar")],
        [
            0xcbf2_9ce4_8422_2325,
            0xaf63_dc4c_8601_ec8c,
            0x8594_4171_f739_67e8
        ]
    );
    let first = splitmix64(0);
    let second = splitmix64(0x9e37_79b9_7f4a_7c15);
    assert_eq!(
        [first, second],
        [0xe220_a839_7b1d_cdaf, 0x6e78_9e6a_a1b9_65f4]
    );
}

/// Once every entry has had a day, the oldest claim comes round, and two
/// entries last claimed the same day go in key order.
#[test]
fn a_fully_used_pack_repeats_the_oldest_claim_lowest_key_first() {
    let pack = [3, 1, 2];
    let day = NaiveDate::from_ymd_opt(2026, 10, 28).expect("valid date");
    let last_used = HashMap::from([
        (1, day),
        (2, day - Duration::days(1)),
        (3, day - Duration::days(1)),
    ]);

    assert_eq!(
        choose_for_date(&pack, |id| id, &last_used, "seed"),
        Some(&2)
    );
    assert_eq!(
        choose_for_date(&[] as &[i32], |id| id, &HashMap::new(), "seed"),
        None
    );
}
