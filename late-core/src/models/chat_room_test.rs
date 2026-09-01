use crate::{
    models::{
        chat_message::{ChatMessage, ChatMessageParams},
        chat_room::ChatRoom,
        chat_room_member::ChatRoomMember,
        user::{User, UserParams},
    },
    test_utils::{create_test_user, test_db},
};

#[tokio::test]
async fn test_chat_room_lounge_and_language() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");

    let lounge1 = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge");
    assert_eq!(lounge1.kind, "lounge");
    assert_eq!(lounge1.slug.as_deref(), Some("lounge"));
    assert_eq!(lounge1.visibility, "public");
    assert!(lounge1.auto_join);

    let lounge2 = ChatRoom::find_lounge(&client).await.unwrap().unwrap();
    assert_eq!(lounge1.id, lounge2.id);

    let lang = ChatRoom::get_or_create_language(&client, "es")
        .await
        .expect("create lang");
    assert_eq!(lang.kind, "language");
    assert_eq!(lang.language_code.as_deref(), Some("es"));
    assert_eq!(lang.slug.as_deref(), Some("lang-es"));
    assert_eq!(lang.visibility, "public");
    assert!(!lang.auto_join);
}

#[tokio::test]
async fn test_chat_room_public_and_private_topics() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");

    let public_room = ChatRoom::get_or_create_public_room(&client, "side")
        .await
        .expect("create public room");
    let public_room_again = ChatRoom::get_or_create_public_room(&client, "side")
        .await
        .expect("get public room");
    let creator = create_test_user(&test_db.db, "topics_creator").await;
    let private_room = ChatRoom::create_private_room(&client, "side", creator.id)
        .await
        .expect("create private room");

    assert_eq!(public_room.id, public_room_again.id);
    assert_eq!(public_room.visibility, "public");
    assert!(!public_room.auto_join);
    assert_eq!(private_room.visibility, "private");
    assert!(!private_room.auto_join);
    assert_ne!(public_room.id, private_room.id);

    let duplicate_private = ChatRoom::create_private_room(&client, "side", creator.id).await;
    assert!(
        duplicate_private.is_err(),
        "expected duplicate private room to fail"
    );
}

#[tokio::test]
async fn test_chat_room_topic_slug_normalization() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");

    let public_room = ChatRoom::get_or_create_public_room(&client, " Rust Nerds \n")
        .await
        .expect("create normalized public room");
    let creator = create_test_user(&test_db.db, "slug_creator").await;
    let private_room = ChatRoom::create_private_room(&client, "vps/d9d0", creator.id)
        .await
        .expect("create normalized private room");

    assert_eq!(public_room.slug.as_deref(), Some("rust-nerds"));
    assert_eq!(private_room.slug.as_deref(), Some("vps-d9d0"));
}

#[tokio::test]
async fn test_chat_room_dm() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");

    let user1 = User::create(
        &client,
        UserParams {
            fingerprint: "dm-user-1".to_string(),
            username: "u1".to_string(),
            settings: serde_json::json!({}),
        },
    )
    .await
    .unwrap();

    let user2 = User::create(
        &client,
        UserParams {
            fingerprint: "dm-user-2".to_string(),
            username: "u2".to_string(),
            settings: serde_json::json!({}),
        },
    )
    .await
    .unwrap();

    let dm1 = ChatRoom::get_or_create_dm(&client, user1.id, user2.id)
        .await
        .unwrap();
    let dm2 = ChatRoom::get_or_create_dm(&client, user2.id, user1.id)
        .await
        .unwrap();

    assert_eq!(dm1.id, dm2.id);
    assert_eq!(dm1.kind, "dm");
}

#[tokio::test]
async fn owner_is_the_creator_then_succeeds_to_the_next_member() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");

    let creator = create_test_user(&test_db.db, "owner_creator").await;
    let second = create_test_user(&test_db.db, "owner_second").await;
    let third = create_test_user(&test_db.db, "owner_third").await;

    let room = ChatRoom::create_private_room(&client, "hall", creator.id)
        .await
        .expect("create private room");
    for member in [creator.id, second.id, third.id] {
        ChatRoomMember::join(&client, room.id, member)
            .await
            .expect("join room");
    }

    assert_eq!(
        ChatRoom::owner_id(&client, room.id).await.expect("owner"),
        Some(creator.id),
        "the recorded creator owns the room while they are in it"
    );

    ChatRoomMember::leave(&client, room.id, creator.id)
        .await
        .expect("creator leaves");
    assert_eq!(
        ChatRoom::owner_id(&client, room.id).await.expect("owner"),
        Some(second.id),
        "ownership succeeds to the earliest remaining member"
    );

    ChatRoomMember::leave(&client, room.id, second.id)
        .await
        .expect("second leaves");
    ChatRoomMember::leave(&client, room.id, third.id)
        .await
        .expect("third leaves");
    assert_eq!(
        ChatRoom::owner_id(&client, room.id).await.expect("owner"),
        None,
        "an empty room has no owner"
    );
}

