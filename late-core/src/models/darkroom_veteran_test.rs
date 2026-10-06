use crate::{
    models::darkroom_veteran::DarkroomVeteran,
    test_utils::{create_test_user, test_db},
};

#[tokio::test]
async fn an_account_that_never_finished_is_not_a_veteran() {
    let test_db = test_db().await;
    let user = create_test_user(&test_db.db, "darkroom-veteran-fresh").await;
    let client = test_db.db.get().await.expect("db client");

    assert!(
        !DarkroomVeteran::has_escaped(&client, user.id)
            .await
            .expect("check a fresh account"),
        "the battleship must stay locked until a run is finished"
    );
}

#[tokio::test]
async fn finishing_twice_records_one_veteran() {
    let test_db = test_db().await;
    let user = create_test_user(&test_db.db, "darkroom-veteran-record").await;
    let client = test_db.db.get().await.expect("db client");

    DarkroomVeteran::record(&client, user.id)
        .await
        .expect("first escape");
    // A replay reaches the same ending; recording it again must not conflict.
    DarkroomVeteran::record(&client, user.id)
        .await
        .expect("second escape");

    assert!(
        DarkroomVeteran::has_escaped(&client, user.id)
            .await
            .expect("check a finished account")
    );
    let rows = client
        .query(
            "SELECT 1 FROM darkroom_veterans WHERE user_id = $1",
            &[&user.id],
        )
        .await
        .expect("count rows");
    assert_eq!(rows.len(), 1, "one row per account, however many escapes");
}

#[tokio::test]
async fn one_account_finishing_never_unlocks_anothers_battleship() {
    let test_db = test_db().await;
    let flier = create_test_user(&test_db.db, "darkroom-veteran-flier").await;
    let bystander = create_test_user(&test_db.db, "darkroom-veteran-bystander").await;
    let client = test_db.db.get().await.expect("db client");

    DarkroomVeteran::record(&client, flier.id)
        .await
        .expect("record escape");

    assert!(
        !DarkroomVeteran::has_escaped(&client, bystander.id)
            .await
            .expect("check the bystander")
    );
}
