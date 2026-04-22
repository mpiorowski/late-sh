use late_core::models::{
    chat_message::{ChatMessage, ChatMessageParams},
    chat_room::{ChatRoom, ChatRoomParams},
    chat_room_member::ChatRoomMember,
    profile::{Profile, ProfileParams},
    user::User,
};
use late_ssh::app::chat::notifications::svc::NotificationService;
use late_ssh::app::chat::svc::{ChatEvent, ChatService};
use tokio::time::{Duration, timeout};
use uuid::Uuid;

use super::helpers::new_test_db;
use late_core::test_utils::create_test_user;

#[tokio::test]
async fn emits_send_failed_event_when_sender_is_not_room_member() {
    let test_db = new_test_db().await;
    let service = ChatService::new(
        test_db.db.clone(),
        NotificationService::new(test_db.db.clone()),
    );
    let mut events = service.subscribe_events();
    let user_id = Uuid::now_v7();
    let room_id = Uuid::now_v7();
    let request_id = Uuid::now_v7();

    service.send_message_task(
        user_id,
        room_id,
        None,
        "hello".to_string(),
        request_id,
        false,
    );

    let event = timeout(Duration::from_secs(2), events.recv())
        .await
        .expect("event timeout")
        .expect("event");
    match event {
        ChatEvent::SendFailed {
            user_id: event_user_id,
            request_id: got_request,
            ..
        } => {
            assert_eq!(event_user_id, user_id);
            assert_eq!(got_request, request_id);
        }
        _ => panic!("expected send failed event"),
    }
}

#[tokio::test]
async fn emits_message_created_and_send_succeeded_when_sender_is_member() {
    let test_db = new_test_db().await;
    let service = ChatService::new(
        test_db.db.clone(),
        NotificationService::new(test_db.db.clone()),
    );
    let mut events = service.subscribe_events();
    let client = test_db.db.get().await.expect("db client");

    let user = create_test_user(&test_db.db, "alice").await;
    let room = ChatRoom::get_or_create_language(&client, "en")
        .await
        .expect("room");
    ChatRoomMember::join(&client, room.id, user.id)
        .await
        .expect("join");

    let request_id = Uuid::now_v7();
    service.send_message_task(
        user.id,
        room.id,
        room.slug.clone(),
        "hello world".to_string(),
        request_id,
        false,
    );

    let first = timeout(Duration::from_secs(2), events.recv())
        .await
        .expect("first event timeout")
        .expect("first event");
    let second = timeout(Duration::from_secs(2), events.recv())
        .await
        .expect("second event timeout")
        .expect("second event");

    let mut saw_created = false;
    let mut saw_success = false;
    for event in [first, second] {
        match event {
            ChatEvent::MessageCreated { message, .. } => {
                saw_created = true;
                assert_eq!(message.room_id, room.id);
                assert_eq!(message.user_id, user.id);
                assert_eq!(message.body, "hello world");
            }
            ChatEvent::SendSucceeded {
                user_id,
                request_id: got_request,
            } => {
                saw_success = true;
                assert_eq!(user_id, user.id);
                assert_eq!(got_request, request_id);
            }
            _ => {}
        }
    }
    assert!(saw_created, "expected MessageCreated event");
    assert!(saw_success, "expected SendSucceeded event");
}

#[tokio::test]
async fn emits_message_reactions_updated_when_member_reacts() {
    let test_db = new_test_db().await;
    let service = ChatService::new(
        test_db.db.clone(),
        NotificationService::new(test_db.db.clone()),
    );
    let mut events = service.subscribe_events();
    let client = test_db.db.get().await.expect("db client");

    let author = create_test_user(&test_db.db, "author").await;
    let reactor = create_test_user(&test_db.db, "reactor").await;
    let room = ChatRoom::get_or_create_language(&client, "en")
        .await
        .expect("room");
    ChatRoomMember::join(&client, room.id, author.id)
        .await
        .expect("join author");
    ChatRoomMember::join(&client, room.id, reactor.id)
        .await
        .expect("join reactor");
    let message = ChatMessage::create(
        &client,
        ChatMessageParams {
            room_id: room.id,
            user_id: author.id,
            body: "hello".to_string(),
        },
    )
    .await
    .expect("message");

    service.toggle_message_reaction_task(reactor.id, message.id, 4);

    let event = timeout(Duration::from_secs(2), events.recv())
        .await
        .expect("event timeout")
        .expect("event");
    match event {
        ChatEvent::MessageReactionsUpdated {
            room_id,
            message_id,
            reactions,
            ..
        } => {
            assert_eq!(room_id, room.id);
            assert_eq!(message_id, message.id);
            assert_eq!(reactions.len(), 1);
            assert_eq!(reactions[0].kind, 4);
            assert_eq!(reactions[0].count, 1);
        }
        _ => panic!("expected message reactions updated event"),
    }
}

