use crate::{
    models::bashquest_graduate::BashquestGraduate,
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
async fn list_all_returns_every_graduate() {
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
