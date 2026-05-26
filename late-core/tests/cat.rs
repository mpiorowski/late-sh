use chrono::Utc;
use late_core::{models::cat::CatCompanion, test_utils::test_db};

#[tokio::test]
async fn ensure_creates_default_companion_for_new_user() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let user = late_core::test_utils::create_test_user(&test_db.db, "cat-model-new").await;

    let cat = CatCompanion::ensure(&client, user.id)
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

    let first = CatCompanion::ensure(&client, user.id)
        .await
        .expect("ensure");
    CatCompanion::touch_fed(&client, user.id)
        .await
        .expect("touch fed");
    let second = CatCompanion::ensure(&client, user.id)
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

    CatCompanion::ensure(&client, user.id)
        .await
        .expect("ensure");
    CatCompanion::touch_fed(&client, user.id)
        .await
        .expect("fed");
    CatCompanion::touch_watered(&client, user.id)
        .await
        .expect("watered");
    CatCompanion::touch_played(&client, user.id)
        .await
        .expect("played");
    CatCompanion::touch_groomed(&client, user.id)
        .await
        .expect("groomed");
    CatCompanion::touch_treated(&client, user.id)
        .await
        .expect("treated");

    let cat = CatCompanion::ensure(&client, user.id)
        .await
        .expect("reload");
    assert!(cat.last_fed.is_some());
    assert!(cat.last_watered.is_some());
    assert!(cat.last_played.is_some());
    assert!(cat.last_groomed.is_some());
    assert!(cat.last_treated.is_some());
}

#[tokio::test]
async fn new_companion_starts_with_zero_streak() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let user = late_core::test_utils::create_test_user(&test_db.db, "cat-streak-zero").await;

    let cat = CatCompanion::ensure(&client, user.id)
        .await
        .expect("ensure");

    assert_eq!(cat.care_streak_days, 0);
    assert_eq!(cat.care_streak_last_day, None);
}

#[tokio::test]
async fn first_care_action_sets_streak_to_one() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let user = late_core::test_utils::create_test_user(&test_db.db, "cat-streak-first").await;

    CatCompanion::ensure(&client, user.id).await.expect("ensure");
    CatCompanion::touch_fed(&client, user.id).await.expect("fed");

    let cat = CatCompanion::ensure(&client, user.id).await.expect("reload");
    assert_eq!(cat.care_streak_days, 1);
    assert_eq!(cat.care_streak_last_day, Some(Utc::now().date_naive()));
}

#[tokio::test]
async fn multiple_care_actions_same_day_keep_streak_at_one() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let user = late_core::test_utils::create_test_user(&test_db.db, "cat-streak-same-day").await;

    CatCompanion::ensure(&client, user.id).await.expect("ensure");
    CatCompanion::touch_fed(&client, user.id).await.expect("fed");
    CatCompanion::touch_watered(&client, user.id).await.expect("watered");
    CatCompanion::touch_played(&client, user.id).await.expect("played");

    let cat = CatCompanion::ensure(&client, user.id).await.expect("reload");
    assert_eq!(
        cat.care_streak_days, 1,
        "the streak counts days, not actions — same-day care must not advance it"
    );
}

#[tokio::test]
async fn care_after_yesterday_extends_streak() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let user = late_core::test_utils::create_test_user(&test_db.db, "cat-streak-extend").await;

    CatCompanion::ensure(&client, user.id).await.expect("ensure");
    client
        .execute(
            "UPDATE cat_companions SET care_streak_days = 4, care_streak_last_day = current_date - 1 WHERE user_id = $1",
            &[&user.id],
        )
        .await
        .expect("seed streak at 4 ending yesterday");

    CatCompanion::touch_fed(&client, user.id).await.expect("fed today");

    let cat = CatCompanion::ensure(&client, user.id).await.expect("reload");
    assert_eq!(cat.care_streak_days, 5);
    assert_eq!(cat.care_streak_last_day, Some(Utc::now().date_naive()));
}

#[tokio::test]
async fn care_after_a_gap_resets_streak_to_one() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let user = late_core::test_utils::create_test_user(&test_db.db, "cat-streak-reset").await;

    CatCompanion::ensure(&client, user.id).await.expect("ensure");
    client
        .execute(
            "UPDATE cat_companions SET care_streak_days = 12, care_streak_last_day = current_date - 3 WHERE user_id = $1",
            &[&user.id],
        )
        .await
        .expect("seed lapsed streak");

    CatCompanion::touch_fed(&client, user.id).await.expect("fed today");

    let cat = CatCompanion::ensure(&client, user.id).await.expect("reload");
    assert_eq!(
        cat.care_streak_days, 1,
        "a multi-day gap breaks the streak and restarts the counter"
    );
}
