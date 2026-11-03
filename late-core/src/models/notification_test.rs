use crate::{
    models::{
        chat_message::{ChatMessage, ChatMessageParams},
        chat_room::ChatRoom,
        notification::Notification,
        user::User,
    },
    test_utils::{create_test_user, test_db},
};

/// Opening the Mentions entry stamps every mention still unread and says which,
/// so the list can keep its dots on them. What the list hands the attention
/// guest then agrees with the count: nothing is read in one and unread in the
/// other. A later mention is unread again, and the next open stamps only that.
#[tokio::test]
async fn opening_the_feed_stamps_every_unread_mention_and_says_which() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let room = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge");
    let actor = create_test_user(&test_db.db, "open-feed-actor").await;
    let reader = create_test_user(&test_db.db, "open-feed-reader").await;
    let mention = async |body: &str| {
        let message = ChatMessage::create(
            &client,
            ChatMessageParams {
                room_id: room.id,
                user_id: actor.id,
                body: body.to_string(),
            },
        )
        .await
        .expect("create message");
        Notification::create_mentions_batch(&client, &[reader.id], actor.id, message.id, room.id)
            .await
            .expect("create mention");
        message.id
    };

    let seen_in_room = mention("@open-feed-reader seen in the room").await;
    let unread = mention("@open-feed-reader never rendered").await;
    Notification::mark_read_for_messages(&client, reader.id, &[seen_in_room])
        .await
        .expect("mark rendered mention read");

    let stamped = Notification::mark_all_read(&client, reader.id)
        .await
        .expect("open the feed");
    let listed = Notification::list_for_user(&client, reader.id, 10)
        .await
        .expect("list");
    let unread_row = listed
        .iter()
        .find(|view| view.message_id == unread)
        .expect("the unread mention is listed");
    assert_eq!(
        stamped,
        vec![unread_row.id],
        "only what was still unread is this visit's"
    );
    assert!(listed.iter().all(|view| view.read_at.is_some()));
    assert_eq!(
        Notification::unread_count(&client, reader.id)
            .await
            .expect("count after open"),
        0
    );

    mention("@open-feed-reader one more").await;
    assert_eq!(
        Notification::unread_count(&client, reader.id)
            .await
            .expect("count after a new mention"),
        1
    );
    assert_eq!(
        Notification::mark_all_read(&client, reader.id)
            .await
            .expect("open the feed again")
            .len(),
        1
    );
}

/// Opening the feed is scoped to its owner in the query itself.
#[tokio::test]
async fn opening_the_feed_leaves_other_users_mentions_unread() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let room = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge");
    let actor = create_test_user(&test_db.db, "open-scope-actor").await;
    let reader = create_test_user(&test_db.db, "open-scope-reader").await;
    let other = create_test_user(&test_db.db, "open-scope-other").await;
    let message = ChatMessage::create(
        &client,
        ChatMessageParams {
            room_id: room.id,
            user_id: actor.id,
            body: "@open-scope-reader hello".to_string(),
        },
    )
    .await
    .expect("create message");
    Notification::create_mentions_batch(&client, &[reader.id], actor.id, message.id, room.id)
        .await
        .expect("create mention");

    let stamped = Notification::mark_all_read(&client, other.id)
        .await
        .expect("open as the wrong user");
    assert!(stamped.is_empty());
    assert_eq!(
        Notification::unread_count(&client, reader.id)
            .await
            .expect("count"),
        1
    );
}

#[tokio::test]
async fn mention_notifications_skip_recipients_who_ignore_actor() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let room = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge");
    let actor = create_test_user(&test_db.db, "mention-ignored-actor").await;
    let reader = create_test_user(&test_db.db, "mention-ignore-reader").await;

    User::add_ignored_user_id(&client, reader.id, actor.id)
        .await
        .expect("ignore actor");

    let message = ChatMessage::create(
        &client,
        ChatMessageParams {
            room_id: room.id,
            user_id: actor.id,
            body: "@mention-ignore-reader hello".to_string(),
        },
    )
    .await
    .expect("create mention message");

    let inserted =
        Notification::create_mentions_batch(&client, &[reader.id], actor.id, message.id, room.id)
            .await
            .expect("create mention notification");
    assert_eq!(inserted, 0);

    let list = Notification::list_for_user(&client, reader.id, 50)
        .await
        .expect("list notifications");
    assert!(list.is_empty());

    let unread = Notification::unread_count(&client, reader.id)
        .await
        .expect("count notifications");
    assert_eq!(unread, 0);
}

#[tokio::test]
async fn mention_feed_hides_existing_notifications_after_actor_is_ignored() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let room = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge");
    let actor = create_test_user(&test_db.db, "mention-hide-actor").await;
    let reader = create_test_user(&test_db.db, "mention-hide-reader").await;

    let message = ChatMessage::create(
        &client,
        ChatMessageParams {
            room_id: room.id,
            user_id: actor.id,
            body: "@mention-hide-reader hello".to_string(),
        },
    )
    .await
    .expect("create mention message");
    Notification::create_mentions_batch(&client, &[reader.id], actor.id, message.id, room.id)
        .await
        .expect("create mention notification");

    assert_eq!(
        Notification::unread_count(&client, reader.id)
            .await
            .expect("count before ignore"),
        1
    );
    assert_eq!(
        Notification::list_for_user(&client, reader.id, 50)
            .await
            .expect("list before ignore")
            .len(),
        1
    );

    User::add_ignored_user_id(&client, reader.id, actor.id)
        .await
        .expect("ignore actor");

    assert_eq!(
        Notification::unread_count(&client, reader.id)
            .await
            .expect("count after ignore"),
        0
    );
    assert!(
        Notification::list_for_user(&client, reader.id, 50)
            .await
            .expect("list after ignore")
            .is_empty()
    );
}

