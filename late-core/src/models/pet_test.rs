use crate::{
    models::pet::{
        LifeStage, PET_NAME_MAX_CHARS, PetCompanion, PetMood, PetSpecies, normalize_pet_name,
        pet_age_anchor, pet_age_days, pet_age_label,
    },
    test_utils::test_db,
};
use chrono::Utc;

#[test]
fn normalize_trims_and_collapses_whitespace() {
    assert_eq!(
        normalize_pet_name("  Whiskers  ").as_deref(),
        Some("Whiskers")
    );
    assert_eq!(
        normalize_pet_name("Mr   Mittens").as_deref(),
        Some("Mr Mittens")
    );
}

#[test]
fn normalize_caps_length_to_max() {
    let very_long = "a".repeat(200);
    let out = normalize_pet_name(&very_long).expect("non-empty");
    assert_eq!(out.chars().count(), PET_NAME_MAX_CHARS);
}

#[test]
fn normalize_returns_none_for_empty_or_whitespace_only() {
    assert!(normalize_pet_name("").is_none());
    assert!(normalize_pet_name("   ").is_none());
}

#[test]
fn life_stage_buckets() {
    assert_eq!(LifeStage::from_age_days(0), LifeStage::Young);
    assert_eq!(LifeStage::from_age_days(6), LifeStage::Young);
    assert_eq!(LifeStage::from_age_days(7), LifeStage::Junior);
    assert_eq!(LifeStage::from_age_days(29), LifeStage::Junior);
    assert_eq!(LifeStage::from_age_days(30), LifeStage::Adult);
    assert_eq!(LifeStage::from_age_days(179), LifeStage::Adult);
    assert_eq!(LifeStage::from_age_days(180), LifeStage::Senior);
    assert_eq!(LifeStage::from_age_days(10_000), LifeStage::Senior);
}

#[test]
fn life_stage_clamps_negative_days() {
    assert_eq!(LifeStage::from_age_days(-3), LifeStage::Young);
}

#[test]
fn life_stage_label_uses_species() {
    assert_eq!(LifeStage::Young.label(PetSpecies::Cat), "Kitten");
    assert_eq!(LifeStage::Young.label(PetSpecies::Dog), "Puppy");
    assert_eq!(LifeStage::Young.label(PetSpecies::Bird), "Chick");
    assert_eq!(LifeStage::Senior.label(PetSpecies::Cat), "Wise Old Cat");
    assert_eq!(LifeStage::Senior.label(PetSpecies::Dog), "Senior Dog");
    assert_eq!(LifeStage::Senior.label(PetSpecies::Bird), "Old Bird");
}

#[test]
fn species_and_mood_round_trip_through_their_column_strings() {
    for species in PetSpecies::ALL {
        assert_eq!(PetSpecies::parse(species.as_str()), Some(species));
    }
    assert_eq!(PetSpecies::parse("fish"), None);
    // The Shop's `t` key rings through every species and comes back.
    assert_eq!(PetSpecies::Cat.next().next().next(), PetSpecies::Cat);
    for mood in PetMood::ALL {
        assert_eq!(PetMood::parse(mood.as_str()), Some(mood));
    }
    assert_eq!(PetMood::parse("hungry"), None);
}

#[test]
fn pet_age_days_is_zero_for_future_created() {
    use chrono::TimeZone;
    let now = Utc.with_ymd_and_hms(2026, 5, 25, 12, 0, 0).unwrap();
    let future = Utc.with_ymd_and_hms(2026, 5, 26, 12, 0, 0).unwrap();
    assert_eq!(pet_age_days(future, now), 0);
}

#[test]
fn pet_age_anchor_prefers_adoption_timestamp() {
    use chrono::TimeZone;
    let created = Utc.with_ymd_and_hms(2026, 5, 1, 12, 0, 0).unwrap();
    let adopted = Utc.with_ymd_and_hms(2026, 5, 20, 12, 0, 0).unwrap();
    assert_eq!(pet_age_anchor(created, Some(adopted)), adopted);
    assert_eq!(pet_age_anchor(created, None), created);
}

