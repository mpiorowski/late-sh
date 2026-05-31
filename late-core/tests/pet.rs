use late_core::{models::pet::PetCompanion, test_utils::test_db};

#[tokio::test]
async fn ensure_creates_default_companion_for_new_user() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let user = late_core::test_utils::create_test_user(&test_db.db, "cat-model-new").await;

    let cat = PetCompanion::ensure(&client, user.id)
        .await
        .expect("ensure");

    assert_eq!(cat.user_id, user.id);
    assert_eq!(cat.last_fed, None);
    assert_eq!(cat.last_watered, None);
    assert_eq!(cat.last_played, None);
    assert_eq!(cat.last_groomed, None);
    assert_eq!(cat.last_treated, None);
}

#[tokio::test]
async fn ensure_is_idempotent_and_does_not_reset_care() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let user = late_core::test_utils::create_test_user(&test_db.db, "cat-model-idem").await;

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
async fn touch_actions_record_independent_timestamps() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let user = late_core::test_utils::create_test_user(&test_db.db, "cat-model-touch").await;

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
    PetCompanion::touch_groomed(&client, user.id)
        .await
        .expect("groomed");
    PetCompanion::touch_treated(&client, user.id)
        .await
        .expect("treated");

    let cat = PetCompanion::ensure(&client, user.id)
        .await
        .expect("reload");
    assert!(cat.last_fed.is_some());
    assert!(cat.last_watered.is_some());
    assert!(cat.last_played.is_some());
    assert!(cat.last_groomed.is_some());
    assert!(cat.last_treated.is_some());
}
