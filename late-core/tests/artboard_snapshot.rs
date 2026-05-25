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
    Snapshot::upsert(
        &client,
        "curated:2026-04-23",
        canvas.clone(),
        provenance.clone(),
    )
    .await
    .expect("insert curated snapshot");
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

    let archives = Snapshot::list_archive_summaries(&client, 10, 0)
        .await
        .expect("list archive summaries");
    let archive_keys: Vec<_> = archives
        .iter()
        .map(|snapshot| snapshot.board_key.as_str())
        .collect();
    assert_eq!(
        archive_keys,
        vec![
            "monthly:2026-04",
            "daily:2026-04-22",
            "daily:2026-04-21",
            "curated:2026-04-23",
        ]
    );

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

#[tokio::test]
async fn artboard_snapshot_insert_if_absent_preserves_existing_snapshot() {
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
        "cells": [[{"x": 1, "y": 1}, {"Narrow": "X"}]],
        "colors": [],
    });
    let second_provenance = serde_json::json!({
        "cells": [[{"x": 1, "y": 1}, "mat"]]
    });

    let inserted = Snapshot::insert_if_absent(
        &client,
        "curated:2026-05-25",
        first_canvas.clone(),
        first_provenance.clone(),
    )
    .await
    .expect("insert snapshot");
    assert!(inserted.is_some());

    let duplicate = Snapshot::insert_if_absent(
        &client,
        "curated:2026-05-25",
        second_canvas,
        second_provenance,
    )
    .await
    .expect("duplicate insert should not error");
    assert!(duplicate.is_none());

    let reloaded = Snapshot::find_by_board_key(&client, "curated:2026-05-25")
        .await
        .expect("reload snapshot")
        .expect("snapshot exists");
    assert_eq!(reloaded.canvas, first_canvas);
    assert_eq!(reloaded.provenance, first_provenance);
}

#[tokio::test]
async fn artboard_snapshot_copy_board_key_if_absent_preserves_existing_target() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("failed to get connection");

    let source_canvas = serde_json::json!({
        "width": 384,
        "height": 192,
        "cells": [[{"x": 1, "y": 1}, {"Narrow": "S"}]],
        "colors": [],
    });
    let source_provenance = serde_json::json!({
        "cells": [[{"x": 1, "y": 1}, "source"]]
    });
    let target_canvas = serde_json::json!({
        "width": 384,
        "height": 192,
        "cells": [[{"x": 2, "y": 2}, {"Narrow": "T"}]],
        "colors": [],
    });
    let target_provenance = serde_json::json!({
        "cells": [[{"x": 2, "y": 2}, "target"]]
    });

    Snapshot::upsert(
        &client,
        "daily:2026-05-25",
        source_canvas.clone(),
        source_provenance.clone(),
    )
    .await
    .expect("insert source");
    let copied =
        Snapshot::copy_board_key_if_absent(&client, "daily:2026-05-25", "curated:2026-05-25")
            .await
            .expect("copy source");
    assert!(copied.is_some());

    Snapshot::upsert(
        &client,
        "curated:2026-05-26",
        target_canvas.clone(),
        target_provenance.clone(),
    )
    .await
    .expect("insert target");
    let duplicate =
        Snapshot::copy_board_key_if_absent(&client, "daily:2026-05-25", "curated:2026-05-26")
            .await
            .expect("copy should not overwrite target");
    assert!(duplicate.is_none());

    let reloaded = Snapshot::find_by_board_key(&client, "curated:2026-05-26")
        .await
        .expect("reload target")
        .expect("target exists");
    assert_eq!(reloaded.canvas, target_canvas);
    assert_eq!(reloaded.provenance, target_provenance);
}
