use crate::app::artboard::provenance::ArtboardProvenance;
use crate::app::chat::notifications::svc::NotificationService;
use crate::app::chat::svc::{ChatEvent, ChatReactionAction, ChatService};
use crate::authz::Permissions;
use crate::dartboard;
use crate::moderation::command::{RoomModAction, ServerUserAction};
use crate::moderation::event::ModerationEvent;
use crate::moderation::service::{ModerationInfra, RoomModRequest, RoomRef};
use crate::session::{SessionMessage, SessionRegistry};
use crate::state::{ActiveSession, ActiveUser};
use dartboard_core::{Canvas, CanvasOp, Pos, RgbColor};
use late_core::models::{
    artboard::Snapshot as ArtboardSnapshot,
    artboard_ban::ArtboardBan,
    chat_message::{ChatMessage, ChatMessageParams},
    chat_room::{ChatRoom, ChatRoomParams},
    chat_room_member::ChatRoomMember,
    chat_slow_mode::ChatSlowMode,
    game_room::GameKind,
    moderation_audit_log::ModerationAuditLog,
    profile::{Profile, ProfileParams},
    room_ban::RoomBan,
    server_ban::ServerBan,
    user::{RightSidebarMode, User, default_right_sidebar_components},
};
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use tokio::time::{Duration, sleep, timeout};
use uuid::Uuid;

use crate::test_helpers::new_test_db;
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
async fn send_pre_translates_to_english_for_opted_in_authors() {
    use crate::app::ai::svc::AiService;
    use crate::app::ai::translate::{TranslationOutcome, TranslationService};
    use late_core::models::message_translation::TranslateLang;

    let test_db = new_test_db().await;
    let translation = TranslationService::new(test_db.db.clone(), AiService::new(false, None));
    let service = ChatService::new(
        test_db.db.clone(),
        NotificationService::new(test_db.db.clone()),
    )
    .with_translation_service(translation.clone());
    let client = test_db.db.get().await.expect("db client");
    let author = create_test_user(&test_db.db, "pretranslate_author").await;
    let room = ChatRoom::ensure_lounge(&client).await.expect("lounge");
    ChatRoomMember::join(&client, room.id, author.id)
        .await
        .expect("join author");
    // Opt in through the production settings write, so the whole chain
    // (settings key -> send hook -> service request) is what's pinned.
    Profile::update(
        &client,
        author.id,
        ProfileParams {
            username: "pretranslate_author".to_string(),
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
            theme_id: None,
            enable_background_color: false,
            text_brightness_adjustment: 0,
            show_right_sidebar: true,
            right_sidebar_mode: RightSidebarMode::On,
            right_sidebar_components: default_right_sidebar_components(),
            show_room_list_sidebar: true,
            room_list_mode: late_core::models::user::RoomListMode::On,
            keep_composer_focused: false,
            start_with_music_muted: false,
            land_on_home: false,
            show_flag_fallback: false,
            show_pet_strip: true,
            translate_to: TranslateLang::En,
            auto_translate: false,
            translate_mine_to_en: true,
            favorite_room_ids: Vec::new(),
            favorite_theme_ids: Vec::new(),
        },
    )
    .await
    .expect("opt author in");

    // A bystander with default settings pins the opt-in gate: their send
    // completes first (SendSucceeded lands after the pre-translate hook
    // runs), so if the gate ever disappeared, their request would fire
    // before the opted author's and the id assertion below would catch it.
    let bystander = create_test_user(&test_db.db, "pretranslate_bystander").await;
    ChatRoomMember::join(&client, room.id, bystander.id)
        .await
        .expect("join bystander");

    let mut translations = translation.subscribe();
    let mut chat_events = service.subscribe_events();
    let bystander_request = Uuid::now_v7();
    service.send_message_task(
        bystander.id,
        room.id,
        None,
        "salut tout le monde".to_string(),
        bystander_request,
        false,
    );
    loop {
        let event = timeout(Duration::from_secs(5), chat_events.recv())
            .await
            .expect("bystander send timeout")
            .expect("chat channel open");
        if matches!(event, ChatEvent::SendSucceeded { request_id, .. } if request_id == bystander_request)
        {
            break;
        }
    }

    service.send_message_task(
        author.id,
        room.id,
        None,
        "bonjour tout le monde".to_string(),
        Uuid::now_v7(),
        false,
    );
    let opted_message_id = loop {
        let event = timeout(Duration::from_secs(5), chat_events.recv())
            .await
            .expect("author send timeout")
            .expect("chat channel open");
        if let ChatEvent::MessageCreated { message, .. } = event
            && message.user_id == author.id
        {
            break message.id;
        }
    };

    // AI is disabled, so the request resolves as Failed; the event alone
    // proves the send path fired an English request, and its message id
    // proves it fired for the opted-in author only.
    let event = timeout(Duration::from_secs(5), translations.recv())
        .await
        .expect("translation event timeout")
        .expect("translation channel open");
    assert_eq!(event.message_id, opted_message_id);
    assert_eq!(event.room_id, room.id);
    assert_eq!(event.target, TranslateLang::En);
    assert!(matches!(event.outcome, TranslationOutcome::Failed));
    assert!(
        event.author_shared,
        "the author's opt-in marks the request shared, so every English reader displays it"
    );
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

    service.toggle_message_reaction_task(reactor.id, message.id, "👀".to_string());

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
            assert_eq!(reactions[0].icon, "👀");
            assert_eq!(reactions[0].count, 1);
        }
        _ => panic!("expected message reactions updated event"),
    }

    let event = timeout(Duration::from_secs(2), events.recv())
        .await
        .expect("event timeout")
        .expect("event");
    match event {
        ChatEvent::MessageReactionDelta(delta) => {
            assert_eq!(delta.room_id, room.id);
            assert_eq!(delta.message_id, message.id);
            assert_eq!(delta.actor_user_id, reactor.id);
            assert_eq!(delta.icon, "👀");
            assert_eq!(delta.action, ChatReactionAction::React);
            assert_eq!(delta.previous_icon, None);
            assert_eq!(
                delta.target_user_ids, None,
                "public room deltas broadcast; sessions filter by membership"
            );
        }
        _ => panic!("expected message reaction delta event"),
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
    let room = ChatRoom::find_non_dm_by_slug(&client, "announcements")
        .await
        .expect("find announcements room")
        .expect("announcements room");
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
async fn publishes_summary_with_rooms_and_unread_counts() {
    let test_db = new_test_db().await;
    let service = ChatService::new(
        test_db.db.clone(),
        NotificationService::new(test_db.db.clone()),
    );
    let client = test_db.db.get().await.expect("db client");

    let target_user = create_test_user(&test_db.db, "target").await;
    let author_user = create_test_user(&test_db.db, "author").await;

    let lounge_room = ChatRoom::create(
        &client,
        ChatRoomParams {
            kind: "lounge".to_string(),
            visibility: "public".to_string(),
            auto_join: true,
            permanent: true,
            slug: Some("lounge".to_string()),
            language_code: None,
            dm_user_a: None,
            dm_user_b: None,
            topic: None,
            rules: None,
            created_by: None,
        },
    )
    .await
    .expect("create lounge room");
    let lang_room = ChatRoom::get_or_create_language(&client, "en")
        .await
        .expect("language room");

    ChatRoomMember::join(&client, lounge_room.id, target_user.id)
        .await
        .expect("join target lounge");
    ChatRoomMember::join(&client, lang_room.id, target_user.id)
        .await
        .expect("join target language");
    ChatRoomMember::join(&client, lounge_room.id, author_user.id)
        .await
        .expect("join author lounge");
    ChatRoomMember::join(&client, lang_room.id, author_user.id)
        .await
        .expect("join author language");

    let lounge_message = ChatMessage::create(
        &client,
        ChatMessageParams {
            room_id: lounge_room.id,
            user_id: author_user.id,
            body: "g-msg".to_string(),
        },
    )
    .await
    .expect("lounge message");
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
    let (mut state_rx, _event_rx, _refresh_tx, refresh_task) =
        service.start_user_refresh_task(target_user.id, room_rx);

    timeout(Duration::from_secs(2), state_rx.changed())
        .await
        .expect("state timeout")
        .expect("watch changed");
    let snapshot = state_rx.borrow_and_update().clone();

    assert_eq!(snapshot.user_id, Some(target_user.id));
    assert_eq!(snapshot.lounge_room_id, Some(lounge_room.id));
    assert_eq!(snapshot.unread_counts.get(&lounge_room.id), Some(&1));
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

    let lounge_in_snapshot = snapshot
        .chat_rooms
        .iter()
        .find(|(room, _)| room.id == lounge_room.id)
        .expect("lounge room present");
    assert!(
        lounge_in_snapshot.1.is_empty(),
        "summary refresh should not preload lounge room history"
    );
    assert_ne!(lounge_message.id, lang_message.id);
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

    let lounge_room = ChatRoom::create(
        &client,
        ChatRoomParams {
            kind: "lounge".to_string(),
            visibility: "public".to_string(),
            auto_join: true,
            permanent: true,
            slug: Some("lounge".to_string()),
            language_code: None,
            dm_user_a: None,
            dm_user_b: None,
            topic: None,
            rules: None,
            created_by: None,
        },
    )
    .await
    .expect("create lounge room");
    let lang_room = ChatRoom::get_or_create_language(&client, "fr")
        .await
        .expect("language room");

    ChatRoomMember::join(&client, lounge_room.id, target_user.id)
        .await
        .expect("join target lounge");
    ChatRoomMember::join(&client, lang_room.id, target_user.id)
        .await
        .expect("join target language");
    ChatRoomMember::join(&client, lounge_room.id, author_user.id)
        .await
        .expect("join author lounge");

    let lounge_message = ChatMessage::create(
        &client,
        ChatMessageParams {
            room_id: lounge_room.id,
            user_id: author_user.id,
            body: "fallback-msg".to_string(),
        },
    )
    .await
    .expect("lounge message");

    let (_room_tx, room_rx) = tokio::sync::watch::channel(None);
    let (mut state_rx, _event_rx, _refresh_tx, refresh_task) =
        service.start_user_refresh_task(target_user.id, room_rx);

    timeout(Duration::from_secs(2), state_rx.changed())
        .await
        .expect("state timeout")
        .expect("watch changed");
    let snapshot = state_rx.borrow_and_update().clone();

    let lounge_entry = snapshot
        .chat_rooms
        .iter()
        .find(|(room, _)| room.id == lounge_room.id)
        .expect("lounge room present");
    assert!(
        lounge_entry.1.is_empty(),
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
    assert_eq!(lounge_message.room_id, lounge_room.id);
    refresh_task.abort();
}

#[tokio::test]
async fn room_tail_task_loads_favorite_room_history() {
    let test_db = new_test_db().await;
    let service = ChatService::new(
        test_db.db.clone(),
        NotificationService::new(test_db.db.clone()),
    );
    let client = test_db.db.get().await.expect("db client");

    let target_user = create_test_user(&test_db.db, "favorite_target").await;
    let author_user = create_test_user(&test_db.db, "favorite_author").await;

    let lounge_room = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    let favorite_room = ChatRoom::get_or_create_public_room(&client, "favorites")
        .await
        .expect("favorite room");

    ChatRoomMember::join(&client, lounge_room.id, target_user.id)
        .await
        .expect("join target lounge");
    ChatRoomMember::join(&client, favorite_room.id, target_user.id)
        .await
        .expect("join target favorite");
    ChatRoomMember::join(&client, lounge_room.id, author_user.id)
        .await
        .expect("join author lounge");
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
            text_brightness_adjustment: 0,
            show_right_sidebar: true,
            right_sidebar_mode: RightSidebarMode::On,
            right_sidebar_components: default_right_sidebar_components(),
            show_room_list_sidebar: true,
            room_list_mode: late_core::models::user::RoomListMode::On,
            keep_composer_focused: false,
            start_with_music_muted: false,
            land_on_home: false,
            show_flag_fallback: false,
            show_pet_strip: true,
            translate_to: late_core::models::message_translation::TranslateLang::En,
            auto_translate: false,
            translate_mine_to_en: false,
            favorite_room_ids: vec![favorite_room.id],
            favorite_theme_ids: Vec::new(),
        },
    )
    .await
    .expect("update favorites");

    let (_room_tx, room_rx) = tokio::sync::watch::channel(None);
    let (_snapshot_rx, mut events, _refresh_tx, refresh_task) =
        service.start_user_refresh_task(target_user.id, room_rx);
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
    refresh_task.abort();
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

    let lounge_room = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge_room.id, target_user.id)
        .await
        .expect("join target");
    ChatRoomMember::join(&client, lounge_room.id, ignored_user.id)
        .await
        .expect("join ignored user");

    User::add_ignored_user_id(&client, target_user.id, ignored_user.id)
        .await
        .expect("persist ignored user id");

    let (_room_tx, room_rx) = tokio::sync::watch::channel(Some(lounge_room.id));
    let (mut state_rx, _event_rx, _refresh_tx, refresh_task) =
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

    let lounge_room = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    let discover_room = ChatRoom::get_or_create_public_room(&client, "rust")
        .await
        .expect("create discover room");
    let joined_room = ChatRoom::get_or_create_public_room(&client, "elixir")
        .await
        .expect("create joined room");

    ChatRoomMember::join(&client, lounge_room.id, target_user.id)
        .await
        .expect("join target lounge");
    ChatRoomMember::join(&client, lounge_room.id, author_user.id)
        .await
        .expect("join author lounge");
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

    let (_room_tx, room_rx) = tokio::sync::watch::channel(None);
    let (_snapshot_rx, mut events, _refresh_tx, refresh_task) =
        service.start_user_refresh_task(target_user.id, room_rx);
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
    refresh_task.abort();
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
    let (mut snapshot_a_rx, _event_a_rx, refresh_a, task_a) =
        service.start_user_refresh_task(user_a.id, room_a_rx);
    let (mut snapshot_b_rx, _event_b_rx, _refresh_b, task_b) =
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
    let room = ChatRoom::create_private_room(&client, "staff", admin.id)
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
    let entries: Vec<_> = audit
        .iter()
        .filter(|entry| {
            entry.actor_user_id == admin.id
                && entry.action == "message_delete"
                && entry.target_id == Some(msg.id)
        })
        .collect();
    assert_eq!(entries.len(), 1);
    assert_eq!(
        entries[0].metadata["body"], "admin will delete this",
        "deleted body must survive in the audit log; it is the only pointer to uploaded image urls"
    );
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
    let lounge_room = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge_room.id, viewer.id)
        .await
        .expect("join viewer");
    ChatRoomMember::join(&client, lounge_room.id, target.id)
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
    let lounge_room = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge_room.id, viewer.id)
        .await
        .expect("join viewer");
    ChatRoomMember::join(&client, lounge_room.id, target.id)
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
async fn ignore_user_task_emits_error_for_self() {
    let test_db = new_test_db().await;
    let service = ChatService::new(
        test_db.db.clone(),
        NotificationService::new(test_db.db.clone()),
    );
    let mut events = service.subscribe_events();

    let viewer = create_test_user(&test_db.db, "ignore_self").await;

    service.ignore_user_task(viewer.id, "ignore_self".to_string());

    let event = timeout(Duration::from_secs(2), events.recv())
        .await
        .expect("event timeout")
        .expect("event");
    match event {
        ChatEvent::IgnoreFailed { user_id, message } => {
            assert_eq!(user_id, viewer.id);
            assert_eq!(message, "Cannot ignore yourself");
        }
        other => panic!("expected IgnoreFailed, got {other:?}"),
    }
}

