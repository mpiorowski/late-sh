use crate::{
    models::{
        chat_message::{ChatMessage, ChatMessageParams},
        chat_message_reaction::ChatMessageReaction,
        chat_room::ChatRoom,
        user::{User, UserParams},
    },
    test_utils::test_db,
};

#[tokio::test]
async fn test_chat_message() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");

    let room = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge");

    let user = User::create(
        &client,
        UserParams {
            fingerprint: "msg-user-1".to_string(),
            username: "u1".to_string(),
            settings: serde_json::json!({}),
        },
    )
    .await
    .unwrap();

    let msg1 = ChatMessage::create(
        &client,
        ChatMessageParams {
            room_id: room.id,
            user_id: user.id,
            body: "Hello world".to_string(),
        },
    )
    .await
    .unwrap();
    assert_eq!(msg1.reply_to_message_id, None);

    let msgs = ChatMessage::list_recent(&client, room.id, 10)
        .await
        .unwrap();
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].id, msg1.id);

    let edited = ChatMessage::edit_by_author(&client, msg1.id, user.id, "Hello modified")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(edited.body, "Hello modified");
    assert!(edited.updated > edited.created);

    ChatMessage::delete_by_author(&client, msg1.id, user.id)
        .await
        .unwrap();

    let msgs_after_delete = ChatMessage::list_recent(&client, room.id, 10)
        .await
        .unwrap();
    assert!(msgs_after_delete.is_empty());
}

#[tokio::test]
async fn chat_message_can_reference_reply_target() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");

    let room = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge");

    let user = User::create(
        &client,
        UserParams {
            fingerprint: "reply-user-1".to_string(),
            username: "replyuser".to_string(),
            settings: serde_json::json!({}),
        },
    )
    .await
    .unwrap();

    let original = ChatMessage::create(
        &client,
        ChatMessageParams {
            room_id: room.id,
            user_id: user.id,
            body: "original".to_string(),
        },
    )
    .await
    .unwrap();
    let reply = ChatMessage::create_with_reply_to(
        &client,
        ChatMessageParams {
            room_id: room.id,
            user_id: user.id,
            body: "> @replyuser: original\nreply".to_string(),
        },
        Some(original.id),
    )
    .await
    .unwrap();

    assert_eq!(reply.reply_to_message_id, Some(original.id));

    let msgs = ChatMessage::list_recent(&client, room.id, 10)
        .await
        .unwrap();
    let listed_reply = msgs
        .iter()
        .find(|message| message.id == reply.id)
        .expect("reply listed");
    assert_eq!(listed_reply.reply_to_message_id, Some(original.id));
}

#[tokio::test]
async fn chat_message_reactions_toggle_and_summarize() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");

    let room = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge");

    let author = User::create(
        &client,
        UserParams {
            fingerprint: "reaction-author".to_string(),
            username: "author".to_string(),
            settings: serde_json::json!({}),
        },
    )
    .await
    .unwrap();
    let viewer = User::create(
        &client,
        UserParams {
            fingerprint: "reaction-viewer".to_string(),
            username: "viewer".to_string(),
            settings: serde_json::json!({}),
        },
    )
    .await
    .unwrap();

    let message = ChatMessage::create(
        &client,
        ChatMessageParams {
            room_id: room.id,
            user_id: author.id,
            body: "react to me".to_string(),
        },
    )
    .await
    .unwrap();

    ChatMessageReaction::toggle(&client, message.id, author.id, "👍")
        .await
        .unwrap();
    ChatMessageReaction::toggle(&client, message.id, viewer.id, "😂")
        .await
        .unwrap();
    ChatMessageReaction::toggle(&client, message.id, viewer.id, "😂")
        .await
        .unwrap();
    let kaomoji = "(╯`Д´)╯︵ ┻━┻";
    ChatMessageReaction::toggle(&client, message.id, viewer.id, kaomoji)
        .await
        .unwrap();

    let summaries = ChatMessageReaction::list_summaries_for_messages(&client, &[message.id])
        .await
        .unwrap();
    let reactions = summaries.get(&message.id).expect("reactions");
    assert_eq!(reactions.len(), 2);
    assert_eq!(reactions[0].icon, "👍");
    assert_eq!(reactions[0].count, 1);
    assert_eq!(reactions[1].icon, kaomoji);
    assert_eq!(reactions[1].count, 1);

    let owners = ChatMessageReaction::list_owners_for_message(&client, message.id)
        .await
        .unwrap();
    assert_eq!(owners.len(), 2);
    assert_eq!(owners[0].icon, "👍");
    assert_eq!(owners[0].user_ids, vec![author.id]);
    assert_eq!(owners[1].icon, kaomoji);
    assert_eq!(owners[1].user_ids, vec![viewer.id]);
}

/// Search and context windows must skip bot replies directed at an ignored
/// user, not just messages the ignored user authored (the cannot-be-heard-
/// by-proxy invariant).
#[tokio::test]
async fn search_and_context_exclude_replies_to_ignored_users() {
    use crate::models::chat_room_member::ChatRoomMember;

    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");

    let room = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge");

    let mut users = Vec::new();
    for (fingerprint, username) in [
        ("ignore-viewer", "iviewer"),
        ("ignore-target", "itarget"),
        ("ignore-bot", "ibot"),
    ] {
        users.push(
            User::create(
                &client,
                UserParams {
                    fingerprint: fingerprint.to_string(),
                    username: username.to_string(),
                    settings: serde_json::json!({}),
                },
            )
            .await
            .unwrap(),
        );
    }
    let (viewer, ignored, bot) = (&users[0], &users[1], &users[2]);
    ChatRoomMember::join(&client, room.id, viewer.id)
        .await
        .unwrap();

    let plain = ChatMessage::create(
        &client,
        ChatMessageParams {
            room_id: room.id,
            user_id: bot.id,
            body: "deploy finished".to_string(),
        },
    )
    .await
    .unwrap();
    let reply_to_ignored = ChatMessage::create_with_reply_targets(
        &client,
        ChatMessageParams {
            room_id: room.id,
            user_id: bot.id,
            body: "deploy failed for you".to_string(),
        },
        None,
        Some(ignored.id),
    )
    .await
    .unwrap();
    let anchor = ChatMessage::create(
        &client,
        ChatMessageParams {
            room_id: room.id,
            user_id: bot.id,
            body: "deploy anchor".to_string(),
        },
    )
    .await
    .unwrap();

    let hits = ChatMessage::search_for_user(&client, viewer.id, "deploy", None, &[ignored.id], 50)
        .await
        .unwrap();
    let hit_ids: Vec<_> = hits.iter().map(|m| m.id).collect();
    assert!(hit_ids.contains(&plain.id));
    assert!(hit_ids.contains(&anchor.id));
    assert!(!hit_ids.contains(&reply_to_ignored.id));

    let (before, after) = ChatMessage::list_around(
        &client,
        room.id,
        anchor.created,
        anchor.id,
        &[ignored.id],
        10,
    )
    .await
    .unwrap();
    let window_ids: Vec<_> = before.iter().chain(after.iter()).map(|m| m.id).collect();
    assert!(window_ids.contains(&plain.id));
    assert!(!window_ids.contains(&reply_to_ignored.id));
}
