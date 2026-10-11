use late_core::models::artboard_piece::{ArtboardPiece, HangOutcome, HangParams};
use late_core::test_utils::create_test_user;
use serde_json::json;
use uuid::Uuid;

use super::{
    ContentRatingAction, ContentRatingOutcome, GalleryResult, GalleryService, SplashRefresh,
};
use crate::test_helpers::new_test_db;
use late_core::models::artboard_piece_rating::{ArtContentRating, ArtboardPieceRating};
use late_core::models::user::ArtSplashMode;

pub(crate) fn hang_params(user_id: Uuid, title: &str) -> HangParams {
    HangParams {
        user_id,
        title: title.to_string(),
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
        content_hash: format!("hash-{title}"),
    }
}

async fn today_piece(db: &late_core::db::Db, service: &GalleryService, owner: Uuid) -> Uuid {
    let client = db.get().await.unwrap();
    let HangOutcome::Hung(piece) =
        ArtboardPiece::hang(&client, hang_params(owner, "today's splash"))
            .await
            .unwrap()
    else {
        panic!("hang");
    };
    client.execute("UPDATE artboard_pieces SET created = CURRENT_TIMESTAMP - INTERVAL '1 day' WHERE id = $1", &[&piece.id]).await.unwrap();
    service
        .refresh_splash(chrono::Utc::now().date_naive())
        .await
        .unwrap();
    assert_eq!(service.splash_piece().unwrap().piece.id, piece.id);
    piece.id
}

#[tokio::test]
async fn splash_uses_fresh_ratings_removal_day_and_fuse_without_refreshing_canvas() {
    let test_db = new_test_db().await;
    let service = GalleryService::new(test_db.db.clone());
    let owner = create_test_user(&test_db.db, "splash-owner").await;
    let voters = [
        create_test_user(&test_db.db, "splash-voter-a").await,
        create_test_user(&test_db.db, "splash-voter-b").await,
    ];
    let piece = today_piece(&test_db.db, &service, owner.id).await;
    assert!(
        service
            .splash_piece_for_mode(ArtSplashMode::Sfw)
            .await
            .is_some()
    );
    assert!(
        service
            .splash_piece_for_mode(ArtSplashMode::Never)
            .await
            .is_none()
    );
    for voter in &voters {
        let mut client = test_db.db.get().await.unwrap();
        let tx = client.transaction().await.unwrap();
        ArtboardPieceRating::set_vote(&tx, piece, voter.id, Some(ArtContentRating::Nsfw))
            .await
            .unwrap();
        tx.commit().await.unwrap();
    }
    assert!(
        !service
            .splash_piece()
            .unwrap()
            .piece
            .content_rating
            .determination()
            .0
            .is_nsfw(),
        "canvas cache still holds old rating"
    );
    assert!(
        service
            .splash_piece_for_mode(ArtSplashMode::Sfw)
            .await
            .is_none()
    );
    assert!(
        service
            .splash_piece_for_mode(ArtSplashMode::Always)
            .await
            .unwrap()
            .piece
            .content_rating
            .determination()
            .0
            .is_nsfw()
    );
    let mut client = test_db.db.get().await.unwrap();
    let tx = client.transaction().await.unwrap();
    ArtboardPieceRating::set_vote(&tx, piece, voters[0].id, None)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert!(
        service
            .splash_piece_for_mode(ArtSplashMode::Sfw)
            .await
            .is_some()
    );
    client
        .execute(
            "UPDATE artboard_pieces SET removed_at = CURRENT_TIMESTAMP WHERE id = $1",
            &[&piece],
        )
        .await
        .unwrap();
    assert!(
        service
            .splash_piece_for_mode(ArtSplashMode::Always)
            .await
            .is_none()
    );
    // A day with nothing queued is the cup, never the previous day's piece.
    assert!(
        service
            .splash_piece_for_day(
                ArtSplashMode::Always,
                chrono::Utc::now().date_naive() + chrono::Duration::days(1)
            )
            .await
            .is_none()
    );
}

#[tokio::test]
async fn failed_fresh_splash_check_falls_back_to_the_cup() {
    let test_db = new_test_db().await;
    let owner = create_test_user(&test_db.db, "splash-failure-owner").await;
    let service = GalleryService::new(test_db.db.clone());
    today_piece(&test_db.db, &service, owner.id).await;
    test_db
        .db
        .get()
        .await
        .unwrap()
        .batch_execute("DROP VIEW artboard_piece_content_ratings")
        .await
        .unwrap();
    for mode in [
        ArtSplashMode::Sfw,
        ArtSplashMode::Always,
        ArtSplashMode::Never,
    ] {
        assert!(service.splash_piece_for_mode(mode).await.is_none());
    }
}

