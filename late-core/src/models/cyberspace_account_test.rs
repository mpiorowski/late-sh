use crate::{
    models::cyberspace_account::{CmailThread, CyberspaceAccount},
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

#[tokio::test]
async fn pinned_circ_rooms_round_trip_and_survive_a_relink() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let user = create_test_user(&test_db.db, "cs-circ").await;

    CyberspaceAccount::upsert_for_user(&client, user.id, "uid-1", "odd", "refresh-1")
        .await
        .expect("link");

    // A fresh link pins nothing: the rail shows the pane and no rooms.
    let found = CyberspaceAccount::find_by_user_id(&client, user.id)
        .await
        .expect("find")
        .expect("linked");
    assert!(found.circ_rooms.is_empty());

    // The list is written whole, and order is the user's rail order.
    let rooms = vec!["general".to_string(), "tech".to_string()];
    CyberspaceAccount::set_circ_rooms(&client, user.id, &rooms)
        .await
        .expect("pin rooms");
    let found = CyberspaceAccount::find_by_user_id(&client, user.id)
        .await
        .expect("find")
        .expect("linked");
    assert_eq!(found.circ_rooms, rooms);

    // Signing in again is the same person's rail, so the pins stay.
    CyberspaceAccount::upsert_for_user(&client, user.id, "uid-1", "odd", "refresh-2")
        .await
        .expect("re-link");
    let found = CyberspaceAccount::find_by_user_id(&client, user.id)
        .await
        .expect("find")
        .expect("linked");
    assert_eq!(found.circ_rooms, rooms);
}

#[tokio::test]
async fn circ_room_read_cursors_round_trip_and_prune_with_the_pins() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let user = create_test_user(&test_db.db, "cs-circ-reads").await;

    CyberspaceAccount::upsert_for_user(&client, user.id, "uid-1", "odd", "refresh-1")
        .await
        .expect("link");
    let rooms = vec!["general".to_string(), "tech".to_string()];
    CyberspaceAccount::set_circ_rooms(&client, user.id, &rooms)
        .await
        .expect("pin rooms");

    // A fresh link has read nothing, which must read as no dots, not all dots.
    let found = CyberspaceAccount::find_by_user_id(&client, user.id)
        .await
        .expect("find")
        .expect("linked");
    assert!(found.room_read_cursors().is_empty());

    CyberspaceAccount::mark_circ_room_read(&client, user.id, "general", 1_700_000_000_000)
        .await
        .expect("mark general read");
    CyberspaceAccount::mark_circ_room_read(&client, user.id, "tech", 1_700_000_100_000)
        .await
        .expect("mark tech read");
    // A later visit moves the cursor in place.
    CyberspaceAccount::mark_circ_room_read(&client, user.id, "general", 1_700_000_200_000)
        .await
        .expect("re-mark general");
    let cursors = CyberspaceAccount::find_by_user_id(&client, user.id)
        .await
        .expect("find")
        .expect("linked")
        .room_read_cursors();
    assert_eq!(cursors.get("general"), Some(&1_700_000_200_000));
    assert_eq!(cursors.get("tech"), Some(&1_700_000_100_000));

    // Unpinning a room takes its cursor with it; the survivor keeps its own.
    CyberspaceAccount::set_circ_rooms(&client, user.id, &["tech".to_string()])
        .await
        .expect("unpin general");
    let cursors = CyberspaceAccount::find_by_user_id(&client, user.id)
        .await
        .expect("find")
        .expect("linked")
        .room_read_cursors();
    assert_eq!(cursors.get("general"), None);
    assert_eq!(cursors.get("tech"), Some(&1_700_000_100_000));
}

#[tokio::test]
async fn pinned_cmail_threads_round_trip_and_survive_a_relink() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let user = create_test_user(&test_db.db, "cs-cmail").await;

    CyberspaceAccount::upsert_for_user(&client, user.id, "uid-1", "odd", "refresh-1")
        .await
        .expect("link");

    // A fresh link pins nothing.
    let found = CyberspaceAccount::find_by_user_id(&client, user.id)
        .await
        .expect("find")
        .expect("linked");
    assert!(found.cmail_threads().is_empty());

    // Their id addresses the conversation and the username labels the rail
    // row: an opaque id is not something a reader recognizes, and the row has
    // to draw before anything is fetched.
    let threads = vec![
        CmailThread {
            id: "conv-1".to_string(),
            username: "alice".to_string(),
        },
        CmailThread {
            id: "conv-2".to_string(),
            username: "bob".to_string(),
        },
    ];
    CyberspaceAccount::set_cmail_threads(&client, user.id, &threads)
        .await
        .expect("pin conversations");
    let found = CyberspaceAccount::find_by_user_id(&client, user.id)
        .await
        .expect("find")
        .expect("linked");
    assert_eq!(found.cmail_threads(), threads);

    // Signing in again is the same person's rail, so the pins stay.
    CyberspaceAccount::upsert_for_user(&client, user.id, "uid-1", "odd", "refresh-2")
        .await
        .expect("re-link");
    let found = CyberspaceAccount::find_by_user_id(&client, user.id)
        .await
        .expect("find")
        .expect("linked");
    assert_eq!(found.cmail_threads(), threads);
}
