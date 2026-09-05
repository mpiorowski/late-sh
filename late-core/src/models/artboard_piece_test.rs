use chrono::Utc;
use serde_json::json;
use uuid::Uuid;

use crate::models::artboard_piece::{
    ApplauseOutcome, ArtboardPiece, HangOutcome, HangParams, ListingCounts, PIECE_DAILY_CAP,
    PieceListing, PieceLookup, PodiumPiece, TakeDownOutcome,
};
use crate::models::profile_award::snapshot_previous_month_profile_awards;
use crate::test_utils::{create_test_user, roll_artboard_pieces_back_a_month, test_db};

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
    assert_eq!(
        ArtboardPiece::lookup_by_id_prefix(&client, &id[..12])
            .await
            .expect("lookup"),
        PieceLookup::NotFound,
        "a piece that is down is not a mod's target twice"
    );
    assert!(
        ArtboardPiece::remove(&client, piece.id)
            .await
            .expect("remove again")
            .is_none()
    );
    // Soft: the row is still there, marked, for the cap and the rail.
    let row = client
        .query_one(
            "SELECT removed_at IS NOT NULL AS down FROM artboard_pieces WHERE id = $1",
            &[&piece.id],
        )
        .await
        .expect("row");
    assert!(row.get::<_, bool>("down"));
}