#[tokio::test]
async fn rating_task_refuses_self_votes_foreign_flags_and_a_dbless_gallery_without_changing_it() {
    let test_db = new_test_db().await;
    let owner = create_test_user(&test_db.db, "rating-owner").await;
    let viewer = create_test_user(&test_db.db, "rating-viewer").await;
    let service = GalleryService::new(test_db.db.clone());
    let piece = today_piece(&test_db.db, &service, owner.id).await;
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    service.content_rating_task(
        piece,
        owner.id,
        1,
        Some(ContentRatingAction::Vote(Some(ArtContentRating::Nsfw))),
        tx.clone(),
    );
    assert!(matches!(
        rx.recv().await.unwrap(),
        GalleryResult::ContentRating {
            outcome: ContentRatingOutcome::OwnPiece,
            ..
        }
    ));
    service.content_rating_task(
        piece,
        viewer.id,
        2,
        Some(ContentRatingAction::OwnerFlag(true)),
        tx.clone(),
    );
    assert!(matches!(
        rx.recv().await.unwrap(),
        GalleryResult::ContentRating {
            outcome: ContentRatingOutcome::NotYours,
            ..
        }
    ));
    service.content_rating_task(
        piece,
        owner.id,
        2,
        Some(ContentRatingAction::OwnerFlag(true)),
        tx.clone(),
    );
    assert!(matches!(
        rx.recv().await.unwrap(),
        GalleryResult::ContentRating {
            outcome: ContentRatingOutcome::Rated(summary),
            ..
        } if summary.owner_marked_nsfw
    ));
    let client = test_db.db.get().await.unwrap();
    GalleryService::disabled().content_rating_task(
        piece,
        viewer.id,
        3,
        Some(ContentRatingAction::Vote(Some(ArtContentRating::Nsfw))),
        tx,
    );
    assert!(matches!(
        rx.recv().await.unwrap(),
        GalleryResult::ContentRating {
            outcome: ContentRatingOutcome::Closed,
            ..
        }
    ));
    let summary = ArtboardPieceRating::read(&client, piece, viewer.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(summary.nsfw_votes, 0);
    assert!(summary.owner_marked_nsfw);
}

/// The refresh publishes the day's piece into the watch and every login
/// that day shows it; an empty day, and a service with no database,
/// publish nothing and every login is the cup.
#[tokio::test]
async fn the_refresh_publishes_todays_piece_for_every_login_that_day() {
    let test_db = new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let painter = create_test_user(&test_db.db, "splash-refresh-painter").await;
    let today = chrono::Utc::now().date_naive();
    let tomorrow = today + chrono::Duration::days(1);

    let service = GalleryService::new(test_db.db.clone());

    // Nothing hung before today: the watch is empty and the door is the cup.
    assert_eq!(
        service.refresh_splash(today).await.expect("refresh"),
        SplashRefresh::Wall {
            piece: None,
            queued: 0
        }
    );
    assert_eq!(service.splash_piece(), None);

    // A piece hung today is tomorrow's; the refresh for tomorrow assigns
    // it and publishes it.
    let hung = match ArtboardPiece::hang(&client, hang_params(painter.id, "dawn"))
        .await
        .expect("hang")
    {
        HangOutcome::Hung(piece) => piece,
        other => panic!("expected the piece to hang, got {other:?}"),
    };
    let published = match service.refresh_splash(tomorrow).await.expect("refresh") {
        SplashRefresh::Wall {
            piece: Some(piece),
            queued: 0,
        } => *piece,
        other => panic!("expected tomorrow's piece and an empty queue, got {other:?}"),
    };
    assert_eq!(published.piece.id, hung.id);
    assert_eq!(published.piece.title, "dawn");
    assert_eq!(published.shown_on, tomorrow);
    // Every login of the day reads it, not only the first.
    assert_eq!(service.splash_piece(), Some(published.clone()));
    assert_eq!(service.splash_piece(), Some(published));

    // No database: the refresh is off and every login is the cup.
    let off = GalleryService::disabled();
    assert_eq!(
        off.refresh_splash(tomorrow).await.expect("refresh"),
        SplashRefresh::Off
    );
    assert_eq!(off.splash_piece(), None);
}

#[tokio::test]
async fn first_login_after_midnight_shows_the_new_days_piece() {
    let test_db = new_test_db().await;
    let service = GalleryService::new(test_db.db.clone());
    let owner = create_test_user(&test_db.db, "midnight-owner").await;
    let client = test_db.db.get().await.unwrap();
    let mut pieces = Vec::new();
    for (title, days_ago) in [("yesterday's splash", 3_i32), ("today's splash", 2_i32)] {
        let HangOutcome::Hung(piece) = ArtboardPiece::hang(&client, hang_params(owner.id, title))
            .await
            .unwrap()
        else {
            panic!("hang");
        };
        client
            .execute(
                "UPDATE artboard_pieces
                 SET created = CURRENT_TIMESTAMP - make_interval(days => $2)
                 WHERE id = $1",
                &[&piece.id, &days_ago],
            )
            .await
            .unwrap();
        pieces.push(piece.id);
    }
    let today = chrono::Utc::now().date_naive();
    service
        .refresh_splash(today - chrono::Duration::days(1))
        .await
        .unwrap();
    assert_eq!(service.splash_piece().unwrap().piece.id, pieces[0]);

    // The hourly refresh has not run since midnight: the cache still holds
    // yesterday's piece when the first login of the day arrives.
    let splash = service
        .splash_piece_for_day(ArtSplashMode::Sfw, today)
        .await
        .expect("the new day's piece, not the cup");
    assert_eq!((splash.piece.id, splash.shown_on), (pieces[1], today));
}

#[tokio::test]
async fn a_failed_rating_request_shows_fixed_copy_not_the_database_error() {
    let test_db = new_test_db().await;
    let owner = create_test_user(&test_db.db, "rating-failure-owner").await;
    let viewer = create_test_user(&test_db.db, "rating-failure-viewer").await;
    let service = GalleryService::new(test_db.db.clone());
    let piece = today_piece(&test_db.db, &service, owner.id).await;
    test_db
        .db
        .get()
        .await
        .unwrap()
        .batch_execute("DROP VIEW artboard_piece_content_ratings")
        .await
        .unwrap();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    service.content_rating_task(piece, viewer.id, 1, None, tx);
    let GalleryResult::ContentRatingFailed { error, .. } = rx.recv().await.unwrap() else {
        panic!("a database failure is ContentRatingFailed");
    };
    assert_eq!(error, "The content rating did not go through. Try again.");
}
