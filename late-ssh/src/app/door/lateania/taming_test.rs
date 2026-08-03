use super::*;

#[test]
fn there_are_fifty_tameable_beasts_ordered_small_to_large() {
    // Fifty classic beasts, plus the ten Wildbound rideables at the summit.
    assert_eq!(TAMEABLE_COUNT, 60, "fifty beasts + ten Wildbound mounts");
    // The taming difficulty is non-decreasing across the list (small -> large
    // -> harder and harder), the fifty classic beasts spanning 1..=50 and the
    // Wildbound mounts continuing above them.
    for w in TAMEABLE.windows(2) {
        assert!(
            w[1].tame_level >= w[0].tame_level,
            "tame level must not fall going down the list ({} -> {})",
            w[0].name,
            w[1].name
        );
    }
    assert_eq!(
        TAMEABLE[0].tame_level, 1,
        "the first beast is a novice tame"
    );
    assert_eq!(
        TAMEABLE[TAMEABLE_COUNT - 1].tame_level,
        100,
        "the last beast needs a taming grandmaster (the Wildbound summit)"
    );
    // Every tameable is marked tameable, has a name/glyph, and non-trivial
    // stats that trend up with size.
    for s in TAMEABLE {
        assert!(s.is_tameable(), "{} should be tameable", s.name);
        assert!(s.base_hp > 0 && s.base_attack > 0, "{} has stats", s.name);
    }
    // Bigger beasts are stronger companions: the largest out-muscles the
    // smallest by a wide margin.
    assert!(TAMEABLE[TAMEABLE_COUNT - 1].base_hp > TAMEABLE[0].base_hp * 5);
}

#[test]
fn tameable_keys_are_unique_and_resolve() {
    let mut keys: Vec<&str> = TAMEABLE.iter().map(|s| s.key).collect();
    keys.sort_unstable();
    keys.dedup();
    assert_eq!(keys.len(), TAMEABLE_COUNT, "tameable keys are unique");
    for s in TAMEABLE {
        assert_eq!(tameable_by_key(s.key).map(|x| x.key), Some(s.key));
    }
}

#[test]
fn every_beast_has_a_roaming_spot_in_broceliande() {
    let beasts = wild_beasts();
    assert_eq!(beasts.len(), TAMEABLE_COUNT, "one roaming spot per beast");
    // Every spot points at a real species index, and all fifty species appear.
    let mut seen = std::collections::HashSet::new();
    for b in beasts {
        assert!(b.species < TAMEABLE_COUNT);
        seen.insert(b.species);
    }
    assert_eq!(seen.len(), TAMEABLE_COUNT, "all fifty beasts are placed");
}

#[test]
fn tame_chance_rises_with_surplus_and_refuses_under_level() {
    let beast = &TAMEABLE[TAMEABLE_COUNT - 1]; // needs level 50
    // A novice cannot tame the greatest beast.
    assert_eq!(tame_chance(0, beast), 0);
    // The first beast (level 1) is a coin-toss for a rank beginner and a near
    // sure thing for a trained tamer.
    let easy = &TAMEABLE[0];
    assert_eq!(tame_chance(0, easy), 40, "at exactly the required level");
    let trained = super::super::skills::xp_for_skill_level(10);
    assert!(
        tame_chance(trained, easy) > tame_chance(0, easy),
        "surplus level raises the odds"
    );
    // The chance is capped below certainty.
    let master = super::super::skills::xp_for_skill_level(50);
    assert!(tame_chance(master, easy) <= 95, "never a sure thing");
}

#[test]
fn pet_skills_unlock_on_the_ladder() {
    assert_eq!(pet_skills_at(1).count(), 0, "no skills before level 2");
    assert_eq!(pet_skills_at(2).count(), 1, "savage bite at 2");
    assert_eq!(pet_skills_at(4).count(), 2, "rend at 4");
    assert_eq!(pet_skills_at(6).count(), 3, "roar at 6");
    assert_eq!(pet_skills_at(8).count(), 4, "guard at 8");
    assert_eq!(pet_skills_at(10).count(), PET_SKILLS.len(), "pounce at 10");
    // Every rung is reachable within the pet level cap.
    assert!(
        PET_SKILLS
            .iter()
            .all(|s| s.level <= super::super::pets::PET_MAX_LEVEL),
        "all pet skills unlock at or below PET_MAX_LEVEL"
    );
    // Unlock levels are strictly increasing.
    for w in PET_SKILLS.windows(2) {
        assert!(w[1].level > w[0].level, "pet skill unlocks climb");
    }
}

// Wildbound mounts: ten rideable beasts (wild + mythical), every key a real
// tameable species, strides 2..=5 with the summit stride hitting 5, and the
// mythical fliers gated at the top of the doubled taming ladder.
#[test]
fn ten_rideable_beasts_climb_to_a_stride_of_five() {
    use super::{RIDEABLE, mount_stride, tameable_by_key};
    assert!(RIDEABLE.len() >= 10, "at least ten rideable beasts");
    let mut top = 0;
    for &(key, stride) in RIDEABLE {
        let species = tameable_by_key(key)
            .unwrap_or_else(|| panic!("rideable {key} is not a tameable species"));
        assert!(
            species.tame_level >= 55,
            "{key} should gate in the Wildbound band"
        );
        assert!(
            (2..=5).contains(&stride),
            "{key} stride {stride} out of band"
        );
        top = top.max(stride);
        assert_eq!(mount_stride(key), Some(stride));
    }
    assert_eq!(top, 5, "the best mounts skip five rooms a step");
    // A beast that isn't in the table can't be ridden.
    assert_eq!(mount_stride("wt_hare"), None);
}
