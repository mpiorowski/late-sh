//! Integration tests for character sheets (model + chat service tasks)
//! against a real ephemeral DB.

use late_core::models::{
    character_sheet::{CharacterSheet, CharacterSheetParams},
    chat_room::ChatRoom,
    chat_room_member::ChatRoomMember,
};
use late_core::test_utils::create_test_user;
use late_ssh::app::chat::notifications::svc::NotificationService;
use late_ssh::app::chat::svc::{ChatEvent, ChatService};
use tokio::sync::broadcast;
use tokio::time::{Duration, sleep, timeout};
use uuid::Uuid;

use super::helpers::new_test_db;

fn sheet_params(user_id: Uuid, room_id: Uuid, name: &str, body: &str) -> CharacterSheetParams {
    CharacterSheetParams {
        user_id,
        room_id,
        name: name.to_string(),
        body: body.to_string(),
    }
}

async fn joined_dnd_room(client: &tokio_postgres::Client, user_ids: &[Uuid]) -> ChatRoom {
    let room = ChatRoom::get_or_create_public_room(client, "dnd")
        .await
        .expect("create dnd room");
    for user_id in user_ids {
        ChatRoomMember::join(client, room.id, *user_id)
            .await
            .expect("join dnd room");
    }
    room
}

#[tokio::test]
async fn character_sheet_upsert_creates_then_updates() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "sheet-model-it").await;
    let client = test_db.db.get().await.expect("db client");
    let room = joined_dnd_room(&client, &[user.id]).await;

    let missing = CharacterSheet::find_by_user_room(&client, user.id, room.id)
        .await
        .expect("find");
    assert!(missing.is_none());

    let created = CharacterSheet::upsert(
        &client,
        sheet_params(user.id, room.id, "Tav", "Half-elf bard"),
    )
    .await
    .expect("insert");
    assert_eq!(created.name, "Tav");
    assert_eq!(created.body, "Half-elf bard");

    let updated = CharacterSheet::upsert(
        &client,
        sheet_params(user.id, room.id, "Tav II", "Now a paladin"),
    )
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

fn sheet_service(test_db: &late_core::test_utils::TestDb) -> ChatService {
    ChatService::new(
        test_db.db.clone(),
        NotificationService::new(test_db.db.clone()),
    )
}

/// Receive events until a sheet-related one arrives (other events may
/// interleave on the shared broadcast channel).
async fn next_sheet_event(events: &mut broadcast::Receiver<ChatEvent>) -> ChatEvent {
    timeout(Duration::from_secs(2), async {
        loop {
            let event = events.recv().await.expect("event");
            if matches!(
                event,
                ChatEvent::OpenSheetResolved { .. } | ChatEvent::SheetError { .. }
            ) {
                return event;
            }
        }
    })
    .await
    .expect("sheet event timeout")
}

#[tokio::test]
async fn open_sheet_task_returns_empty_editable_draft_for_own_missing_sheet() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "sheet-own-it").await;
    let client = test_db.db.get().await.expect("db client");
    let room = joined_dnd_room(&client, &[user.id]).await;
    let service = sheet_service(&test_db);
    let mut events = service.subscribe_events();

    service.open_sheet_task(user.id, room.id, None);

    match next_sheet_event(&mut events).await {
        ChatEvent::OpenSheetResolved {
            user_id,
            room_id,
            target_user_id,
            target_username,
            name,
            body,
        } => {
            assert_eq!(user_id, user.id);
            assert_eq!(room_id, room.id);
            assert_eq!(target_user_id, user.id);
            assert_eq!(target_username, "sheet-own-it");
            assert_eq!(name, "");
            assert_eq!(body, "");
        }
        other => panic!("expected OpenSheetResolved, got {other:?}"),
    }
}

#[tokio::test]
async fn open_sheet_task_resolves_another_users_sheet() {
    let test_db = new_test_db().await;
    let viewer = create_test_user(&test_db.db, "sheet-viewer-it").await;
    let owner = create_test_user(&test_db.db, "sheet-owner-it").await;
    let client = test_db.db.get().await.expect("db client");
    let room = joined_dnd_room(&client, &[viewer.id, owner.id]).await;
    CharacterSheet::upsert(
        &client,
        sheet_params(owner.id, room.id, "Gimli", "Axe enthusiast"),
    )
    .await
    .expect("seed sheet");
    let service = sheet_service(&test_db);
    let mut events = service.subscribe_events();

    service.open_sheet_task(viewer.id, room.id, Some("sheet-owner-it".to_string()));

    match next_sheet_event(&mut events).await {
        ChatEvent::OpenSheetResolved {
            user_id,
            target_user_id,
            target_username,
            name,
            body,
            ..
        } => {
            assert_eq!(user_id, viewer.id);
            assert_eq!(target_user_id, owner.id);
            assert_eq!(target_username, "sheet-owner-it");
            assert_eq!(name, "Gimli");
            assert_eq!(body, "Axe enthusiast");
        }
        other => panic!("expected OpenSheetResolved, got {other:?}"),
    }
}

