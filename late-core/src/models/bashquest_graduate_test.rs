use crate::{
    models::{bashquest_graduate::BashquestGraduate, user::User},
    test_utils::{create_test_user, test_db},
};

#[tokio::test]
async fn record_is_idempotent_per_account() {
    let test_db = test_db().await;
    let user = create_test_user(&test_db.db, "bashquest-grad").await;
    let client = test_db.db.get().await.expect("db client");

    assert!(
        BashquestGraduate::find_by_user_id(&client, user.id)
            .await
            .expect("check ungraduated")
            .is_none()
    );

    let first = BashquestGraduate::record(&client, user.id, "grad_one", "cert text", "digest-1")
        .await
        .expect("first record");
    assert!(first);

    // A re-report (e.g. the graduate logging back in) must not overwrite the
    // original certificate or be treated as a fresh graduation.
    let second =
        BashquestGraduate::record(&client, user.id, "grad_one", "different cert", "digest-2")
            .await
            .expect("second record");
    assert!(!second);

    let stored = BashquestGraduate::find_by_user_id(&client, user.id)
        .await
        .expect("find after record")
        .expect("graduate exists");
    assert_eq!(stored.certificate, "cert text");
    assert_eq!(stored.certificate_digest, "digest-1");
}

#[tokio::test]
async fn list_all_returns_every_graduate_with_a_live_account() {
    let test_db = test_db().await;
    let user_a = create_test_user(&test_db.db, "bashquest-grad-a").await;
    let user_b = create_test_user(&test_db.db, "bashquest-grad-b").await;
    let client = test_db.db.get().await.expect("db client");

    BashquestGraduate::record(&client, user_a.id, "grad_a", "cert a", "digest-a")
        .await
        .expect("record a");
    BashquestGraduate::record(&client, user_b.id, "grad_b", "cert b", "digest-b")
        .await
        .expect("record b");

    let all = BashquestGraduate::list_all(&client)
        .await
        .expect("list all");
    let handles: Vec<&str> = all.iter().map(|g| g.handle.as_str()).collect();
    assert!(handles.contains(&"grad_a"));
    assert!(handles.contains(&"grad_b"));
}

/// Anything built on `list_all` is keyed on the player's handle, so deleting
/// the late.sh account must drop the graduate out of that list. The row itself
/// is kept (`user_id` is `ON DELETE SET NULL`), it just stops being listed.
#[tokio::test]
async fn a_deleted_account_drops_out_of_the_public_list_but_keeps_its_row() {
    let test_db = test_db().await;
    let user = create_test_user(&test_db.db, "bashquest-grad-gone").await;
    let client = test_db.db.get().await.expect("db client");

    BashquestGraduate::record(&client, user.id, "grad_gone", "cert gone", "digest-gone")
        .await
        .expect("record");

    User::delete_by_id(&client, user.id)
        .await
        .expect("delete user");

    let listed = BashquestGraduate::list_all(&client)
        .await
        .expect("list all");
    assert!(!listed.iter().any(|g| g.handle == "grad_gone"));

    let row = client
        .query_one(
            "SELECT user_id, certificate FROM bashquest_graduates WHERE handle = $1",
            &[&"grad_gone"],
        )
        .await
        .expect("row survives the account deletion");
    assert_eq!(row.get::<_, Option<uuid::Uuid>>("user_id"), None);
    assert_eq!(row.get::<_, String>("certificate"), "cert gone");
}