#[tokio::test]
async fn ignore_user_task_emits_error_for_duplicate() {
    let test_db = new_test_db().await;
    let service = ChatService::new(
        test_db.db.clone(),
        NotificationService::new(test_db.db.clone()),
    );
    let mut events = service.subscribe_events();
    let client = test_db.db.get().await.expect("db client");

    let viewer = create_test_user(&test_db.db, "ignore_dup_viewer").await;
    let target = create_test_user(&test_db.db, "ignore_dup_target").await;
    let lounge_room = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge_room.id, viewer.id)
        .await
        .expect("join viewer");
    ChatRoomMember::join(&client, lounge_room.id, target.id)
        .await
        .expect("join target");
    User::add_ignored_user_id(&client, viewer.id, target.id)
        .await
        .expect("seed ignored user id");

    service.ignore_user_task(viewer.id, "ignore_dup_target".to_string());

    let event = timeout(Duration::from_secs(2), events.recv())
        .await
        .expect("event timeout")
        .expect("event");
    match event {
        ChatEvent::IgnoreFailed { user_id, message } => {
            assert_eq!(user_id, viewer.id);
            assert_eq!(message, "@ignore_dup_target is already ignored");
        }
        other => panic!("expected IgnoreFailed, got {other:?}"),
    }
}

#[tokio::test]
async fn unignore_user_task_emits_error_for_missing_user() {
    let test_db = new_test_db().await;
    let service = ChatService::new(
        test_db.db.clone(),
        NotificationService::new(test_db.db.clone()),
    );
    let mut events = service.subscribe_events();

    let viewer = create_test_user(&test_db.db, "unignore_missing_viewer").await;

    service.unignore_user_task(viewer.id, "no_such_user".to_string());

    let event = timeout(Duration::from_secs(2), events.recv())
        .await
        .expect("event timeout")
        .expect("event");
    match event {
        ChatEvent::IgnoreFailed { user_id, message } => {
            assert_eq!(user_id, viewer.id);
            assert_eq!(message, "User 'no_such_user' not found");
        }
        other => panic!("expected IgnoreFailed, got {other:?}"),
    }
}

#[tokio::test]
async fn unignore_user_task_emits_error_for_not_ignored_entry() {
    let test_db = new_test_db().await;
    let service = ChatService::new(
        test_db.db.clone(),
        NotificationService::new(test_db.db.clone()),
    );
    let mut events = service.subscribe_events();
    let client = test_db.db.get().await.expect("db client");

    let viewer = create_test_user(&test_db.db, "unignore_entry_viewer").await;
    let target = create_test_user(&test_db.db, "unignore_missing_target").await;
    let lounge_room = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge_room.id, viewer.id)
        .await
        .expect("join viewer");
    ChatRoomMember::join(&client, lounge_room.id, target.id)
        .await
        .expect("join target");

    service.unignore_user_task(viewer.id, "unignore_missing_target".to_string());

    let event = timeout(Duration::from_secs(2), events.recv())
        .await
        .expect("event timeout")
        .expect("event");
    match event {
        ChatEvent::IgnoreFailed { user_id, message } => {
            assert_eq!(user_id, viewer.id);
            assert_eq!(message, "@unignore_missing_target is not ignored");
        }
        other => panic!("expected IgnoreFailed, got {other:?}"),
    }
}

#[tokio::test]
async fn friend_user_task_persists_and_emits_update() {
    let test_db = new_test_db().await;
    let service = ChatService::new(
        test_db.db.clone(),
        NotificationService::new(test_db.db.clone()),
    );
    let mut events = service.subscribe_events();
    let client = test_db.db.get().await.expect("db client");

    let viewer = create_test_user(&test_db.db, "friend_viewer").await;
    let target = create_test_user(&test_db.db, "friend_target").await;

    service.friend_user_task(viewer.id, "friend_target".to_string());

    let event = timeout(Duration::from_secs(2), events.recv())
        .await
        .expect("event timeout")
        .expect("event");
    match event {
        ChatEvent::FriendListUpdated {
            user_id,
            friend_user_ids,
            target_user_id,
            target_username,
            message,
        } => {
            assert_eq!(user_id, viewer.id);
            assert_eq!(friend_user_ids, vec![target.id]);
            assert_eq!(target_user_id, target.id);
            assert_eq!(target_username, "friend_target");
            assert_eq!(message, "Added @friend_target to friends");
        }
        other => panic!("expected FriendListUpdated, got {other:?}"),
    }

    let friends = User::friend_user_ids(&client, viewer.id)
        .await
        .expect("load friend list");
    assert_eq!(friends, vec![target.id]);
}

#[tokio::test]
async fn unfriend_user_task_persists_and_emits_update() {
    let test_db = new_test_db().await;
    let service = ChatService::new(
        test_db.db.clone(),
        NotificationService::new(test_db.db.clone()),
    );
    let mut events = service.subscribe_events();
    let client = test_db.db.get().await.expect("db client");

    let viewer = create_test_user(&test_db.db, "unfriend_viewer").await;
    let target = create_test_user(&test_db.db, "unfriend_target").await;
    User::add_friend_user_id(&client, viewer.id, target.id)
        .await
        .expect("seed friend user id");

    service.unfriend_user_task(viewer.id, "unfriend_target".to_string());

    let event = timeout(Duration::from_secs(2), events.recv())
        .await
        .expect("event timeout")
        .expect("event");
    match event {
        ChatEvent::FriendListUpdated {
            user_id,
            friend_user_ids,
            target_user_id,
            target_username,
            message,
        } => {
            assert_eq!(user_id, viewer.id);
            assert!(friend_user_ids.is_empty());
            assert_eq!(target_user_id, target.id);
            assert_eq!(target_username, "unfriend_target");
            assert_eq!(message, "Removed @unfriend_target from friends");
        }
        other => panic!("expected FriendListUpdated, got {other:?}"),
    }

    let friends = User::friend_user_ids(&client, viewer.id)
        .await
        .expect("load friend list");
    assert!(friends.is_empty());
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
        "ban #mod-ban-room @mod_ban_target 1h test cleanup".to_string(),
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
            audio_source: late_core::models::user::AudioSource::default(),
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
            audio_source: late_core::models::user::AudioSource::default(),
            sessions: vec![ActiveSession {
                token: session_token.clone(),
                fingerprint: Some(target.fingerprint.clone()),
                peer_ip: Some(peer_ip),
                afk: None,
            }],
            connection_count: 1,
            last_login_at: std::time::Instant::now(),
        },
    )])));
    let registry = SessionRegistry::new();
    let (session_tx, mut session_rx) = tokio::sync::mpsc::channel(1);
    registry
        .register(session_token, session_tx, uuid::Uuid::now_v7(), None)
        .await;
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
        "kick server @server_kick_target cool off".to_string(),
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
            audio_source: late_core::models::user::AudioSource::default(),
            sessions: vec![ActiveSession {
                token: session_token.clone(),
                fingerprint: Some(target.fingerprint.clone()),
                peer_ip: Some(peer_ip),
                afk: None,
            }],
            connection_count: 1,
            last_login_at: std::time::Instant::now(),
        },
    )])));
    let registry = SessionRegistry::new();
    let (session_tx, mut session_rx) = tokio::sync::mpsc::channel(1);
    registry
        .register(session_token, session_tx, uuid::Uuid::now_v7(), None)
        .await;
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
        "ban server @server_ban_target 1h test ban".to_string(),
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
            audio_source: late_core::models::user::AudioSource::default(),
            sessions: vec![ActiveSession {
                token: session_token.clone(),
                fingerprint: Some(target.fingerprint.clone()),
                peer_ip: None,
                afk: None,
            }],
            connection_count: 1,
            last_login_at: std::time::Instant::now(),
        },
    )])));
    let registry = SessionRegistry::new();
    let (session_tx, mut session_rx) = tokio::sync::mpsc::channel(1);
    registry
        .register(session_token, session_tx, uuid::Uuid::now_v7(), None)
        .await;
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
        "ban artboard @artboard_ban_target 1h paint cooldown".to_string(),
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
async fn mod_artboard_curate_command_copies_daily_snapshot_and_disambiguates_key() {
    let test_db = new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let actor = create_test_user(&test_db.db, "artboard_curate_actor").await;

    let mut daily_canvas = Canvas::with_size(dartboard::CANVAS_WIDTH, dartboard::CANVAS_HEIGHT);
    let _ = daily_canvas.put_glyph(Pos { x: 0, y: 0 }, 'D');
    let mut daily_provenance = ArtboardProvenance::default();
    daily_provenance.set_username(Pos { x: 0, y: 0 }, "daily_owner");
    ArtboardSnapshot::upsert(
        &client,
        "daily:2026-05-25",
        serde_json::to_value(&daily_canvas).expect("serialize daily canvas"),
        serde_json::to_value(&daily_provenance).expect("serialize daily provenance"),
    )
    .await
    .expect("insert daily snapshot");
    ArtboardSnapshot::upsert(
        &client,
        "curated:2026-05-25",
        serde_json::json!({"width":384,"height":192,"cells":[],"colors":[]}),
        serde_json::json!({"cells":[]}),
    )
    .await
    .expect("insert existing curated snapshot");

    let service = ChatService::new(
        test_db.db.clone(),
        NotificationService::new(test_db.db.clone()),
    );
    let mut events = service.subscribe_events();
    let mut moderation_events = service.subscribe_moderation_events();

    let request_id = Uuid::now_v7();
    service.run_mod_command_task(
        actor.id,
        Permissions::new(false, true),
        request_id,
        "artboard curate 2026-05-25 saved before cleanup".to_string(),
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
            assert_eq!(
                lines,
                vec!["curated artboard snapshot curated:2026-05-25-2 from daily:2026-05-25"]
            );
        }
        other => panic!("expected ModCommandOutput, got {other:?}"),
    }

    let moderation_event = timeout(Duration::from_secs(2), moderation_events.recv())
        .await
        .expect("moderation event timeout")
        .expect("moderation event");
    match moderation_event {
        ModerationEvent::ArtboardCurated {
            actor_user_id,
            board_key,
            reason,
        } => {
            assert_eq!(actor_user_id, actor.id);
            assert_eq!(board_key, "curated:2026-05-25-2");
            assert_eq!(reason, "saved before cleanup");
        }
        other => panic!("expected artboard curated moderation event, got {other:?}"),
    }

    let curated = ArtboardSnapshot::find_by_board_key(&client, "curated:2026-05-25-2")
        .await
        .expect("load curated snapshot")
        .expect("curated snapshot exists");
    let curated_canvas: Canvas =
        serde_json::from_value(curated.canvas).expect("decode curated canvas");
    let curated_provenance: ArtboardProvenance =
        serde_json::from_value(curated.provenance).expect("decode curated provenance");
    assert_eq!(curated_canvas.get(Pos { x: 0, y: 0 }), 'D');
    assert_eq!(
        curated_provenance.username_at(&curated_canvas, Pos { x: 0, y: 0 }),
        Some("daily_owner")
    );

    let audit = ModerationAuditLog::all(&client).await.expect("audit log");
    let audit_count = audit
        .iter()
        .filter(|entry| {
            entry.actor_user_id == actor.id
                && entry.action == "artboard_curate"
                && entry.target_kind == "artboard"
                && entry.metadata["source_key"] == "daily:2026-05-25"
                && entry.metadata["target_key"] == "curated:2026-05-25-2"
                && entry.metadata["reason"] == "saved before cleanup"
        })
        .count();
    assert_eq!(audit_count, 1);
}