#[tokio::test]
async fn emits_send_failed_event_when_non_admin_posts_to_announcements() {
    let test_db = new_test_db().await;
    let service = ChatService::new(
        test_db.db.clone(),
        NotificationService::new(test_db.db.clone()),
    );
    let mut events = service.subscribe_events();
    let client = test_db.db.get().await.expect("db client");

    let user = create_test_user(&test_db.db, "alice").await;
    let room = ChatRoom::ensure_permanent(&client, "announcements")
        .await
        .expect("room");
    ChatRoomMember::join(&client, room.id, user.id)
        .await
        .expect("join");

    let request_id = Uuid::now_v7();
    service.send_message_task(
        user.id,
        room.id,
        room.slug.clone(),
        "not allowed".to_string(),
        request_id,
        false,
    );

    let event = timeout(Duration::from_secs(2), events.recv())
        .await
        .expect("event timeout")
        .expect("event");
    match event {
        ChatEvent::SendFailed {
            user_id,
            request_id: got_request,
            message,
        } => {
            assert_eq!(user_id, user.id);
            assert_eq!(got_request, request_id);
            assert_eq!(message, "Only admins can post in #announcements.");
        }
        _ => panic!("expected send failed event"),
    }
}

#[tokio::test]
async fn publishes_snapshot_with_selected_general_usernames_and_unread_counts() {
    let test_db = new_test_db().await;
    let service = ChatService::new(
        test_db.db.clone(),
        NotificationService::new(test_db.db.clone()),
    );
    let mut state_rx = service.subscribe_state();
    let client = test_db.db.get().await.expect("db client");

    let target_user = create_test_user(&test_db.db, "target").await;
    let author_user = create_test_user(&test_db.db, "author").await;

    let general_room = ChatRoom::create(
        &client,
        ChatRoomParams {
            kind: "general".to_string(),
            visibility: "public".to_string(),
            auto_join: true,
            permanent: true,
            slug: Some("general".to_string()),
            language_code: None,
            dm_user_a: None,
            dm_user_b: None,
        },
    )
    .await
    .expect("create general room");
    let lang_room = ChatRoom::get_or_create_language(&client, "en")
        .await
        .expect("language room");

    ChatRoomMember::join(&client, general_room.id, target_user.id)
        .await
        .expect("join target general");
    ChatRoomMember::join(&client, lang_room.id, target_user.id)
        .await
        .expect("join target language");
    ChatRoomMember::join(&client, general_room.id, author_user.id)
        .await
        .expect("join author general");
    ChatRoomMember::join(&client, lang_room.id, author_user.id)
        .await
        .expect("join author language");

    let general_message = ChatMessage::create(
        &client,
        ChatMessageParams {
            room_id: general_room.id,
            user_id: author_user.id,
            body: "g-msg".to_string(),
        },
    )
    .await
    .expect("general message");
    let lang_message = ChatMessage::create(
        &client,
        ChatMessageParams {
            room_id: lang_room.id,
            user_id: author_user.id,
            body: "l-msg".to_string(),
        },
    )
    .await
    .expect("language message");

    service.list_chats_task(target_user.id, Some(lang_room.id));

    timeout(Duration::from_secs(2), state_rx.changed())
        .await
        .expect("state timeout")
        .expect("watch changed");
    let snapshot = state_rx.borrow_and_update().clone();

    assert_eq!(snapshot.user_id, Some(target_user.id));
    assert_eq!(snapshot.general_room_id, Some(general_room.id));
    assert_eq!(
        snapshot.usernames.get(&author_user.id).map(String::as_str),
        Some("author")
    );
    assert_eq!(snapshot.unread_counts.get(&general_room.id), Some(&1));
    assert_eq!(snapshot.unread_counts.get(&lang_room.id), Some(&1));
    assert!(snapshot.ignored_user_ids.is_empty());

    let selected_room = snapshot
        .chat_rooms
        .iter()
        .find(|(room, _)| room.id == lang_room.id)
        .expect("selected room present");
    assert!(selected_room.1.iter().any(|m| m.id == lang_message.id));

    // General always ships with its tail populated, even when another room is
    // selected — the dashboard card depends on this to stay warm.
    let general_in_snapshot = snapshot
        .chat_rooms
        .iter()
        .find(|(room, _)| room.id == general_room.id)
        .expect("general room present");
    assert!(
        general_in_snapshot
            .1
            .iter()
            .any(|m| m.id == general_message.id)
    );
}

