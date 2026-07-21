use super::*;

#[test]
fn there_are_fifty_tameable_beasts_ordered_small_to_large() {
    assert_eq!(TAMEABLE_COUNT, 50, "fifty tameable beasts");
    // The taming difficulty is non-decreasing across the list (small -> large
    // -> harder and harder), and spans the whole 1..=50 range.
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
        50,
        "the last beast needs a master tamer"
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
    assert_eq!(pet_skills_at(1).count(), 0, "no skills before level 3");
    assert_eq!(pet_skills_at(3).count(), 1, "savage bite at 3");
    assert_eq!(pet_skills_at(8).count(), 2, "rend at 8");
    assert_eq!(pet_skills_at(15).count(), 3, "roar at 15");
    assert_eq!(pet_skills_at(22).count(), 4, "guard at 22");
    assert_eq!(pet_skills_at(30).count(), PET_SKILLS.len(), "pounce at 30");
    // Unlock levels are strictly increasing.
    for w in PET_SKILLS.windows(2) {
        assert!(w[1].level > w[0].level, "pet skill unlocks climb");
    }
}