#[tokio::test]
async fn mod_artboard_curate_live_flushes_and_copies_main_snapshot() {
    let test_db = new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let actor = create_test_user(&test_db.db, "artboard_curate_live_actor").await;

    let mut main_canvas = Canvas::with_size(dartboard::CANVAS_WIDTH, dartboard::CANVAS_HEIGHT);
    let _ = main_canvas.put_glyph(Pos { x: 0, y: 0 }, 'M');
    let mut main_provenance = ArtboardProvenance::default();
    main_provenance.set_username(Pos { x: 0, y: 0 }, "main_owner");

    let mut live_canvas = Canvas::with_size(dartboard::CANVAS_WIDTH, dartboard::CANVAS_HEIGHT);
    let _ = live_canvas.put_glyph(Pos { x: 0, y: 0 }, 'L');
    let mut live_provenance = ArtboardProvenance::default();
    live_provenance.set_username(Pos { x: 0, y: 0 }, "live_owner");

    ArtboardSnapshot::upsert(
        &client,
        ArtboardSnapshot::MAIN_BOARD_KEY,
        serde_json::to_value(&main_canvas).expect("serialize main canvas"),
        serde_json::to_value(&main_provenance).expect("serialize main provenance"),
    )
    .await
    .expect("insert main snapshot");

    let shared_provenance = live_provenance.shared();
    let server = dartboard::spawn_persistent_server_with_interval(
        test_db.db.clone(),
        Some(live_canvas),
        shared_provenance.clone(),
        Duration::from_secs(60 * 60),
    );
    let service = ChatService::new(
        test_db.db.clone(),
        NotificationService::new(test_db.db.clone()),
    )
    .with_moderation_infra(
        ModerationInfra::default().with_artboard_handles(server.clone(), shared_provenance.clone()),
    );
    let mut events = service.subscribe_events();

    let request_id = Uuid::now_v7();
    let target_key = dartboard::curated_board_key(chrono::Utc::now().date_naive(), 0);
    service.run_mod_command_task(
        actor.id,
        Permissions::new(false, true),
        request_id,
        "artboard curate live preserve live".to_string(),
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
            assert_eq!(
                lines,
                vec![format!("curated artboard snapshot {target_key} from main")]
            );
        }
        other => panic!("expected ModCommandOutput, got {other:?}"),
    }

    let curated = ArtboardSnapshot::find_by_board_key(&client, &target_key)
        .await
        .expect("load curated snapshot")
        .expect("curated snapshot exists");
    let curated_canvas: Canvas =
        serde_json::from_value(curated.canvas).expect("decode curated canvas");
    assert_eq!(curated_canvas.get(Pos { x: 0, y: 0 }), 'L');

    let main = ArtboardSnapshot::find_by_board_key(&client, ArtboardSnapshot::MAIN_BOARD_KEY)
        .await
        .expect("load main snapshot")
        .expect("main snapshot exists");
    let main_canvas: Canvas = serde_json::from_value(main.canvas).expect("decode main canvas");
    assert_eq!(main_canvas.get(Pos { x: 0, y: 0 }), 'L');
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
        "ban server @list_server_target 1h server reason",
        "ban artboard @list_artboard_target 1h art reason",
        "ban #list-bans-room @list_room_target 1h room reason",
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
        "view bans".to_string(),
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

/// `ban stream` is the persistent half of the stream kill switch: it must end
/// the broadcast in flight, not just refuse the next one. The registry entry
/// going away is what kills the watch and publisher URLs.
#[tokio::test]
async fn mod_stream_ban_ends_the_live_stream_and_persists_the_block() {
    let test_db = new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let voice = crate::app::voice::svc::VoiceService::new(
        crate::app::voice::svc::VoiceConfig::enabled(
            "wss://rtc.test".to_string(),
            "http://livekit-sv.test".to_string(),
            "test-key".to_string(),
            "test-secret".to_string(),
            "late-voice".to_string(),
        )
        .expect("voice config"),
    );
    let (activity_tx, _activity_rx) = tokio::sync::broadcast::channel(16);
    let stream = crate::app::stream::svc::StreamService::new(
        test_db.db.clone(),
        voice,
        crate::app::activity::publisher::ActivityPublisher::new(test_db.db.clone(), activity_tx),
        "https://late.test".to_string(),
    );
    let service = ChatService::new(
        test_db.db.clone(),
        NotificationService::new(test_db.db.clone()),
    )
    .with_moderation_infra(ModerationInfra::default().with_stream(stream.clone()));
    let mut events = service.subscribe_events();

    let actor = create_test_user(&test_db.db, "stream_mod_actor").await;
    let target = create_test_user(&test_db.db, "stream_mod_target").await;

    let mut stream_events = stream.subscribe_events();
    stream.go_live_task(target.id, target.username.clone(), Some("demo".to_string()));
    timeout(Duration::from_secs(5), stream_events.recv())
        .await
        .expect("go live event timeout")
        .expect("go live event");
    assert!(
        stream.snapshot().for_user(target.id).is_some(),
        "target should be registered as streaming before the ban"
    );

    let request_id = Uuid::now_v7();
    service.run_mod_command_task(
        actor.id,
        Permissions::new(false, true),
        request_id,
        "ban stream @stream_mod_target 1h nsfw".to_string(),
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
            assert_eq!(lines, vec!["stream-banned @stream_mod_target"]);
        }
        other => panic!("expected ModCommandOutput, got {other:?}"),
    }

    assert!(
        stream.snapshot().for_user(target.id).is_none(),
        "the ban must tear the live stream out of the registry"
    );
    let ban = late_core::models::stream_ban::StreamBan::find_active_for_user(&client, target.id)
        .await
        .expect("stream ban lookup")
        .expect("stream ban is active");
    assert!(ban.expires_at.is_some(), "1h ban should carry an expiry");
    assert_eq!(ban.reason, "nsfw");

    service.run_mod_command_task(
        actor.id,
        Permissions::new(false, true),
        Uuid::now_v7(),
        "unban stream @stream_mod_target".to_string(),
    );
    let event = timeout(Duration::from_secs(2), events.recv())
        .await
        .expect("event timeout")
        .expect("event");
    match event {
        ChatEvent::ModCommandOutput { lines, success, .. } => {
            assert!(success, "unexpected mod command failure: {lines:?}");
            assert_eq!(lines, vec!["removed stream ban for @stream_mod_target"]);
        }
        other => panic!("expected ModCommandOutput, got {other:?}"),
    }
    assert!(
        !late_core::models::stream_ban::StreamBan::is_active_for_user(&client, target.id)
            .await
            .expect("stream ban lookup")
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
        "kick server @list_audit_target audit reason".to_string(),
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
        "view audit".to_string(),
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
                    .any(|line| line == "recent audit log entries (page 1, 15 per page)")
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
            audio_source: late_core::models::user::AudioSource::default(),
            sessions: vec![ActiveSession {
                token: session_token.clone(),
                fingerprint: Some(target.fingerprint.clone()),
                peer_ip: None,
                afk: None,
            }],
            connection_count: 1,
            last_login_at: std::time::Instant::now(),
        },
    )])));
    let registry = SessionRegistry::new();
    let (session_tx, mut session_rx) = tokio::sync::mpsc::channel(1);
    registry
        .register(session_token, session_tx, uuid::Uuid::now_v7(), None)
        .await;
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
        "ban #room-notify @room_notify_target 1h test".to_string(),
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
async fn mod_slow_command_creates_row_audits_and_notifies_target_session() {
    let test_db = new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let actor = create_test_user(&test_db.db, "slow_actor").await;
    let target = create_test_user(&test_db.db, "slow_target").await;
    let room = ChatRoom::get_or_create_public_room(&client, "slow-notify")
        .await
        .expect("create room");
    ChatRoomMember::join(&client, room.id, target.id)
        .await
        .expect("join target");

    let session_token = "slow-notify-session".to_string();
    let active_users = Arc::new(Mutex::new(HashMap::from([(
        target.id,
        ActiveUser {
            username: target.username.clone(),
            fingerprint: Some(target.fingerprint.clone()),
            audio_source: late_core::models::user::AudioSource::default(),
            sessions: vec![ActiveSession {
                token: session_token.clone(),
                fingerprint: Some(target.fingerprint.clone()),
                peer_ip: None,
                afk: None,
            }],
            connection_count: 1,
            last_login_at: std::time::Instant::now(),
        },
    )])));
    let registry = SessionRegistry::new();
    let (session_tx, mut session_rx) = tokio::sync::mpsc::channel(1);
    registry
        .register(session_token, session_tx, target.id, None)
        .await;
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
        "slow #slow-notify @slow_target 90s permanent high volume".to_string(),
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
            assert_eq!(
                lines,
                vec!["slowed @slow_target in #slow-notify: one message every 1m 30s for permanent"]
            );
        }
        other => panic!("expected ModCommandOutput, got {other:?}"),
    }

    let message = timeout(Duration::from_secs(2), session_rx.recv())
        .await
        .expect("session message timeout")
        .expect("session message");
    match message {
        SessionMessage::Toast { message, error } => {
            assert!(error);
            assert_eq!(
                message,
                "Slow mode in #slow-notify: one message every 1m 30s. No expiry set."
            );
        }
        other => panic!("expected toast message, got {other:?}"),
    }

    let slow_mode = ChatSlowMode::find_active_for_room_and_user(&client, room.id, target.id)
        .await
        .expect("slow mode lookup")
        .expect("active slow mode");
    assert_eq!(slow_mode.interval_secs, 90);
    assert!(slow_mode.expires_at.is_none());

    let audit = ModerationAuditLog::all(&client).await.expect("audit log");
    let audit_count = audit
        .iter()
        .filter(|entry| {
            entry.actor_user_id == actor.id
                && entry.action == "room_slow"
                && entry.target_id == Some(target.id)
        })
        .count();
    assert_eq!(audit_count, 1);
}