#[tokio::test]
async fn falls_back_to_first_room_when_selected_room_is_none() {
    let test_db = new_test_db().await;
    let service = ChatService::new(
        test_db.db.clone(),
        NotificationService::new(test_db.db.clone()),
    );
    let mut state_rx = service.subscribe_state();
    let client = test_db.db.get().await.expect("db client");

    let target_user = create_test_user(&test_db.db, "target2").await;
    let author_user = create_test_user(&test_db.db, "author2").await;

    let general_room = ChatRoom::create(
        &client,
        ChatRoomParams {
            kind: "general".to_string(),
            visibility: "public".to_string(),
            auto_join: true,
            permanent: true,
            slug: Some("general".to_string()),
            language_code: None,
            dm_user_a: None,
            dm_user_b: None,
        },
    )
    .await
    .expect("create general room");
    let lang_room = ChatRoom::get_or_create_language(&client, "fr")
        .await
        .expect("language room");

    ChatRoomMember::join(&client, general_room.id, target_user.id)
        .await
        .expect("join target general");
    ChatRoomMember::join(&client, lang_room.id, target_user.id)
        .await
        .expect("join target language");
    ChatRoomMember::join(&client, general_room.id, author_user.id)
        .await
        .expect("join author general");

    let general_message = ChatMessage::create(
        &client,
        ChatMessageParams {
            room_id: general_room.id,
            user_id: author_user.id,
            body: "fallback-msg".to_string(),
        },
    )
    .await
    .expect("general message");

    service.list_chats_task(target_user.id, None);

    timeout(Duration::from_secs(2), state_rx.changed())
        .await
        .expect("state timeout")
        .expect("watch changed");
    let snapshot = state_rx.borrow_and_update().clone();

    let general_entry = snapshot
        .chat_rooms
        .iter()
        .find(|(room, _)| room.id == general_room.id)
        .expect("general room present");
    assert!(
        general_entry.1.iter().any(|m| m.id == general_message.id),
        "expected fallback to first room (general) with messages"
    );
    let other_entry = snapshot
        .chat_rooms
        .iter()
        .find(|(room, _)| room.id == lang_room.id)
        .expect("lang room present");
    assert!(
        other_entry.1.is_empty(),
        "non-selected room should not include messages in snapshot"
    );
}

