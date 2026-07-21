use super::*;

#[test]
fn fifty_levels_are_reachable_and_capped() {
    // Enough xp for any conceivable grind still caps at MAX_LEVEL.
    assert_eq!(level_for_xp(i64::MAX / 2), Class::MAX_LEVEL);
    assert_eq!(level_for_xp(0), 1);
}

#[test]
fn xp_curve_is_strictly_increasing() {
    for l in 2..=Class::MAX_LEVEL {
        assert!(
            xp_for_level(l) > xp_for_level(l - 1),
            "xp curve must rise at level {l}"
        );
    }
}

#[test]
fn xp_curve_slows_after_early_story_levels() {
    assert_eq!(xp_for_level(8), 25 * 7 * 7 + (15 * 7 * 7 * 7) / 10);
    assert!(xp_for_level(15) > 22_000);
    assert!(xp_for_level(30) > 240_000);
    assert!(xp_for_level(50) > 1_200_000);
}

#[test]
fn level_and_xp_round_trip() {
    for l in 1..=Class::MAX_LEVEL {
        let xp = xp_for_level(l);
        assert_eq!(level_for_xp(xp), l, "xp boundary for level {l}");
    }
}

#[test]
fn every_class_grows_hp_to_fifty() {
    for class in Class::ALL {
        let lo = class.stats_at(1).max_hp;
        let hi = class.stats_at(50).max_hp;
        assert!(hi > lo * 3, "{:?} should grow substantially by 50", class);
    }
}

#[test]
fn all_classes_round_trip_their_persistence_key_and_are_distinct() {
    assert_eq!(Class::ALL.len(), 17, "seventeen classes now");
    let mut keys = std::collections::HashSet::new();
    let mut names = std::collections::HashSet::new();
    for class in Class::ALL {
        // Stable persistence key survives a round trip.
        assert_eq!(Class::from_key(class.as_key()), Some(class));
        assert!(keys.insert(class.as_key()), "duplicate class key");
        assert!(names.insert(class.name()), "duplicate class name");
        // Every class has a non-empty tagline/description and a usable resource.
        assert!(!class.tagline().is_empty());
        assert!(!class.trait_name().is_empty());
        assert!(class.stats_at(1).max_resource > 0, "{:?}", class);
    }
    // The two newcomers landed with their intended identities.
    assert_eq!(Class::Druid.resource(), Resource::Spirit);
    assert_eq!(Class::Necromancer.resource(), Resource::Souls);
    assert_eq!(Class::from_key("druid"), Some(Class::Druid));
    assert_eq!(Class::from_key("necromancer"), Some(Class::Necromancer));
}

#[test]
fn the_five_newcomers_are_fully_wired() {
    let newcomers = [
        (Class::Beastlord, "beastlord", Resource::Spirit),
        (Class::Skald, "skald", Resource::Tempo),
        (Class::Runemaster, "runemaster", Resource::Mana),
        (Class::Valewalker, "valewalker", Resource::Focus),
        (Class::Spiritmaster, "spiritmaster", Resource::Souls),
    ];
    for (class, key, resource) in newcomers {
        assert_eq!(class.as_key(), key);
        assert_eq!(Class::from_key(key), Some(class));
        assert_eq!(class.resource(), resource);
        assert!(!class.tagline().is_empty());
        assert!(!class.description().is_empty());
        assert!(!class.trait_name().is_empty());
        assert!(!class.trait_desc().is_empty());
        // Each newcomer offers exactly two archetype paths at ARCHETYPE_LEVEL.
        let paths = archetypes_for(class);
        assert_eq!(paths.len(), 2, "{class:?} needs two archetypes");
        for path in paths {
            assert_eq!(archetype_by_key(path.key).map(|a| a.key), Some(path.key));
        }
    }
}

#[test]
fn archetype_keys_are_globally_unique() {
    let mut keys = std::collections::HashSet::new();
    for a in ARCHETYPES {
        assert!(keys.insert(a.key), "duplicate archetype key {}", a.key);
    }
    // Every class has exactly two paths.
    for class in Class::ALL {
        assert_eq!(archetypes_for(class).len(), 2, "{class:?}");
    }
}

#[test]
fn milestones_land_every_five_levels_and_no_level_is_dead() {
    assert!(level_milestone(4).is_none());
    assert_eq!(level_milestone(5), Some("Blooded"));
    assert!(level_milestone(7).is_none());
    assert_eq!(level_milestone(50), Some("Ascended"));
    assert_eq!(milestone_hp_bonus(4), 0);
    assert_eq!(milestone_hp_bonus(5), 5);
    assert_eq!(milestone_hp_bonus(50), 50);
    assert_eq!(current_milestone(23), Some("Veteran"));
    assert!(current_milestone(4).is_none());
    // Every level for every class either grows a stat or is a milestone -
    // there are no dead levels.
    for c in Class::ALL {
        for l in 2..=Class::MAX_LEVEL {
            let cur = c.stats_at(l);
            let prev = c.stats_at(l - 1);
            let grew = cur.max_hp > prev.max_hp
                || cur.attack > prev.attack
                || cur.max_resource > prev.max_resource;
            assert!(
                grew || level_milestone(l).is_some(),
                "{c:?} level {l} grants nothing"
            );
        }
    }
}