#[tokio::test]
async fn owner_of_a_room_without_a_recorded_creator_is_its_first_member() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");

    // Every room that predates `created_by` looks like this: no creator on
    // record, so the earliest member holds it.
    let first = create_test_user(&test_db.db, "legacy_first").await;
    let later = create_test_user(&test_db.db, "legacy_later").await;
    let room = ChatRoom::get_or_create_public_room(&client, "legacy")
        .await
        .expect("create public room");
    assert!(room.created_by.is_none());
    ChatRoomMember::join(&client, room.id, first.id)
        .await
        .expect("first joins");
    ChatRoomMember::join(&client, room.id, later.id)
        .await
        .expect("later joins");

    assert_eq!(
        ChatRoom::owner_id(&client, room.id).await.expect("owner"),
        Some(first.id)
    );
}

#[tokio::test]
async fn setting_topic_and_rules_stores_blanks_as_unset() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");

    let creator = create_test_user(&test_db.db, "info_creator").await;
    let room = ChatRoom::create_private_room(&client, "info", creator.id)
        .await
        .expect("create private room");
    assert_eq!(room.created_by, Some(creator.id));

    let room = ChatRoom::set_topic_and_rules(&client, room.id, Some("  books  "), Some("be kind"))
        .await
        .expect("set info");
    assert_eq!(room.topic.as_deref(), Some("books"));
    assert_eq!(room.rules.as_deref(), Some("be kind"));

    let room = ChatRoom::set_topic_and_rules(&client, room.id, Some("   "), None)
        .await
        .expect("clear info");
    assert_eq!(room.topic, None, "a blank topic clears back to unset");
    assert_eq!(room.rules, None);
}

#[tokio::test]
async fn room_state_caps_the_unread_count() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");

    let room = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge");
    let reader = User::create(
        &client,
        UserParams {
            fingerprint: "room-state-cap-reader".to_string(),
            username: "cap_reader".to_string(),
            settings: serde_json::json!({}),
        },
    )
    .await
    .expect("create reader");
    let author = User::create(
        &client,
        UserParams {
            fingerprint: "room-state-cap-author".to_string(),
            username: "cap_author".to_string(),
            settings: serde_json::json!({}),
        },
    )
    .await
    .expect("create author");
    ChatRoomMember::join(&client, room.id, reader.id)
        .await
        .expect("join reader");

    let over_cap = ChatRoomMember::UNREAD_COUNT_CAP + 20;
    for n in 0..over_cap {
        ChatMessage::create(
            &client,
            ChatMessageParams {
                room_id: room.id,
                user_id: author.id,
                body: format!("message {n}"),
            },
        )
        .await
        .expect("create message");
    }

    let state = ChatRoom::list_for_user_with_state(&client, reader.id, None)
        .await
        .expect("room state");

    assert_eq!(
        state.unread_counts.get(&room.id),
        Some(&ChatRoomMember::UNREAD_COUNT_CAP),
        "a room with more unread than the cap reports exactly the cap, not the true total"
    );
    assert!(
        state
            .last_message_at
            .get(&room.id)
            .copied()
            .flatten()
            .is_some(),
        "the room carries the timestamp of its newest message"
    );
}

#[tokio::test]
async fn room_state_excludes_system_activity_lines_from_unread() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");

    let room = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge");
    let reader = User::create(
        &client,
        UserParams {
            fingerprint: "room-state-system-reader".to_string(),
            username: "sys_reader".to_string(),
            settings: serde_json::json!({}),
        },
    )
    .await
    .expect("create reader");
    let system = User::create(
        &client,
        UserParams {
            fingerprint: "room-state-system-bot".to_string(),
            username: "sys_bot".to_string(),
            settings: serde_json::json!({ "bot": true, "system": true }),
        },
    )
    .await
    .expect("create system user");
    ChatRoomMember::join(&client, room.id, reader.id)
        .await
        .expect("join reader");

    // An ambient activity line, and a real message from the same author.
    for body in ["· someone joined", "heads up: new public room opened"] {
        ChatMessage::create(
            &client,
            ChatMessageParams {
                room_id: room.id,
                user_id: system.id,
                body: body.to_string(),
            },
        )
        .await
        .expect("create message");
    }

    let state = ChatRoom::list_for_user_with_state(&client, reader.id, Some(system.id))
        .await
        .expect("room state");
    assert_eq!(
        state.unread_counts.get(&room.id),
        Some(&1),
        "the `· ` activity line is excluded, the bot's real message still counts"
    );

    // Without the system id the bot is just another author, so both count.
    let state = ChatRoom::list_for_user_with_state(&client, reader.id, None)
        .await
        .expect("room state");
    assert_eq!(
        state.unread_counts.get(&room.id),
        Some(&2),
        "with no system user known, nothing is excluded"
    );
}

