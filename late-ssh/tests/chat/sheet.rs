//! Integration tests for character sheets (model + chat service tasks)
//! against a real ephemeral DB.

use late_core::models::{character_sheet::CharacterSheet, chat_room::ChatRoom};
use late_core::test_utils::create_test_user;

use super::helpers::new_test_db;

#[tokio::test]
async fn character_sheet_upsert_creates_then_updates() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "sheet-model-it").await;
    let client = test_db.db.get().await.expect("db client");
    let room = ChatRoom::ensure_general(&client).await.expect("room");

    let missing = CharacterSheet::find_by_user_room(&client, user.id, room.id)
        .await
        .expect("find");
    assert!(missing.is_none());

    let created = CharacterSheet::upsert(&client, user.id, room.id, "Tav", "Half-elf bard")
        .await
        .expect("insert");
    assert_eq!(created.name, "Tav");
    assert_eq!(created.body, "Half-elf bard");

    let updated = CharacterSheet::upsert(&client, user.id, room.id, "Tav II", "Now a paladin")
        .await
        .expect("update");
    assert_eq!(updated.id, created.id, "upsert must update the same row");
    assert_eq!(updated.name, "Tav II");

    let found = CharacterSheet::find_by_user_room(&client, user.id, room.id)
        .await
        .expect("find")
        .expect("row exists");
    assert_eq!(found.name, "Tav II");
    assert_eq!(found.body, "Now a paladin");
}