#[tokio::test]
async fn mod_server_slow_command_creates_server_row_and_notifies_target_session() {
    let test_db = new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let actor = create_test_user(&test_db.db, "server_slow_actor").await;
    let target = create_test_user(&test_db.db, "server_slow_target").await;

    let session_token = "server-slow-session".to_string();
    let active_users = Arc::new(Mutex::new(HashMap::from([(
        target.id,
        ActiveUser {
            username: target.username.clone(),
            fingerprint: Some(target.fingerprint.clone()),
            audio_source: late_core::models::user::AudioSource::default(),
            sessions: vec![ActiveSession {
                token: session_token.clone(),
                fingerprint: Some(target.fingerprint.clone()),
                peer_ip: None,
                afk: None,
            }],
            connection_count: 1,
            last_login_at: std::time::Instant::now(),
        },
    )])));
    let registry = SessionRegistry::new();
    let (session_tx, mut session_rx) = tokio::sync::mpsc::channel(1);
    registry
        .register(session_token, session_tx, target.id, None)
        .await;
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
        "slow server @server_slow_target 90s permanent high volume".to_string(),
    );

    let event = timeout(Duration::from_secs(2), events.recv())
        .await
        .expect("event timeout")
        .expect("event");
    match event {
        ChatEvent::ModCommandOutput { lines, success, .. } => {
            assert!(success, "unexpected mod command failure: {lines:?}");
            assert_eq!(
                lines,
                vec![
                    "slowed @server_slow_target in server: one message every 1m 30s for permanent"
                ]
            );
        }
        other => panic!("expected ModCommandOutput, got {other:?}"),
    }

    let message = timeout(Duration::from_secs(2), session_rx.recv())
        .await
        .expect("session message timeout")
        .expect("session message");
    match message {
        SessionMessage::Toast { message, error } => {
            assert!(error);
            assert_eq!(
                message,
                "Slow mode in server: one message every 1m 30s. No expiry set."
            );
        }
        other => panic!("expected toast message, got {other:?}"),
    }

    let slow_mode = ChatSlowMode::find_active_server_for_user(&client, target.id)
        .await
        .expect("slow mode lookup")
        .expect("active server slow mode");
    assert_eq!(slow_mode.interval_secs, 90);
    assert!(slow_mode.room_id.is_none());

    let audit = ModerationAuditLog::all(&client).await.expect("audit log");
    assert!(audit.iter().any(|entry| {
        entry.actor_user_id == actor.id
            && entry.action == "server_slow"
            && entry.target_id == Some(target.id)
    }));
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
            audio_source: late_core::models::user::AudioSource::default(),
            sessions: vec![ActiveSession {
                token: session_token.clone(),
                fingerprint: Some(target.fingerprint.clone()),
                peer_ip: None,
                afk: None,
            }],
            connection_count: 1,
            last_login_at: std::time::Instant::now(),
        },
    )])));
    let registry = SessionRegistry::new();
    let (session_tx, mut session_rx) = tokio::sync::mpsc::channel(1);
    registry
        .register(session_token, session_tx, uuid::Uuid::now_v7(), None)
        .await;
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
        "admin grant mod @grant_mod_target".to_string(),
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
async fn admin_ultimate_cast_command_broadcasts_to_active_sessions_and_audits() {
    let test_db = new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let actor = create_test_user(&test_db.db, "ultimate_cast_admin").await;
    let target = create_test_user(&test_db.db, "ultimate_cast_target").await;

    let actor_token = "ultimate-admin-session".to_string();
    let target_token = "ultimate-target-session".to_string();
    let active_users = Arc::new(Mutex::new(HashMap::from([
        (
            actor.id,
            ActiveUser {
                username: actor.username.clone(),
                fingerprint: Some(actor.fingerprint.clone()),
                audio_source: late_core::models::user::AudioSource::default(),
                sessions: vec![ActiveSession {
                    token: actor_token.clone(),
                    fingerprint: Some(actor.fingerprint.clone()),
                    peer_ip: None,
                    afk: None,
                }],
                connection_count: 1,
                last_login_at: std::time::Instant::now(),
            },
        ),
        (
            target.id,
            ActiveUser {
                username: target.username.clone(),
                fingerprint: Some(target.fingerprint.clone()),
                audio_source: late_core::models::user::AudioSource::default(),
                sessions: vec![ActiveSession {
                    token: target_token.clone(),
                    fingerprint: Some(target.fingerprint.clone()),
                    peer_ip: None,
                    afk: None,
                }],
                connection_count: 1,
                last_login_at: std::time::Instant::now(),
            },
        ),
    ])));
    let registry = SessionRegistry::new();
    let (actor_session_tx, mut actor_session_rx) = tokio::sync::mpsc::channel(1);
    let (target_session_tx, mut target_session_rx) = tokio::sync::mpsc::channel(1);
    registry
        .register(actor_token, actor_session_tx, actor.id, None)
        .await;
    registry
        .register(target_token, target_session_tx, target.id, None)
        .await;
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
        "admin ultimate cast thematrix".to_string(),
    );

    let actor_message = timeout(Duration::from_secs(2), actor_session_rx.recv())
        .await
        .expect("actor session message timeout")
        .expect("actor session message");
    let target_message = timeout(Duration::from_secs(2), target_session_rx.recv())
        .await
        .expect("target session message timeout")
        .expect("target session message");
    for message in [actor_message, target_message] {
        match message {
            SessionMessage::UltimateCast {
                ultimate_id,
                duration_ms,
                ..
            } => {
                assert_eq!(ultimate_id, "thematrix");
                assert!(duration_ms > 0);
            }
            other => panic!("expected ultimate cast message, got {other:?}"),
        }
    }

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
            assert_eq!(lines, vec!["cast The Matrix ultimate to 2 active sessions"]);
        }
        other => panic!("expected ModCommandOutput, got {other:?}"),
    }

    let audit = ModerationAuditLog::all(&client).await.expect("audit log");
    assert!(audit.iter().any(|entry| {
        entry.actor_user_id == actor.id
            && entry.action == "ultimate_cast"
            && entry.target_kind == "ultimate"
            && entry.metadata["ultimate_id"] == "thematrix"
            && entry.metadata["notified_sessions"] == 2
    }));
}

#[tokio::test]
async fn moderator_cannot_run_admin_ultimate_cast_command() {
    let test_db = new_test_db().await;
    let actor = create_test_user(&test_db.db, "ultimate_cast_mod").await;
    let service = ChatService::new(
        test_db.db.clone(),
        NotificationService::new(test_db.db.clone()),
    );
    let mut events = service.subscribe_events();

    let request_id = Uuid::now_v7();
    service.run_mod_command_task(
        actor.id,
        Permissions::new(false, true),
        request_id,
        "admin ultimate cast thematrix".to_string(),
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
            assert!(!success);
            assert_eq!(lines, vec!["error: admin only"]);
        }
        other => panic!("expected ModCommandOutput, got {other:?}"),
    }
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

#[tokio::test]
async fn send_message_task_rejects_active_slow_mode_until_interval_passes() {
    let test_db = new_test_db().await;
    let service = ChatService::new(
        test_db.db.clone(),
        NotificationService::new(test_db.db.clone()),
    );
    let mut events = service.subscribe_events();
    let client = test_db.db.get().await.expect("db client");

    let actor = create_test_user(&test_db.db, "send_slow_actor").await;
    let user = create_test_user(&test_db.db, "send_slow_target").await;
    let room = ChatRoom::get_or_create_public_room(&client, "send-slow-room")
        .await
        .expect("create room");
    ChatRoomMember::join(&client, room.id, user.id)
        .await
        .expect("join user");

    let first_request_id = Uuid::now_v7();
    service.send_message_task(
        user.id,
        room.id,
        room.slug.clone(),
        "first message".to_string(),
        first_request_id,
        false,
    );
    let mut saw_created = false;
    let mut saw_success = false;
    for _ in 0..2 {
        let event = timeout(Duration::from_secs(2), events.recv())
            .await
            .expect("first send event timeout")
            .expect("first send event");
        match event {
            ChatEvent::MessageCreated { message, .. } => {
                saw_created = true;
                assert_eq!(message.body, "first message");
            }
            ChatEvent::SendSucceeded { request_id, .. } => {
                saw_success = true;
                assert_eq!(request_id, first_request_id);
            }
            other => panic!("unexpected first send event: {other:?}"),
        }
    }
    assert!(saw_created);
    assert!(saw_success);

    ChatSlowMode::activate(
        &client,
        room.id,
        user.id,
        actor.id,
        90,
        "too fast",
        Some(chrono::Utc::now() + chrono::Duration::hours(1)),
    )
    .await
    .expect("activate slow mode");

    let second_request_id = Uuid::now_v7();
    service.send_message_task(
        user.id,
        room.id,
        room.slug.clone(),
        "second message".to_string(),
        second_request_id,
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
            assert_eq!(got_request, second_request_id);
            assert!(
                message.starts_with("Slow mode in #send-slow-room: wait "),
                "{message}"
            );
            assert!(message.ends_with(" before sending again."), "{message}");
        }
        other => panic!("expected SendFailed, got {other:?}"),
    }
}

