use chrono::NaiveDate;
use late_core::{
    models::{
        chips::Difficulty,
        sliding_puzzle::{DailyWin, Game, GameParams},
    },
    test_utils::create_test_user,
};
use std::time::Duration;

use super::svc::SlidingPuzzleService;
use crate::{
    app::activity::event::{ActivityGame, ActivityKind},
    test_helpers::new_test_db,
};
use tokio::sync::broadcast;

#[tokio::test]
async fn load_waits_for_queued_saves_before_reading_reconnect_state() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "sliding-puzzle-load-barrier").await;
    let (activity_tx, _) = broadcast::channel(8);
    let service = SlidingPuzzleService::new(test_db.db.clone(), activity_tx);
    let initial = GameParams {
        user_id: user.id,
        mode: "personal".to_string(),
        difficulty_key: Difficulty::Easy.key().to_string(),
        puzzle_date: None,
        puzzle_seed: 1,
        tiles: vec![1, 2, 3, 4, 5, 6, 7, 0, 8],
        moves: 11,
    };
    let mut lock_client = test_db.db.get().await.expect("lock client");
    Game::upsert(&lock_client, initial.clone())
        .await
        .expect("seed personal slot");
    let transaction = lock_client.transaction().await.expect("lock transaction");
    transaction
        .query_one(
            "SELECT id FROM sliding_puzzle_games
             WHERE user_id = $1 AND difficulty_key = $2 AND mode = 'personal'
             FOR UPDATE",
            &[&user.id, &Difficulty::Easy.key()],
        )
        .await
        .expect("lock personal slot");

    service.save_game_task(GameParams {
        puzzle_seed: 2,
        tiles: vec![1, 2, 3, 4, 5, 6, 0, 7, 8],
        moves: 12,
        ..initial
    });
    let load = tokio::spawn({
        let service = service.clone();
        async move { service.load_games(user.id).await }
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(
        !load.is_finished(),
        "reconnect load bypassed the queued save"
    );

    transaction.commit().await.expect("release personal slot");
    let games = load.await.expect("load task").expect("load games");
    let personal = games
        .into_iter()
        .find(|game| game.mode == "personal" && game.difficulty_key == Difficulty::Easy.key())
        .expect("personal slot");
    assert_eq!(personal.puzzle_seed, 2);
    assert_eq!(personal.moves, 12);
}

/// The flush barrier only orders queued writes ahead of the read; the database
/// is the authority. A dead queue must not turn a reconnect into an empty
/// restore that overwrites persisted boards.
#[tokio::test]
async fn load_reads_persisted_rows_when_the_save_queue_worker_is_gone() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "sliding-puzzle-dead-queue").await;
    let (activity_tx, _) = broadcast::channel(8);
    let service = SlidingPuzzleService::new(test_db.db.clone(), activity_tx);
    let client = test_db.db.get().await.expect("db client");
    Game::upsert(
        &client,
        GameParams {
            user_id: user.id,
            mode: "personal".to_string(),
            difficulty_key: Difficulty::Easy.key().to_string(),
            puzzle_date: None,
            puzzle_seed: 7,
            tiles: vec![1, 2, 3, 4, 5, 6, 7, 0, 8],
            moves: 9,
        },
    )
    .await
    .expect("seed personal slot");

    // Start the queue on a runtime that then goes away: the worker dies with
    // it, so the barrier can never answer again while this handle lives on.
    let queue_owner = service.clone();
    std::thread::spawn(move || {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("doomed runtime")
            .block_on(async move {
                queue_owner
                    .flush_game_saves()
                    .await
                    .expect("save queue starts healthy");
            });
    })
    .join()
    .expect("doomed runtime thread");

    let personal = service
        .load_games(user.id)
        .await
        .expect("dead queue still restores from the database")
        .into_iter()
        .find(|game| game.mode == "personal")
        .expect("personal slot");
    assert_eq!(personal.moves, 9);
}

#[tokio::test]
async fn queued_game_saves_preserve_transition_order_across_service_clones() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "sliding-puzzle-save-order").await;
    let today = NaiveDate::from_ymd_opt(2026, 8, 21).unwrap();
    let (activity_tx, _) = broadcast::channel(8);
    let service = SlidingPuzzleService::new(test_db.db.clone(), activity_tx);
    let clone = service.clone();

    service.save_game_task(GameParams {
        user_id: user.id,
        mode: "daily".to_string(),
        difficulty_key: Difficulty::Easy.key().to_string(),
        puzzle_date: Some(today),
        puzzle_seed: 1,
        tiles: vec![1, 2, 3, 4, 5, 6, 7, 0, 8],
        moves: 11,
    });
    clone.save_game_task(GameParams {
        user_id: user.id,
        mode: "daily".to_string(),
        difficulty_key: Difficulty::Easy.key().to_string(),
        puzzle_date: Some(today),
        puzzle_seed: 1,
        tiles: vec![1, 2, 3, 4, 5, 6, 0, 7, 8],
        moves: 12,
    });
    service
        .flush_game_saves()
        .await
        .expect("flush queued saves");

    let easy = service
        .load_games(user.id)
        .await
        .expect("load saved games")
        .into_iter()
        .find(|game| game.difficulty_key == Difficulty::Easy.key())
        .expect("saved easy game");
    assert_eq!(easy.tiles, vec![1, 2, 3, 4, 5, 6, 0, 7, 8]);
    assert_eq!(easy.moves, 12);
}