#[tokio::test]
async fn publishes_snapshot_with_favorite_room_history() {
    let test_db = new_test_db().await;
    let service = ChatService::new(
        test_db.db.clone(),
        NotificationService::new(test_db.db.clone()),
    );
    let mut state_rx = service.subscribe_state();
    let client = test_db.db.get().await.expect("db client");

    let target_user = create_test_user(&test_db.db, "favorite_target").await;
    let author_user = create_test_user(&test_db.db, "favorite_author").await;

    let general_room = ChatRoom::ensure_general(&client)
        .await
        .expect("ensure general room");
    let favorite_room = ChatRoom::get_or_create_public_room(&client, "favorites")
        .await
        .expect("favorite room");

    ChatRoomMember::join(&client, general_room.id, target_user.id)
        .await
        .expect("join target general");
    ChatRoomMember::join(&client, favorite_room.id, target_user.id)
        .await
        .expect("join target favorite");
    ChatRoomMember::join(&client, general_room.id, author_user.id)
        .await
        .expect("join author general");
    ChatRoomMember::join(&client, favorite_room.id, author_user.id)
        .await
        .expect("join author favorite");

    ChatMessage::create(
        &client,
        ChatMessageParams {
            room_id: favorite_room.id,
            user_id: author_user.id,
            body: "favorite backlog".to_string(),
        },
    )
    .await
    .expect("favorite message");

    Profile::update(
        &client,
        target_user.id,
        ProfileParams {
            username: "favorite_target".to_string(),
            bio: String::new(),
            country: None,
            timezone: None,
            notify_kinds: Vec::new(),
            notify_bell: false,
            notify_cooldown_mins: 0,
            notify_format: None,
            theme_id: Some("late".to_string()),
            enable_background_color: false,
            show_dashboard_header: true,
            show_right_sidebar: true,
            show_games_sidebar: true,
            favorite_room_ids: vec![favorite_room.id],
        },
    )
    .await
    .expect("update favorites");

    service.list_chats_task(target_user.id, Some(general_room.id));

    timeout(Duration::from_secs(2), state_rx.changed())
        .await
        .expect("state timeout")
        .expect("watch changed");
    let snapshot = state_rx.borrow_and_update().clone();

    let favorite_in_snapshot = snapshot
        .chat_rooms
        .iter()
        .find(|(room, _)| room.id == favorite_room.id)
        .expect("favorite room present");
    assert!(
        favorite_in_snapshot
            .1
            .iter()
            .any(|message| message.body == "favorite backlog"),
        "favorite room should preload its history in the snapshot"
    );
}

#[tokio::test]
async fn publishes_snapshot_with_persisted_ignored_user_ids() {
    let test_db = new_test_db().await;
    let service = ChatService::new(
        test_db.db.clone(),
        NotificationService::new(test_db.db.clone()),
    );
    let mut state_rx = service.subscribe_state();
    let client = test_db.db.get().await.expect("db client");

    let target_user = create_test_user(&test_db.db, "target_ignore_snapshot").await;
    let ignored_user = create_test_user(&test_db.db, "author_ignore_snapshot").await;

    let general_room = ChatRoom::ensure_general(&client)
        .await
        .expect("ensure general room");
    ChatRoomMember::join(&client, general_room.id, target_user.id)
        .await
        .expect("join target");
    ChatRoomMember::join(&client, general_room.id, ignored_user.id)
        .await
        .expect("join ignored user");

    User::add_ignored_user_id(&client, target_user.id, ignored_user.id)
        .await
        .expect("persist ignored user id");

    service.list_chats_task(target_user.id, Some(general_room.id));

    timeout(Duration::from_secs(2), state_rx.changed())
        .await
        .expect("state timeout")
        .expect("watch changed");
    let snapshot = state_rx.borrow_and_update().clone();

    assert_eq!(snapshot.ignored_user_ids, vec![ignored_user.id]);
}

