use crate::{
    models::{
        chat_message::{ChatMessage, ChatMessageParams},
        chat_room::ChatRoom,
        message_translation::{MessageTranslation, TranslateLang, needs_translation},
        user::{User, UserParams},
    },
    test_utils::test_db,
};

#[test]
fn needs_translation_compares_dominant_script_against_target() {
    // Foreign script for the target -> translate.
    assert!(needs_translation(
        "你好，我刚发现这个地方",
        TranslateLang::En
    ));
    assert!(needs_translation("서버 너무 좋다", TranslateLang::En));
    assert!(needs_translation(
        "what a cozy place",
        TranslateLang::ZhHans
    ));
    assert!(needs_translation("check the arcade", TranslateLang::Ko));
    assert!(needs_translation("これはすごいですね", TranslateLang::En));
    assert!(needs_translation("यह जगह बहुत अच्छी है", TranslateLang::En));
    assert!(needs_translation("привет, как дела", TranslateLang::En));
    assert!(needs_translation("สวัสดีครับ ทุกคน", TranslateLang::En));
    assert!(needs_translation("what a cozy place", TranslateLang::Ja));
    assert!(needs_translation("what a cozy place", TranslateLang::Th));
    assert!(needs_translation("what a cozy place", TranslateLang::Hi));

    // Same script as the target -> nothing to do.
    assert!(!needs_translation(
        "just chatting in english",
        TranslateLang::En
    ));
    assert!(!needs_translation("你好你好你好", TranslateLang::ZhHans));
    assert!(!needs_translation("좋아요 좋아요", TranslateLang::Ko));
    assert!(!needs_translation("これはすごいですね", TranslateLang::Ja));
    assert!(!needs_translation("สวัสดีครับ ทุกคน", TranslateLang::Th));
    assert!(!needs_translation("यह जगह बहुत अच्छी है", TranslateLang::Hi));

    // Targets sharing a script (the Latin roster; Russian/Ukrainian on
    // Cyrillic) can't be cleared by script alone: everything scripted goes
    // to the model, even same-language bodies.
    assert!(needs_translation(
        "just chatting in english",
        TranslateLang::Es
    ));
    assert!(needs_translation("hola a todos", TranslateLang::Es));
    assert!(needs_translation(
        "bonjour tout le monde",
        TranslateLang::Fr
    ));
    assert!(needs_translation("cześć wszystkim", TranslateLang::Pl));
    assert!(needs_translation("привет, как дела", TranslateLang::Uk));
    assert!(needs_translation(
        "你好，我刚发现这个地方",
        TranslateLang::Pt
    ));

    // Mixed bodies go by the dominant script.
    assert!(needs_translation("看看这个 lol", TranslateLang::En));
    assert!(!needs_translation("check this out 哈哈", TranslateLang::En));

    // Unscripted or near-empty bodies never qualify.
    assert!(!needs_translation("👍👍👍", TranslateLang::En));
    assert!(!needs_translation("42", TranslateLang::ZhHans));
    assert!(!needs_translation("ok", TranslateLang::ZhHans));
    assert!(!needs_translation("", TranslateLang::En));
    assert!(!needs_translation(&"字".repeat(2_000), TranslateLang::En));
}

#[test]
fn translate_lang_keys_round_trip_and_cycle_covers_the_roster() {
    for lang in TranslateLang::ALL {
        assert_eq!(TranslateLang::from_key(lang.as_str()), Some(lang));
        assert_eq!(lang.cycle(true).cycle(false), lang);
    }
    assert_eq!(TranslateLang::from_key("nope"), None);
    let mut seen = vec![TranslateLang::En];
    let mut current = TranslateLang::En;
    loop {
        current = current.cycle(true);
        if current == TranslateLang::En {
            break;
        }
        seen.push(current);
    }
    assert_eq!(seen.len(), TranslateLang::ALL.len());
}

#[tokio::test]
async fn cache_rows_upsert_read_and_die_with_the_message() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let room = ChatRoom::ensure_lounge(&client).await.expect("lounge");
    let user = User::create(
        &client,
        UserParams {
            fingerprint: "translation-user-1".to_string(),
            username: "tr1".to_string(),
            settings: serde_json::json!({}),
        },
    )
    .await
    .unwrap();
    let msg = ChatMessage::create(
        &client,
        ChatMessageParams {
            room_id: room.id,
            user_id: user.id,
            body: "你好".to_string(),
        },
    )
    .await
    .unwrap();

    assert!(
        MessageTranslation::upsert_if_current(&client, msg.id, TranslateLang::En, "你好", "hello")
            .await
            .unwrap()
    );
    // Re-upsert replaces, one row per (message, lang).
    assert!(
        MessageTranslation::upsert_if_current(
            &client,
            msg.id,
            TranslateLang::En,
            "你好",
            "hello there",
        )
        .await
        .unwrap()
    );
    assert!(
        MessageTranslation::upsert_if_current(&client, msg.id, TranslateLang::Ko, "你好", "안녕")
            .await
            .unwrap()
    );

    // A result whose source body no longer matches (the message was edited
    // mid-flight) must not land: it describes text that no longer exists.
    assert!(
        !MessageTranslation::upsert_if_current(
            &client,
            msg.id,
            TranslateLang::En,
            "pre-edit body",
            "stale translation",
        )
        .await
        .unwrap()
    );

    let cached = MessageTranslation::get_many(&client, &[msg.id], TranslateLang::En)
        .await
        .unwrap();
    assert_eq!(cached.get(&msg.id).map(String::as_str), Some("hello there"));

    let deleted = MessageTranslation::delete_for_message(&client, msg.id)
        .await
        .unwrap();
    assert_eq!(deleted, 2);
    let cached = MessageTranslation::get_many(&client, &[msg.id], TranslateLang::En)
        .await
        .unwrap();
    assert!(cached.is_empty());
}
