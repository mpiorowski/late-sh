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

#[tokio::test]
async fn completion_queue_rejects_personal_game_params() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "sliding-puzzle-personal-complete-guard").await;
    let today = NaiveDate::from_ymd_opt(2026, 8, 21).unwrap();
    let (activity_tx, mut activity_rx) = broadcast::channel(8);
    let service = SlidingPuzzleService::new(test_db.db.clone(), activity_tx);

    service.complete_game_task(
        GameParams {
            user_id: user.id,
            mode: "personal".to_string(),
            difficulty_key: Difficulty::Easy.key().to_string(),
            puzzle_date: None,
            puzzle_seed: 1,
            tiles: vec![1, 2, 3, 4, 5, 6, 7, 8, 0],
            moves: 5,
        },
        Difficulty::Easy,
        today,
        5,
    );
    service
        .flush_game_saves()
        .await
        .expect("flush rejected completion");

    let client = test_db.db.get().await.expect("db client");
    assert!(
        DailyWin::find(&client, user.id, Difficulty::Easy.key(), today)
            .await
            .expect("daily win lookup")
            .is_none()
    );
    assert!(activity_rx.try_recv().is_err());
    assert!(
        service
            .load_games(user.id)
            .await
            .expect("load games")
            .is_empty()
    );
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

/// Only a genuinely finished daily board pays out: the tiles have to be the
/// canonical solved board for that difficulty, the seed has to be the one
/// today's date derives, and it has to have taken at least one move.
#[tokio::test]
async fn completion_queue_rejects_unsolved_forged_seed_and_zero_move_daily_params() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "sliding-puzzle-daily-complete-guard").await;
    let today = NaiveDate::from_ymd_opt(2026, 8, 21).unwrap();
    let (activity_tx, mut activity_rx) = broadcast::channel(8);
    let service = SlidingPuzzleService::new(test_db.db.clone(), activity_tx);
    let daily_seed = super::state::daily_seed(today, Difficulty::Easy) as i64;
    let solved = GameParams {
        user_id: user.id,
        mode: "daily".to_string(),
        difficulty_key: Difficulty::Easy.key().to_string(),
        puzzle_date: Some(today),
        puzzle_seed: daily_seed,
        tiles: vec![1, 2, 3, 4, 5, 6, 7, 8, 0],
        moves: 5,
    };

    for (label, params, moves) in [
        (
            "unsolved tiles",
            GameParams {
                tiles: vec![1, 2, 3, 4, 5, 6, 7, 0, 8],
                ..solved.clone()
            },
            5,
        ),
        (
            "forged daily seed",
            GameParams {
                puzzle_seed: daily_seed ^ 1,
                ..solved.clone()
            },
            5,
        ),
        (
            "zero moves",
            GameParams {
                moves: 0,
                ..solved.clone()
            },
            0,
        ),
    ] {
        service.complete_game_task(params, Difficulty::Easy, today, moves);
        service.flush_game_saves().await.expect(label);

        let client = test_db.db.get().await.expect("db client");
        assert!(
            DailyWin::find(&client, user.id, Difficulty::Easy.key(), today)
                .await
                .expect("daily win lookup")
                .is_none(),
            "{label} recorded a daily win"
        );
        assert!(
            activity_rx.try_recv().is_err(),
            "{label} published activity"
        );
        assert!(
            service
                .load_games(user.id)
                .await
                .expect("load games")
                .is_empty(),
            "{label} persisted a board"
        );
    }
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
