use late_core::{
    models::goldfish::{GoldfishBowl, MAX_FRIENDS},
    test_utils::test_db,
};

#[tokio::test]
async fn ensure_creates_default_bowl_for_new_user() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let user = late_core::test_utils::create_test_user(&test_db.db, "fish-model-new").await;

    let bowl = GoldfishBowl::ensure(&client, user.id)
        .await
        .expect("ensure");

    assert_eq!(bowl.user_id, user.id);
    assert_eq!(bowl.last_fed, None);
    assert_eq!(bowl.last_decorated, None);
    assert_eq!(bowl.last_lit, None);
    assert_eq!(bowl.last_water_changed, None);
    assert_eq!(bowl.friend_count, 0);
}

#[tokio::test]
async fn touch_actions_record_independent_timestamps() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let user = late_core::test_utils::create_test_user(&test_db.db, "fish-model-touch").await;

    GoldfishBowl::ensure(&client, user.id)
        .await
        .expect("ensure");
    GoldfishBowl::touch_fed(&client, user.id)
        .await
        .expect("fed");
    GoldfishBowl::touch_decorated(&client, user.id)
        .await
        .expect("decorated");
    GoldfishBowl::touch_lit(&client, user.id)
        .await
        .expect("lit");
    GoldfishBowl::touch_water_changed(&client, user.id)
        .await
        .expect("water changed");

    let bowl = GoldfishBowl::ensure(&client, user.id)
        .await
        .expect("reload");
    assert!(bowl.last_fed.is_some());
    assert!(bowl.last_decorated.is_some());
    assert!(bowl.last_lit.is_some());
    assert!(bowl.last_water_changed.is_some());
}

#[tokio::test]
async fn add_friend_increments_and_clamps_at_max() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let user = late_core::test_utils::create_test_user(&test_db.db, "fish-model-friends").await;

    GoldfishBowl::ensure(&client, user.id)
        .await
        .expect("ensure");

    // Push well past the cap; the SQL LEAST() must hold the line.
    for _ in 0..(MAX_FRIENDS + 5) {
        GoldfishBowl::add_friend(&client, user.id)
            .await
            .expect("add friend");
    }

    let bowl = GoldfishBowl::ensure(&client, user.id)
        .await
        .expect("reload");
    assert_eq!(
        bowl.friend_count, MAX_FRIENDS,
        "friend_count must never exceed MAX_FRIENDS"
    );
}