#[tokio::test]
async fn send_message_task_applies_server_slow_to_public_rooms_but_not_dms() {
    let test_db = new_test_db().await;
    let service = ChatService::new(
        test_db.db.clone(),
        NotificationService::new(test_db.db.clone()),
    );
    let mut events = service.subscribe_events();
    let client = test_db.db.get().await.expect("db client");

    let actor = create_test_user(&test_db.db, "send_server_slow_actor").await;
    let user = create_test_user(&test_db.db, "send_server_slow_target").await;
    let peer = create_test_user(&test_db.db, "send_server_slow_peer").await;
    let room = ChatRoom::get_or_create_public_room(&client, "send-server-slow-room")
        .await
        .expect("create room");
    let other_room = ChatRoom::get_or_create_public_room(&client, "send-server-slow-other")
        .await
        .expect("create other room");
    ChatRoomMember::join(&client, room.id, user.id)
        .await
        .expect("join user");
    ChatRoomMember::join(&client, other_room.id, user.id)
        .await
        .expect("join other room user");
    let dm = ChatRoom::get_or_create_dm(&client, user.id, peer.id)
        .await
        .expect("create dm");
    ChatRoomMember::join(&client, dm.id, user.id)
        .await
        .expect("join dm user");
    ChatRoomMember::join(&client, dm.id, peer.id)
        .await
        .expect("join dm peer");

    let first_request_id = Uuid::now_v7();
    service.send_message_task(
        user.id,
        room.id,
        room.slug.clone(),
        "first public message".to_string(),
        first_request_id,
        false,
    );
    for _ in 0..2 {
        let _ = timeout(Duration::from_secs(2), events.recv())
            .await
            .expect("first send event timeout")
            .expect("first send event");
    }

    ChatSlowMode::activate_server(
        &client,
        user.id,
        actor.id,
        90,
        "too fast",
        Some(chrono::Utc::now() + chrono::Duration::hours(1)),
    )
    .await
    .expect("activate server slow mode");

    let second_request_id = Uuid::now_v7();
    service.send_message_task(
        user.id,
        other_room.id,
        other_room.slug.clone(),
        "second public message in another room".to_string(),
        second_request_id,
        false,
    );
    let event = timeout(Duration::from_secs(2), events.recv())
        .await
        .expect("event timeout")
        .expect("event");
    match event {
        ChatEvent::SendFailed {
            request_id,
            message,
            ..
        } => {
            assert_eq!(request_id, second_request_id);
            assert!(
                message.starts_with("Slow mode in server: wait "),
                "{message}"
            );
        }
        other => panic!("expected SendFailed, got {other:?}"),
    }

    let dm_request_id = Uuid::now_v7();
    service.send_message_task(
        user.id,
        dm.id,
        dm.slug.clone(),
        "dm is not throttled".to_string(),
        dm_request_id,
        false,
    );
    let mut saw_created = false;
    let mut saw_success = false;
    for _ in 0..2 {
        let event = timeout(Duration::from_secs(2), events.recv())
            .await
            .expect("dm send event timeout")
            .expect("dm send event");
        match event {
            ChatEvent::MessageCreated { message, .. } => {
                saw_created = true;
                assert_eq!(message.body, "dm is not throttled");
            }
            ChatEvent::SendSucceeded { request_id, .. } => {
                saw_success = true;
                assert_eq!(request_id, dm_request_id);
            }
            other => panic!("unexpected dm send event: {other:?}"),
        }
    }
    assert!(saw_created);
    assert!(saw_success);
}

#[tokio::test]
async fn message_search_respects_membership_scope_and_exclusions() {
    let test_db = new_test_db().await;
    let service = ChatService::new(
        test_db.db.clone(),
        NotificationService::new(test_db.db.clone()),
    );
    let client = test_db.db.get().await.expect("db client");

    let searcher = create_test_user(&test_db.db, "search_searcher").await;
    let author = create_test_user(&test_db.db, "search_author").await;
    let ignored = create_test_user(&test_db.db, "search_ignored").await;

    let joined_room = ChatRoom::get_or_create_public_room(&client, "search-joined")
        .await
        .expect("joined room");
    let other_room = ChatRoom::get_or_create_public_room(&client, "search-other")
        .await
        .expect("other room");
    let game_room = ChatRoom::get_or_create_game_room(
        &client,
        late_core::models::game_room::GameKind::Poker,
        "poker",
    )
    .await
    .expect("game room");

    for user_id in [searcher.id, author.id, ignored.id] {
        ChatRoomMember::join(&client, joined_room.id, user_id)
            .await
            .expect("join joined room");
        ChatRoomMember::join(&client, game_room.id, user_id)
            .await
            .expect("join game room");
    }
    ChatRoomMember::join(&client, other_room.id, author.id)
        .await
        .expect("author joins other room");

    for (room_id, user_id, body) in [
        (joined_room.id, author.id, "the deploy failed at midnight"),
        (joined_room.id, ignored.id, "my deploy also failed"),
        (joined_room.id, author.id, "unrelated chatter"),
        (
            other_room.id,
            author.id,
            "deploy failed where searcher is not a member",
        ),
        (game_room.id, author.id, "deploy failed in game room"),
    ] {
        ChatMessage::create(
            &client,
            ChatMessageParams {
                room_id,
                user_id,
                body: body.to_string(),
            },
        )
        .await
        .expect("create message");
    }

    let (_room_tx, room_rx) = tokio::sync::watch::channel(None);
    let (_snapshot_rx, mut events, _refresh_tx, refresh_task) =
        service.start_user_refresh_task(searcher.id, room_rx);
    let request_id = Uuid::now_v7();
    service.search_messages_task(
        searcher.id,
        request_id,
        None,
        "DEPLOY FAILED".to_string(),
        vec![ignored.id],
    );

    let event = timeout(Duration::from_secs(2), events.recv())
        .await
        .expect("search event timeout")
        .expect("search event");
    match event {
        ChatEvent::MessageSearchLoaded {
            user_id,
            request_id: loaded_request_id,
            messages,
            usernames,
        } => {
            assert_eq!(user_id, searcher.id);
            assert_eq!(loaded_request_id, request_id);
            assert_eq!(messages.len(), 1, "only the joined-room hit survives");
            assert_eq!(messages[0].body, "the deploy failed at midnight");
            assert_eq!(
                usernames.get(&author.id).map(String::as_str),
                Some("search_author")
            );
        }
        other => panic!("unexpected search event: {other:?}"),
    }
    refresh_task.abort();
}

#[tokio::test]
async fn message_search_scopes_to_room_and_escapes_like_metacharacters() {
    let test_db = new_test_db().await;
    let service = ChatService::new(
        test_db.db.clone(),
        NotificationService::new(test_db.db.clone()),
    );
    let client = test_db.db.get().await.expect("db client");

    let searcher = create_test_user(&test_db.db, "scoped_searcher").await;
    let room_a = ChatRoom::get_or_create_public_room(&client, "scoped-a")
        .await
        .expect("room a");
    let room_b = ChatRoom::get_or_create_public_room(&client, "scoped-b")
        .await
        .expect("room b");
    ChatRoomMember::join(&client, room_a.id, searcher.id)
        .await
        .expect("join a");
    ChatRoomMember::join(&client, room_b.id, searcher.id)
        .await
        .expect("join b");

    for (room_id, body) in [
        (room_a.id, "progress is 50% done"),
        (room_a.id, "progress is 50x done"),
        (room_b.id, "progress is 50% done elsewhere"),
    ] {
        ChatMessage::create(
            &client,
            ChatMessageParams {
                room_id,
                user_id: searcher.id,
                body: body.to_string(),
            },
        )
        .await
        .expect("create message");
    }

    let (_room_tx, room_rx) = tokio::sync::watch::channel(None);
    let (_snapshot_rx, mut events, _refresh_tx, refresh_task) =
        service.start_user_refresh_task(searcher.id, room_rx);
    let request_id = Uuid::now_v7();
    service.search_messages_task(
        searcher.id,
        request_id,
        Some(room_a.id),
        "50% done".to_string(),
        Vec::new(),
    );

    let event = timeout(Duration::from_secs(2), events.recv())
        .await
        .expect("search event timeout")
        .expect("search event");
    match event {
        ChatEvent::MessageSearchLoaded { messages, .. } => {
            assert_eq!(
                messages.len(),
                1,
                "% must match literally and room b must be filtered out"
            );
            assert_eq!(messages[0].body, "progress is 50% done");
        }
        other => panic!("unexpected search event: {other:?}"),
    }
    refresh_task.abort();
}

#[tokio::test]
async fn message_context_window_surrounds_hit_chronologically() {
    let test_db = new_test_db().await;
    let client = test_db.db.get().await.expect("db client");

    let author = create_test_user(&test_db.db, "context_author").await;
    let ignored = create_test_user(&test_db.db, "context_ignored").await;
    let room = ChatRoom::get_or_create_public_room(&client, "context-room")
        .await
        .expect("room");
    ChatRoomMember::join(&client, room.id, author.id)
        .await
        .expect("join author");
    ChatRoomMember::join(&client, room.id, ignored.id)
        .await
        .expect("join ignored");

    let mut created_messages = Vec::new();
    for index in 0..9 {
        // Message 4 from the ignored user must be skipped by the window.
        let user_id = if index == 4 { ignored.id } else { author.id };
        let message = ChatMessage::create(
            &client,
            ChatMessageParams {
                room_id: room.id,
                user_id,
                body: format!("context message {index}"),
            },
        )
        .await
        .expect("create message");
        created_messages.push(message);
    }

    let hit = &created_messages[5];
    let (before, after) = ChatMessage::list_around(
        &client,
        room.id,
        author.id,
        hit.created,
        hit.id,
        &[ignored.id],
        3,
    )
    .await
    .expect("list around");

    let before_bodies: Vec<&str> = before.iter().map(|m| m.body.as_str()).collect();
    let after_bodies: Vec<&str> = after.iter().map(|m| m.body.as_str()).collect();
    assert_eq!(
        before_bodies,
        vec![
            "context message 1",
            "context message 2",
            "context message 3"
        ],
        "chronological, skipping the ignored author's message 4"
    );
    assert_eq!(
        after_bodies,
        vec![
            "context message 6",
            "context message 7",
            "context message 8"
        ]
    );
}

/// Room-info authority: a private room answers to its owner, a public one to
/// the house. The service is the gate; the UI only decides what to show.
#[tokio::test]
async fn only_the_owner_or_a_mod_may_set_room_info() {
    let test_db = new_test_db().await;
    let service = ChatService::new(
        test_db.db.clone(),
        NotificationService::new(test_db.db.clone()),
    );
    let mut events = service.subscribe_events();
    let client = test_db.db.get().await.expect("db client");

    let owner = create_test_user(&test_db.db, "info_owner").await;
    let member = create_test_user(&test_db.db, "info_member").await;
    let private = ChatRoom::create_private_room(&client, "parlour", owner.id)
        .await
        .expect("create private room");
    ChatRoomMember::join(&client, private.id, owner.id)
        .await
        .expect("owner joins");
    ChatRoomMember::join(&client, private.id, member.id)
        .await
        .expect("member joins");

    service.set_room_info_task(
        owner.id,
        false,
        private.id,
        Some("cards and tea".to_string()),
        None,
    );
    expect_room_info_updated(&mut events, owner.id).await;
    assert_eq!(
        current_topic(&test_db.db, private.id).await.as_deref(),
        Some("cards and tea")
    );

    // A member who does not own the room is refused, and nothing is written.
    service.set_room_info_task(
        member.id,
        false,
        private.id,
        Some("mine now".to_string()),
        None,
    );
    expect_room_failed(&mut events, member.id).await;
    assert_eq!(
        current_topic(&test_db.db, private.id).await.as_deref(),
        Some("cards and tea"),
        "a non-owner must not change a private room's info"
    );

    // Public rooms are hosted: the person who opened it has no say, a mod does.
    let opener = create_test_user(&test_db.db, "info_opener").await;
    let public = ChatRoom::get_or_create_public_room(&client, "commons")
        .await
        .expect("create public room");
    ChatRoomMember::join(&client, public.id, opener.id)
        .await
        .expect("opener joins");
    ChatRoom::set_creator(&client, public.id, opener.id)
        .await
        .expect("record creator");

    service.set_room_info_task(opener.id, false, public.id, Some("mine".to_string()), None);
    expect_room_failed(&mut events, opener.id).await;
    assert_eq!(
        current_topic(&test_db.db, public.id).await,
        None,
        "opening a public room does not grant its topic"
    );

    service.set_room_info_task(
        opener.id,
        true,
        public.id,
        Some("the town square".to_string()),
        None,
    );
    expect_room_info_updated(&mut events, opener.id).await;
    assert_eq!(
        current_topic(&test_db.db, public.id).await.as_deref(),
        Some("the town square")
    );
}

/// Await the room-info success event for `actor`, skipping unrelated traffic.
async fn expect_room_info_updated(
    events: &mut tokio::sync::broadcast::Receiver<ChatEvent>,
    actor: Uuid,
) {
    loop {
        let event = timeout(Duration::from_secs(2), events.recv())
            .await
            .expect("room info event timeout")
            .expect("event");
        match event {
            ChatEvent::RoomInfoUpdated { user_id, .. } if user_id == actor => return,
            ChatEvent::RoomFailed { user_id, message } if user_id == actor => {
                panic!("expected the write to be allowed, got: {message}")
            }
            _ => {}
        }
    }
}

