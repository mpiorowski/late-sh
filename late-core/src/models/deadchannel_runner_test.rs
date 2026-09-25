use crate::models::deadchannel_runner::{DeadchannelRunner, RunnerOrigin, StandingRunner};
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

    let standing = DeadchannelRunner::list_standing(&client)
        .await
        .expect("list standing");
    assert_eq!(
        standing,
        vec![StandingRunner {
            user_id: user.id,
            look: first_look,
            level: 1
        }]
    );
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
        DeadchannelRunner::list_standing(&client)
            .await
            .expect("list standing")
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
        DeadchannelRunner::list_standing(&client)
            .await
            .expect("list standing again"),
        vec![StandingRunner {
            user_id: user.id,
            look,
            level: 1
        }]
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
        DeadchannelRunner::list_standing(&client)
            .await
            .expect("list standing"),
        vec![StandingRunner {
            user_id: user.id,
            look: new_look.clone(),
            level: 1
        }]
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

#[tokio::test]
async fn the_guide_is_claimed_once_and_only_by_a_standing_runner() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let user = create_test_user(&test_db.db, "runner-guide").await;

    // Nobody to stamp: no row yet.
    assert!(
        !DeadchannelRunner::mark_guide_seen(&client, user.id)
            .await
            .expect("mark guide seen with no runner")
    );

    let look = serde_json::json!({"hood": "hood.cross"});
    DeadchannelRunner::ensure_for_user(&client, user.id, &look)
        .await
        .expect("ensure");

    // The first descent claims it; the second finds it claimed.
    assert!(
        DeadchannelRunner::mark_guide_seen(&client, user.id)
            .await
            .expect("first descent")
    );
    assert!(
        !DeadchannelRunner::mark_guide_seen(&client, user.id)
            .await
            .expect("second descent")
    );
    let row = DeadchannelRunner::find_by_user(&client, user.id)
        .await
        .expect("find")
        .expect("row");
    assert!(row.guide_seen_at.is_some());

    // A leaver cannot be stamped, and the stamp survives the leave.
    let other = create_test_user(&test_db.db, "runner-guide-left").await;
    DeadchannelRunner::ensure_for_user(&client, other.id, &look)
        .await
        .expect("ensure other");
    DeadchannelRunner::mark_left(&client, other.id)
        .await
        .expect("mark left");
    assert!(
        !DeadchannelRunner::mark_guide_seen(&client, other.id)
            .await
            .expect("mark guide seen after leaving")
    );
}
