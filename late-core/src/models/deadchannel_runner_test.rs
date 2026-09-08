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
