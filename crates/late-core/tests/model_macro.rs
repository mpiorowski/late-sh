use late_core::test_utils::test_db;

late_core::model! {
    table = "macro_items";
    params = ItemParams;
    struct Item {
        @data
        pub name: String,
        pub note: String,
    }
}

#[tokio::test]
async fn model_macro_crud() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("failed to get connection");

    client
        .execute(
            "CREATE TEMP TABLE macro_items (\
                id uuid primary key default uuidv7(),\
                created timestamptz not null default current_timestamp,\
                updated timestamptz not null default current_timestamp,\
                name text not null,\
                note text not null\
            )",
            &[],
        )
        .await
        .expect("failed to create temp table");

    let created = Item::create(
        &client,
        ItemParams {
            name: "first".to_string(),
            note: "note-a".to_string(),
        },
    )
    .await
    .expect("create failed");
    assert_eq!(created.name, "first");

    let found = Item::get(&client, created.id).await.expect("get failed");
    assert!(found.is_some());

    let all = Item::all(&client).await.expect("all failed");
    assert_eq!(all.len(), 1);

    let updated = Item::update(
        &client,
        created.id,
        ItemParams {
            name: "second".to_string(),
            note: "note-b".to_string(),
        },
    )
    .await
    .expect("update failed");
    assert_eq!(updated.name, "second");
    assert_eq!(updated.note, "note-b");

    let deleted = Item::delete(&client, created.id)
        .await
        .expect("delete failed");
    assert_eq!(deleted, 1);

    let missing = Item::get(&client, created.id)
        .await
        .expect("get after delete failed");
    assert!(missing.is_none());
}

#[tokio::test]
async fn model_macro_all_orders_by_created_desc() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("failed to get connection");

    client
        .execute(
            "CREATE TEMP TABLE macro_items (\
                id uuid primary key default uuidv7(),\
                created timestamptz not null default current_timestamp,\
                updated timestamptz not null default current_timestamp,\
                name text not null,\
                note text not null\
            )",
            &[],
        )
        .await
        .expect("failed to create temp table");

    // Insert with explicit created timestamps to guarantee order
    client
        .execute(
            "INSERT INTO macro_items (name, note, created) VALUES \
                ('oldest', 'n', '2020-01-01T00:00:00Z'),\
                ('middle', 'n', '2022-01-01T00:00:00Z'),\
                ('newest', 'n', '2024-01-01T00:00:00Z')",
            &[],
        )
        .await
        .expect("failed to insert rows");

    let all = Item::all(&client).await.expect("all failed");
    assert_eq!(all.len(), 3);
    assert_eq!(all[0].name, "newest");
    assert_eq!(all[1].name, "middle");
    assert_eq!(all[2].name, "oldest");
}