#[tokio::test]
async fn a_hanger_takes_down_only_their_own_piece_and_only_this_month() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let painter = create_test_user(&test_db.db, "takedown-painter").await;
    let other = create_test_user(&test_db.db, "takedown-other").await;
    let fan = create_test_user(&test_db.db, "takedown-fan").await;

    let first = hang(&client, hang_params(painter.id, "first", "hash-td-1")).await;
    let second = hang(&client, hang_params(painter.id, "second", "hash-td-2")).await;
    hang(&client, hang_params(painter.id, "third", "hash-td-3")).await;

    // Owner scope is in the UPDATE: somebody else's `x` changes nothing.
    assert_eq!(
        ArtboardPiece::take_down(&client, first.id, other.id)
            .await
            .expect("take down"),
        TakeDownOutcome::NotYours
    );
    assert_eq!(
        ArtboardPiece::take_down(&client, first.id, painter.id)
            .await
            .expect("take down"),
        TakeDownOutcome::TakenDown
    );
    assert!(
        ArtboardPiece::find(&client, painter.id, first.id)
            .await
            .expect("find")
            .is_none()
    );
    assert_eq!(
        ArtboardPiece::take_down(&client, first.id, painter.id)
            .await
            .expect("take down again"),
        TakeDownOutcome::NotFound
    );
    assert_eq!(
        ArtboardPiece::toggle_applause(&client, first.id, fan.id)
            .await
            .expect("applaud"),
        ApplauseOutcome::NotFound
    );

    // The row stays: the day's cap still counts it, and the same cells
    // cannot come back this month.
    assert_eq!(
        ArtboardPiece::hang(&client, hang_params(painter.id, "fourth", "hash-td-4"))
            .await
            .expect("hang"),
        HangOutcome::DailyCapReached,
        "hang-and-regret does not buy a fourth piece"
    );
    assert_eq!(
        ArtboardPiece::hang(&client, hang_params(other.id, "copy", "hash-td-1"))
            .await
            .expect("hang"),
        HangOutcome::Duplicate,
        "the duplicate rail sees the taken-down row"
    );
    assert_eq!(
        ArtboardPiece::listing_counts(&client, painter.id)
            .await
            .expect("counts"),
        ListingCounts {
            this_month: 2,
            newest: 2,
            hall_of_fame: 0,
            mine: 2,
        }
    );

    // Once the month rolls, the wall is settled: no applause, no
    // withdrawal, no take-down.
    assert_eq!(
        ArtboardPiece::toggle_applause(&client, second.id, fan.id)
            .await
            .expect("applaud"),
        ApplauseOutcome::Applauded(1)
    );
    roll_artboard_pieces_back_a_month(&client).await;
    assert_eq!(
        ArtboardPiece::toggle_applause(&client, second.id, fan.id)
            .await
            .expect("applaud"),
        ApplauseOutcome::Closed,
        "a withdrawal on a settled month is refused too"
    );
    assert_eq!(
        ArtboardPiece::toggle_applause(&client, second.id, other.id)
            .await
            .expect("applaud"),
        ApplauseOutcome::Closed
    );
    assert_eq!(
        ArtboardPiece::applause_count(&client, second.id)
            .await
            .expect("count"),
        1
    );
    assert_eq!(
        ArtboardPiece::take_down(&client, second.id, painter.id)
            .await
            .expect("take down"),
        TakeDownOutcome::Closed
    );
    // A mod still can.
    assert!(
        ArtboardPiece::remove(&client, second.id)
            .await
            .expect("remove")
            .is_some()
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

#[tokio::test]
async fn the_podium_and_the_wall_read_best_first_and_break_ties_by_hang() {
    let test_db = test_db().await;
    let mut client = test_db.db.get().await.expect("db client");
    let painter = create_test_user(&test_db.db, "podium-painter").await;
    // The daily cap is three, so the fourth piece is somebody else's.
    let other = create_test_user(&test_db.db, "podium-other").await;
    let mut fans = Vec::new();
    for index in 0..4 {
        fans.push(create_test_user(&test_db.db, &format!("podium-fan-{index}")).await);
    }

    // Four pieces the same day: `early` and `late` tie at three, `best`
    // has four, `quiet` has none. `late` is the other hanger's.
    let early = hang(&client, hang_params(painter.id, "early", "hash-early")).await;
    let best = hang(&client, hang_params(painter.id, "best", "hash-best")).await;
    let late = hang(&client, hang_params(other.id, "late", "hash-late")).await;
    let quiet = hang(&client, hang_params(other.id, "quiet", "hash-quiet")).await;
    for (piece, hands) in [(&early, 3), (&best, 4), (&late, 3)] {
        for fan in &fans[..hands] {
            ArtboardPiece::toggle_applause(&client, piece.id, fan.id)
                .await
                .expect("applaud");
        }
    }

    // The paper's wall: best first, the tie to the earlier hang, and any
    // count qualifies, so `quiet` would be fourth.
    let today = Utc::now().date_naive();
    let wall = ArtboardPiece::most_applauded_hung_on(&client, today, 3)
        .await
        .expect("wall");
    assert_eq!(
        wall.iter().map(|piece| piece.id).collect::<Vec<_>>(),
        vec![best.id, early.id, late.id]
    );
    let whole_day = ArtboardPiece::most_applauded_hung_on(&client, today, 10)
        .await
        .expect("wall");
    assert_eq!(whole_day.last().map(|piece| piece.id), Some(quiet.id));
    let yesterday = today.pred_opt().unwrap();
    assert!(
        ArtboardPiece::most_applauded_hung_on(&client, yesterday, 3)
            .await
            .expect("wall")
            .is_empty(),
        "a day that hung nothing is an empty wall"
    );

    // Last month's podium is the award's: nothing until the month rolls
    // and the snapshot mints it, then one place per hanger in the award's
    // order (`early` is the painter's second best, so it is not on it),
    // and the floor keeps `quiet` off it.
    roll_artboard_pieces_back_a_month(&client).await;
    assert!(
        ArtboardPiece::previous_month_podium(&client)
            .await
            .expect("podium")
            .is_empty(),
        "no podium before the award pass"
    );
    snapshot_previous_month_profile_awards(&mut client)
        .await
        .expect("snapshot");
    let places = |podium: Vec<PodiumPiece>| {
        podium
            .into_iter()
            .map(|entry| (entry.place, entry.piece.id))
            .collect::<Vec<_>>()
    };
    let podium = ArtboardPiece::previous_month_podium(&client)
        .await
        .expect("podium");
    assert_eq!(places(podium), vec![(1, best.id), (2, late.id)]);

    // A mod removal after the month settled keeps the places: the winner's
    // next piece hangs for `ART1`, and once nothing of theirs is left the
    // place is a gap, not a promotion.
    ArtboardPiece::remove(&client, best.id)
        .await
        .expect("remove");
    let podium = ArtboardPiece::previous_month_podium(&client)
        .await
        .expect("podium");
    assert_eq!(places(podium), vec![(1, early.id), (2, late.id)]);
    ArtboardPiece::remove(&client, early.id)
        .await
        .expect("remove");
    let podium = ArtboardPiece::previous_month_podium(&client)
        .await
        .expect("podium");
    assert_eq!(places(podium), vec![(2, late.id)]);
}
