use late_core::{
    models::{
        chat_room::ChatRoom,
        user::{User, UserParams},
    },
    test_utils::test_db,
};

#[tokio::test]
async fn test_chat_room_general_and_language() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");

    let general1 = ChatRoom::ensure_general(&client)
        .await
        .expect("ensure general");
    assert_eq!(general1.kind, "general");
    assert_eq!(general1.slug.as_deref(), Some("general"));
    assert_eq!(general1.visibility, "public");
    assert!(general1.auto_join);

    let general2 = ChatRoom::find_general(&client).await.unwrap().unwrap();
    assert_eq!(general1.id, general2.id);

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
    let private_room = ChatRoom::create_private_room(&client, "side")
        .await
        .expect("create private room");

    assert_eq!(public_room.id, public_room_again.id);
    assert_eq!(public_room.visibility, "public");
    assert!(!public_room.auto_join);
    assert_eq!(private_room.visibility, "private");
    assert!(!private_room.auto_join);
    assert_ne!(public_room.id, private_room.id);

    let duplicate_private = ChatRoom::create_private_room(&client, "side").await;
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
    let private_room = ChatRoom::create_private_room(&client, "vps/d9d0")
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