#[tokio::test]
async fn publishes_snapshot_with_discover_rooms_for_public_rooms_user_has_not_joined() {
    let test_db = new_test_db().await;
    let service = ChatService::new(
        test_db.db.clone(),
        NotificationService::new(test_db.db.clone()),
    );
    let mut state_rx = service.subscribe_state();
    let client = test_db.db.get().await.expect("db client");

    let target_user = create_test_user(&test_db.db, "discover_target").await;
    let author_user = create_test_user(&test_db.db, "discover_author").await;

    let general_room = ChatRoom::ensure_general(&client)
        .await
        .expect("ensure general room");
    let discover_room = ChatRoom::get_or_create_public_room(&client, "rust")
        .await
        .expect("create discover room");
    let joined_room = ChatRoom::get_or_create_public_room(&client, "elixir")
        .await
        .expect("create joined room");

    ChatRoomMember::join(&client, general_room.id, target_user.id)
        .await
        .expect("join target general");
    ChatRoomMember::join(&client, general_room.id, author_user.id)
        .await
        .expect("join author general");
    ChatRoomMember::join(&client, discover_room.id, author_user.id)
        .await
        .expect("join author discover room");
    ChatRoomMember::join(&client, joined_room.id, target_user.id)
        .await
        .expect("join target joined room");
    ChatRoomMember::join(&client, joined_room.id, author_user.id)
        .await
        .expect("join author joined room");

    ChatMessage::create(
        &client,
        ChatMessageParams {
            room_id: discover_room.id,
            user_id: author_user.id,
            body: "discover-msg".to_string(),
        },
    )
    .await
    .expect("discover message");
    ChatMessage::create(
        &client,
        ChatMessageParams {
            room_id: joined_room.id,
            user_id: author_user.id,
            body: "joined-msg".to_string(),
        },
    )
    .await
    .expect("joined message");

    service.list_chats_task(target_user.id, Some(general_room.id));

    timeout(Duration::from_secs(2), state_rx.changed())
        .await
        .expect("state timeout")
        .expect("watch changed");
    let snapshot = state_rx.borrow_and_update().clone();

    assert_eq!(snapshot.discover_rooms.len(), 1);
    assert_eq!(snapshot.discover_rooms[0].room_id, discover_room.id);
    assert_eq!(snapshot.discover_rooms[0].slug, "rust");
    assert_eq!(snapshot.discover_rooms[0].member_count, 1);
    assert_eq!(snapshot.discover_rooms[0].message_count, 1);
}

#[tokio::test]
async fn join_public_room_task_only_adds_requesting_user() {
    let test_db = new_test_db().await;
    let service = ChatService::new(
        test_db.db.clone(),
        NotificationService::new(test_db.db.clone()),
    );
    let mut events = service.subscribe_events();
    let client = test_db.db.get().await.expect("db client");

    let target_user = create_test_user(&test_db.db, "discover_join_target").await;
    let existing_member = create_test_user(&test_db.db, "discover_join_existing").await;
    let untouched_user = create_test_user(&test_db.db, "discover_join_untouched").await;
    let room = ChatRoom::get_or_create_public_room(&client, "zig")
        .await
        .expect("create room");

    ChatRoomMember::join(&client, room.id, existing_member.id)
        .await
        .expect("join existing member");

    service.join_public_room_task(target_user.id, room.id, "zig".to_string());

    let event = timeout(Duration::from_secs(2), events.recv())
        .await
        .expect("event timeout")
        .expect("event");
    match event {
        ChatEvent::RoomJoined {
            user_id,
            room_id,
            slug,
        } => {
            assert_eq!(user_id, target_user.id);
            assert_eq!(room_id, room.id);
            assert_eq!(slug, "zig");
        }
        other => panic!("expected RoomJoined, got {other:?}"),
    }

    assert!(
        ChatRoomMember::is_member(&client, room.id, target_user.id)
            .await
            .unwrap()
    );
    assert!(
        !ChatRoomMember::is_member(&client, room.id, untouched_user.id)
            .await
            .unwrap()
    );
}

// --- delete message: regression tests for user_id on MessageDeleted ---

#[tokio::test]
async fn message_deleted_event_carries_deleter_user_id() {
    let test_db = new_test_db().await;
    let service = ChatService::new(
        test_db.db.clone(),
        NotificationService::new(test_db.db.clone()),
    );
    let mut events = service.subscribe_events();
    let client = test_db.db.get().await.expect("db client");

    let author = create_test_user(&test_db.db, "author_del").await;
    let room = ChatRoom::get_or_create_language(&client, "de")
        .await
        .expect("room");
    ChatRoomMember::join(&client, room.id, author.id)
        .await
        .expect("join");

    let msg = ChatMessage::create(
        &client,
        ChatMessageParams {
            room_id: room.id,
            user_id: author.id,
            body: "to be deleted".to_string(),
        },
    )
    .await
    .expect("create message");

    service.delete_message_task(author.id, msg.id, false);

    let event = timeout(Duration::from_secs(2), events.recv())
        .await
        .expect("event timeout")
        .expect("event");
    match event {
        ChatEvent::MessageDeleted {
            user_id,
            room_id,
            message_id,
        } => {
            assert_eq!(user_id, author.id, "deleter user_id must match");
            assert_eq!(room_id, room.id);
            assert_eq!(message_id, msg.id);
        }
        other => panic!("expected MessageDeleted, got {other:?}"),
    }
}

