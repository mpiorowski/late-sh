use crate::app::door::lateania::abilities::*;
use crate::app::door::lateania::classes::Class;

#[test]
fn every_class_has_a_level_one_ability() {
    for class in Class::ALL {
        let early = unlocked_for(class, 1);
        assert!(!early.is_empty(), "{:?} has no level-1 ability", class);
    }
}

#[test]
fn ability_ids_are_unique() {
    let mut ids: Vec<u32> = ABILITIES.iter().map(|a| a.id).collect();
    ids.sort_unstable();
    let n = ids.len();
    ids.dedup();
    assert_eq!(n, ids.len(), "duplicate ability id");
}

#[test]
fn every_class_has_a_capstone_at_fifty() {
    for class in Class::ALL {
        let capstone = ABILITIES
            .iter()
            .any(|a| a.class == class && a.level_req == 50);
        assert!(capstone, "{:?} has no level-50 capstone", class);
    }
}

#[test]
fn unlocks_are_monotonic_with_level() {
    for class in Class::ALL {
        let low = unlocked_for(class, 10).len();
        let high = unlocked_for(class, 50).len();
        assert!(high >= low, "{:?} unlocks should not shrink", class);
        assert!(high >= 8, "{:?} should have a deep kit by 50", class);
    }
}