/// Await the refusal for `actor`. Waiting on the event the service actually
/// emits is what makes "nothing was written" assertions deterministic.
async fn expect_room_failed(
    events: &mut tokio::sync::broadcast::Receiver<ChatEvent>,
    actor: Uuid,
) -> String {
    loop {
        let event = timeout(Duration::from_secs(2), events.recv())
            .await
            .expect("room failure event timeout")
            .expect("event");
        match event {
            ChatEvent::RoomFailed { user_id, message } if user_id == actor => return message,
            ChatEvent::RoomInfoUpdated { user_id, .. } if user_id == actor => {
                panic!("expected the write to be refused")
            }
            _ => {}
        }
    }
}

async fn current_topic(db: &late_core::db::Db, room_id: Uuid) -> Option<String> {
    let client = db.get().await.expect("db client");
    ChatRoom::get(&client, room_id)
        .await
        .expect("get room")
        .expect("room exists")
        .topic
}

/// Opening a brand-new public room tells two audiences: the creator, in the
/// room they just opened, and the mods, in #moderators. Neither line carries
/// the system-feed prefix, so both render as messages instead of being
/// diverted into the activity ticker.
#[tokio::test]
async fn opening_a_new_public_room_asks_the_creator_and_reports_to_moderators() {
    let test_db = new_test_db().await;
    let service = ChatService::new(
        test_db.db.clone(),
        NotificationService::new(test_db.db.clone()),
    );
    let client = test_db.db.get().await.expect("db client");

    User::create(
        &client,
        late_core::models::user::UserParams {
            fingerprint: "system-fp-000".to_string(),
            username: "system".to_string(),
            settings: serde_json::json!({ "bot": true, "system": true }),
        },
    )
    .await
    .expect("system user");
    let staff = create_test_user(&test_db.db, "announce_staff").await;
    let moderators = ChatRoom::create_private_room(&client, "moderators", staff.id)
        .await
        .expect("moderators room");
    let creator = create_test_user(&test_db.db, "announce_creator").await;

    service.open_public_room_task(creator.id, "greenhouse".to_string());

    let room_id = wait_for_public_room(&test_db.db, "greenhouse").await;
    let welcome = wait_for_message_containing(&test_db.db, room_id, "greenhouse").await;
    assert!(
        !welcome.starts_with('\u{b7}'),
        "a room notice must not look like a feed line: {welcome}"
    );
    assert!(welcome.contains("rules"), "the creator is asked for rules");

    let report = wait_for_message_containing(&test_db.db, moderators.id, "greenhouse").await;
    assert!(
        report.contains("announce_creator"),
        "mods learn who opened it"
    );
}

async fn wait_for_public_room(db: &late_core::db::Db, slug: &str) -> Uuid {
    for _ in 0..50 {
        let client = db.get().await.expect("db client");
        let found = ChatRoom::find_topic_room(&client, "public", slug)
            .await
            .expect("find room");
        drop(client);
        if let Some(room) = found {
            return room.id;
        }
        sleep(Duration::from_millis(20)).await;
    }
    panic!("public room was never created");
}

async fn wait_for_message_containing(
    db: &late_core::db::Db,
    room_id: Uuid,
    needle: &str,
) -> String {
    for _ in 0..50 {
        let client = db.get().await.expect("db client");
        let messages = ChatMessage::list_recent(&client, room_id, 20)
            .await
            .expect("list messages");
        drop(client);
        if let Some(found) = messages.iter().find(|m| m.body.contains(needle)) {
            return found.body.clone();
        }
        sleep(Duration::from_millis(20)).await;
    }
    panic!("no message containing {needle:?} landed in the room");
}

/// A permanent, reasonless room action against `username` in the room. Chat
/// commands carry the room id, so these tests do too.
fn room_request(action: RoomModAction, room_id: Uuid, username: &str) -> RoomModRequest {
    RoomModRequest {
        action,
        room: RoomRef::Id(room_id),
        username: username.to_string(),
        duration: None,
        reason: String::new(),
    }
}

/// `/kick` in a private room: its owner may remove a regular member, a plain
/// member may not, and staff are out of reach. The work happens through the
/// moderation service, so membership, audit trail and session effects are the
/// same as a mod kick.
#[tokio::test]
async fn private_room_owner_can_kick_regulars_but_not_staff() {
    let test_db = new_test_db().await;
    let service = ChatService::new(
        test_db.db.clone(),
        NotificationService::new(test_db.db.clone()),
    );
    let client = test_db.db.get().await.expect("db client");

    let owner = create_test_user(&test_db.db, "kick_owner").await;
    let guest = create_test_user(&test_db.db, "kick_guest").await;
    let staff = create_test_user(&test_db.db, "kick_staff").await;
    User::set_moderator(&client, staff.id, true)
        .await
        .expect("promote staff");

    let room = ChatRoom::create_private_room(&client, "study", owner.id)
        .await
        .expect("create private room");
    for member in [owner.id, guest.id, staff.id] {
        ChatRoomMember::join(&client, room.id, member)
            .await
            .expect("join room");
    }

    let regular = Permissions::new(false, false);
    let mut events = service.subscribe_events();

    // A member who does not own the room cannot throw anyone out.
    service.room_mod_task(
        guest.id,
        regular,
        room_request(RoomModAction::Kick, room.id, "kick_owner"),
    );
    expect_room_mod_failed(&mut events, guest.id).await;
    assert!(
        is_member(&test_db.db, room.id, owner.id).await,
        "a non-owner must not be able to kick"
    );

    // The owner cannot reach staff either: ownership carries no rank.
    service.room_mod_task(
        owner.id,
        regular,
        room_request(RoomModAction::Kick, room.id, "kick_staff"),
    );
    expect_room_mod_failed(&mut events, owner.id).await;
    assert!(
        is_member(&test_db.db, room.id, staff.id).await,
        "an owner must not be able to kick a moderator"
    );

    // But the owner does keep the door.
    service.room_mod_task(
        owner.id,
        regular,
        room_request(RoomModAction::Kick, room.id, "kick_guest"),
    );
    expect_room_mod_succeeded(&mut events, owner.id).await;
    assert!(
        !is_member(&test_db.db, room.id, guest.id).await,
        "the owner may remove a regular member"
    );

    // A private room's owner keeps the door only. Banning is a stream-room
    // power: an invite-only room cannot be walked back into, so its owner has
    // no need of the lock and does not get it.
    service.room_mod_task(
        owner.id,
        regular,
        room_request(RoomModAction::Ban, room.id, "kick_guest"),
    );
    expect_room_mod_failed(&mut events, owner.id).await;
}

/// `/ban` in a stream room: the streamer may ban a regular and lift it again,
/// staff stay out of reach, and a viewer holds no power in someone else's
/// room. The ban is what a streamer actually needs, since a kicked viewer
/// walks straight back into a public room from the rail.
#[tokio::test]
async fn stream_room_owner_can_ban_and_unban_regulars_but_not_staff() {
    let test_db = new_test_db().await;
    let service = ChatService::new(
        test_db.db.clone(),
        NotificationService::new(test_db.db.clone()),
    );
    let client = test_db.db.get().await.expect("db client");

    let streamer = create_test_user(&test_db.db, "ban_streamer").await;
    let heckler = create_test_user(&test_db.db, "ban_heckler").await;
    let staff = create_test_user(&test_db.db, "ban_staff").await;
    User::set_moderator(&client, staff.id, true)
        .await
        .expect("promote staff");

    let room = ChatRoom::get_or_create_stream_room(&client, &streamer.username, streamer.id)
        .await
        .expect("create stream room");
    for member in [streamer.id, heckler.id, staff.id] {
        ChatRoomMember::join(&client, room.id, member)
            .await
            .expect("join room");
    }

    let regular = Permissions::new(false, false);
    let mut events = service.subscribe_events();

    // A viewer holds nothing in a room that is not theirs.
    service.room_mod_task(
        heckler.id,
        regular,
        room_request(RoomModAction::Ban, room.id, "ban_streamer"),
    );
    expect_room_mod_failed(&mut events, heckler.id).await;

    // Ownership carries no rank, so staff are untouchable in it.
    service.room_mod_task(
        streamer.id,
        regular,
        room_request(RoomModAction::Ban, room.id, "ban_staff"),
    );
    expect_room_mod_failed(&mut events, streamer.id).await;

    // The streamer bans a regular: membership drops and the row is written.
    service.room_mod_task(
        streamer.id,
        regular,
        RoomModRequest {
            reason: "shouting".to_string(),
            ..room_request(RoomModAction::Ban, room.id, "ban_heckler")
        },
    );
    expect_room_mod_succeeded(&mut events, streamer.id).await;
    assert!(
        !is_member(&test_db.db, room.id, heckler.id).await,
        "a banned viewer must lose room membership"
    );
    assert!(
        RoomBan::is_active_for_room_and_user(&client, room.id, heckler.id)
            .await
            .expect("ban lookup"),
        "the ban must persist as a row, not just a membership drop"
    );

    // And can lift it again.
    service.room_mod_task(
        streamer.id,
        regular,
        room_request(RoomModAction::Unban, room.id, "ban_heckler"),
    );
    expect_room_mod_succeeded(&mut events, streamer.id).await;
    assert!(
        !RoomBan::is_active_for_room_and_user(&client, room.id, heckler.id)
            .await
            .expect("ban lookup"),
        "unban must clear the row"
    );
}

/// A ban placed by staff is not the streamer's to touch. Ownership grants the
/// caps but no rank, so a streamer must be refused both lifting a staff ban
/// and overwriting it with a softer one; staff themselves stay unaffected,
/// and an *expired* staff ban no longer stands in the way of a fresh one.
#[tokio::test]
async fn a_streamer_cannot_lift_or_replace_a_staff_ban_on_their_room() {
    let test_db = new_test_db().await;
    let service = ChatService::new(
        test_db.db.clone(),
        NotificationService::new(test_db.db.clone()),
    );
    let client = test_db.db.get().await.expect("db client");

    let streamer = create_test_user(&test_db.db, "staffban_streamer").await;
    let heckler = create_test_user(&test_db.db, "staffban_heckler").await;
    let staff = create_test_user(&test_db.db, "staffban_staff").await;
    User::set_moderator(&client, staff.id, true)
        .await
        .expect("promote staff");

    let room = ChatRoom::get_or_create_stream_room(&client, &streamer.username, streamer.id)
        .await
        .expect("create stream room");
    for member in [streamer.id, heckler.id, staff.id] {
        ChatRoomMember::join(&client, room.id, member)
            .await
            .expect("join room");
    }

    let regular = Permissions::new(false, false);
    let moderator = Permissions::new(false, true);
    let mut events = service.subscribe_events();

    // Staff ban the heckler in the streamer's room, permanently.
    service.room_mod_task(
        staff.id,
        moderator,
        room_request(RoomModAction::Ban, room.id, "staffban_heckler"),
    );
    expect_room_mod_succeeded(&mut events, staff.id).await;

    // The streamer may not lift it.
    service.room_mod_task(
        streamer.id,
        regular,
        room_request(RoomModAction::Unban, room.id, "staffban_heckler"),
    );
    expect_room_mod_failed(&mut events, streamer.id).await;

    // Nor overwrite it with one that lapses in a second.
    service.room_mod_task(
        streamer.id,
        regular,
        RoomModRequest {
            duration: Some(chrono::Duration::seconds(1)),
            ..room_request(RoomModAction::Ban, room.id, "staffban_heckler")
        },
    );
    expect_room_mod_failed(&mut events, streamer.id).await;

    let ban = RoomBan::find_for_room_and_user(&client, room.id, heckler.id)
        .await
        .expect("ban lookup")
        .expect("staff ban row");
    assert_eq!(
        ban.actor_user_id, staff.id,
        "the staff ban must survive untouched"
    );
    assert_eq!(
        ban.expires_at, None,
        "the staff ban must stay permanent, not shortened by the streamer"
    );

    // Staff are unaffected by the guard: they lift their own ban fine.
    service.room_mod_task(
        staff.id,
        moderator,
        room_request(RoomModAction::Unban, room.id, "staffban_heckler"),
    );
    expect_room_mod_succeeded(&mut events, staff.id).await;

    // An expired staff ban is history, not a claim: the streamer may ban over
    // it.
    RoomBan::activate(
        &client,
        room.id,
        heckler.id,
        staff.id,
        "old trouble",
        Some(chrono::Utc::now() - chrono::Duration::hours(1)),
    )
    .await
    .expect("seed expired staff ban");
    service.room_mod_task(
        streamer.id,
        regular,
        room_request(RoomModAction::Ban, room.id, "staffban_heckler"),
    );
    expect_room_mod_succeeded(&mut events, streamer.id).await;
    let ban = RoomBan::find_for_room_and_user(&client, room.id, heckler.id)
        .await
        .expect("ban lookup")
        .expect("streamer ban row");
    assert_eq!(
        ban.actor_user_id, streamer.id,
        "an expired staff ban must not block the streamer's fresh ban"
    );
}

