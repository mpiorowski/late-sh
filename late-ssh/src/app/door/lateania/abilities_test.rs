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

// Wildbound: ten new skills per class in the 51..=100 band, unique ids, sane
// numbers, and every one castable with the class's own resource.
#[test]
fn wildbound_gives_every_class_ten_skills_past_fifty() {
    use std::collections::HashSet;
    let mut ids = HashSet::new();
    for a in ABILITIES {
        assert!(ids.insert(a.id), "duplicate ability id {}", a.id);
        // Wildbound entries (ids 3000+) always carry a real magnitude; a few
        // older utility abilities are legitimately magnitude-0 stuns.
        if a.id >= 3000 {
            assert!(a.magnitude > 0, "{} has a non-positive magnitude", a.name);
        }
        assert!(a.cost > 0, "{} costs nothing", a.name);
        assert_eq!(
            a.resource,
            a.class.resource(),
            "{} is paid in the wrong resource for {:?}",
            a.name,
            a.class
        );
    }
    for class in Class::ALL {
        let new_band: Vec<_> = ABILITIES
            .iter()
            .filter(|a| a.class == class && (51..=100).contains(&a.level_req))
            .collect();
        assert!(
            new_band.len() >= 10,
            "{class:?} has only {} Wildbound skills (wanted 10+)",
            new_band.len()
        );
        // The climb should hand out something new all the way up.
        assert!(
            new_band.iter().any(|a| a.level_req == 100),
            "{class:?} has no capstone at 100"
        );
    }
}

#[test]
fn the_summit_is_level_one_hundred() {
    assert_eq!(Class::MAX_LEVEL, 100);
    // The xp curve keeps growing monotonically to the new cap.
    let mut prev = crate::app::door::lateania::classes::xp_for_level(1);
    for level in 2..=Class::MAX_LEVEL {
        let xp = crate::app::door::lateania::classes::xp_for_level(level);
        assert!(xp > prev, "xp curve dips at level {level}");
        prev = xp;
    }
    // Stats keep climbing too.
    for class in Class::ALL {
        let a = class.stats_at(50);
        let b = class.stats_at(100);
        assert!(
            b.max_hp > a.max_hp && b.attack > a.attack,
            "{class:?} stops growing"
        );
    }
}
