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
fn the_stable_climbs_to_mid_game_and_leaves_the_top_to_taming() {
    // The Stable list reads as a ladder: every pet is bought, and each costs
    // more and sits on a higher rung than the one before it.
    let ladder: Vec<(i64, i32)> = PET_SPECIES
        .iter()
        .map(|s| match s.source {
            PetSource::Stable { price, rung } => (price, rung),
            PetSource::Wild { .. } => panic!("{} is on the Stable list but wild", s.name),
        })
        .collect();
    for (w, names) in ladder.windows(2).zip(PET_SPECIES.windows(2)) {
        assert!(
            w[1].0 > w[0].0 && w[1].1 > w[0].1,
            "{} must cost more and rank higher than {}",
            names[1].name,
            names[0].name
        );
    }
    // Gold reaches the middle of the Greenwood, not its end: the second half
    // of the ladder is the taming trade's alone.
    let (_, top_rung) = ladder[ladder.len() - 1];
    assert!(
        top_rung <= 35,
        "the best Stable pet sits at rung {top_rung}"
    );
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

fn keys(kennel: &Kennel) -> Vec<&'static str> {
    kennel.resting().iter().map(|p| p.species.key).collect()
}

#[test]
fn a_tame_never_replaces_the_companion_at_your_heel() {
    use crate::app::door::lateania::taming::TAMEABLE;
    let (hare, hedgehog) = (&TAMEABLE[0], &TAMEABLE[1]);
    let bear = &PET_SPECIES[PET_SPECIES.len() - 1];
    let mut kennel = Kennel::default();
    let mut active = Some(Pet::new(bear, 900));

    // The first hare goes home; the leveled bear stays at the heel.
    assert_eq!(kennel.adopt_tamed(&mut active, hare), Adopted::Kenneled);
    // A second hare adds nothing: the tame only trains the trade.
    assert_eq!(kennel.adopt_tamed(&mut active, hare), Adopted::AlreadyOwned);
    assert_eq!(kennel.adopt_tamed(&mut active, hedgehog), Adopted::Kenneled);
    // Taming the species already at the heel adds nothing either.
    assert_eq!(kennel.adopt_tamed(&mut active, bear), Adopted::AlreadyOwned);

    let heel = active.expect("the bear is still led");
    assert_eq!((heel.species.key, heel.loyalty_xp), (bear.key, 900));
    // Strongest rung first.
    assert_eq!(keys(&kennel), vec![hedgehog.key, hare.key]);
}

#[test]
fn a_tame_with_no_companion_takes_the_heel() {
    use crate::app::door::lateania::taming::TAMEABLE;
    let mut kennel = Kennel::default();
    let mut active = None;
    assert_eq!(kennel.adopt_tamed(&mut active, &TAMEABLE[0]), Adopted::AtHeel);
    assert_eq!(active.map(|p| p.species.key), Some(TAMEABLE[0].key));
    assert!(kennel.resting().is_empty());
}

#[test]
fn buying_sends_the_old_companion_home_and_calling_out_swaps_them_back() {
    let (cat, bear) = (&PET_SPECIES[0], &PET_SPECIES[PET_SPECIES.len() - 1]);
    let mut kennel = Kennel::default();
    let mut active = Some(Pet::new(bear, 900));

    let Bought::AtHeel { sent_home } = kennel.adopt_bought(&mut active, cat) else {
        panic!("a species not owned can be bought");
    };
    assert_eq!(sent_home.map(|s| s.key), Some(bear.key));
    assert_eq!(active.map(|p| p.species.key), Some(cat.key));
    assert!(matches!(kennel.adopt_bought(&mut active, bear), Bought::AlreadyOwned));

    // The bear comes back with its loyalty intact, and the cat goes home.
    let CalledOut::Swapped { called, sent_home } = kennel.call_out(&mut active, bear.key) else {
        panic!("the bear rests in the kennel");
    };
    assert_eq!(called.key, bear.key);
    assert_eq!(sent_home.map(|s| s.key), Some(cat.key));
    let heel = active.expect("the bear is led again");
    assert_eq!((heel.species.key, heel.loyalty_xp), (bear.key, 900));
    assert_eq!(keys(&kennel), vec![cat.key]);
    assert!(matches!(kennel.call_out(&mut active, bear.key), CalledOut::NotKenneled));
}