/// Slugs are not globally unique: a public topic room may share its slug with
/// a stream room (stream slugs are just `{username}-live`). Chat commands
/// therefore name the room by id, so the action lands on the room the actor
/// is sitting in, never on a namesake.
#[tokio::test]
async fn chat_ban_lands_on_the_room_the_actor_is_in_not_a_slug_namesake() {
    let test_db = new_test_db().await;
    let service = ChatService::new(
        test_db.db.clone(),
        NotificationService::new(test_db.db.clone()),
    );
    let client = test_db.db.get().await.expect("db client");

    let streamer = create_test_user(&test_db.db, "namesake_streamer").await;
    let heckler = create_test_user(&test_db.db, "namesake_heckler").await;

    let room = ChatRoom::get_or_create_stream_room(&client, &streamer.username, streamer.id)
        .await
        .expect("create stream room");
    let slug = room.slug.clone().expect("stream room slug");
    let namesake = ChatRoom::get_or_create_public_room(&client, &slug)
        .await
        .expect("create namesake topic room");
    assert_ne!(
        namesake.id, room.id,
        "the namesake must be a distinct room for this test to mean anything"
    );
    for member in [streamer.id, heckler.id] {
        ChatRoomMember::join(&client, room.id, member)
            .await
            .expect("join room");
    }

    let mut events = service.subscribe_events();
    service.room_mod_task(
        streamer.id,
        Permissions::new(false, false),
        room_request(RoomModAction::Ban, room.id, "namesake_heckler"),
    );
    expect_room_mod_succeeded(&mut events, streamer.id).await;
    assert!(
        RoomBan::is_active_for_room_and_user(&client, room.id, heckler.id)
            .await
            .expect("ban lookup"),
        "the ban must land on the stream room"
    );
    assert!(
        !RoomBan::is_active_for_room_and_user(&client, namesake.id, heckler.id)
            .await
            .expect("ban lookup"),
        "the namesake topic room must be untouched"
    );
}

/// Ownership powers are scoped to *stream* rooms, not to game rooms at large.
/// Other game rooms (house tables, daily match chats) also carry a
/// `created_by`, and without the `game_kind` check that user would inherit a
/// streamer's powers over everyone sitting there.
#[tokio::test]
async fn a_non_stream_game_room_has_no_owner_moderator() {
    let test_db = new_test_db().await;
    let service = ChatService::new(
        test_db.db.clone(),
        NotificationService::new(test_db.db.clone()),
    );
    let client = test_db.db.get().await.expect("db client");

    let creator = create_test_user(&test_db.db, "table_creator").await;
    let player = create_test_user(&test_db.db, "table_player").await;

    let room = ChatRoom::get_or_create_game_room(&client, GameKind::Poker, "poker-owner-test")
        .await
        .expect("create game room");
    ChatRoom::set_creator(&client, room.id, creator.id)
        .await
        .expect("set creator");
    for member in [creator.id, player.id] {
        ChatRoomMember::join(&client, room.id, member)
            .await
            .expect("join room");
    }

    let mut events = service.subscribe_events();
    service.room_mod_task(
        creator.id,
        Permissions::new(false, false),
        room_request(RoomModAction::Ban, room.id, "table_player"),
    );
    expect_room_mod_failed(&mut events, creator.id).await;
    assert!(
        is_member(&test_db.db, room.id, player.id).await,
        "a game room that is not a stream room has no owner-moderator"
    );
}

/// What makes a ban mean anything in a public room: the rail's join path must
/// refuse a banned user, or they are back in the room the moment they click
/// it. Enforced down in `ChatRoomMember::join` so every join path inherits it;
/// pinned here because a streamer's ban is worthless without it.
#[tokio::test]
async fn banned_user_cannot_rejoin_a_public_game_room() {
    let test_db = new_test_db().await;
    let service = ChatService::new(
        test_db.db.clone(),
        NotificationService::new(test_db.db.clone()),
    );
    let client = test_db.db.get().await.expect("db client");

    let streamer = create_test_user(&test_db.db, "rejoin_streamer").await;
    let heckler = create_test_user(&test_db.db, "rejoin_heckler").await;
    let room = ChatRoom::get_or_create_stream_room(&client, &streamer.username, streamer.id)
        .await
        .expect("create stream room");

    // Anyone may walk into a stream room from the rail.
    service
        .join_game_room(heckler.id, room.id)
        .await
        .expect("first join");
    assert!(is_member(&test_db.db, room.id, heckler.id).await);

    RoomBan::activate(&client, room.id, heckler.id, streamer.id, "shouting", None)
        .await
        .expect("ban");
    ChatRoomMember::leave(&client, room.id, heckler.id)
        .await
        .expect("leave");

    let error = service
        .join_game_room(heckler.id, room.id)
        .await
        .expect_err("a banned user must not rejoin");
    assert!(
        error.to_string().contains("banned"),
        "expected a ban refusal, got: {error}"
    );
    assert!(
        !is_member(&test_db.db, room.id, heckler.id).await,
        "a refused join must not restore membership"
    );
}

/// Await the refusal for `actor`. The event is what makes the assertion
/// below it deterministic.
async fn expect_room_mod_failed(
    events: &mut tokio::sync::broadcast::Receiver<ChatEvent>,
    actor: Uuid,
) {
    loop {
        let event = timeout(Duration::from_secs(2), events.recv())
            .await
            .expect("room mod failure event timeout")
            .expect("event");
        match event {
            ChatEvent::RoomModFailed { user_id, .. } if user_id == actor => return,
            ChatEvent::RoomModSucceeded { user_id, .. } if user_id == actor => {
                panic!("expected the room action to be refused")
            }
            _ => {}
        }
    }
}

async fn expect_room_mod_succeeded(
    events: &mut tokio::sync::broadcast::Receiver<ChatEvent>,
    actor: Uuid,
) {
    loop {
        let event = timeout(Duration::from_secs(2), events.recv())
            .await
            .expect("room mod success event timeout")
            .expect("event");
        match event {
            ChatEvent::RoomModSucceeded { user_id, .. } if user_id == actor => return,
            ChatEvent::RoomModFailed { user_id, message } if user_id == actor => {
                panic!("expected the room action to be allowed, got: {message}")
            }
            _ => {}
        }
    }
}

async fn is_member(db: &late_core::db::Db, room_id: Uuid, user_id: Uuid) -> bool {
    let client = db.get().await.expect("db client");
    ChatRoomMember::is_member(&client, room_id, user_id)
        .await
        .expect("membership check")
}

// ── Gilds ───────────────────────────────────────────────────
//
// Every refusal below must leave the ledger untouched: a gild that does not
// land is free. `gild_ledger_total` is the witness for that, and the happy
// path asserts the split the same way.

mod gild {
    use super::*;
    use late_core::models::chat_message_gild::{ChatMessageGild, GildTier};
    use late_core::models::chips::{ChipMove, UserChips};

    /// A public room, a message in it by `author`, and `buyer` in the room
    /// with enough chips for any tier.
    struct GildFixture {
        service: ChatService,
        db: late_core::db::Db,
        buyer: Uuid,
        author: Uuid,
        message_id: Uuid,
    }

    async fn public_room(db: &late_core::db::Db, slug: &str) -> ChatRoom {
        let client = db.get().await.expect("db client");
        ChatRoom::create(
            &client,
            ChatRoomParams {
                kind: "topic".to_string(),
                visibility: "public".to_string(),
                auto_join: false,
                permanent: false,
                slug: Some(slug.to_string()),
                language_code: None,
                dm_user_a: None,
                dm_user_b: None,
                topic: None,
                rules: None,
                created_by: None,
            },
        )
        .await
        .expect("create room")
    }

    async fn message_in(db: &late_core::db::Db, room_id: Uuid, user_id: Uuid) -> ChatMessage {
        let client = db.get().await.expect("db client");
        ChatMessage::create(
            &client,
            ChatMessageParams {
                room_id,
                user_id,
                body: "worth paying for".to_string(),
            },
        )
        .await
        .expect("create message")
    }

    async fn join(db: &late_core::db::Db, room_id: Uuid, user_id: Uuid) {
        let client = db.get().await.expect("db client");
        ChatRoomMember::join(&client, room_id, user_id)
            .await
            .expect("join room");
    }

    /// Stake a user so every tier is affordable; a refusal test must fail on
    /// its own rule, never on the balance.
    async fn stake(db: &late_core::db::Db, user_id: Uuid) {
        let client = db.get().await.expect("db client");
        UserChips::ensure(&client, user_id)
            .await
            .expect("chips row");
        UserChips::apply(&**client, user_id, ChipMove::Credit, 100_000, None)
            .await
            .expect("stake")
            .expect("credit lands");
    }

    /// Every ledger row written against this message, summed. Zero means
    /// nothing was charged and nothing was paid.
    async fn gild_ledger_total(db: &late_core::db::Db, message_id: Uuid) -> i64 {
        let client = db.get().await.expect("db client");
        let row = client
            .query_one(
                "SELECT COUNT(*)::bigint AS rows
                 FROM chip_ledger
                 WHERE source_ref = $1",
                &[&message_id.to_string()],
            )
            .await
            .expect("ledger count");
        row.get("rows")
    }

    async fn fixture(slug: &str) -> (late_core::test_utils::TestDb, GildFixture) {
        let test_db = new_test_db().await;
        let service = ChatService::new(
            test_db.db.clone(),
            NotificationService::new(test_db.db.clone()),
        );
        let author = create_test_user(&test_db.db, &format!("{slug}-author")).await;
        let buyer = create_test_user(&test_db.db, &format!("{slug}-buyer")).await;
        let room = public_room(&test_db.db, slug).await;
        join(&test_db.db, room.id, author.id).await;
        join(&test_db.db, room.id, buyer.id).await;
        stake(&test_db.db, buyer.id).await;
        let message = message_in(&test_db.db, room.id, author.id).await;
        let fixture = GildFixture {
            service,
            db: test_db.db.clone(),
            buyer: buyer.id,
            author: author.id,
            message_id: message.id,
        };
        (test_db, fixture)
    }

    /// Drive one gild and wait for its verdict.
    async fn gild(fixture: &GildFixture, buyer: Uuid, tier: GildTier) -> ChatEvent {
        let mut events = fixture.service.subscribe_events();
        fixture
            .service
            .gild_message_task(buyer, fixture.message_id, tier);
        loop {
            let event = timeout(Duration::from_secs(5), events.recv())
                .await
                .expect("gild event timeout")
                .expect("gild event");
            if matches!(
                event,
                ChatEvent::GildSucceeded { .. } | ChatEvent::GildFailed { .. }
            ) {
                return event;
            }
        }
    }

