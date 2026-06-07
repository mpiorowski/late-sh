use std::time::Duration;

use late_core::{
    models::game_room::{GameKind, GameRoom},
    test_utils::test_db,
};
use serde_json::json;

#[tokio::test]
async fn inactive_open_rooms_are_hard_deleted_with_game_chat() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");

    let room = GameRoom::create_with_chat_room(
        &client,
        GameKind::Blackjack,
        "cleanup-open",
        "Cleanup Open",
        json!({}),
        None,
    )
    .await
    .expect("create game room");
    client
        .execute(
            "UPDATE game_rooms
             SET updated = current_timestamp - interval '2 hours'
             WHERE id = $1",
            &[&room.id],
        )
        .await
        .expect("age game room");

    let deleted = GameRoom::delete_inactive_open(&client, Duration::from_secs(60 * 60))
        .await
        .expect("delete inactive open rooms");

    assert_eq!(deleted, 1);
    assert_game_room_count(&client, room.id, 0).await;
    assert_chat_room_count(&client, room.chat_room_id, 0).await;
}

#[tokio::test]
async fn inactive_in_round_rooms_are_not_deleted() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");

    let room = GameRoom::create_with_chat_room(
        &client,
        GameKind::Tron,
        "cleanup-active",
        "Cleanup Active",
        json!({}),
        None,
    )
    .await
    .expect("create game room");
    client
        .execute(
            "UPDATE game_rooms
             SET status = 'in_round',
                 updated = current_timestamp - interval '2 hours'
             WHERE id = $1",
            &[&room.id],
        )
        .await
        .expect("mark active and stale");

    let deleted = GameRoom::delete_inactive_open(&client, Duration::from_secs(60 * 60))
        .await
        .expect("delete inactive open rooms");

    assert_eq!(deleted, 0);
    assert_game_room_count(&client, room.id, 1).await;
    assert_chat_room_count(&client, room.chat_room_id, 1).await;
}

#[tokio::test]
async fn restart_reconciliation_preserves_only_active_chess_rounds() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");

    let blackjack = create_in_round_room(
        &client,
        GameKind::Blackjack,
        "reconcile-blackjack",
        json!({}),
    )
    .await;
    let active_chess = create_in_round_room(
        &client,
        GameKind::Chess,
        "reconcile-active-chess",
        json!({ "phase": "Active" }),
    )
    .await;
    let finished_chess = create_in_round_room(
        &client,
        GameKind::Chess,
        "reconcile-finished-chess",
        json!({ "phase": "Finished" }),
    )
    .await;

    let reconciled = GameRoom::reconcile_in_round_after_restart(&client)
        .await
        .expect("reconcile statuses");

    assert_eq!(reconciled, 2);
    assert_eq!(room_status(&client, blackjack.id).await, "open");
    assert_eq!(room_status(&client, active_chess.id).await, "in_round");
    assert_eq!(room_status(&client, finished_chess.id).await, "open");
}

async fn create_in_round_room(
    client: &tokio_postgres::Client,
    game_kind: GameKind,
    slug: &str,
    runtime_state: serde_json::Value,
) -> GameRoom {
    let room = GameRoom::create_with_chat_room(client, game_kind, slug, slug, json!({}), None)
        .await
        .expect("create game room");
    client
        .execute(
            "UPDATE game_rooms
             SET status = 'in_round',
                 runtime_state = $2,
                 updated = current_timestamp - interval '2 hours'
             WHERE id = $1",
            &[&room.id, &runtime_state],
        )
        .await
        .expect("mark in round");
    room
}

async fn assert_game_room_count(
    client: &tokio_postgres::Client,
    room_id: uuid::Uuid,
    expected: i64,
) {
    let row = client
        .query_one(
            "SELECT COUNT(*)::bigint AS count FROM game_rooms WHERE id = $1",
            &[&room_id],
        )
        .await
        .expect("count game room");
    assert_eq!(row.get::<_, i64>("count"), expected);
}

async fn assert_chat_room_count(
    client: &tokio_postgres::Client,
    room_id: uuid::Uuid,
    expected: i64,
) {
    let row = client
        .query_one(
            "SELECT COUNT(*)::bigint AS count FROM chat_rooms WHERE id = $1",
            &[&room_id],
        )
        .await
        .expect("count chat room");
    assert_eq!(row.get::<_, i64>("count"), expected);
}

async fn room_status(client: &tokio_postgres::Client, room_id: uuid::Uuid) -> String {
    client
        .query_one("SELECT status FROM game_rooms WHERE id = $1", &[&room_id])
        .await
        .expect("load status")
        .get("status")
}