#[tokio::test]
async fn admin_delete_event_carries_admin_user_id_not_author() {
    let test_db = new_test_db().await;
    let service = ChatService::new(
        test_db.db.clone(),
        NotificationService::new(test_db.db.clone()),
    );
    let mut events = service.subscribe_events();
    let client = test_db.db.get().await.expect("db client");

    let author = create_test_user(&test_db.db, "msg_author").await;
    let admin = create_test_user(&test_db.db, "admin_user").await;
    let room = ChatRoom::get_or_create_language(&client, "es")
        .await
        .expect("room");
    ChatRoomMember::join(&client, room.id, author.id)
        .await
        .expect("join author");

    let msg = ChatMessage::create(
        &client,
        ChatMessageParams {
            room_id: room.id,
            user_id: author.id,
            body: "admin will delete this".to_string(),
        },
    )
    .await
    .expect("create message");

    // Admin deletes another user's message
    service.delete_message_task(admin.id, msg.id, true);

    let event = timeout(Duration::from_secs(2), events.recv())
        .await
        .expect("event timeout")
        .expect("event");
    match event {
        ChatEvent::MessageDeleted {
            user_id,
            room_id,
            message_id,
        } => {
            assert_eq!(
                user_id, admin.id,
                "event must carry the admin's id, not the author's"
            );
            assert_ne!(user_id, author.id);
            assert_eq!(room_id, room.id);
            assert_eq!(message_id, msg.id);
        }
        other => panic!("expected MessageDeleted, got {other:?}"),
    }
}

#[tokio::test]
async fn ignore_user_task_persists_and_emits_update() {
    let test_db = new_test_db().await;
    let service = ChatService::new(
        test_db.db.clone(),
        NotificationService::new(test_db.db.clone()),
    );
    let mut events = service.subscribe_events();
    let client = test_db.db.get().await.expect("db client");

    let viewer = create_test_user(&test_db.db, "ignore_viewer").await;
    let target = create_test_user(&test_db.db, "ignore_target").await;
    let general_room = ChatRoom::ensure_general(&client)
        .await
        .expect("ensure general room");
    ChatRoomMember::join(&client, general_room.id, viewer.id)
        .await
        .expect("join viewer");
    ChatRoomMember::join(&client, general_room.id, target.id)
        .await
        .expect("join target");

    service.ignore_user_task(viewer.id, "ignore_target".to_string());

    let event = timeout(Duration::from_secs(2), events.recv())
        .await
        .expect("event timeout")
        .expect("event");
    match event {
        ChatEvent::IgnoreListUpdated {
            user_id,
            ignored_user_ids,
            message,
        } => {
            assert_eq!(user_id, viewer.id);
            assert_eq!(ignored_user_ids, vec![target.id]);
            assert_eq!(message, "Ignored @ignore_target");
        }
        other => panic!("expected IgnoreListUpdated, got {other:?}"),
    }

    let ignored = User::ignored_user_ids(&client, viewer.id)
        .await
        .expect("load ignore list");
    assert_eq!(ignored, vec![target.id]);
}

#[tokio::test]
async fn unignore_user_task_persists_and_emits_update() {
    let test_db = new_test_db().await;
    let service = ChatService::new(
        test_db.db.clone(),
        NotificationService::new(test_db.db.clone()),
    );
    let mut events = service.subscribe_events();
    let client = test_db.db.get().await.expect("db client");

    let viewer = create_test_user(&test_db.db, "unignore_viewer").await;
    let target = create_test_user(&test_db.db, "unignore_target").await;
    let general_room = ChatRoom::ensure_general(&client)
        .await
        .expect("ensure general room");
    ChatRoomMember::join(&client, general_room.id, viewer.id)
        .await
        .expect("join viewer");
    ChatRoomMember::join(&client, general_room.id, target.id)
        .await
        .expect("join target");
    User::add_ignored_user_id(&client, viewer.id, target.id)
        .await
        .expect("seed ignored user id");

    service.unignore_user_task(viewer.id, "unignore_target".to_string());

    let event = timeout(Duration::from_secs(2), events.recv())
        .await
        .expect("event timeout")
        .expect("event");
    match event {
        ChatEvent::IgnoreListUpdated {
            user_id,
            ignored_user_ids,
            message,
        } => {
            assert_eq!(user_id, viewer.id);
            assert!(ignored_user_ids.is_empty());
            assert_eq!(message, "Unignored @unignore_target");
        }
        other => panic!("expected IgnoreListUpdated, got {other:?}"),
    }

    let ignored = User::ignored_user_ids(&client, viewer.id)
        .await
        .expect("load ignore list");
    assert!(ignored.is_empty());
}

