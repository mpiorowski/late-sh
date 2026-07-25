use crate::{
    models::{
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
