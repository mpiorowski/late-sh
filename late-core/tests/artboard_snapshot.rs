use late_core::models::artboard::Snapshot;
use late_core::test_utils::test_db;

#[tokio::test]
async fn artboard_snapshot_upsert_replaces_existing_canvas() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("failed to get connection");

    let first_canvas = serde_json::json!({
        "width": 384,
        "height": 192,
        "cells": [],
        "colors": [],
    });
    let first_provenance = serde_json::json!({
        "cells": []
    });
    let second_canvas = serde_json::json!({
        "width": 384,
        "height": 192,
        "cells": [[{"x": 3, "y": 2}, {"Narrow": "A"}]],
        "colors": [],
    });
    let second_provenance = serde_json::json!({
        "cells": [[{"x": 3, "y": 2}, "mat"]]
    });

    Snapshot::upsert(
        &client,
        Snapshot::MAIN_BOARD_KEY,
        first_canvas,
        first_provenance,
    )
    .await
    .expect("insert snapshot");
    let updated = Snapshot::upsert(
        &client,
        Snapshot::MAIN_BOARD_KEY,
        second_canvas.clone(),
        second_provenance.clone(),
    )
    .await
    .expect("update snapshot");

    assert_eq!(updated.canvas, second_canvas);
    assert_eq!(updated.provenance, second_provenance);

    let reloaded = Snapshot::find_by_board_key(&client, Snapshot::MAIN_BOARD_KEY)
        .await
        .expect("reload snapshot")
        .expect("snapshot exists");
    assert_eq!(reloaded.canvas, second_canvas);
    assert_eq!(reloaded.provenance, second_provenance);

    let count = client
        .query_one(
            "SELECT COUNT(*)::int AS count FROM artboard_snapshots WHERE board_key = $1",
            &[&Snapshot::MAIN_BOARD_KEY],
        )
        .await
        .expect("count snapshots")
        .get::<_, i32>("count");
    assert_eq!(count, 1);
}

#[tokio::test]
async fn artboard_snapshot_prefix_listing_and_delete_by_board_key_work() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("failed to get connection");

    let canvas = serde_json::json!({
        "width": 384,
        "height": 192,
        "cells": [],
        "colors": [],
    });
    let provenance = serde_json::json!({
        "cells": []
    });

    Snapshot::upsert(
        &client,
        "daily:2026-04-21",
        canvas.clone(),
        provenance.clone(),
    )
    .await
    .expect("insert first daily snapshot");
    Snapshot::upsert(
        &client,
        "daily:2026-04-22",
        canvas.clone(),
        provenance.clone(),
    )
    .await
    .expect("insert second daily snapshot");
    Snapshot::upsert(&client, "monthly:2026-04", canvas, provenance)
        .await
        .expect("insert monthly snapshot");

    let daily = Snapshot::list_by_board_key_prefix(&client, "daily:")
        .await
        .expect("list daily snapshots");
    let keys: Vec<_> = daily
        .iter()
        .map(|snapshot| snapshot.board_key.as_str())
        .collect();
    assert_eq!(keys, vec!["daily:2026-04-22", "daily:2026-04-21"]);

    let deleted = Snapshot::delete_by_board_key(&client, "daily:2026-04-21")
        .await
        .expect("delete one daily snapshot");
    assert_eq!(deleted, 1);

    let daily = Snapshot::list_by_board_key_prefix(&client, "daily:")
        .await
        .expect("reload daily snapshots");
    let keys: Vec<_> = daily
        .iter()
        .map(|snapshot| snapshot.board_key.as_str())
        .collect();
    assert_eq!(keys, vec!["daily:2026-04-22"]);
}