#[tokio::test]
async fn ignore_user_task_emits_error_for_self_or_duplicate() {
    let test_db = new_test_db().await;
    let service = ChatService::new(
        test_db.db.clone(),
        NotificationService::new(test_db.db.clone()),
    );
    let mut events = service.subscribe_events();
    let client = test_db.db.get().await.expect("db client");

    let viewer = create_test_user(&test_db.db, "ignore_self").await;

    service.ignore_user_task(viewer.id, "ignore_self".to_string());

    let first = timeout(Duration::from_secs(2), events.recv())
        .await
        .expect("event timeout")
        .expect("event");
    match first {
        ChatEvent::IgnoreFailed { user_id, message } => {
            assert_eq!(user_id, viewer.id);
            assert_eq!(message, "Cannot ignore yourself");
        }
        other => panic!("expected IgnoreFailed, got {other:?}"),
    }

    let target = create_test_user(&test_db.db, "ignore_dup_target").await;
    let general_room = ChatRoom::ensure_general(&client)
        .await
        .expect("ensure general room");
    ChatRoomMember::join(&client, general_room.id, viewer.id)
        .await
        .expect("join viewer");
    ChatRoomMember::join(&client, general_room.id, target.id)
        .await
        .expect("join target");
    User::add_ignored_user_id(&client, viewer.id, target.id)
        .await
        .expect("seed ignored user id");

    service.ignore_user_task(viewer.id, "ignore_dup_target".to_string());

    let second = timeout(Duration::from_secs(2), events.recv())
        .await
        .expect("event timeout")
        .expect("event");
    match second {
        ChatEvent::IgnoreFailed { user_id, message } => {
            assert_eq!(user_id, viewer.id);
            assert_eq!(message, "@ignore_dup_target is already ignored");
        }
        other => panic!("expected IgnoreFailed, got {other:?}"),
    }
}

#[tokio::test]
async fn unignore_user_task_emits_error_for_missing_user_or_entry() {
    let test_db = new_test_db().await;
    let service = ChatService::new(
        test_db.db.clone(),
        NotificationService::new(test_db.db.clone()),
    );
    let mut events = service.subscribe_events();
    let client = test_db.db.get().await.expect("db client");

    let viewer = create_test_user(&test_db.db, "unignore_missing_viewer").await;

    service.unignore_user_task(viewer.id, "no_such_user".to_string());

    let first = timeout(Duration::from_secs(2), events.recv())
        .await
        .expect("event timeout")
        .expect("event");
    match first {
        ChatEvent::IgnoreFailed { user_id, message } => {
            assert_eq!(user_id, viewer.id);
            assert_eq!(message, "User 'no_such_user' not found");
        }
        other => panic!("expected IgnoreFailed, got {other:?}"),
    }

    let target = create_test_user(&test_db.db, "unignore_missing_target").await;
    let general_room = ChatRoom::ensure_general(&client)
        .await
        .expect("ensure general room");
    ChatRoomMember::join(&client, general_room.id, viewer.id)
        .await
        .expect("join viewer");
    ChatRoomMember::join(&client, general_room.id, target.id)
        .await
        .expect("join target");

    service.unignore_user_task(viewer.id, "unignore_missing_target".to_string());

    let second = timeout(Duration::from_secs(2), events.recv())
        .await
        .expect("event timeout")
        .expect("event");
    match second {
        ChatEvent::IgnoreFailed { user_id, message } => {
            assert_eq!(user_id, viewer.id);
            assert_eq!(message, "@unignore_missing_target is not ignored");
        }
        other => panic!("expected IgnoreFailed, got {other:?}"),
    }
}
