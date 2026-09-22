use late_core::models::app_flag::AppFlags;
use late_core::models::artboard_piece::{ArtboardPiece, HangOutcome, HangParams};
use late_core::test_utils::create_test_user;
use serde_json::json;
use uuid::Uuid;

use super::{GalleryService, SplashRefresh};
use crate::test_helpers::{new_test_db, test_app_flags_rx};

fn hang_params(user_id: Uuid, title: &str) -> HangParams {
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

/// The refresh publishes the day's piece into the watch and the login
/// claim hands it to an account once; an empty day, and the switch off,
/// publish nothing and every login is the cup.
#[tokio::test]
async fn the_refresh_publishes_todays_piece_and_the_claim_shows_it_once() {
    let test_db = new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let painter = create_test_user(&test_db.db, "splash-refresh-painter").await;
    let viewer = create_test_user(&test_db.db, "splash-refresh-viewer").await;
    let today = chrono::Utc::now().date_naive();
    let tomorrow = today + chrono::Duration::days(1);

    let service = GalleryService::new(test_db.db.clone(), test_app_flags_rx());

    // Nothing hung before today: the watch is empty and the door is the cup.
    assert_eq!(
        service.refresh_splash(today).await.expect("refresh"),
        SplashRefresh::default()
    );
    assert_eq!(service.splash_wall(), None);
    assert_eq!(service.claim_splash_piece(viewer.id).await, None);

    // A piece hung today is tomorrow's; the refresh for tomorrow assigns
    // it and publishes it.
    let hung = match ArtboardPiece::hang(&client, hang_params(painter.id, "dawn"))
        .await
        .expect("hang")
    {
        HangOutcome::Hung(piece) => piece,
        other => panic!("expected the piece to hang, got {other:?}"),
    };
    let refreshed = service.refresh_splash(tomorrow).await.expect("refresh");
    let published = refreshed.piece.expect("tomorrow's piece");
    assert_eq!(published.piece.id, hung.id);
    assert_eq!(published.piece.title, "dawn");
    assert_eq!(published.shown_on, tomorrow);
    assert_eq!(refreshed.queued, 0);
    assert_eq!(service.splash_wall(), Some(published.clone()));

    // The first login of the day gets it, the next is the cup.
    assert_eq!(
        service.claim_splash_piece(viewer.id).await,
        Some(published.clone())
    );
    assert_eq!(service.claim_splash_piece(viewer.id).await, None);
    // Another account has its own view.
    assert_eq!(
        service.claim_splash_piece(painter.id).await,
        Some(published)
    );

    // The switch off empties the watch on the next refresh, and a login
    // that never saw the piece gets the cup.
    let (_flags_tx, flags_rx) = tokio::sync::watch::channel(Some(AppFlags {
        haunt_enabled: true,
        haunt_live: false,
        paper_enabled: true,
        paper_outside_enabled: false,
        artboard_gallery_enabled: false,
        jobs_enabled: false,
    }));
    let off = GalleryService::new(test_db.db.clone(), flags_rx);
    assert_eq!(
        off.refresh_splash(tomorrow).await.expect("refresh"),
        SplashRefresh::default()
    );
    assert_eq!(off.splash_wall(), None);
    let bystander = create_test_user(&test_db.db, "splash-refresh-bystander").await;
    assert_eq!(off.claim_splash_piece(bystander.id).await, None);
}
