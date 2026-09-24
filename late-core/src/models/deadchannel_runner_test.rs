use crate::models::deadchannel_runner::{DeadchannelRunner, RunnerOrigin};
use crate::test_utils::{create_test_user, test_db};

#[tokio::test]
async fn ensure_creates_once_and_keeps_the_first_look() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let user = create_test_user(&test_db.db, "runner-one").await;

    assert!(
        DeadchannelRunner::find_by_user(&client, user.id)
            .await
            .expect("find")
            .is_none()
    );

    let first_look = serde_json::json!({"hood": "hood.cross"});
    let (created, origin) = DeadchannelRunner::ensure_for_user(&client, user.id, &first_look)
        .await
        .expect("ensure");
    assert_eq!(origin, RunnerOrigin::Created);
    assert_eq!(created.user_id, user.id);
    assert_eq!(created.look, first_look);

    // A second device joining at the same time loses its look: one runner,
    // the first look, the same id, and the insert says so rather than the
    // caller guessing from the look that came back.
    let second_look = serde_json::json!({"hood": "hood.plain"});
    let (again, origin) = DeadchannelRunner::ensure_for_user(&client, user.id, &second_look)
        .await
        .expect("ensure again");
    assert_eq!(origin, RunnerOrigin::Existing);
    assert_eq!(again.id, created.id);
    assert_eq!(again.look, first_look);

    let looks = DeadchannelRunner::list_looks(&client)
        .await
        .expect("list looks");
    assert_eq!(looks, vec![(user.id, first_look)]);
}

#[tokio::test]
async fn leaving_hides_the_runner_but_keeps_the_character() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let user = create_test_user(&test_db.db, "runner-two").await;

    let look = serde_json::json!({"hood": "hood.cross"});
    let (created, _) = DeadchannelRunner::ensure_for_user(&client, user.id, &look)
        .await
        .expect("ensure");

    assert!(
        DeadchannelRunner::mark_left(&client, user.id)
            .await
            .expect("mark left")
    );
    // Leaving twice writes once: the second call has no door to close, so
    // it notifies no replica.
    assert!(
        !DeadchannelRunner::mark_left(&client, user.id)
            .await
            .expect("mark left again")
    );

    // Gone from the directory, so the gate is shut everywhere and the old
    // messages lose their portrait.
    assert!(
        DeadchannelRunner::list_looks(&client)
            .await
            .expect("list looks")
            .is_empty()
    );
    // The character is still there, stamped.
    let left = DeadchannelRunner::find_by_user(&client, user.id)
        .await
        .expect("find")
        .expect("row survives the leave");
    assert_eq!(left.id, created.id);
    assert_eq!(left.look, look);
    assert!(left.left_at.is_some());

    // An invited rejoin brings back the same face, whatever look it offers.
    let offered = serde_json::json!({"hood": "hood.plain"});
    let (back, origin) = DeadchannelRunner::ensure_for_user(&client, user.id, &offered)
        .await
        .expect("ensure back");
    assert_eq!(origin, RunnerOrigin::Returned);
    assert_eq!(back.id, created.id);
    assert_eq!(back.look, look);
    assert!(back.left_at.is_none());
    assert_eq!(
        DeadchannelRunner::list_looks(&client)
            .await
            .expect("list looks again"),
        vec![(user.id, look)]
    );
}

#[tokio::test]
async fn the_tailor_dresses_a_standing_runner_only() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let user = create_test_user(&test_db.db, "runner-three").await;

    let look = serde_json::json!({"hood": "hood.cross"});
    DeadchannelRunner::ensure_for_user(&client, user.id, &look)
        .await
        .expect("ensure");

    let new_look = serde_json::json!({"hood": "hood.plain"});
    assert!(
        DeadchannelRunner::store_look(&client, user.id, &new_look)
            .await
            .expect("store look")
    );
    assert_eq!(
        DeadchannelRunner::list_looks(&client)
            .await
            .expect("list looks"),
        vec![(user.id, new_look.clone())]
    );

    // A runner who left keeps the look they left in.
    DeadchannelRunner::mark_left(&client, user.id)
        .await
        .expect("mark left");
    assert!(
        !DeadchannelRunner::store_look(&client, user.id, &look)
            .await
            .expect("store look after leaving")
    );
    let left = DeadchannelRunner::find_by_user(&client, user.id)
        .await
        .expect("find")
        .expect("row");
    assert_eq!(left.look, new_look);
}
