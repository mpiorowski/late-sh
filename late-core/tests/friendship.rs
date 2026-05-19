use late_core::models::friendship::{Friendship, FriendshipStatus, SendOutcome};
use late_core::test_utils::test_db;

#[tokio::test]
async fn send_request_creates_pending_edge() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let a = late_core::test_utils::create_test_user(&test_db.db, "friend-a").await;
    let b = late_core::test_utils::create_test_user(&test_db.db, "friend-b").await;

    let outcome = Friendship::send_request(&client, a.id, b.id)
        .await
        .expect("send");
    assert_eq!(outcome, SendOutcome::Sent);

    let from_a = Friendship::status(&client, a.id, b.id)
        .await
        .expect("status from a");
    let from_b = Friendship::status(&client, b.id, a.id)
        .await
        .expect("status from b");
    assert_eq!(from_a, FriendshipStatus::OutgoingPending);
    assert_eq!(from_b, FriendshipStatus::IncomingPending);
}

#[tokio::test]
async fn duplicate_request_is_a_noop() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let a = late_core::test_utils::create_test_user(&test_db.db, "dup-a").await;
    let b = late_core::test_utils::create_test_user(&test_db.db, "dup-b").await;

    assert_eq!(
        Friendship::send_request(&client, a.id, b.id)
            .await
            .expect("first send"),
        SendOutcome::Sent
    );
    assert_eq!(
        Friendship::send_request(&client, a.id, b.id)
            .await
            .expect("second send"),
        SendOutcome::AlreadyExists
    );
}

#[tokio::test]
async fn reverse_pending_request_auto_accepts() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let a = late_core::test_utils::create_test_user(&test_db.db, "auto-a").await;
    let b = late_core::test_utils::create_test_user(&test_db.db, "auto-b").await;

    Friendship::send_request(&client, a.id, b.id)
        .await
        .expect("a -> b");
    let outcome = Friendship::send_request(&client, b.id, a.id)
        .await
        .expect("b -> a");
    assert_eq!(outcome, SendOutcome::AutoAccepted);

    assert_eq!(
        Friendship::status(&client, a.id, b.id)
            .await
            .expect("status"),
        FriendshipStatus::Friends
    );
}

#[tokio::test]
async fn self_request_is_silently_ignored() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let a = late_core::test_utils::create_test_user(&test_db.db, "self-a").await;

    let outcome = Friendship::send_request(&client, a.id, a.id)
        .await
        .expect("self");
    assert_eq!(outcome, SendOutcome::SelfRequest);
    assert_eq!(
        Friendship::status(&client, a.id, a.id)
            .await
            .expect("status"),
        FriendshipStatus::None
    );
}

#[tokio::test]
async fn accept_promotes_pending_to_friends() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let a = late_core::test_utils::create_test_user(&test_db.db, "accept-a").await;
    let b = late_core::test_utils::create_test_user(&test_db.db, "accept-b").await;

    Friendship::send_request(&client, a.id, b.id)
        .await
        .expect("send");
    assert!(
        Friendship::accept(&client, b.id, a.id)
            .await
            .expect("accept")
    );
    assert_eq!(
        Friendship::status(&client, a.id, b.id)
            .await
            .expect("status"),
        FriendshipStatus::Friends
    );

    // Accepting again does nothing — there is no longer a pending row.
    assert!(
        !Friendship::accept(&client, b.id, a.id)
            .await
            .expect("re-accept")
    );
}

#[tokio::test]
async fn decline_removes_pending_only() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let a = late_core::test_utils::create_test_user(&test_db.db, "decline-a").await;
    let b = late_core::test_utils::create_test_user(&test_db.db, "decline-b").await;

    Friendship::send_request(&client, a.id, b.id)
        .await
        .expect("send");
    assert_eq!(
        Friendship::decline_or_cancel(&client, b.id, a.id)
            .await
            .expect("decline"),
        1
    );
    assert_eq!(
        Friendship::status(&client, a.id, b.id)
            .await
            .expect("status"),
        FriendshipStatus::None
    );

    // Decline never touches an accepted friendship.
    Friendship::send_request(&client, a.id, b.id)
        .await
        .expect("re-send");
    Friendship::accept(&client, b.id, a.id)
        .await
        .expect("accept");
    assert_eq!(
        Friendship::decline_or_cancel(&client, b.id, a.id)
            .await
            .expect("decline on accepted"),
        0
    );
}

#[tokio::test]
async fn unfriend_removes_only_accepted_edges() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let a = late_core::test_utils::create_test_user(&test_db.db, "unf-a").await;
    let b = late_core::test_utils::create_test_user(&test_db.db, "unf-b").await;

    Friendship::send_request(&client, a.id, b.id)
        .await
        .expect("send");
    // Pending — unfriend should not touch it.
    assert_eq!(
        Friendship::unfriend(&client, a.id, b.id)
            .await
            .expect("unfriend pending"),
        0
    );

    Friendship::accept(&client, b.id, a.id)
        .await
        .expect("accept");
    assert_eq!(
        Friendship::unfriend(&client, a.id, b.id)
            .await
            .expect("unfriend accepted"),
        1
    );
    assert_eq!(
        Friendship::status(&client, a.id, b.id)
            .await
            .expect("status"),
        FriendshipStatus::None
    );
}

#[tokio::test]
async fn lists_return_correct_buckets_for_each_user() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let me = late_core::test_utils::create_test_user(&test_db.db, "lists-me").await;
    let friend = late_core::test_utils::create_test_user(&test_db.db, "lists-friend").await;
    let pending_in = late_core::test_utils::create_test_user(&test_db.db, "lists-pending-in").await;
    let pending_out =
        late_core::test_utils::create_test_user(&test_db.db, "lists-pending-out").await;

    Friendship::send_request(&client, me.id, friend.id)
        .await
        .expect("send to friend");
    Friendship::accept(&client, friend.id, me.id)
        .await
        .expect("friend accepts");
    Friendship::send_request(&client, pending_in.id, me.id)
        .await
        .expect("incoming");
    Friendship::send_request(&client, me.id, pending_out.id)
        .await
        .expect("outgoing");

    let friends = Friendship::list_friends(&client, me.id)
        .await
        .expect("list_friends");
    assert_eq!(friends.len(), 1);
    assert_eq!(friends[0].user_id, friend.id);
    assert_eq!(friends[0].username, "lists-friend");

    let incoming = Friendship::list_incoming(&client, me.id)
        .await
        .expect("list_incoming");
    assert_eq!(incoming.len(), 1);
    assert_eq!(incoming[0].other_user_id, pending_in.id);

    let outgoing = Friendship::list_outgoing(&client, me.id)
        .await
        .expect("list_outgoing");
    assert_eq!(outgoing.len(), 1);
    assert_eq!(outgoing[0].other_user_id, pending_out.id);
}
