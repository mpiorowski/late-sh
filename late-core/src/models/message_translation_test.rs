use crate::{
    models::{
        chat_message::{ChatMessage, ChatMessageParams},
        chat_room::ChatRoom,
        message_translation::{
            CachedTranslation, MessageTranslation, TranslateLang, needs_translation,
            translation_source_text,
        },
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
    assert!(!needs_translation("你好你好你好", TranslateLang::ZhHans));
    assert!(!needs_translation("좋아요 좋아요", TranslateLang::Ko));
    assert!(!needs_translation("これはすごいですね", TranslateLang::Ja));
    assert!(!needs_translation("สวัสดีครับ ทุกคน", TranslateLang::Th));
    assert!(!needs_translation("यह जगह बहुत अच्छी है", TranslateLang::Hi));

    // Targets sharing a script (English and the rest of the Latin roster;
    // Russian/Ukrainian on Cyrillic) can't be cleared by script alone:
    // everything scripted goes to the model, even same-language bodies, and
    // the model reports same-language instead of translating. A French
    // message must reach the model for an English reader.
    assert!(needs_translation(
        "just chatting in english",
        TranslateLang::En
    ));
    assert!(needs_translation(
        "bonjour tout le monde",
        TranslateLang::En
    ));
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
fn needs_translation_judges_the_reply_text_not_the_quoted_line() {
    // The composer stores a reply as "> @author: preview\nreply text". The
    // quote is someone else's message, already on screen above; only the
    // reply text decides whether a translation is worth asking for.

    // Unscripted reply under a scripted quote: nothing to translate.
    assert!(!needs_translation(
        "> @bob: bonjour tout le monde\n👍👍",
        TranslateLang::ZhHans
    ));
    // Latin reply under a Han-dominant quote: the reply still needs
    // translating for a Chinese reader even though the body's dominant
    // script is Han.
    assert!(needs_translation(
        "> @lin: 你好你好你好你好你好你好你好你好你好你好\nok yes see you tonight",
        TranslateLang::ZhHans
    ));
}

#[test]
fn translation_source_text_drops_only_a_leading_quote_line() {
    // A reply: quote line goes, reply text stays.
    assert_eq!(
        translation_source_text("> @bob: hello there\nça va très bien"),
        "ça va très bien"
    );
    // Multi-line reply text stays whole.
    assert_eq!(
        translation_source_text("> @bob: hello\nline one\nline two"),
        "line one\nline two"
    );
    // Not a reply: untouched.
    assert_eq!(translation_source_text("plain message"), "plain message");
    assert_eq!(
        translation_source_text("first\n> quoted markdown later"),
        "first\n> quoted markdown later"
    );
    // A body that is only a quote line passes through whole.
    assert_eq!(
        translation_source_text("> @bob: hello there"),
        "> @bob: hello there"
    );
    assert_eq!(
        translation_source_text("> @bob: hello\n  "),
        "> @bob: hello\n  "
    );
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
        MessageTranslation::upsert_if_current(
            &client,
            msg.id,
            TranslateLang::En,
            "你好",
            &CachedTranslation::Translated("hello".to_string()),
        )
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
            &CachedTranslation::Translated("hello there".to_string()),
        )
        .await
        .unwrap()
    );
    // A same-language verdict is cached like a translation and replaces on
    // conflict too, so a verdict can flip after an edit re-request.
    assert!(
        MessageTranslation::upsert_if_current(
            &client,
            msg.id,
            TranslateLang::ZhHans,
            "你好",
            &CachedTranslation::SameLanguage,
        )
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
            &CachedTranslation::Translated("stale translation".to_string()),
        )
        .await
        .unwrap()
    );

    let cached = MessageTranslation::get_many(&client, &[msg.id], TranslateLang::En)
        .await
        .unwrap();
    assert_eq!(
        cached.get(&msg.id),
        Some(&CachedTranslation::Translated("hello there".to_string()))
    );
    let cached = MessageTranslation::get_many(&client, &[msg.id], TranslateLang::ZhHans)
        .await
        .unwrap();
    assert_eq!(cached.get(&msg.id), Some(&CachedTranslation::SameLanguage));

    let deleted = MessageTranslation::delete_for_message(&client, msg.id)
        .await
        .unwrap();
    assert_eq!(deleted, 2);
    let cached = MessageTranslation::get_many(&client, &[msg.id], TranslateLang::En)
        .await
        .unwrap();
    assert!(cached.is_empty());
}
