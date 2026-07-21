use crate::app::door::lateania::pets::*;

#[test]
fn species_keys_are_unique_and_round_trip() {
    for s in PET_SPECIES {
        assert_eq!(pet_species_by_key(s.key).map(|x| x.key), Some(s.key));
    }
    let mut keys: Vec<&str> = PET_SPECIES.iter().map(|s| s.key).collect();
    keys.sort_unstable();
    keys.dedup();
    assert_eq!(keys.len(), PET_SPECIES.len(), "species keys are unique");
}

#[test]
fn feeding_grows_loyalty_health_and_attack() {
    let species = pet_species_by_key("war_hound").unwrap();
    let mut pet = Pet::new(species, 0);
    assert_eq!(pet.level(), 1);
    let hp1 = pet.max_hp();
    let atk1 = pet.attack();
    // Four feedings = LOYALTY_PER_LEVEL of loyalty = one level.
    for _ in 0..(LOYALTY_PER_LEVEL / FEED_LOYALTY) {
        pet.feed();
    }
    assert_eq!(pet.level(), 2, "a full bar of loyalty levels the pet");
    assert!(pet.max_hp() > hp1, "leveling raises max HP");
    assert!(pet.attack() > atk1, "leveling raises attack");
    assert_eq!(pet.hp, pet.max_hp(), "feeding heals to full");
}

#[test]
fn level_and_health_are_capped() {
    let species = pet_species_by_key("emberdrake").unwrap();
    let pet = Pet::new(species, LOYALTY_PER_LEVEL * 1000);
    assert_eq!(pet.level(), PET_MAX_LEVEL);
    assert_eq!(pet.loyalty_pct(), 100);
}