/// Rendering the message a mention rides on clears that mention and only that
/// mention, so a mention never stays on the rail badge after you read the
/// message where it was actually said. The room's coarse read cursor is
/// deliberately not the fix: it moves whenever the room is merely opened,
/// which would also clear mentions above the loaded tail that were never on
/// screen.
#[tokio::test]
async fn rendering_a_mention_clears_it_and_leaves_unrendered_ones_unread() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let room = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge");
    let actor = create_test_user(&test_db.db, "render-read-actor").await;
    let reader = create_test_user(&test_db.db, "render-read-reader").await;

    let old_message = ChatMessage::create(
        &client,
        ChatMessageParams {
            room_id: room.id,
            user_id: actor.id,
            body: "@render-read-reader way up in the backlog".to_string(),
        },
    )
    .await
    .expect("create old message");
    let seen_message = ChatMessage::create(
        &client,
        ChatMessageParams {
            room_id: room.id,
            user_id: actor.id,
            body: "@render-read-reader over here".to_string(),
        },
    )
    .await
    .expect("create seen message");
    for message in [&old_message, &seen_message] {
        Notification::create_mentions_batch(&client, &[reader.id], actor.id, message.id, room.id)
            .await
            .expect("create mention");
    }

    assert_eq!(
        Notification::unread_count(&client, reader.id)
            .await
            .expect("count before"),
        2
    );

    // Only the newer message was rendered; the old one sits above the tail.
    let cleared = Notification::mark_read_for_messages(&client, reader.id, &[seen_message.id])
        .await
        .expect("mark rendered mention read");
    assert_eq!(cleared, 1);

    assert_eq!(
        Notification::unread_count(&client, reader.id)
            .await
            .expect("count after"),
        1,
        "the unrendered mention above the tail must stay unread"
    );

    let listed = Notification::list_for_user(&client, reader.id, 10)
        .await
        .expect("list");
    let read_ids: Vec<_> = listed
        .iter()
        .filter(|view| view.read_at.is_some())
        .map(|view| view.message_id)
        .collect();
    assert_eq!(
        read_ids,
        vec![seen_message.id],
        "only the rendered mention should carry a read stamp"
    );
}

/// The read stamp is owner-scoped in the query itself: another user marking
/// the same message read must not clear this user's mention.
#[tokio::test]
async fn marking_read_is_scoped_to_the_mentioned_user() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let room = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge");
    let actor = create_test_user(&test_db.db, "scope-actor").await;
    let reader = create_test_user(&test_db.db, "scope-reader").await;
    let other = create_test_user(&test_db.db, "scope-other").await;

    let message = ChatMessage::create(
        &client,
        ChatMessageParams {
            room_id: room.id,
            user_id: actor.id,
            body: "@scope-reader hello".to_string(),
        },
    )
    .await
    .expect("create message");
    Notification::create_mentions_batch(&client, &[reader.id], actor.id, message.id, room.id)
        .await
        .expect("create mention");

    let cleared = Notification::mark_read_for_messages(&client, other.id, &[message.id])
        .await
        .expect("mark as the wrong user");
    assert_eq!(cleared, 0);

    assert_eq!(
        Notification::unread_count(&client, reader.id)
            .await
            .expect("count"),
        1,
        "another user's read must not clear this user's mention"
    );

    // Repeating the stamp as the right user clears it exactly once.
    let cleared = Notification::mark_read_for_messages(&client, reader.id, &[message.id])
        .await
        .expect("mark as the mentioned user");
    assert_eq!(cleared, 1);
    let cleared = Notification::mark_read_for_messages(&client, reader.id, &[message.id])
        .await
        .expect("mark again");
    assert_eq!(cleared, 0, "an already-read mention is a no-op");
}

/// A name said at the bar out back mentions nobody. The room is one of
/// `chat_room::HIDDEN_ROOM_KINDS`: it is public and seats every account,
/// so the public-room arm would otherwise resolve the name and hand the
/// recipient a mention the rail cannot open.
#[tokio::test]
async fn a_name_said_in_a_hidden_room_mentions_nobody() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let bar = ChatRoom::ensure_nightcap(&client)
        .await
        .expect("ensure nightcap");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge");
    let speaker = create_test_user(&test_db.db, "bar-speaker").await;
    let named = create_test_user(&test_db.db, "bar-named").await;
    let names = vec![named.username.clone()];

    let at_the_bar = Notification::resolve_mentioned_user_ids(&client, &names, speaker.id, bar.id)
        .await
        .expect("resolve at the bar");
    assert!(at_the_bar.is_empty(), "the bar mentioned {at_the_bar:?}");

    let in_the_lounge =
        Notification::resolve_mentioned_user_ids(&client, &names, speaker.id, lounge.id)
            .await
            .expect("resolve in the lounge");
    assert_eq!(in_the_lounge, vec![named.id]);
}
