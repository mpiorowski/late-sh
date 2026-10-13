use late_core::models::artboard_piece::{ArtboardPiece, HangOutcome, HangParams};
use late_core::test_utils::create_test_user;
use serde_json::json;
use uuid::Uuid;

use super::{GalleryService, SplashRefresh};
use crate::test_helpers::new_test_db;

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

/// The piece greets only users who turned the Artboard content disclaimer
/// off (Always View); everyone else, newcomers first, gets the cup. A piece
/// taken down leaves new logins at once, before the hourly refresh.
#[tokio::test]
async fn splash_shows_only_without_the_disclaimer_and_only_while_the_piece_is_up() {
    let test_db = new_test_db().await;
    let service = GalleryService::new(test_db.db.clone());
    let owner = create_test_user(&test_db.db, "splash-owner").await;
    let piece = today_piece(&test_db.db, &service, owner.id).await;

    assert!(service.splash_piece_for_login(true).await.is_none());
    assert_eq!(
        service
            .splash_piece_for_login(false)
            .await
            .expect("the day's piece")
            .piece
            .id,
        piece
    );

    test_db
        .db
        .get()
        .await
        .unwrap()
        .execute(
            "UPDATE artboard_pieces SET removed_at = CURRENT_TIMESTAMP WHERE id = $1",
            &[&piece],
        )
        .await
        .unwrap();
    assert!(
        service.splash_piece().is_some(),
        "the watch still holds the piece until the hourly refresh"
    );
    assert!(service.splash_piece_for_login(false).await.is_none());
    // A day with nothing queued is the cup, never the previous day's piece.
    assert!(
        service
            .splash_piece_for_day(
                false,
                chrono::Utc::now().date_naive() + chrono::Duration::days(1)
            )
            .await
            .is_none()
    );
}

#[tokio::test]
async fn failed_splash_removal_check_falls_back_to_the_cup() {
    let test_db = new_test_db().await;
    let owner = create_test_user(&test_db.db, "splash-failure-owner").await;
    let service = GalleryService::new(test_db.db.clone());
    today_piece(&test_db.db, &service, owner.id).await;
    test_db
        .db
        .get()
        .await
        .unwrap()
        .batch_execute("ALTER TABLE artboard_pieces RENAME COLUMN splash_on TO splash_day")
        .await
        .unwrap();
    assert!(service.splash_piece_for_login(false).await.is_none());
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
        .splash_piece_for_day(false, today)
        .await
        .expect("the new day's piece, not the cup");
    assert_eq!((splash.piece.id, splash.shown_on), (pieces[1], today));
}
