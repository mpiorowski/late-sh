use std::time::Duration;

use late_core::models::chat_message::{ChatMessage, ChatMessageParams};
use late_core::models::chat_room::ChatRoom;
use late_core::models::message_translation::{MessageTranslation, TranslateLang};
use uuid::Uuid;

use super::{TranslationOutcome, TranslationService};
use crate::app::ai::svc::AiService;
use crate::test_helpers::new_test_db;

async fn seeded_message(db: &late_core::db::Db, body: &str) -> (Uuid, Uuid) {
    let client = db.get().await.expect("db client");
    let room = ChatRoom::ensure_lounge(&client).await.expect("lounge");
    let user = late_core::test_utils::create_test_user(db, "translator").await;
    let message = ChatMessage::create(
        &client,
        ChatMessageParams {
            room_id: room.id,
            user_id: user.id,
            body: body.to_string(),
        },
    )
    .await
    .expect("create message");
    (message.id, room.id)
}

#[tokio::test]
async fn cached_translation_is_served_without_the_api() {
    let test_db = new_test_db().await;
    let (message_id, room_id) = seeded_message(&test_db.db, "你好").await;
    let client = test_db.db.get().await.expect("db client");
    MessageTranslation::upsert_if_current(&client, message_id, TranslateLang::En, "你好", "hello")
        .await
        .expect("seed cache");

    // AI disabled: a cache hit is the only way this can produce text.
    let service = TranslationService::new(test_db.db.clone(), AiService::new(false, None));
    let mut events = service.subscribe();
    service.request(message_id, room_id, "你好".to_string(), TranslateLang::En);

    let event = tokio::time::timeout(Duration::from_secs(5), events.recv())
        .await
        .expect("event before timeout")
        .expect("channel open");
    assert_eq!(event.message_id, message_id);
    assert_eq!(event.room_id, room_id);
    assert_eq!(event.target, TranslateLang::En);
    match event.outcome {
        TranslationOutcome::Translated(text) => assert_eq!(text, "hello"),
        TranslationOutcome::Failed => panic!("cache hit must not fail"),
    }
}

#[tokio::test]
async fn cache_miss_with_ai_disabled_reports_failure() {
    let test_db = new_test_db().await;
    let (message_id, room_id) = seeded_message(&test_db.db, "서버 너무 좋다").await;

    let service = TranslationService::new(test_db.db.clone(), AiService::new(false, None));
    let mut events = service.subscribe();
    service.request(
        message_id,
        room_id,
        "서버 너무 좋다".to_string(),
        TranslateLang::En,
    );

    let event = tokio::time::timeout(Duration::from_secs(5), events.recv())
        .await
        .expect("event before timeout")
        .expect("channel open");
    assert_eq!(event.message_id, message_id);
    assert!(matches!(event.outcome, TranslationOutcome::Failed));

    // The single-flight set must be clear again: a retry produces another
    // event instead of being swallowed as a duplicate.
    service.request(
        message_id,
        room_id,
        "서버 너무 좋다".to_string(),
        TranslateLang::En,
    );
    let retry = tokio::time::timeout(Duration::from_secs(5), events.recv())
        .await
        .expect("retry event before timeout")
        .expect("channel open");
    assert_eq!(retry.message_id, message_id);
}
