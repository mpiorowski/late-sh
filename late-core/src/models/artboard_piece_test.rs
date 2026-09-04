use serde_json::json;
use uuid::Uuid;

use crate::models::artboard_piece::{
    ApplauseOutcome, ArtboardPiece, HangOutcome, HangParams, ListingCounts, PIECE_DAILY_CAP,
    PieceListing, PieceLookup,
};
use crate::test_utils::{create_test_user, test_db};

pub(crate) fn hang_params(user_id: Uuid, title: &str, content_hash: &str) -> HangParams {
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
        content_hash: content_hash.to_string(),
    }
}

async fn hang(client: &impl deadpool_postgres::GenericClient, params: HangParams) -> ArtboardPiece {
    match ArtboardPiece::hang(client, params).await.expect("hang") {
        HangOutcome::Hung(piece) => piece,
        other => panic!("expected the piece to hang, got {other:?}"),
    }
}

#[tokio::test]
async fn applause_is_one_per_person_revocable_and_never_for_your_own_piece() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let painter = create_test_user(&test_db.db, "gallery-painter").await;
    let fan = create_test_user(&test_db.db, "gallery-fan").await;

    let piece = hang(&client, hang_params(painter.id, "sunset", "hash-sunset")).await;
    assert_eq!(piece.applause, 0);
    assert_eq!(piece.username, painter.username);

    assert_eq!(
        ArtboardPiece::toggle_applause(&client, piece.id, fan.id)
            .await
            .expect("applaud"),
        ApplauseOutcome::Applauded(1)
    );
    // A second clap from the same hands is a no-op on the count.
    let listed = ArtboardPiece::list(&client, fan.id, PieceListing::ThisMonth)
        .await
        .expect("list");
    let shown = listed
        .iter()
        .find(|item| item.id == piece.id)
        .expect("the piece is on this month's wall");
    assert_eq!(shown.applause, 1);
    assert!(shown.applauded_by_viewer);

    assert_eq!(
        ArtboardPiece::toggle_applause(&client, piece.id, fan.id)
            .await
            .expect("withdraw"),
        ApplauseOutcome::Withdrawn(0)
    );
    assert_eq!(
        ArtboardPiece::toggle_applause(&client, piece.id, painter.id)
            .await
            .expect("own piece"),
        ApplauseOutcome::OwnPiece
    );
    assert_eq!(
        ArtboardPiece::toggle_applause(&client, Uuid::now_v7(), fan.id)
            .await
            .expect("missing piece"),
        ApplauseOutcome::NotFound
    );

    let mine = ArtboardPiece::list(&client, painter.id, PieceListing::Mine)
        .await
        .expect("mine");
    assert_eq!(
        mine.iter().map(|item| item.id).collect::<Vec<_>>(),
        vec![piece.id]
    );
    let counts = ArtboardPiece::counts_for_user(&client, painter.id)
        .await
        .expect("counts");
    assert_eq!((counts.pieces, counts.applause), (1, 0));
}

#[tokio::test]
async fn the_daily_cap_and_the_duplicate_rail_refuse_in_sql() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let painter = create_test_user(&test_db.db, "gallery-capped").await;

    for n in 0..PIECE_DAILY_CAP {
        hang(
            &client,
            hang_params(painter.id, &format!("piece {n}"), &format!("hash-{n}")),
        )
        .await;
    }
    assert_eq!(
        ArtboardPiece::hang(&client, hang_params(painter.id, "one more", "hash-more"))
            .await
            .expect("hang"),
        HangOutcome::DailyCapReached
    );

    let other = create_test_user(&test_db.db, "gallery-copycat").await;
    assert_eq!(
        ArtboardPiece::hang(&client, hang_params(other.id, "same cells", "hash-0"))
            .await
            .expect("hang"),
        HangOutcome::Duplicate
    );
}

#[tokio::test]
async fn a_mod_takes_a_piece_down_by_id_prefix() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let painter = create_test_user(&test_db.db, "gallery-removed").await;
    let piece = hang(&client, hang_params(painter.id, "gone soon", "hash-gone")).await;

    let id = piece.id.to_string();
    assert_eq!(
        ArtboardPiece::lookup_by_id_prefix(&client, &id[..4])
            .await
            .expect("lookup"),
        PieceLookup::NotFound,
        "a prefix under the minimum is never looked up"
    );
    assert_eq!(
        ArtboardPiece::lookup_by_id_prefix(&client, &id[..12])
            .await
            .expect("lookup"),
        PieceLookup::One(piece.id)
    );

    let removed = ArtboardPiece::remove(&client, piece.id)
        .await
        .expect("remove")
        .expect("the piece was there");
    assert_eq!(removed.title, "gone soon");
    assert!(
        ArtboardPiece::find(&client, painter.id, piece.id)
            .await
            .expect("find")
            .is_none()
    );
    assert!(
        ArtboardPiece::remove(&client, piece.id)
            .await
            .expect("remove again")
            .is_none()
    );
}

#[tokio::test]
async fn listing_counts_answer_without_listing() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let painter = create_test_user(&test_db.db, "gallery-counter").await;
    let other = create_test_user(&test_db.db, "gallery-counted").await;

    assert_eq!(
        ArtboardPiece::listing_counts(&client, painter.id)
            .await
            .expect("counts"),
        ListingCounts::default()
    );

    hang(&client, hang_params(painter.id, "one", "hash-count-1")).await;
    hang(&client, hang_params(painter.id, "two", "hash-count-2")).await;
    hang(&client, hang_params(other.id, "three", "hash-count-3")).await;

    assert_eq!(
        ArtboardPiece::listing_counts(&client, painter.id)
            .await
            .expect("counts"),
        ListingCounts {
            this_month: 3,
            newest: 3,
            hall_of_fame: 0,
            mine: 2,
        }
    );
}