#[test]
fn pet_age_label_formats_typical_durations() {
    use chrono::TimeZone;
    let now = Utc.with_ymd_and_hms(2026, 6, 1, 12, 0, 0).unwrap();
    let cases: &[(i64, &str)] = &[
        (0, "today"),
        (1, "1 day"),
        (3, "3 days"),
        (13, "13 days"),
        (14, "2 weeks"),
        (21, "3 weeks"),
        (30, "1 month"),
        (90, "3 months"),
        (180, "6 months"),
        (365, "1 year"),
        (800, "2 years"),
    ];
    for (days, expected) in cases {
        let created = now - chrono::Duration::days(*days);
        assert_eq!(
            pet_age_label(created, now),
            *expected,
            "wrong label for {days} days ago"
        );
    }
}

#[tokio::test]
async fn ensure_creates_a_sleeping_cat_for_a_new_user() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let user = crate::test_utils::create_test_user(&test_db.db, "cat-model-new").await;

    let pet = PetCompanion::ensure(&client, user.id)
        .await
        .expect("ensure");

    assert_eq!(pet.user_id, user.id);
    assert_eq!(pet.species(), PetSpecies::Cat);
    assert_eq!(pet.mood(), PetMood::Asleep);
    assert_eq!(pet.name, None);
}

#[tokio::test]
async fn ensure_is_idempotent_and_keeps_the_mood() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let user = crate::test_utils::create_test_user(&test_db.db, "cat-model-idem").await;

    let first = PetCompanion::ensure(&client, user.id)
        .await
        .expect("ensure");
    PetCompanion::set_mood(&client, user.id, PetMood::Proud)
        .await
        .expect("set mood");
    let second = PetCompanion::ensure(&client, user.id)
        .await
        .expect("ensure again");

    assert_eq!(first.id, second.id);
    assert_eq!(
        second.mood(),
        PetMood::Proud,
        "re-ensuring must not wipe the inferred mood"
    );
}

#[tokio::test]
async fn the_mood_clock_moves_only_when_the_mood_does() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let user = crate::test_utils::create_test_user(&test_db.db, "cat-model-clock").await;
    PetCompanion::ensure(&client, user.id)
        .await
        .expect("ensure");

    PetCompanion::set_mood(&client, user.id, PetMood::Chatty)
        .await
        .expect("chatty");
    let chatty = PetCompanion::ensure(&client, user.id)
        .await
        .expect("reload");
    PetCompanion::set_mood(&client, user.id, PetMood::Chatty)
        .await
        .expect("chatty again");
    let still_chatty = PetCompanion::ensure(&client, user.id)
        .await
        .expect("reload");
    assert_eq!(still_chatty.mood_since, chatty.mood_since);

    PetCompanion::set_mood(&client, user.id, PetMood::Vibing)
        .await
        .expect("vibing");
    let vibing = PetCompanion::ensure(&client, user.id)
        .await
        .expect("reload");
    assert_eq!(vibing.mood(), PetMood::Vibing);
    assert!(vibing.mood_since > chatty.mood_since);
}

#[tokio::test]
async fn writes_are_scoped_to_the_owner() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let owner = crate::test_utils::create_test_user(&test_db.db, "cat-model-owner").await;
    let other = crate::test_utils::create_test_user(&test_db.db, "cat-model-other").await;

    let owner_pet = PetCompanion::ensure(&client, owner.id)
        .await
        .expect("ensure owner pet");
    let other_pet = PetCompanion::ensure(&client, other.id)
        .await
        .expect("ensure other pet");
    assert_ne!(owner_pet.id, other_pet.id);

    PetCompanion::set_mood(&client, owner.id, PetMood::Sulking)
        .await
        .expect("owner sulks");
    PetCompanion::set_species(&client, owner.id, PetSpecies::Bird)
        .await
        .expect("owner picks the bird");

    let other_after = PetCompanion::ensure(&client, other.id)
        .await
        .expect("reload other pet");
    assert_eq!(other_after.id, other_pet.id);
    assert_eq!(other_after.mood(), PetMood::Asleep);
    assert_eq!(other_after.species(), PetSpecies::Cat);
    let owner_after = PetCompanion::ensure(&client, owner.id)
        .await
        .expect("reload owner pet");
    assert_eq!(owner_after.mood(), PetMood::Sulking);
    assert_eq!(owner_after.species(), PetSpecies::Bird);
}