#[tokio::test]
async fn service_loads_upserted_slots_and_publishes_only_the_first_same_day_win() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "sliding-puzzle-service-it").await;
    let client = test_db.db.get().await.expect("db client");
    let today = NaiveDate::from_ymd_opt(2026, 8, 21).unwrap();
    let (activity_tx, mut activity_rx) = broadcast::channel(8);
    let service = SlidingPuzzleService::new(test_db.db.clone(), activity_tx);

    Game::upsert(
        &client,
        GameParams {
            user_id: user.id,
            mode: "daily".to_string(),
            difficulty_key: Difficulty::Easy.key().to_string(),
            puzzle_date: Some(today),
            puzzle_seed: 1,
            tiles: vec![1, 2, 3, 4, 5, 6, 7, 0, 8],
            moves: 12,
        },
    )
    .await
    .expect("save game");
    let games = service.load_games(user.id).await.expect("load games");
    assert_eq!(games.len(), 1);
    assert_eq!(games[0].moves, 12);

    service
        .record_win_and_publish(user.id, Difficulty::Easy, today, 12)
        .await
        .expect("record first win");
    service
        .record_win_and_publish(user.id, Difficulty::Easy, today, 8)
        .await
        .expect("record replay win");

    let event = activity_rx.recv().await.expect("first activity event");
    assert!(matches!(
        event.kind,
        ActivityKind::GameWon {
            game: ActivityGame::SlidingPuzzle,
            ref detail,
            score: Some(12),
        } if detail.as_deref() == Some("easy")
    ));
    assert!(
        activity_rx.try_recv().is_err(),
        "replay must not publish another win"
    );

    let best = DailyWin::find(&client, user.id, Difficulty::Easy.key(), today)
        .await
        .expect("load best win")
        .expect("win exists");
    assert_eq!(best.moves, 8, "same-day replay keeps the lower move count");
}

/// The board asks for the day's art once, off the tick, and the state
/// lands it as tiles for every difficulty with the piece credited. The
/// claim is the gallery model's (`ArtboardPiece::feature_for_day`); this
/// pins the session side of it.
#[tokio::test]
async fn the_board_lands_yesterdays_gallery_piece_as_its_art() {
    use super::{
        art::{TileGeometry, TileView},
        state::{ArtStatus, State},
    };
    use crate::test_helpers::wait_until;
    use late_core::models::artboard_piece::{ArtboardPiece, HangOutcome, HangParams};
    use serde_json::json;

    let test_db = new_test_db().await;
    let painter = create_test_user(&test_db.db, "puzzle-art-painter").await;
    let player = create_test_user(&test_db.db, "puzzle-art-player").await;
    let client = test_db.db.get().await.expect("db client");
    let piece = match ArtboardPiece::hang(
        &client,
        HangParams {
            user_id: painter.id,
            title: "night train".to_string(),
            width: 12,
            height: 4,
            canvas: json!({
                "width": 12,
                "height": 4,
                "cells": [[{"x": 0, "y": 0}, {"Narrow": "#"}]],
                "colors": [],
            }),
            provenance: json!({ "cells": [[{"x": 0, "y": 0}, "painter"]] }),
            glyph_count: 40,
            own_share_percent: 100,
            content_hash: "hash-night-train".to_string(),
        },
    )
    .await
    .expect("hang")
    {
        HangOutcome::Hung(piece) => piece,
        other => panic!("expected the piece to hang, got {other:?}"),
    };
    client
        .execute(
            "UPDATE artboard_pieces SET created = created - INTERVAL '1 day' WHERE id = $1",
            &[&piece.id],
        )
        .await
        .expect("backdate the piece to yesterday");

    let (activity, _) = broadcast::channel(8);
    let service = SlidingPuzzleService::new(test_db.db.clone(), activity);
    let mut state = State::new(player.id, service, Vec::new());
    assert_eq!(state.tile_view(), TileView::Art);
    assert_eq!(state.art_status(), ArtStatus::Loading);
    assert_eq!(state.art_credit(), None);

    let mut ticks = 0;
    wait_until(
        || {
            ticks += 1;
            state.poll_art();
            let ready = state.art_status() == ArtStatus::Ready;
            async move { ready }
        },
        "the day's art to land",
    )
    .await;
    assert!(ticks > 1, "the load is asynchronous, not a blocking tick");
    assert_eq!(
        state.art_credit().as_deref(),
        Some(format!("night train by @{}", painter.username).as_str())
    );
    // A 12x4 piece pads out to the minimum tile on every board size.
    assert_eq!(
        state.art_tile_geometry(),
        Some(TileGeometry {
            width: 6,
            height: 3
        })
    );
    state.next_difficulty();
    assert_eq!(state.art_grid().expect("hard grid").lines.len(), 15);

    // Numbered tiles hide the art without dropping it; the grid is back
    // on the next toggle with nothing to reload.
    state.toggle_tile_view();
    assert_eq!(state.art_status(), ArtStatus::Numbered);
    assert_eq!(state.art_tile_geometry(), None);
    state.toggle_tile_view();
    assert_eq!(state.art_status(), ArtStatus::Ready);
    assert_eq!(
        ArtboardPiece::feature_for_day(&client, state.puzzle_date())
            .await
            .expect("claim")
            .map(|featured| featured.id),
        Some(piece.id)
    );
}