#[tokio::test]
async fn open_sheet_task_errors_when_target_has_no_sheet() {
    let test_db = new_test_db().await;
    let viewer = create_test_user(&test_db.db, "sheet-nosheet-viewer-it").await;
    let _owner = create_test_user(&test_db.db, "sheet-nosheet-owner-it").await;
    let client = test_db.db.get().await.expect("db client");
    let room = joined_dnd_room(&client, &[viewer.id, _owner.id]).await;
    let service = sheet_service(&test_db);
    let mut events = service.subscribe_events();

    service.open_sheet_task(
        viewer.id,
        room.id,
        Some("sheet-nosheet-owner-it".to_string()),
    );

    match next_sheet_event(&mut events).await {
        ChatEvent::SheetError { user_id, message } => {
            assert_eq!(user_id, viewer.id);
            assert!(
                message.contains("has no character sheet"),
                "unexpected message: {message}"
            );
            assert!(message.contains("sheet-nosheet-owner-it"));
        }
        other => panic!("expected SheetError, got {other:?}"),
    }
}

#[tokio::test]
async fn open_sheet_task_errors_for_unknown_username() {
    let test_db = new_test_db().await;
    let viewer = create_test_user(&test_db.db, "sheet-unknown-it").await;
    let client = test_db.db.get().await.expect("db client");
    let room = joined_dnd_room(&client, &[viewer.id]).await;
    let service = sheet_service(&test_db);
    let mut events = service.subscribe_events();

    service.open_sheet_task(viewer.id, room.id, Some("no-such-user".to_string()));

    match next_sheet_event(&mut events).await {
        ChatEvent::SheetError { user_id, message } => {
            assert_eq!(user_id, viewer.id);
            assert!(
                message.contains("not found"),
                "unexpected message: {message}"
            );
        }
        other => panic!("expected SheetError, got {other:?}"),
    }
}

#[tokio::test]
async fn open_sheet_task_errors_outside_dnd_room() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "sheet-wrong-room-it").await;
    let client = test_db.db.get().await.expect("db client");
    let room = ChatRoom::ensure_lounge(&client).await.expect("lounge room");
    ChatRoomMember::join(&client, room.id, user.id)
        .await
        .expect("join lounge room");
    let service = sheet_service(&test_db);
    let mut events = service.subscribe_events();

    service.open_sheet_task(user.id, room.id, None);

    match next_sheet_event(&mut events).await {
        ChatEvent::SheetError { user_id, message } => {
            assert_eq!(user_id, user.id);
            assert!(
                message.contains("only available in #dnd"),
                "unexpected message: {message}"
            );
        }
        other => panic!("expected SheetError, got {other:?}"),
    }
}

#[tokio::test]
async fn open_sheet_task_resolves_own_existing_sheet_as_editable_target() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "sheet-own-existing-it").await;
    let client = test_db.db.get().await.expect("db client");
    let room = joined_dnd_room(&client, &[user.id]).await;
    CharacterSheet::upsert(
        &client,
        sheet_params(user.id, room.id, "Elrond", "Lord of Rivendell"),
    )
    .await
    .expect("seed sheet");
    let service = sheet_service(&test_db);
    let mut events = service.subscribe_events();

    service.open_sheet_task(user.id, room.id, None);

    match next_sheet_event(&mut events).await {
        ChatEvent::OpenSheetResolved {
            user_id,
            target_user_id,
            target_username,
            name,
            body,
            ..
        } => {
            assert_eq!(user_id, user.id);
            assert_eq!(target_user_id, user.id);
            assert_eq!(target_username, "sheet-own-existing-it");
            assert_eq!(name, "Elrond");
            assert_eq!(body, "Lord of Rivendell");
        }
        other => panic!("expected OpenSheetResolved, got {other:?}"),
    }
}

#[tokio::test]
async fn save_sheet_task_upserts_row() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "sheet-save-it").await;
    let client = test_db.db.get().await.expect("db client");
    let room = joined_dnd_room(&client, &[user.id]).await;
    let service = sheet_service(&test_db);

    service.save_sheet_task(
        user.id,
        room.id,
        "Aragorn".to_string(),
        "King of Gondor".to_string(),
    );

    timeout(Duration::from_secs(2), async {
        loop {
            let row = CharacterSheet::find_by_user_room(&client, user.id, room.id)
                .await
                .expect("find");
            if let Some(sheet) = row {
                assert_eq!(sheet.name, "Aragorn");
                assert_eq!(sheet.body, "King of Gondor");
                break;
            }
            sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("save timeout");
}

#[tokio::test]
async fn save_sheet_task_errors_outside_dnd_room() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "sheet-save-wrong-room-it").await;
    let client = test_db.db.get().await.expect("db client");
    let room = ChatRoom::ensure_lounge(&client).await.expect("lounge room");
    ChatRoomMember::join(&client, room.id, user.id)
        .await
        .expect("join lounge room");
    let service = sheet_service(&test_db);
    let mut events = service.subscribe_events();

    service.save_sheet_task(
        user.id,
        room.id,
        "Boromir".to_string(),
        "Captain of Gondor".to_string(),
    );

    match next_sheet_event(&mut events).await {
        ChatEvent::SheetError { user_id, message } => {
            assert_eq!(user_id, user.id);
            assert!(
                message.contains("only available in #dnd"),
                "unexpected message: {message}"
            );
        }
        other => panic!("expected SheetError, got {other:?}"),
    }
    let row = CharacterSheet::find_by_user_room(&client, user.id, room.id)
        .await
        .expect("find");
    assert!(row.is_none());
}
