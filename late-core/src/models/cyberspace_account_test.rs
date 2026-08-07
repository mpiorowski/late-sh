use crate::{
    models::cyberspace_account::CyberspaceAccount,
    test_utils::{create_test_user, test_db},
};

#[tokio::test]
async fn upsert_replaces_existing_link_and_delete_forgets_it() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let user = create_test_user(&test_db.db, "cs-linker").await;

    let first = CyberspaceAccount::upsert_for_user(&client, user.id, "uid-1", "odd", "refresh-1")
        .await
        .expect("first link");
    assert_eq!(first.cs_username, "odd");

    // Re-link replaces identity and token in place, never a second row.
    let second = CyberspaceAccount::upsert_for_user(&client, user.id, "uid-2", "odd2", "refresh-2")
        .await
        .expect("re-link");
    assert_eq!(second.id, first.id);

    let found = CyberspaceAccount::find_by_user_id(&client, user.id)
        .await
        .expect("find")
        .expect("linked");
    assert_eq!(found.cs_user_id, "uid-2");
    assert_eq!(found.cs_username, "odd2");
    assert_eq!(found.refresh_token, "refresh-2");

    assert!(
        CyberspaceAccount::delete_for_user(&client, user.id)
            .await
            .expect("delete")
    );
    assert!(
        CyberspaceAccount::find_by_user_id(&client, user.id)
            .await
            .expect("find after delete")
            .is_none()
    );
    // A second delete finds nothing to forget.
    assert!(
        !CyberspaceAccount::delete_for_user(&client, user.id)
            .await
            .expect("second delete")
    );
}

#[tokio::test]
async fn the_feed_read_cursor_persists_and_survives_a_re_link() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let user = create_test_user(&test_db.db, "cs-reader").await;

    let fresh = CyberspaceAccount::upsert_for_user(&client, user.id, "uid-1", "odd", "refresh-1")
        .await
        .expect("link");
    assert_eq!(
        fresh.feed_read_at, None,
        "a new link has read nothing, which must not read as a full page of unread"
    );

    // Microsecond precision: timestamptz truncates below that, so a
    // nanosecond-resolution `Utc::now()` would not round-trip exactly.
    let read_at: chrono::DateTime<chrono::Utc> =
        "2026-08-07T12:34:56.123456Z".parse().expect("read stamp");
    CyberspaceAccount::mark_feed_read(&client, user.id, read_at)
        .await
        .expect("mark read");

    // Signing in again is the same person's reading, so the cursor stays put.
    CyberspaceAccount::upsert_for_user(&client, user.id, "uid-1", "odd", "refresh-2")
        .await
        .expect("re-link");
    let found = CyberspaceAccount::find_by_user_id(&client, user.id)
        .await
        .expect("find")
        .expect("linked");
    assert_eq!(found.feed_read_at, Some(read_at));
}

#[tokio::test]
async fn links_are_scoped_to_their_owner() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let owner = create_test_user(&test_db.db, "cs-owner").await;
    let other = create_test_user(&test_db.db, "cs-other").await;

    CyberspaceAccount::upsert_for_user(&client, owner.id, "uid-a", "owner", "token-a")
        .await
        .expect("link owner");
    CyberspaceAccount::upsert_for_user(&client, other.id, "uid-b", "other", "token-b")
        .await
        .expect("link other");

    assert!(
        CyberspaceAccount::delete_for_user(&client, owner.id)
            .await
            .expect("delete owner")
    );

    // Deleting one link never touches another user's row.
    let other_row = CyberspaceAccount::find_by_user_id(&client, other.id)
        .await
        .expect("find other")
        .expect("other still linked");
    assert_eq!(other_row.refresh_token, "token-b");
}
