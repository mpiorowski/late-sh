use crate::app::door::lateania::skills::*;

#[test]
fn keys_round_trip_and_are_unique() {
    let mut seen = std::collections::HashSet::new();
    for s in GatherSkill::ALL {
        assert!(seen.insert(s.key()), "duplicate skill key {}", s.key());
        assert_eq!(GatherSkill::from_key(s.key()), Some(s));
    }
    assert_eq!(GatherSkill::from_key("nonsense"), None);
}

#[test]
fn indices_are_unique_and_dense() {
    let mut idx: Vec<u32> = GatherSkill::ALL.iter().map(|s| s.index()).collect();
    idx.sort_unstable();
    assert_eq!(idx, vec![0, 1, 2, 3, 4]);
    let mut cidx: Vec<u32> = CraftSkill::ALL.iter().map(|s| s.index()).collect();
    cidx.sort_unstable();
    assert_eq!(cidx, vec![0, 1, 2, 3, 4]);
}

#[test]
fn craft_keys_round_trip_and_are_unique() {
    let mut seen = std::collections::HashSet::new();
    for s in CraftSkill::ALL {
        assert!(seen.insert(s.key()), "duplicate craft key {}", s.key());
        assert_eq!(CraftSkill::from_key(s.key()), Some(s));
    }
    assert_eq!(CraftSkill::from_key("nonsense"), None);
}

#[test]
fn level_one_is_free_and_curve_is_strictly_increasing() {
    assert_eq!(xp_for_skill_level(1), 0);
    assert_eq!(xp_for_skill_level(0), 0);
    for level in 2..=SKILL_MAX_LEVEL {
        assert!(
            xp_for_skill_level(level) > xp_for_skill_level(level - 1),
            "curve must rise at level {level}"
        );
    }
}

#[test]
fn curve_steepens_late() {
    // The cost of a mid-game level must exceed the cost of an early one, and
    // a late level must cost far more still (the "harder and harder" shape).
    let early = xp_for_skill_level(5) - xp_for_skill_level(4);
    let mid = xp_for_skill_level(20) - xp_for_skill_level(19);
    let late = xp_for_skill_level(50) - xp_for_skill_level(49);
    assert!(mid > early);
    assert!(late > mid * 3);
}

#[test]
fn level_for_xp_inverts_the_curve_and_caps() {
    for level in 1..=SKILL_MAX_LEVEL {
        assert_eq!(skill_level_for_xp(xp_for_skill_level(level)), level);
    }
    // One short of a threshold stays on the lower level.
    assert_eq!(
        skill_level_for_xp(xp_for_skill_level(10) - 1),
        9,
        "just under the level-10 threshold is still level 9"
    );
    // Absurd xp still caps at the max level.
    assert_eq!(skill_level_for_xp(i64::MAX / 2), SKILL_MAX_LEVEL);
}

#[test]
fn progress_stays_within_the_level_band() {
    let xp = xp_for_skill_level(7) + 5;
    let (into, need) = skill_progress(xp);
    assert_eq!(into, 5);
    assert_eq!(need, xp_for_skill_level(8) - xp_for_skill_level(7));
    // At the cap there is no "next".
    assert_eq!(skill_progress(xp_for_skill_level(SKILL_MAX_LEVEL)), (0, 0));
}