#[tokio::test]
async fn room_state_lists_the_same_rooms_in_the_same_order_as_list_for_user() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");

    ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge");
    let user = create_test_user(&test_db.db, "room_state_order").await;
    ChatRoomMember::auto_join_public_rooms(&client, user.id)
        .await
        .expect("auto join");
    let private = ChatRoom::create_private_room(&client, "state-order", user.id)
        .await
        .expect("create private room");
    ChatRoomMember::join(&client, private.id, user.id)
        .await
        .expect("join private");

    let plain = ChatRoom::list_for_user(&client, user.id)
        .await
        .expect("list for user");
    let state = ChatRoom::list_for_user_with_state(&client, user.id, None)
        .await
        .expect("room state");

    let plain_ids: Vec<_> = plain.iter().map(|room| room.id).collect();
    let state_ids: Vec<_> = state.rooms.iter().map(|room| room.id).collect();
    assert_eq!(
        plain_ids, state_ids,
        "the merged query must not change which rooms appear or their order"
    );
    for room in &state.rooms {
        assert!(
            state.unread_counts.contains_key(&room.id),
            "every room carries an unread entry, so callers never see absent-vs-zero"
        );
        assert!(state.last_message_at.contains_key(&room.id));
    }
}

#[tokio::test]
async fn test_stream_room_follows_the_account_not_the_username() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let alice = create_test_user(&test_db.db, "streamer-alice").await;
    let usurper = create_test_user(&test_db.db, "streamer-mallory").await;

    let original = ChatRoom::get_or_create_stream_room(&client, &alice.username, alice.id)
        .await
        .expect("alice stream room");
    assert_eq!(original.created_by, Some(alice.id));

    // A second /golive reuses the same room.
    let again = ChatRoom::get_or_create_stream_room(&client, &alice.username, alice.id)
        .await
        .expect("alice stream room again");
    assert_eq!(again.id, original.id);

    // Alice renamed: her room follows her account, old slug and all.
    let renamed = ChatRoom::get_or_create_stream_room(&client, "totally-new-name", alice.id)
        .await
        .expect("renamed stream room");
    assert_eq!(renamed.id, original.id);

    // Another account claiming alice's freed username must NOT inherit her
    // room (and its chat history); it gets a room of its own.
    let taken_over = ChatRoom::get_or_create_stream_room(&client, &alice.username, usurper.id)
        .await
        .expect("usurper stream room");
    assert_ne!(taken_over.id, original.id);
    assert_eq!(taken_over.created_by, Some(usurper.id));
}

#[tokio::test]
async fn the_deadchannel_slug_is_reserved_for_the_game() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let user = create_test_user(&test_db.db, "dc-squatter").await;

    // The game's home channel (GAME.md, First contact): the invitation DM
    // ends in `/join #deadchannel`, so the name has to be waiting for the
    // game, not for whoever typed it first. Same choke point as the lounge
    // reservation; the lowercasing normalize catches case variants.
    assert!(
        ChatRoom::get_or_create_public_room(&client, "deadchannel")
            .await
            .is_err()
    );
    assert!(
        ChatRoom::get_or_create_public_room(&client, "DeadChannel")
            .await
            .is_err()
    );
    assert!(
        ChatRoom::create_private_room(&client, "deadchannel", user.id)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn the_deadchannel_room_is_seeded_once_and_hidden_from_every_listing() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let member = create_test_user(&test_db.db, "dc-runner").await;

    let room = ChatRoom::get_or_create_deadchannel_room(&client)
        .await
        .expect("seed deadchannel");
    assert_eq!(room.kind, "deadchannel");
    assert_eq!(room.slug.as_deref(), Some("deadchannel"));
    assert!(!room.auto_join);
    let again = ChatRoom::get_or_create_deadchannel_room(&client)
        .await
        .expect("re-seed deadchannel");
    assert_eq!(again.id, room.id);

    // Never discoverable, even for a member: browse and IRC listings are
    // kind whitelists, and the new kind is on none of them. The channel is
    // only ever spoken of.
    ChatRoomMember::join(&client, room.id, member.id)
        .await
        .expect("join member");
    let discover = ChatRoom::list_discover_public_topic_rooms(&client)
        .await
        .expect("discover listing");
    assert!(discover.iter().all(|entry| entry.room_id != room.id));
    let summaries = ChatRoom::list_public_topic_room_summaries(&client)
        .await
        .expect("summary listing");
    assert!(
        summaries
            .iter()
            .all(|entry| entry.slug.as_deref() != Some("deadchannel"))
    );
    let irc = ChatRoom::list_irc_channels(&client, member.id)
        .await
        .expect("irc listing");
    assert!(irc.iter().all(|entry| entry.id != room.id));
}
