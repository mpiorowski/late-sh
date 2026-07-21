use crate::{
    models::pet::{
        LifeStage, PET_NAME_MAX_CHARS, PET_SPECIES_CAT, PET_SPECIES_DOG, PetCompanion,
        normalize_pet_name, pet_age_anchor, pet_age_days, pet_age_label,
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
    assert_eq!(LifeStage::Young.label(PET_SPECIES_CAT), "Kitten");
    assert_eq!(LifeStage::Young.label(PET_SPECIES_DOG), "Puppy");
    assert_eq!(LifeStage::Senior.label(PET_SPECIES_CAT), "Wise Old Cat");
    assert_eq!(LifeStage::Senior.label(PET_SPECIES_DOG), "Senior Dog");
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
async fn ensure_creates_default_companion_for_new_user() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let user = crate::test_utils::create_test_user(&test_db.db, "cat-model-new").await;

    let cat = PetCompanion::ensure(&client, user.id)
        .await
        .expect("ensure");

    assert_eq!(cat.user_id, user.id);
    assert_eq!(cat.last_fed, None);
    assert_eq!(cat.last_watered, None);
    assert_eq!(cat.last_played, None);
    assert_eq!(cat.last_treated, None);
}

#[tokio::test]
async fn ensure_is_idempotent_and_does_not_reset_care() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let user = crate::test_utils::create_test_user(&test_db.db, "cat-model-idem").await;

    let first = PetCompanion::ensure(&client, user.id)
        .await
        .expect("ensure");
    PetCompanion::touch_fed(&client, user.id)
        .await
        .expect("touch fed");
    let second = PetCompanion::ensure(&client, user.id)
        .await
        .expect("ensure again");

    assert_eq!(first.id, second.id);
    assert!(
        second.last_fed.is_some(),
        "re-ensuring must not wipe an existing feed timestamp"
    );
}

#[tokio::test]
async fn care_touches_are_scoped_to_the_owner() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let owner = crate::test_utils::create_test_user(&test_db.db, "cat-model-owner").await;
    let other = crate::test_utils::create_test_user(&test_db.db, "cat-model-other").await;

    let owner_cat = PetCompanion::ensure(&client, owner.id)
        .await
        .expect("ensure owner cat");
    let other_cat = PetCompanion::ensure(&client, other.id)
        .await
        .expect("ensure other cat");
    assert_ne!(owner_cat.id, other_cat.id);

    PetCompanion::touch_fed(&client, owner.id)
        .await
        .expect("feed owner cat");

    let other_after = PetCompanion::ensure(&client, other.id)
        .await
        .expect("reload other cat");
    assert_eq!(other_after.id, other_cat.id);
    assert_eq!(
        other_after.last_fed, None,
        "feeding one user's cat must not touch another user's row"
    );
}

#[tokio::test]
async fn touch_actions_record_independent_timestamps() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let user = crate::test_utils::create_test_user(&test_db.db, "cat-model-touch").await;

    PetCompanion::ensure(&client, user.id)
        .await
        .expect("ensure");
    PetCompanion::touch_fed(&client, user.id)
        .await
        .expect("fed");
    PetCompanion::touch_watered(&client, user.id)
        .await
        .expect("watered");
    PetCompanion::touch_played(&client, user.id)
        .await
        .expect("played");
    PetCompanion::touch_treated(&client, user.id)
        .await
        .expect("treated");

    let cat = PetCompanion::ensure(&client, user.id)
        .await
        .expect("reload");
    assert!(cat.last_fed.is_some());
    assert!(cat.last_watered.is_some());
    assert!(cat.last_played.is_some());
    assert!(cat.last_treated.is_some());
}