    fn refusal_message(event: ChatEvent) -> String {
        match event {
            ChatEvent::GildFailed { message, .. } => message,
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn pays_the_author_two_thirds_and_burns_the_rest() {
        let (_test_db, fixture) = fixture("gild-happy").await;
        let event = gild(&fixture, fixture.buyer, GildTier::Silver).await;
        match event {
            ChatEvent::GildSucceeded {
                user_id,
                tier,
                author_user_id,
                ..
            } => {
                assert_eq!(user_id, fixture.buyer);
                assert_eq!(author_user_id, fixture.author);
                assert_eq!(tier, GildTier::Silver);
            }
            other => panic!("expected the gild to land, got {other:?}"),
        }

        let client = fixture.db.get().await.expect("db client");
        let counts = ChatMessageGild::counts_for_author(&client, fixture.author)
            .await
            .expect("counts");
        assert_eq!(counts.silver, 1);
        assert_eq!(counts.total(), 1);
        // One debit and one credit, and their sum is minus the burn.
        assert_eq!(gild_ledger_total(&fixture.db, fixture.message_id).await, 2);
    }

    /// The per-buyer cooldown is a rate limit, not a rule under test here;
    /// lift it so one test can walk a buyer through several buys.
    fn lift_cooldown(fixture: &GildFixture, buyer: Uuid) {
        fixture.service.lift_gild_cooldown(buyer);
    }

    /// A buyer has one slot on a message and it only goes up: a higher tier
    /// raises it at the new tier's full price, the same tier and a lower
    /// tier are refused uncharged, and a raise adds no buyer so it never
    /// fires the #lounge line.
    #[tokio::test]
    async fn raises_a_held_gild_at_full_price_and_never_lowers_it() {
        let (_test_db, fixture) = fixture("gild-up").await;
        let after_bronze = match gild(&fixture, fixture.buyer, GildTier::Bronze).await {
            ChatEvent::GildSucceeded { buyer_balance, .. } => buyer_balance,
            other => panic!("expected the first gild to land, got {other:?}"),
        };

        lift_cooldown(&fixture, fixture.buyer);
        let raised = fixture
            .service
            .gild_message(fixture.buyer, fixture.message_id, GildTier::Gold)
            .await
            .expect("the raise lands");
        assert_eq!(raised.tier, GildTier::Gold);
        assert_eq!(raised.upgraded_from, Some(GildTier::Bronze));
        assert_eq!(raised.total_gilds, 1, "a raise adds no buyer");
        assert!(!raised.fires_feed_line());
        assert_eq!(
            after_bronze - raised.buyer_balance,
            GildTier::Gold.price(),
            "a raise pays the new tier in full, not the difference"
        );

        let client = fixture.db.get().await.expect("db client");
        let counts = ChatMessageGild::counts_for_author(&client, fixture.author)
            .await
            .expect("counts");
        assert_eq!((counts.bronze, counts.gold), (0, 1));
        // Two buys, each a debit and a credit.
        assert_eq!(gild_ledger_total(&fixture.db, fixture.message_id).await, 4);

        lift_cooldown(&fixture, fixture.buyer);
        let lower = gild(&fixture, fixture.buyer, GildTier::Silver).await;
        assert_eq!(
            refusal_message(lower),
            "Your gild on this message is already higher"
        );
        lift_cooldown(&fixture, fixture.buyer);
        let same = gild(&fixture, fixture.buyer, GildTier::Gold).await;
        assert_eq!(
            refusal_message(same),
            "You already gilded this message at that tier"
        );
        assert_eq!(gild_ledger_total(&fixture.db, fixture.message_id).await, 4);
    }

    #[tokio::test]
    async fn refuses_a_self_gild_uncharged() {
        let (_test_db, fixture) = fixture("gild-self").await;
        stake(&fixture.db, fixture.author).await;
        let event = gild(&fixture, fixture.author, GildTier::Bronze).await;
        assert_eq!(refusal_message(event), "You cannot gild your own message");
        assert_eq!(gild_ledger_total(&fixture.db, fixture.message_id).await, 0);
    }

    #[tokio::test]
    async fn refuses_a_dm_uncharged() {
        let test_db = new_test_db().await;
        let service = ChatService::new(
            test_db.db.clone(),
            NotificationService::new(test_db.db.clone()),
        );
        let author = create_test_user(&test_db.db, "gild-dm-author").await;
        let buyer = create_test_user(&test_db.db, "gild-dm-buyer").await;
        let room = {
            let client = test_db.db.get().await.expect("db client");
            ChatRoom::get_or_create_dm(&client, author.id, buyer.id)
                .await
                .expect("dm room")
        };
        join(&test_db.db, room.id, author.id).await;
        join(&test_db.db, room.id, buyer.id).await;
        stake(&test_db.db, buyer.id).await;
        let message = message_in(&test_db.db, room.id, author.id).await;
        let fixture = GildFixture {
            service,
            db: test_db.db.clone(),
            buyer: buyer.id,
            author: author.id,
            message_id: message.id,
        };

        let event = gild(&fixture, fixture.buyer, GildTier::Bronze).await;
        assert_eq!(refusal_message(event), "Gilds only work in public rooms");
        assert_eq!(gild_ledger_total(&fixture.db, fixture.message_id).await, 0);
    }

    #[tokio::test]
    async fn refuses_a_private_room_uncharged() {
        let test_db = new_test_db().await;
        let service = ChatService::new(
            test_db.db.clone(),
            NotificationService::new(test_db.db.clone()),
        );
        let author = create_test_user(&test_db.db, "gild-private-author").await;
        let buyer = create_test_user(&test_db.db, "gild-private-buyer").await;
        let room = {
            let client = test_db.db.get().await.expect("db client");
            ChatRoom::create(
                &client,
                ChatRoomParams {
                    kind: "topic".to_string(),
                    visibility: "private".to_string(),
                    auto_join: false,
                    permanent: false,
                    slug: Some("gild-private".to_string()),
                    language_code: None,
                    dm_user_a: None,
                    dm_user_b: None,
                    topic: None,
                    rules: None,
                    created_by: None,
                },
            )
            .await
            .expect("create room")
        };
        join(&test_db.db, room.id, author.id).await;
        join(&test_db.db, room.id, buyer.id).await;
        stake(&test_db.db, buyer.id).await;
        let message = message_in(&test_db.db, room.id, author.id).await;
        let fixture = GildFixture {
            service,
            db: test_db.db.clone(),
            buyer: buyer.id,
            author: author.id,
            message_id: message.id,
        };

        let event = gild(&fixture, fixture.buyer, GildTier::Bronze).await;
        assert_eq!(refusal_message(event), "Gilds only work in public rooms");
        assert_eq!(gild_ledger_total(&fixture.db, fixture.message_id).await, 0);
    }

    /// Game and stream chats are `kind = 'game'` with `visibility = 'public'`,
    /// so a visibility check alone would let them through. Gilds are for the
    /// rooms on the Home rail, and the refusal says so in its own words.
    #[tokio::test]
    async fn refuses_a_game_room_uncharged() {
        let test_db = new_test_db().await;
        let service = ChatService::new(
            test_db.db.clone(),
            NotificationService::new(test_db.db.clone()),
        );
        let author = create_test_user(&test_db.db, "gild-game-author").await;
        let buyer = create_test_user(&test_db.db, "gild-game-buyer").await;
        let room = {
            let client = test_db.db.get().await.expect("db client");
            ChatRoom::get_or_create_game_room(
                &client,
                late_core::models::game_room::GameKind::Poker,
                "gild-game-table",
            )
            .await
            .expect("game room")
        };
        assert_eq!(
            room.visibility, "public",
            "the fixture must be the public game room shape"
        );
        join(&test_db.db, room.id, author.id).await;
        join(&test_db.db, room.id, buyer.id).await;
        stake(&test_db.db, buyer.id).await;
        let message = message_in(&test_db.db, room.id, author.id).await;
        let fixture = GildFixture {
            service,
            db: test_db.db.clone(),
            buyer: buyer.id,
            author: author.id,
            message_id: message.id,
        };

        let event = gild(&fixture, fixture.buyer, GildTier::Bronze).await;
        assert_eq!(
            refusal_message(event),
            "Gilds do not work in game or stream chats"
        );
        assert_eq!(gild_ledger_total(&fixture.db, fixture.message_id).await, 0);
    }

    #[tokio::test]
    async fn refuses_a_bot_author_uncharged() {
        let test_db = new_test_db().await;
        let service = ChatService::new(
            test_db.db.clone(),
            NotificationService::new(test_db.db.clone()),
        );
        let bot = create_test_user(&test_db.db, "gild-bot").await;
        {
            let client = test_db.db.get().await.expect("db client");
            User::update_settings(&client, bot.id, &serde_json::json!({ "bot": true }))
                .await
                .expect("mark bot");
        }
        let buyer = create_test_user(&test_db.db, "gild-bot-buyer").await;
        let room = public_room(&test_db.db, "gild-bot-room").await;
        join(&test_db.db, room.id, bot.id).await;
        join(&test_db.db, room.id, buyer.id).await;
        stake(&test_db.db, buyer.id).await;
        let message = message_in(&test_db.db, room.id, bot.id).await;
        let fixture = GildFixture {
            service,
            db: test_db.db.clone(),
            buyer: buyer.id,
            author: bot.id,
            message_id: message.id,
        };

        let event = gild(&fixture, fixture.buyer, GildTier::Bronze).await;
        assert_eq!(refusal_message(event), "Bots do not take chips");
        assert_eq!(gild_ledger_total(&fixture.db, fixture.message_id).await, 0);
    }

    #[tokio::test]
    async fn refuses_a_non_member_uncharged() {
        let (test_db, fixture) = fixture("gild-outsider").await;
        let outsider = create_test_user(&test_db.db, "gild-outsider-stranger").await;
        stake(&fixture.db, outsider.id).await;
        let event = gild(&fixture, outsider.id, GildTier::Bronze).await;
        assert_eq!(refusal_message(event), "You are not a member of this room");
        assert_eq!(gild_ledger_total(&fixture.db, fixture.message_id).await, 0);
    }

    /// The second gild inside the window is refused, and the first one's
    /// two ledger rows are still the only ones on the message.
    #[tokio::test]
    async fn refuses_a_second_gild_inside_the_cooldown() {
        let (_test_db, fixture) = fixture("gild-cooldown").await;
        gild(&fixture, fixture.buyer, GildTier::Bronze).await;
        let event = gild(&fixture, fixture.buyer, GildTier::Silver).await;
        assert_eq!(refusal_message(event), "Gilding is on cooldown");
        assert_eq!(gild_ledger_total(&fixture.db, fixture.message_id).await, 2);
    }

    /// A buyer who cannot cover the tier keeps every chip they had.
    #[tokio::test]
    async fn refuses_a_tier_the_balance_cannot_cover() {
        let (test_db, fixture) = fixture("gild-broke").await;
        let broke = create_test_user(&test_db.db, "gild-broke-buyer").await;
        {
            let client = test_db.db.get().await.expect("db client");
            UserChips::ensure(&client, broke.id).await.expect("chips");
            ChatRoomMember::join(
                &client,
                message_room(&test_db.db, fixture.message_id).await,
                broke.id,
            )
            .await
            .expect("join");
        }
        let event = gild(&fixture, broke.id, GildTier::Gold).await;
        assert_eq!(refusal_message(event), "Not enough chips for that tier");
        assert_eq!(gild_ledger_total(&fixture.db, fixture.message_id).await, 0);

        let client = fixture.db.get().await.expect("db client");
        let chips = UserChips::find(&client, broke.id)
            .await
            .expect("chips")
            .expect("row");
        assert_eq!(
            chips.balance,
            late_core::models::chips::INITIAL_CHIP_BALANCE
        );
    }

    async fn message_room(db: &late_core::db::Db, message_id: Uuid) -> Uuid {
        let client = db.get().await.expect("db client");
        ChatMessage::get(&client, message_id)
            .await
            .expect("message")
            .expect("exists")
            .room_id
    }
}
