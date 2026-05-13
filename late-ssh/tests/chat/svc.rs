use dartboard_core::{Canvas, CanvasOp, Pos, RgbColor};
use late_core::models::{
    artboard::Snapshot as ArtboardSnapshot,
    artboard_ban::ArtboardBan,
    chat_message::{ChatMessage, ChatMessageParams},
    chat_room::{ChatRoom, ChatRoomParams},
    chat_room_member::ChatRoomMember,
    moderation_audit_log::ModerationAuditLog,
    profile::{Profile, ProfileParams},
    room_ban::RoomBan,
    server_ban::ServerBan,
    user::{RIGHT_SIDEBAR_SCREEN_COUNT, RightSidebarMode, User},
};
use late_ssh::app::artboard::provenance::ArtboardProvenance;
use late_ssh::app::chat::notifications::svc::NotificationService;
use late_ssh::app::chat::svc::{ChatEvent, ChatService};
use late_ssh::authz::Permissions;
use late_ssh::dartboard;
use late_ssh::moderation::command::ServerUserAction;
use late_ssh::moderation::event::ModerationEvent;
use late_ssh::moderation::service::ModerationInfra;
use late_ssh::session::{SessionMessage, SessionRegistry};
use late_ssh::state::{ActiveSession, ActiveUser};
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use tokio::time::{Duration, sleep, timeout};
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
async fn dm_message_rejoins_recipient_who_left() {
    let test_db = new_test_db().await;
    let service = ChatService::new(
        test_db.db.clone(),
        NotificationService::new(test_db.db.clone()),
    );
    let mut events = service.subscribe_events();
    let client = test_db.db.get().await.expect("db client");

    let sender = create_test_user(&test_db.db, "dm_reopen_sender").await;
    let recipient = create_test_user(&test_db.db, "dm_reopen_recipient").await;
    let room = ChatRoom::get_or_create_dm(&client, sender.id, recipient.id)
        .await
        .expect("dm room");
    ChatRoomMember::join(&client, room.id, sender.id)
        .await
        .expect("join sender");
    ChatRoomMember::join(&client, room.id, recipient.id)
        .await
        .expect("join recipient");
    ChatRoomMember::leave(&client, room.id, recipient.id)
        .await
        .expect("recipient leaves");

    assert!(
        !ChatRoomMember::is_member(&client, room.id, recipient.id)
            .await
            .expect("recipient membership check"),
        "recipient should start outside the DM"
    );

    let request_id = Uuid::now_v7();
    service.send_message_task(
        sender.id,
        room.id,
        room.slug.clone(),
        "ping after leave".to_string(),
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
            ChatEvent::MessageCreated {
                message,
                target_user_ids,
                ..
            } => {
                saw_created = true;
                assert_eq!(message.room_id, room.id);
                assert_eq!(message.user_id, sender.id);
                let targets = target_user_ids.expect("dm message should be targeted");
                assert!(targets.contains(&sender.id));
                assert!(targets.contains(&recipient.id));
            }
            ChatEvent::SendSucceeded {
                user_id,
                request_id: got_request,
            } => {
                saw_success = true;
                assert_eq!(user_id, sender.id);
                assert_eq!(got_request, request_id);
            }
            _ => {}
        }
    }

    assert!(saw_created, "expected MessageCreated event");
    assert!(saw_success, "expected SendSucceeded event");
    assert!(
        ChatRoomMember::is_member(&client, room.id, recipient.id)
            .await
            .expect("recipient membership check"),
        "recipient should be rejoined when a DM arrives"
    );
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
async fn admin_can_toggle_message_pin() {
    let test_db = new_test_db().await;
    let service = ChatService::new(
        test_db.db.clone(),
        NotificationService::new(test_db.db.clone()),
    );
    let client = test_db.db.get().await.expect("db client");

    let admin = create_test_user(&test_db.db, "pin_admin").await;
    let room = ChatRoom::ensure_general(&client)
        .await
        .expect("general room");
    let message = ChatMessage::create(
        &client,
        ChatMessageParams {
            room_id: room.id,
            user_id: admin.id,
            body: "pin me".to_string(),
        },
    )
    .await
    .expect("message");

    let (pinned_tx, _pinned_rx) = tokio::sync::watch::channel(Vec::new());
    service.toggle_message_pin_task(message.id, true, pinned_tx);

    timeout(Duration::from_secs(2), async {
        loop {
            let updated = ChatMessage::get(&client, message.id)
                .await
                .expect("load message")
                .expect("message exists");
            if updated.pinned {
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("pin timeout");
}

#[tokio::test]
async fn non_admin_cannot_toggle_message_pin() {
    let test_db = new_test_db().await;
    let service = ChatService::new(
        test_db.db.clone(),
        NotificationService::new(test_db.db.clone()),
    );
    let client = test_db.db.get().await.expect("db client");

    let user = create_test_user(&test_db.db, "pin_non_admin").await;
    let room = ChatRoom::ensure_general(&client)
        .await
        .expect("general room");
    let message = ChatMessage::create(
        &client,
        ChatMessageParams {
            room_id: room.id,
            user_id: user.id,
            body: "do not pin me".to_string(),
        },
    )
    .await
    .expect("message");

    let (pinned_tx, _pinned_rx) = tokio::sync::watch::channel(Vec::new());
    service.toggle_message_pin_task(message.id, false, pinned_tx);

    tokio::time::sleep(Duration::from_millis(100)).await;
    let updated = ChatMessage::get(&client, message.id)
        .await
        .expect("load message")
        .expect("message exists");
    assert!(!updated.pinned);
}

#[tokio::test]
async fn pinned_messages_task_loads_global_pins() {
    let test_db = new_test_db().await;
    let service = ChatService::new(
        test_db.db.clone(),
        NotificationService::new(test_db.db.clone()),
    );
    let (pinned_tx, mut pinned_rx) = tokio::sync::watch::channel(Vec::new());
    let client = test_db.db.get().await.expect("db client");

    let author = create_test_user(&test_db.db, "pin_author").await;
    let visible_room = ChatRoom::get_or_create_public_room(&client, "pin-visible")
        .await
        .expect("visible room");
    let hidden_room = ChatRoom::get_or_create_public_room(&client, "pin-hidden")
        .await
        .expect("hidden room");

    let visible_message = ChatMessage::create(
        &client,
        ChatMessageParams {
            room_id: visible_room.id,
            user_id: author.id,
            body: "visible pin".to_string(),
        },
    )
    .await
    .expect("visible message");
    let hidden_message = ChatMessage::create(
        &client,
        ChatMessageParams {
            room_id: hidden_room.id,
            user_id: author.id,
            body: "hidden pin".to_string(),
        },
    )
    .await
    .expect("hidden message");
    ChatMessage::set_pinned(&client, visible_message.id, true)
        .await
        .expect("pin visible");
    ChatMessage::set_pinned(&client, hidden_message.id, true)
        .await
        .expect("pin hidden");

    service.load_pinned_messages_task(pinned_tx);

    timeout(Duration::from_secs(2), pinned_rx.changed())
        .await
        .expect("pinned timeout")
        .expect("pinned changed");
    let messages = pinned_rx.borrow_and_update().clone();
    assert!(messages.iter().any(|message| message.body == "visible pin"));
    assert!(messages.iter().any(|message| message.body == "hidden pin"));
}

#[tokio::test]
async fn publishes_summary_with_rooms_and_unread_counts() {
    let test_db = new_test_db().await;
    let service = ChatService::new(
        test_db.db.clone(),
        NotificationService::new(test_db.db.clone()),
    );
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

    let (_room_tx, room_rx) = tokio::sync::watch::channel(Some(lang_room.id));
    let (mut state_rx, _refresh_tx, refresh_task) =
        service.start_user_refresh_task(target_user.id, room_rx);

    timeout(Duration::from_secs(2), state_rx.changed())
        .await
        .expect("state timeout")
        .expect("watch changed");
    let snapshot = state_rx.borrow_and_update().clone();

    assert_eq!(snapshot.user_id, Some(target_user.id));
    assert_eq!(snapshot.general_room_id, Some(general_room.id));
    assert_eq!(snapshot.unread_counts.get(&general_room.id), Some(&1));
    assert_eq!(snapshot.unread_counts.get(&lang_room.id), Some(&1));
    assert!(snapshot.ignored_user_ids.is_empty());

    let selected_room = snapshot
        .chat_rooms
        .iter()
        .find(|(room, _)| room.id == lang_room.id)
        .expect("selected room present");
    assert!(
        selected_room.1.is_empty(),
        "summary refresh should not preload selected room history"
    );

    let general_in_snapshot = snapshot
        .chat_rooms
        .iter()
        .find(|(room, _)| room.id == general_room.id)
        .expect("general room present");
    assert!(
        general_in_snapshot.1.is_empty(),
        "summary refresh should not preload general room history"
    );
    assert_ne!(general_message.id, lang_message.id);
    refresh_task.abort();
}

#[tokio::test]
async fn falls_back_to_first_room_when_selected_room_is_none() {
    let test_db = new_test_db().await;
    let service = ChatService::new(
        test_db.db.clone(),
        NotificationService::new(test_db.db.clone()),
    );
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

    let (_room_tx, room_rx) = tokio::sync::watch::channel(None);
    let (mut state_rx, _refresh_tx, refresh_task) =
        service.start_user_refresh_task(target_user.id, room_rx);

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
        general_entry.1.is_empty(),
        "summary refresh should not preload fallback room history"
    );
    let other_entry = snapshot
        .chat_rooms
        .iter()
        .find(|(room, _)| room.id == lang_room.id)
        .expect("lang room present");
    assert!(
        other_entry.1.is_empty(),
        "non-selected room should not include messages in summary"
    );
    assert_eq!(general_message.room_id, general_room.id);
    refresh_task.abort();
}

#[tokio::test]
async fn room_tail_task_loads_favorite_room_history() {
    let test_db = new_test_db().await;
    let service = ChatService::new(
        test_db.db.clone(),
        NotificationService::new(test_db.db.clone()),
    );
    let mut events = service.subscribe_events();
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
            ide: None,
            terminal: None,
            os: None,
            langs: Vec::new(),
            notify_kinds: Vec::new(),
            notify_bell: false,
            notify_cooldown_mins: 0,
            notify_format: None,
            theme_id: Some("late".to_string()),
            enable_background_color: false,
            show_dashboard_header: true,
            show_right_sidebar: true,
            right_sidebar_mode: RightSidebarMode::On,
            right_sidebar_screens: (1..=RIGHT_SIDEBAR_SCREEN_COUNT).collect(),
            show_room_list_sidebar: true,
            show_settings_on_connect: true,
            favorite_room_ids: vec![favorite_room.id],
        },
    )
    .await
    .expect("update favorites");

    service.load_room_tail_task(target_user.id, favorite_room.id);

    let event = timeout(Duration::from_secs(2), events.recv())
        .await
        .expect("event timeout")
        .expect("event");
    match event {
        ChatEvent::RoomTailLoaded {
            user_id,
            room_id,
            messages,
            usernames,
            ..
        } => {
            assert_eq!(user_id, target_user.id);
            assert_eq!(room_id, favorite_room.id);
            assert!(
                messages
                    .iter()
                    .any(|message| message.body == "favorite backlog")
            );
            assert_eq!(
                usernames.get(&author_user.id).map(String::as_str),
                Some("favorite_author")
            );
        }
        other => panic!("expected RoomTailLoaded, got {other:?}"),
    }
}

#[tokio::test]
async fn publishes_snapshot_with_persisted_ignored_user_ids() {
    let test_db = new_test_db().await;
    let service = ChatService::new(
        test_db.db.clone(),
        NotificationService::new(test_db.db.clone()),
    );
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

    let (_room_tx, room_rx) = tokio::sync::watch::channel(Some(general_room.id));
    let (mut state_rx, _refresh_tx, refresh_task) =
        service.start_user_refresh_task(target_user.id, room_rx);

    timeout(Duration::from_secs(2), state_rx.changed())
        .await
        .expect("state timeout")
        .expect("watch changed");
    let snapshot = state_rx.borrow_and_update().clone();

    assert_eq!(snapshot.ignored_user_ids, vec![ignored_user.id]);
    refresh_task.abort();
}

#[tokio::test]
async fn discover_task_lists_public_rooms_user_has_not_joined() {
    let test_db = new_test_db().await;
    let service = ChatService::new(
        test_db.db.clone(),
        NotificationService::new(test_db.db.clone()),
    );
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

    let mut events = service.subscribe_events();
    service.list_discover_rooms_task(target_user.id);

    let event = timeout(Duration::from_secs(2), events.recv())
        .await
        .expect("event timeout")
        .expect("event");
    match event {
        ChatEvent::DiscoverRoomsLoaded { user_id, rooms } => {
            assert_eq!(user_id, target_user.id);
            assert_eq!(rooms.len(), 1);
            assert_eq!(rooms[0].room_id, discover_room.id);
            assert_eq!(rooms[0].slug, "rust");
            assert_eq!(rooms[0].member_count, 1);
            assert_eq!(rooms[0].message_count, 1);
        }
        other => panic!("expected DiscoverRoomsLoaded, got {other:?}"),
    }
}

#[tokio::test]
async fn shared_service_refresh_tasks_publish_per_session_snapshots() {
    let test_db = new_test_db().await;
    let service = ChatService::new(
        test_db.db.clone(),
        NotificationService::new(test_db.db.clone()),
    );
    let client = test_db.db.get().await.expect("db client");

    let user_a = create_test_user(&test_db.db, "shared_refresh_a").await;
    let user_b = create_test_user(&test_db.db, "shared_refresh_b").await;
    let author = create_test_user(&test_db.db, "shared_refresh_author").await;

    let room_a = ChatRoom::get_or_create_public_room(&client, "shared-a")
        .await
        .expect("room a");
    let room_b = ChatRoom::get_or_create_public_room(&client, "shared-b")
        .await
        .expect("room b");

    ChatRoomMember::join(&client, room_a.id, user_a.id)
        .await
        .expect("join user a");
    ChatRoomMember::join(&client, room_a.id, author.id)
        .await
        .expect("join author a");
    ChatRoomMember::join(&client, room_b.id, user_b.id)
        .await
        .expect("join user b");
    ChatRoomMember::join(&client, room_b.id, author.id)
        .await
        .expect("join author b");

    ChatMessage::create(
        &client,
        ChatMessageParams {
            room_id: room_a.id,
            user_id: author.id,
            body: "only user a sees this".to_string(),
        },
    )
    .await
    .expect("message a");
    ChatMessage::create(
        &client,
        ChatMessageParams {
            room_id: room_b.id,
            user_id: author.id,
            body: "only user b sees this".to_string(),
        },
    )
    .await
    .expect("message b");

    let (room_a_tx, room_a_rx) = tokio::sync::watch::channel(Some(room_a.id));
    let (_room_b_tx, room_b_rx) = tokio::sync::watch::channel(Some(room_b.id));
    let (mut snapshot_a_rx, refresh_a, task_a) =
        service.start_user_refresh_task(user_a.id, room_a_rx);
    let (mut snapshot_b_rx, _refresh_b, task_b) =
        service.start_user_refresh_task(user_b.id, room_b_rx);

    timeout(Duration::from_secs(2), snapshot_a_rx.changed())
        .await
        .expect("snapshot a timeout")
        .expect("snapshot a changed");
    timeout(Duration::from_secs(2), snapshot_b_rx.changed())
        .await
        .expect("snapshot b timeout")
        .expect("snapshot b changed");

    let snapshot_a = snapshot_a_rx.borrow_and_update().clone();
    let snapshot_b = snapshot_b_rx.borrow_and_update().clone();

    assert_eq!(snapshot_a.user_id, Some(user_a.id));
    assert_eq!(snapshot_b.user_id, Some(user_b.id));
    assert!(
        snapshot_a
            .chat_rooms
            .iter()
            .any(|(room, messages)| { room.id == room_a.id && messages.is_empty() })
    );
    assert!(
        snapshot_b
            .chat_rooms
            .iter()
            .any(|(room, messages)| { room.id == room_b.id && messages.is_empty() })
    );

    room_a_tx
        .send(Some(room_a.id))
        .expect("same selected room send");
    assert!(
        timeout(Duration::from_millis(200), snapshot_a_rx.changed())
            .await
            .is_err(),
        "unchanged selected room sends should not refresh the session"
    );

    refresh_a.send(()).expect("force refresh");
    timeout(Duration::from_secs(2), snapshot_a_rx.changed())
        .await
        .expect("forced snapshot timeout")
        .expect("forced snapshot changed");

    task_a.abort();
    task_b.abort();
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

#[tokio::test]
async fn open_public_room_task_joins_only_creator_and_disables_auto_join() {
    let test_db = new_test_db().await;
    let service = ChatService::new(
        test_db.db.clone(),
        NotificationService::new(test_db.db.clone()),
    );
    let mut events = service.subscribe_events();
    let client = test_db.db.get().await.expect("db client");

    let creator = create_test_user(&test_db.db, "public_creator").await;
    let existing_user = create_test_user(&test_db.db, "public_existing").await;
    let other_user = create_test_user(&test_db.db, "public_other").await;

    service.open_public_room_task(creator.id, "rustaceans".to_string());

    let event = timeout(Duration::from_secs(2), events.recv())
        .await
        .expect("event timeout")
        .expect("event");
    let room_id = match event {
        ChatEvent::RoomJoined {
            user_id,
            room_id,
            slug,
        } => {
            assert_eq!(user_id, creator.id);
            assert_eq!(slug, "rustaceans");
            room_id
        }
        other => panic!("expected RoomJoined, got {other:?}"),
    };

    assert!(
        ChatRoomMember::is_member(&client, room_id, creator.id)
            .await
            .unwrap()
    );
    assert!(
        !ChatRoomMember::is_member(&client, room_id, existing_user.id)
            .await
            .unwrap()
    );
    assert!(
        !ChatRoomMember::is_member(&client, room_id, other_user.id)
            .await
            .unwrap()
    );

    let room = ChatRoom::get(&client, room_id)
        .await
        .expect("reload room")
        .expect("room exists");
    assert!(!room.auto_join);

    let future_user = create_test_user(&test_db.db, "public_future").await;
    ChatRoomMember::auto_join_public_rooms(&client, future_user.id)
        .await
        .expect("auto-join future user");
    assert!(
        !ChatRoomMember::is_member(&client, room_id, future_user.id)
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn fill_room_task_adds_all_users_to_public_room() {
    let test_db = new_test_db().await;
    let service = ChatService::new(
        test_db.db.clone(),
        NotificationService::new(test_db.db.clone()),
    );
    let mut events = service.subscribe_events();
    let client = test_db.db.get().await.expect("db client");

    let admin = create_test_user(&test_db.db, "fill_public_admin").await;
    let existing_member = create_test_user(&test_db.db, "fill_public_existing").await;
    let untouched_user = create_test_user(&test_db.db, "fill_public_untouched").await;
    let room = ChatRoom::get_or_create_public_room(&client, "ops")
        .await
        .expect("create room");
    assert!(!room.auto_join);

    ChatRoomMember::join(&client, room.id, existing_member.id)
        .await
        .expect("join existing member");

    service.fill_room_task(admin.id, "ops".to_string());

    let event = timeout(Duration::from_secs(2), events.recv())
        .await
        .expect("event timeout")
        .expect("event");
    match event {
        ChatEvent::RoomFilled {
            user_id,
            slug,
            users_added,
        } => {
            assert_eq!(user_id, admin.id);
            assert_eq!(slug, "ops");
            assert_eq!(users_added, 2);
        }
        other => panic!("expected RoomFilled, got {other:?}"),
    }

    assert!(
        ChatRoomMember::is_member(&client, room.id, admin.id)
            .await
            .unwrap()
    );
    assert!(
        ChatRoomMember::is_member(&client, room.id, existing_member.id)
            .await
            .unwrap()
    );
    assert!(
        ChatRoomMember::is_member(&client, room.id, untouched_user.id)
            .await
            .unwrap()
    );
    let refreshed_room = ChatRoom::get(&client, room.id)
        .await
        .expect("reload room")
        .expect("room exists");
    assert!(refreshed_room.auto_join);
}

#[tokio::test]
async fn fill_room_task_rejects_private_room() {
    let test_db = new_test_db().await;
    let service = ChatService::new(
        test_db.db.clone(),
        NotificationService::new(test_db.db.clone()),
    );
    let mut events = service.subscribe_events();
    let client = test_db.db.get().await.expect("db client");

    let admin = create_test_user(&test_db.db, "fill_private_admin").await;
    let untouched_user = create_test_user(&test_db.db, "fill_private_untouched").await;
    let room = ChatRoom::create_private_room(&client, "staff")
        .await
        .expect("create private room");

    service.fill_room_task(admin.id, "staff".to_string());

    let event = timeout(Duration::from_secs(2), events.recv())
        .await
        .expect("event timeout")
        .expect("event");
    match event {
        ChatEvent::AdminFailed { user_id, message } => {
            assert_eq!(user_id, admin.id);
            assert_eq!(message, "Only public rooms can be filled");
        }
        other => panic!("expected AdminFailed, got {other:?}"),
    }

    assert!(
        !ChatRoomMember::is_member(&client, room.id, admin.id)
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

    service.delete_message_task(author.id, msg.id, Permissions::default());

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
    service.delete_message_task(admin.id, msg.id, Permissions::new(true, false));

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

    let audit = ModerationAuditLog::all(&client).await.expect("audit log");
    let audit_count = audit
        .iter()
        .filter(|entry| {
            entry.actor_user_id == admin.id
                && entry.action == "message_delete"
                && entry.target_id == Some(msg.id)
        })
        .count();
    assert_eq!(audit_count, 1);
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

#[tokio::test]
async fn mod_room_ban_command_bans_kicks_and_audits() {
    let test_db = new_test_db().await;
    let service = ChatService::new(
        test_db.db.clone(),
        NotificationService::new(test_db.db.clone()),
    );
    let mut events = service.subscribe_events();
    let client = test_db.db.get().await.expect("db client");

    let actor = create_test_user(&test_db.db, "mod_ban_actor").await;
    let target = create_test_user(&test_db.db, "mod_ban_target").await;
    let room = ChatRoom::get_or_create_public_room(&client, "mod-ban-room")
        .await
        .expect("create room");
    ChatRoomMember::join(&client, room.id, target.id)
        .await
        .expect("join target");

    let request_id = Uuid::now_v7();
    service.run_mod_command_task(
        actor.id,
        Permissions::new(false, true),
        request_id,
        "room ban #mod-ban-room @mod_ban_target 1h test cleanup".to_string(),
    );

    let event = timeout(Duration::from_secs(2), events.recv())
        .await
        .expect("event timeout")
        .expect("event");
    match event {
        ChatEvent::ModCommandOutput {
            user_id,
            request_id: got_request,
            lines,
            success,
        } => {
            assert_eq!(user_id, actor.id);
            assert_eq!(got_request, request_id);
            assert!(success, "unexpected mod command failure: {lines:?}");
            assert_eq!(lines, vec!["banned @mod_ban_target in #mod-ban-room"]);
        }
        other => panic!("expected ModCommandOutput, got {other:?}"),
    }

    assert!(
        RoomBan::is_active_for_room_and_user(&client, room.id, target.id)
            .await
            .expect("room ban lookup")
    );
    assert!(
        !ChatRoomMember::is_member(&client, room.id, target.id)
            .await
            .expect("membership lookup")
    );
    let audit = ModerationAuditLog::all(&client).await.expect("audit log");
    let audit_count = audit
        .iter()
        .filter(|entry| {
            entry.actor_user_id == actor.id
                && entry.action == "room_ban"
                && entry.target_id == Some(target.id)
        })
        .count();
    assert_eq!(audit_count, 1);
}

#[tokio::test]
async fn mod_rename_room_command_updates_slug_and_audits() {
    let test_db = new_test_db().await;
    let service = ChatService::new(
        test_db.db.clone(),
        NotificationService::new(test_db.db.clone()),
    );
    let mut events = service.subscribe_events();
    let mut moderation_events = service.subscribe_moderation_events();
    let client = test_db.db.get().await.expect("db client");

    let actor = create_test_user(&test_db.db, "rename_room_actor").await;
    let room = ChatRoom::get_or_create_public_room(&client, "rename-room-old")
        .await
        .expect("create room");

    let request_id = Uuid::now_v7();
    service.run_mod_command_task(
        actor.id,
        Permissions::new(true, false),
        request_id,
        "rename-room #rename-room-old #rename_room_new".to_string(),
    );

    let event = timeout(Duration::from_secs(2), events.recv())
        .await
        .expect("event timeout")
        .expect("event");
    match event {
        ChatEvent::ModCommandOutput {
            user_id,
            request_id: got_request,
            lines,
            success,
        } => {
            assert_eq!(user_id, actor.id);
            assert_eq!(got_request, request_id);
            assert!(success, "unexpected mod command failure: {lines:?}");
            assert_eq!(lines, vec!["renamed #rename-room-old to #rename-room-new"]);
        }
        other => panic!("expected ModCommandOutput, got {other:?}"),
    }

    let renamed = ChatRoom::find_non_dm_by_slug(&client, "rename-room-new")
        .await
        .expect("renamed room lookup")
        .expect("renamed room");
    assert_eq!(renamed.id, room.id);
    assert!(
        ChatRoom::find_non_dm_by_slug(&client, "rename-room-old")
            .await
            .expect("old room lookup")
            .is_none()
    );

    let moderation_event = timeout(Duration::from_secs(2), moderation_events.recv())
        .await
        .expect("moderation event timeout")
        .expect("moderation event");
    match moderation_event {
        ModerationEvent::RoomRenamed {
            actor_user_id,
            room_id,
            old_slug,
            new_slug,
        } => {
            assert_eq!(actor_user_id, actor.id);
            assert_eq!(room_id, room.id);
            assert_eq!(old_slug, "rename-room-old");
            assert_eq!(new_slug, "rename-room-new");
        }
        other => panic!("expected room renamed moderation event, got {other:?}"),
    }

    let audit = ModerationAuditLog::all(&client).await.expect("audit log");
    let audit_count = audit
        .iter()
        .filter(|entry| {
            entry.actor_user_id == actor.id
                && entry.action == "rename_room"
                && entry.target_kind == "room"
                && entry.target_id == Some(room.id)
                && entry.metadata["old_slug"] == "rename-room-old"
                && entry.metadata["new_slug"] == "rename-room-new"
        })
        .count();
    assert_eq!(audit_count, 1);
}

#[tokio::test]
async fn mod_rename_user_command_updates_username_active_user_and_audits() {
    let test_db = new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let actor = create_test_user(&test_db.db, "rename_user_actor").await;
    let target = create_test_user(&test_db.db, "rename_user_old").await;
    let active_users = Arc::new(Mutex::new(HashMap::from([(
        target.id,
        ActiveUser {
            username: target.username.clone(),
            fingerprint: Some(target.fingerprint.clone()),
            peer_ip: None,
            sessions: Vec::new(),
            connection_count: 1,
            last_login_at: std::time::Instant::now(),
        },
    )])));
    let service = ChatService::new_with_active_users(
        test_db.db.clone(),
        NotificationService::new(test_db.db.clone()),
        active_users.clone(),
    );
    let mut events = service.subscribe_events();
    let mut moderation_events = service.subscribe_moderation_events();

    let request_id = Uuid::now_v7();
    service.run_mod_command_task(
        actor.id,
        Permissions::new(false, true),
        request_id,
        "rename-user @rename_user_old @rename_user_new".to_string(),
    );

    let event = timeout(Duration::from_secs(2), events.recv())
        .await
        .expect("event timeout")
        .expect("event");
    match event {
        ChatEvent::ModCommandOutput {
            request_id: got_request,
            lines,
            success,
            ..
        } => {
            assert_eq!(got_request, request_id);
            assert!(success, "unexpected mod command failure: {lines:?}");
            assert_eq!(lines, vec!["renamed @rename_user_old to @rename_user_new"]);
        }
        other => panic!("expected ModCommandOutput, got {other:?}"),
    }

    let moderation_event = timeout(Duration::from_secs(2), moderation_events.recv())
        .await
        .expect("moderation event timeout")
        .expect("moderation event");
    match moderation_event {
        ModerationEvent::UserRenamed {
            actor_user_id,
            target_user_id,
            old_username,
            new_username,
            active_user_updated,
        } => {
            assert_eq!(actor_user_id, actor.id);
            assert_eq!(target_user_id, target.id);
            assert_eq!(old_username, "rename_user_old");
            assert_eq!(new_username, "rename_user_new");
            assert!(active_user_updated);
        }
        other => panic!("expected user renamed moderation event, got {other:?}"),
    }

    assert!(
        User::find_by_username(&client, "rename_user_old")
            .await
            .expect("old username lookup")
            .is_none()
    );
    let renamed = User::find_by_username(&client, "rename_user_new")
        .await
        .expect("new username lookup")
        .expect("renamed user exists");
    assert_eq!(renamed.id, target.id);
    assert_eq!(
        active_users
            .lock()
            .expect("active users lock")
            .get(&target.id)
            .expect("active target")
            .username,
        "rename_user_new"
    );

    let audit = ModerationAuditLog::all(&client).await.expect("audit log");
    let audit_count = audit
        .iter()
        .filter(|entry| {
            entry.actor_user_id == actor.id
                && entry.action == "rename_user"
                && entry.target_kind == "user"
                && entry.target_id == Some(target.id)
                && entry.metadata["old_username"] == "rename_user_old"
                && entry.metadata["new_username"] == "rename_user_new"
        })
        .count();
    assert_eq!(audit_count, 1);
}

#[tokio::test]
async fn mod_server_kick_command_terminates_active_sessions_and_audits() {
    let test_db = new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let actor = create_test_user(&test_db.db, "server_kick_actor").await;
    let target = create_test_user(&test_db.db, "server_kick_target").await;
    let peer_ip: IpAddr = "203.0.113.11".parse().expect("test ip");
    let session_token = "server-kick-session".to_string();
    let active_users = Arc::new(Mutex::new(HashMap::from([(
        target.id,
        ActiveUser {
            username: target.username.clone(),
            fingerprint: Some(target.fingerprint.clone()),
            peer_ip: Some(peer_ip),
            sessions: vec![ActiveSession {
                token: session_token.clone(),
                fingerprint: Some(target.fingerprint.clone()),
                peer_ip: Some(peer_ip),
            }],
            connection_count: 1,
            last_login_at: std::time::Instant::now(),
        },
    )])));
    let registry = SessionRegistry::new();
    let (session_tx, mut session_rx) = tokio::sync::mpsc::channel(1);
    registry.register(session_token, session_tx).await;
    let service = ChatService::new_with_active_users(
        test_db.db.clone(),
        NotificationService::new(test_db.db.clone()),
        active_users,
    )
    .with_session_registry(registry);
    let mut events = service.subscribe_events();

    let request_id = Uuid::now_v7();
    service.run_mod_command_task(
        actor.id,
        Permissions::new(false, true),
        request_id,
        "server kick @server_kick_target cool off".to_string(),
    );

    let event = timeout(Duration::from_secs(2), events.recv())
        .await
        .expect("event timeout")
        .expect("event");
    match event {
        ChatEvent::ModCommandOutput {
            user_id,
            request_id: got_request,
            lines,
            success,
        } => {
            assert_eq!(user_id, actor.id);
            assert_eq!(got_request, request_id);
            assert!(success, "unexpected mod command failure: {lines:?}");
            assert_eq!(lines, vec!["kicked @server_kick_target"]);
        }
        other => panic!("expected ModCommandOutput, got {other:?}"),
    }
    let message = timeout(Duration::from_secs(2), session_rx.recv())
        .await
        .expect("session message timeout")
        .expect("session message");
    match message {
        SessionMessage::Terminate { reason } => assert_eq!(reason, "server kick"),
        other => panic!("expected terminate message, got {other:?}"),
    }

    let audit = ModerationAuditLog::all(&client).await.expect("audit log");
    let audit_count = audit
        .iter()
        .filter(|entry| {
            entry.actor_user_id == actor.id
                && entry.action == "server_kick"
                && entry.target_id == Some(target.id)
        })
        .count();
    assert_eq!(audit_count, 1);
}

#[tokio::test]
async fn mod_server_ban_command_bans_and_terminates_active_sessions() {
    let test_db = new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let actor = create_test_user(&test_db.db, "server_ban_actor").await;
    let target = create_test_user(&test_db.db, "server_ban_target").await;
    let peer_ip: IpAddr = "203.0.113.12".parse().expect("test ip");
    let session_token = "server-ban-session".to_string();
    let active_users = Arc::new(Mutex::new(HashMap::from([(
        target.id,
        ActiveUser {
            username: target.username.clone(),
            fingerprint: Some(target.fingerprint.clone()),
            peer_ip: Some(peer_ip),
            sessions: vec![ActiveSession {
                token: session_token.clone(),
                fingerprint: Some(target.fingerprint.clone()),
                peer_ip: Some(peer_ip),
            }],
            connection_count: 1,
            last_login_at: std::time::Instant::now(),
        },
    )])));
    let registry = SessionRegistry::new();
    let (session_tx, mut session_rx) = tokio::sync::mpsc::channel(1);
    registry.register(session_token, session_tx).await;
    let service = ChatService::new_with_active_users(
        test_db.db.clone(),
        NotificationService::new(test_db.db.clone()),
        active_users,
    )
    .with_session_registry(registry);
    let mut events = service.subscribe_events();
    let mut moderation_events = service.subscribe_moderation_events();

    let request_id = Uuid::now_v7();
    service.run_mod_command_task(
        actor.id,
        Permissions::new(false, true),
        request_id,
        "server ban @server_ban_target 1h test ban".to_string(),
    );

    let event = timeout(Duration::from_secs(2), events.recv())
        .await
        .expect("event timeout")
        .expect("event");
    match event {
        ChatEvent::ModCommandOutput {
            user_id,
            request_id: got_request,
            lines,
            success,
        } => {
            assert_eq!(user_id, actor.id);
            assert_eq!(got_request, request_id);
            assert!(success, "unexpected mod command failure: {lines:?}");
            assert_eq!(lines, vec!["banned @server_ban_target"]);
        }
        other => panic!("expected ModCommandOutput, got {other:?}"),
    }
    let message = timeout(Duration::from_secs(2), session_rx.recv())
        .await
        .expect("session message timeout")
        .expect("session message");
    match message {
        SessionMessage::Terminate { reason } => assert_eq!(reason, "server ban"),
        other => panic!("expected terminate message, got {other:?}"),
    }
    let moderation_event = timeout(Duration::from_secs(2), moderation_events.recv())
        .await
        .expect("moderation event timeout")
        .expect("moderation event");
    match moderation_event {
        ModerationEvent::ServerUserAction {
            actor_user_id,
            target_user_id,
            target_username,
            action,
            reason,
            terminated_sessions,
        } => {
            assert_eq!(actor_user_id, actor.id);
            assert_eq!(target_user_id, target.id);
            assert_eq!(target_username, "server_ban_target");
            assert_eq!(action, ServerUserAction::Ban);
            assert_eq!(reason, "test ban");
            assert_eq!(terminated_sessions, 1);
        }
        other => panic!("expected server user moderation event, got {other:?}"),
    }

    let ban = ServerBan::find_active_for_user_id(&client, target.id)
        .await
        .expect("server ban lookup")
        .expect("active server ban");
    assert_eq!(ban.target_user_id, target.id);
    assert_eq!(ban.ip_address.as_deref(), Some("203.0.113.12"));
    assert_eq!(
        ban.snapshot_username.as_deref(),
        Some(target.username.as_str())
    );
    assert_eq!(
        ban.fingerprint.as_deref(),
        Some(target.fingerprint.as_str())
    );

    let audit = ModerationAuditLog::all(&client).await.expect("audit log");
    let audit_count = audit
        .iter()
        .filter(|entry| {
            entry.actor_user_id == actor.id
                && entry.action == "server_ban"
                && entry.target_id == Some(target.id)
        })
        .count();
    assert_eq!(audit_count, 1);
}

#[tokio::test]
async fn mod_artboard_ban_command_notifies_active_sessions() {
    let test_db = new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let actor = create_test_user(&test_db.db, "artboard_ban_actor").await;
    let target = create_test_user(&test_db.db, "artboard_ban_target").await;
    let session_token = "artboard-ban-session".to_string();
    let active_users = Arc::new(Mutex::new(HashMap::from([(
        target.id,
        ActiveUser {
            username: target.username.clone(),
            fingerprint: Some(target.fingerprint.clone()),
            peer_ip: None,
            sessions: vec![ActiveSession {
                token: session_token.clone(),
                fingerprint: Some(target.fingerprint.clone()),
                peer_ip: None,
            }],
            connection_count: 1,
            last_login_at: std::time::Instant::now(),
        },
    )])));
    let registry = SessionRegistry::new();
    let (session_tx, mut session_rx) = tokio::sync::mpsc::channel(1);
    registry.register(session_token, session_tx).await;
    let service = ChatService::new_with_active_users(
        test_db.db.clone(),
        NotificationService::new(test_db.db.clone()),
        active_users,
    )
    .with_session_registry(registry);
    let mut events = service.subscribe_events();

    let request_id = Uuid::now_v7();
    service.run_mod_command_task(
        actor.id,
        Permissions::new(false, true),
        request_id,
        "artboard ban @artboard_ban_target 1h paint cooldown".to_string(),
    );

    let event = timeout(Duration::from_secs(2), events.recv())
        .await
        .expect("event timeout")
        .expect("event");
    match event {
        ChatEvent::ModCommandOutput {
            user_id,
            request_id: got_request,
            lines,
            success,
        } => {
            assert_eq!(user_id, actor.id);
            assert_eq!(got_request, request_id);
            assert!(success, "unexpected mod command failure: {lines:?}");
            assert_eq!(lines, vec!["artboard-banned @artboard_ban_target"]);
        }
        other => panic!("expected ModCommandOutput, got {other:?}"),
    }
    let message = timeout(Duration::from_secs(2), session_rx.recv())
        .await
        .expect("session message timeout")
        .expect("session message");
    match message {
        SessionMessage::ArtboardBanChanged { banned, expires_at } => {
            assert!(banned);
            assert!(expires_at.is_some());
        }
        other => panic!("expected artboard ban status message, got {other:?}"),
    }

    assert!(
        ArtboardBan::is_active_for_user(&client, target.id)
            .await
            .expect("artboard ban lookup")
    );
}

#[tokio::test]
async fn mod_artboard_restore_command_restores_daily_snapshot_and_audits() {
    let test_db = new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let actor = create_test_user(&test_db.db, "artboard_restore_actor").await;

    let mut main_canvas = Canvas::with_size(dartboard::CANVAS_WIDTH, dartboard::CANVAS_HEIGHT);
    let _ = main_canvas.put_glyph(Pos { x: 0, y: 0 }, 'M');
    let mut main_provenance = ArtboardProvenance::default();
    main_provenance.set_username(Pos { x: 0, y: 0 }, "main_owner");

    let mut daily_canvas = Canvas::with_size(dartboard::CANVAS_WIDTH, dartboard::CANVAS_HEIGHT);
    let _ = daily_canvas.put_glyph(Pos { x: 0, y: 0 }, 'D');
    let mut daily_provenance = ArtboardProvenance::default();
    daily_provenance.set_username(Pos { x: 0, y: 0 }, "daily_owner");

    ArtboardSnapshot::upsert(
        &client,
        ArtboardSnapshot::MAIN_BOARD_KEY,
        serde_json::to_value(&main_canvas).expect("serialize main canvas"),
        serde_json::to_value(&main_provenance).expect("serialize main provenance"),
    )
    .await
    .expect("insert main snapshot");
    ArtboardSnapshot::upsert(
        &client,
        "daily:2026-05-06",
        serde_json::to_value(&daily_canvas).expect("serialize daily canvas"),
        serde_json::to_value(&daily_provenance).expect("serialize daily provenance"),
    )
    .await
    .expect("insert daily snapshot");

    let shared_provenance = main_provenance.shared();
    let server = dartboard::spawn_persistent_server_with_interval(
        test_db.db.clone(),
        Some(main_canvas),
        shared_provenance.clone(),
        Duration::from_millis(10),
    );
    server.submit_op_for(
        0,
        1,
        CanvasOp::PaintCell {
            pos: Pos { x: 0, y: 0 },
            ch: 'O',
            fg: RgbColor::new(1, 2, 3),
        },
    );
    let service = ChatService::new(
        test_db.db.clone(),
        NotificationService::new(test_db.db.clone()),
    )
    .with_moderation_infra(
        ModerationInfra::default().with_artboard_handles(server.clone(), shared_provenance.clone()),
    );
    let mut events = service.subscribe_events();
    let mut moderation_events = service.subscribe_moderation_events();

    let request_id = Uuid::now_v7();
    service.run_mod_command_task(
        actor.id,
        Permissions::new(false, true),
        request_id,
        "artboard restore 2026-05-06 rollback vandalism".to_string(),
    );

    let event = timeout(Duration::from_secs(2), events.recv())
        .await
        .expect("event timeout")
        .expect("event");
    match event {
        ChatEvent::ModCommandOutput {
            request_id: got_request,
            lines,
            success,
            ..
        } => {
            assert_eq!(got_request, request_id);
            assert!(success, "unexpected mod command failure: {lines:?}");
            assert_eq!(lines[0], "restored artboard from daily:2026-05-06");
            assert!(
                lines
                    .get(1)
                    .is_some_and(|line| line.starts_with("backup: restore-backup:main:")),
                "missing backup line: {lines:?}"
            );
        }
        other => panic!("expected ModCommandOutput, got {other:?}"),
    }

    let moderation_event = timeout(Duration::from_secs(2), moderation_events.recv())
        .await
        .expect("moderation event timeout")
        .expect("moderation event");
    match moderation_event {
        ModerationEvent::ArtboardRestored {
            actor_user_id,
            source_key,
            backup_key,
            reason,
        } => {
            assert_eq!(actor_user_id, actor.id);
            assert_eq!(source_key, "daily:2026-05-06");
            assert!(backup_key.is_some());
            assert_eq!(reason, "rollback vandalism");
        }
        other => panic!("expected artboard restored moderation event, got {other:?}"),
    }

    let live_canvas = server.canvas_snapshot();
    assert_eq!(live_canvas.get(Pos { x: 0, y: 0 }), 'D');
    assert_eq!(
        shared_provenance
            .lock()
            .expect("shared provenance lock")
            .username_at(&live_canvas, Pos { x: 0, y: 0 }),
        Some("daily_owner")
    );

    let main_snapshot =
        ArtboardSnapshot::find_by_board_key(&client, ArtboardSnapshot::MAIN_BOARD_KEY)
            .await
            .expect("load restored main")
            .expect("restored main exists");
    let persisted_canvas: Canvas =
        serde_json::from_value(main_snapshot.canvas).expect("decode persisted canvas");
    assert_eq!(persisted_canvas.get(Pos { x: 0, y: 0 }), 'D');
    sleep(Duration::from_millis(50)).await;
    let main_snapshot =
        ArtboardSnapshot::find_by_board_key(&client, ArtboardSnapshot::MAIN_BOARD_KEY)
            .await
            .expect("reload restored main")
            .expect("restored main still exists");
    let persisted_canvas: Canvas =
        serde_json::from_value(main_snapshot.canvas).expect("decode persisted canvas");
    assert_eq!(persisted_canvas.get(Pos { x: 0, y: 0 }), 'D');

    let backups = ArtboardSnapshot::list_by_board_key_prefix(&client, "restore-backup:main:")
        .await
        .expect("backup snapshots");
    assert_eq!(backups.len(), 1);

    let audit = ModerationAuditLog::all(&client).await.expect("audit log");
    let audit_count = audit
        .iter()
        .filter(|entry| {
            entry.actor_user_id == actor.id
                && entry.action == "artboard_restore"
                && entry.target_kind == "artboard"
                && entry.metadata["source_key"] == "daily:2026-05-06"
                && entry.metadata["reason"] == "rollback vandalism"
        })
        .count();
    assert_eq!(audit_count, 1);
}

#[tokio::test]
async fn mod_bans_command_lists_active_bans() {
    let test_db = new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let service = ChatService::new(
        test_db.db.clone(),
        NotificationService::new(test_db.db.clone()),
    );
    let mut events = service.subscribe_events();

    let actor = create_test_user(&test_db.db, "list_bans_actor").await;
    let server_target = create_test_user(&test_db.db, "list_server_target").await;
    let artboard_target = create_test_user(&test_db.db, "list_artboard_target").await;
    let room_target = create_test_user(&test_db.db, "list_room_target").await;
    let room = ChatRoom::get_or_create_public_room(&client, "list-bans-room")
        .await
        .expect("create room");

    for command in [
        "server ban @list_server_target 1h server reason",
        "artboard ban @list_artboard_target 1h art reason",
        "room ban #list-bans-room @list_room_target 1h room reason",
    ] {
        service.run_mod_command_task(
            actor.id,
            Permissions::new(false, true),
            Uuid::now_v7(),
            command.to_string(),
        );
        let event = timeout(Duration::from_secs(2), events.recv())
            .await
            .expect("event timeout")
            .expect("event");
        assert!(matches!(
            event,
            ChatEvent::ModCommandOutput { success: true, .. }
        ));
    }

    let request_id = Uuid::now_v7();
    service.run_mod_command_task(
        actor.id,
        Permissions::new(false, true),
        request_id,
        "bans 10".to_string(),
    );

    let event = timeout(Duration::from_secs(2), events.recv())
        .await
        .expect("event timeout")
        .expect("event");
    match event {
        ChatEvent::ModCommandOutput {
            request_id: got_request,
            lines,
            success,
            ..
        } => {
            assert_eq!(got_request, request_id);
            assert!(success, "unexpected mod command failure: {lines:?}");
            assert!(lines.iter().any(|line| line == "server bans:"));
            assert!(
                lines
                    .iter()
                    .any(|line| line.contains("@list_server_target"))
            );
            assert!(lines.iter().any(|line| line == "artboard bans:"));
            assert!(
                lines
                    .iter()
                    .any(|line| line.contains("@list_artboard_target"))
            );
            assert!(lines.iter().any(|line| line == "room bans:"));
            assert!(lines.iter().any(|line| line.contains("#list-bans-room")));
            assert!(lines.iter().any(|line| line.contains("@list_room_target")));
        }
        other => panic!("expected ModCommandOutput, got {other:?}"),
    }

    assert!(
        RoomBan::is_active_for_room_and_user(&client, room.id, room_target.id)
            .await
            .expect("room ban lookup")
    );
    assert!(
        ServerBan::find_active_for_user_id(&client, server_target.id)
            .await
            .expect("server ban lookup")
            .is_some()
    );
    assert!(
        ArtboardBan::is_active_for_user(&client, artboard_target.id)
            .await
            .expect("artboard ban lookup")
    );
}

#[tokio::test]
async fn mod_audit_command_lists_recent_audit_entries() {
    let test_db = new_test_db().await;
    let service = ChatService::new(
        test_db.db.clone(),
        NotificationService::new(test_db.db.clone()),
    );
    let mut events = service.subscribe_events();

    let actor = create_test_user(&test_db.db, "list_audit_actor").await;
    let _target = create_test_user(&test_db.db, "list_audit_target").await;

    service.run_mod_command_task(
        actor.id,
        Permissions::new(false, true),
        Uuid::now_v7(),
        "server kick @list_audit_target audit reason".to_string(),
    );
    let event = timeout(Duration::from_secs(2), events.recv())
        .await
        .expect("event timeout")
        .expect("event");
    assert!(matches!(
        event,
        ChatEvent::ModCommandOutput { success: true, .. }
    ));

    let request_id = Uuid::now_v7();
    service.run_mod_command_task(
        actor.id,
        Permissions::new(false, true),
        request_id,
        "audit 5".to_string(),
    );

    let event = timeout(Duration::from_secs(2), events.recv())
        .await
        .expect("event timeout")
        .expect("event");
    match event {
        ChatEvent::ModCommandOutput {
            request_id: got_request,
            lines,
            success,
            ..
        } => {
            assert_eq!(got_request, request_id);
            assert!(success, "unexpected mod command failure: {lines:?}");
            assert!(
                lines
                    .iter()
                    .any(|line| line == "recent audit log entries (limit 5)")
            );
            assert!(lines.iter().any(|line| line.contains("@list_audit_actor")
                && line.contains("server_kick")
                && line.contains("@list_audit_target")
                && line.contains("audit reason")));
        }
        other => panic!("expected ModCommandOutput, got {other:?}"),
    }
}

#[tokio::test]
async fn mod_room_ban_command_notifies_target_sessions_to_drop_room() {
    let test_db = new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let actor = create_test_user(&test_db.db, "room_notify_actor").await;
    let target = create_test_user(&test_db.db, "room_notify_target").await;
    let room = ChatRoom::get_or_create_public_room(&client, "room-notify")
        .await
        .expect("create room");
    ChatRoomMember::join(&client, room.id, target.id)
        .await
        .expect("join target");

    let session_token = "room-notify-session".to_string();
    let active_users = Arc::new(Mutex::new(HashMap::from([(
        target.id,
        ActiveUser {
            username: target.username.clone(),
            fingerprint: Some(target.fingerprint.clone()),
            peer_ip: None,
            sessions: vec![ActiveSession {
                token: session_token.clone(),
                fingerprint: Some(target.fingerprint.clone()),
                peer_ip: None,
            }],
            connection_count: 1,
            last_login_at: std::time::Instant::now(),
        },
    )])));
    let registry = SessionRegistry::new();
    let (session_tx, mut session_rx) = tokio::sync::mpsc::channel(1);
    registry.register(session_token, session_tx).await;
    let service = ChatService::new_with_active_users(
        test_db.db.clone(),
        NotificationService::new(test_db.db.clone()),
        active_users,
    )
    .with_session_registry(registry);
    let mut events = service.subscribe_events();

    let request_id = Uuid::now_v7();
    service.run_mod_command_task(
        actor.id,
        Permissions::new(false, true),
        request_id,
        "room ban #room-notify @room_notify_target 1h test".to_string(),
    );

    let event = timeout(Duration::from_secs(2), events.recv())
        .await
        .expect("event timeout")
        .expect("event");
    assert!(matches!(
        event,
        ChatEvent::ModCommandOutput { success: true, .. }
    ));
    let message = timeout(Duration::from_secs(2), session_rx.recv())
        .await
        .expect("session message timeout")
        .expect("session message");
    match message {
        SessionMessage::RoomRemoved {
            room_id,
            slug,
            message,
        } => {
            assert_eq!(room_id, room.id);
            assert_eq!(slug, "room-notify");
            assert_eq!(message, "Banned from room");
        }
        other => panic!("expected room removed message, got {other:?}"),
    }
}

#[tokio::test]
async fn grant_mod_command_updates_active_session_permissions() {
    let test_db = new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let actor = create_test_user(&test_db.db, "grant_mod_actor").await;
    let target = create_test_user(&test_db.db, "grant_mod_target").await;

    let session_token = "grant-mod-session".to_string();
    let active_users = Arc::new(Mutex::new(HashMap::from([(
        target.id,
        ActiveUser {
            username: target.username.clone(),
            fingerprint: Some(target.fingerprint.clone()),
            peer_ip: None,
            sessions: vec![ActiveSession {
                token: session_token.clone(),
                fingerprint: Some(target.fingerprint.clone()),
                peer_ip: None,
            }],
            connection_count: 1,
            last_login_at: std::time::Instant::now(),
        },
    )])));
    let registry = SessionRegistry::new();
    let (session_tx, mut session_rx) = tokio::sync::mpsc::channel(1);
    registry.register(session_token, session_tx).await;
    let service = ChatService::new_with_active_users(
        test_db.db.clone(),
        NotificationService::new(test_db.db.clone()),
        active_users,
    )
    .with_session_registry(registry);
    let mut events = service.subscribe_events();

    let request_id = Uuid::now_v7();
    service.run_mod_command_task(
        actor.id,
        Permissions::new(true, false),
        request_id,
        "grant mod @grant_mod_target".to_string(),
    );

    let event = timeout(Duration::from_secs(2), events.recv())
        .await
        .expect("event timeout")
        .expect("event");
    assert!(matches!(
        event,
        ChatEvent::ModCommandOutput { success: true, .. }
    ));
    let message = timeout(Duration::from_secs(2), session_rx.recv())
        .await
        .expect("session message timeout")
        .expect("session message");
    match message {
        SessionMessage::PermissionsChanged { permissions } => {
            assert_eq!(permissions, Permissions::new(false, true));
        }
        other => panic!("expected permissions changed message, got {other:?}"),
    }

    let updated = User::get(&client, target.id)
        .await
        .expect("user lookup")
        .expect("target user");
    assert!(updated.is_moderator);
}

#[tokio::test]
async fn send_message_task_rejects_active_room_ban_even_if_still_member() {
    let test_db = new_test_db().await;
    let service = ChatService::new(
        test_db.db.clone(),
        NotificationService::new(test_db.db.clone()),
    );
    let mut events = service.subscribe_events();
    let client = test_db.db.get().await.expect("db client");

    let actor = create_test_user(&test_db.db, "send_ban_actor").await;
    let user = create_test_user(&test_db.db, "send_ban_target").await;
    let room = ChatRoom::get_or_create_public_room(&client, "send-ban-room")
        .await
        .expect("create room");
    ChatRoomMember::join(&client, room.id, user.id)
        .await
        .expect("join user before ban");
    RoomBan::activate(&client, room.id, user.id, actor.id, "test ban", None)
        .await
        .expect("activate ban");

    let request_id = Uuid::now_v7();
    service.send_message_task(
        user.id,
        room.id,
        room.slug.clone(),
        "should not send".to_string(),
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
            assert_eq!(message, "You are banned from this room.");
        }
        other => panic!("expected SendFailed, got {other:?}"),
    }
}
