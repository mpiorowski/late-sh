use super::super::pets::PET_SPECIES;
use super::*;

#[test]
fn there_are_sixty_tameable_beasts_ordered_small_to_large() {
    // Fifty classic beasts, plus the ten Wildbound beasts at the summit.
    assert_eq!(TAMEABLE_COUNT, 60, "fifty beasts + ten Wildbound beasts");
    // The taming difficulty is non-decreasing across the list (small -> large
    // -> harder and harder), the fifty classic beasts spanning 1..=54 and the
    // Wildbound beasts continuing above them.
    for w in TAMEABLE.windows(2) {
        assert!(
            w[1].tame_level() >= w[0].tame_level(),
            "tame level must not fall going down the list ({} -> {})",
            w[0].name,
            w[1].name
        );
    }
    assert_eq!(
        TAMEABLE[0].tame_level(),
        1,
        "the first beast is a novice tame"
    );
    assert_eq!(
        TAMEABLE[TAMEABLE_COUNT - 1].tame_level(),
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
fn no_companion_is_out_classed_by_an_easier_one() {
    // Every companion sits on one ladder: a wild beast at the Animal Taming
    // level it needs, a Stable pet at the rung it is priced to match. Climbing
    // that ladder must never hand you a pet you would not want. Pets on the
    // same rung may trade attack for bulk (a hitter vs. a wall is a real
    // choice), so the invariant is Pareto, not monotonic: no pet may be beaten
    // on *both* axes by one at the same or a lower rung.
    //
    // "The same rung" is what keeps a tier free of dead beasts: ten of them
    // once shared taming 50 and eight lost outright to the Scion or the Green
    // Wyrm beside them. Against the Stable it cuts both ways: a bought pet is
    // never worse than the easier wild beasts, and never beats a harder one.
    let pool: Vec<&PetSpecies> = TAMEABLE
        .iter()
        .chain(AELUNOR_TAMEABLE)
        .chain(PET_SPECIES)
        .collect();
    for b in &pool {
        if let Some(better) = pool.iter().find(|c| {
            c.key != b.key
                && c.rung() <= b.rung()
                && c.base_attack >= b.base_attack
                && c.base_hp >= b.base_hp
                && (c.base_attack > b.base_attack || c.base_hp > b.base_hp)
        }) {
            panic!(
                "{} (rung {}, attack {}, hp {}) is out-classed by {} at rung {} \
                 (attack {}, hp {}) - nobody should ever pick it",
                b.name,
                b.rung(),
                b.base_attack,
                b.base_hp,
                better.name,
                better.rung(),
                better.base_attack,
                better.base_hp,
            );
        }
    }
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
    assert_eq!(
        beasts.len(),
        TAMEABLE_COUNT + AELUNOR_TAMEABLE.len(),
        "one roaming spot per beast, Broceliande's fifty-five plus Aelunor's five"
    );
    // Every spot points at a real species index (resolved via `beast_species`,
    // which covers both pools), and every species in both pools appears.
    let mut seen = std::collections::HashSet::new();
    for b in beasts {
        assert!(b.species < TAMEABLE_COUNT + AELUNOR_TAMEABLE.len());
        seen.insert(b.species);
    }
    assert_eq!(
        seen.len(),
        TAMEABLE_COUNT + AELUNOR_TAMEABLE.len(),
        "every beast in both pools is placed"
    );
}

#[test]
fn tame_chance_rises_with_surplus_and_refuses_under_level() {
    let beast = &TAMEABLE[TAMEABLE_COUNT - 1]; // needs level 50
    // A novice cannot tame the greatest beast.
    assert_eq!(tame_chance(0, beast, 0), 0);
    // The first beast (level 1) is a coin-toss for a rank beginner and a near
    // sure thing for a trained tamer.
    let easy = &TAMEABLE[0];
    assert_eq!(tame_chance(0, easy, 0), 40, "at exactly the required level");
    assert_eq!(
        tame_chance(0, easy, 6),
        46,
        "charisma adds its percent points"
    );
    assert_eq!(tame_chance(0, easy, -6), 34, "and takes them away");
    let trained = super::super::skills::xp_for_skill_level(10);
    assert!(
        tame_chance(trained, easy, 0) > tame_chance(0, easy, 0),
        "surplus level raises the odds"
    );
    // The chance is capped below certainty.
    let master = super::super::skills::xp_for_skill_level(50);
    assert!(tame_chance(master, easy, 12) <= 95, "never a sure thing");
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
