use late_core::test_utils::test_db;

#[tokio::test]
async fn health_check() {
    let test_db = test_db().await;
    test_db.db.health().await.expect("health check failed");
}

#[tokio::test]
async fn simple_query() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("failed to get connection");

    let rows = client
        .query("SELECT 1 + 1 AS result", &[])
        .await
        .expect("query failed");

    assert_eq!(rows.len(), 1);
    let result: i32 = rows[0].get("result");
    assert_eq!(result, 2);
}

#[tokio::test]
async fn parameterized_query() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("failed to get connection");

    let rows = client
        .query(
            "SELECT $1::TEXT AS name, $2::INT AS num",
            &[&"alice", &42i32],
        )
        .await
        .expect("query failed");

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get::<_, &str>("name"), "alice");
    assert_eq!(rows[0].get::<_, i32>("num"), 42);
}

#[tokio::test]
async fn pool_reuse() {
    let test_db = test_db().await;
    let db = test_db.db;

    for i in 0..5 {
        let client = db.get().await.expect("failed to get connection");
        let rows = client
            .query("SELECT $1::INT AS n", &[&i])
            .await
            .expect("query failed");
        assert_eq!(rows[0].get::<_, i32>("n"), i);
    }

    let status = db.status();
    assert!(status.size >= 1);
}

#[tokio::test]
async fn concurrent_connections() {
    let test_db = test_db().await;
    let db = test_db.db;

    let handles: Vec<_> = (0..10)
        .map(|i| {
            let db = db.clone();
            tokio::spawn(async move {
                let client = db.get().await.unwrap();
                let rows = client.query("SELECT $1::INT AS n", &[&i]).await.unwrap();
                assert_eq!(rows[0].get::<_, i32>("n"), i);
            })
        })
        .collect();

    for h in handles {
        h.await.expect("task panicked");
    }
}

#[tokio::test]
async fn transaction_commit() {
    let test_db = test_db().await;
    let mut client = test_db.db.get().await.expect("failed to get connection");

    client
        .execute("CREATE TEMP TABLE tx_test (v TEXT)", &[])
        .await
        .unwrap();

    let tx = client.transaction().await.unwrap();
    tx.execute("INSERT INTO tx_test (v) VALUES ($1)", &[&"committed"])
        .await
        .unwrap();
    tx.commit().await.unwrap();

    let rows = client.query("SELECT v FROM tx_test", &[]).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get::<_, &str>("v"), "committed");
}

#[tokio::test]
async fn transaction_rollback() {
    let test_db = test_db().await;
    let mut client = test_db.db.get().await.expect("failed to get connection");

    client
        .execute("CREATE TEMP TABLE rb_test (v TEXT)", &[])
        .await
        .unwrap();

    client
        .execute("INSERT INTO rb_test (v) VALUES ($1)", &[&"kept"])
        .await
        .unwrap();

    let tx = client.transaction().await.unwrap();
    tx.execute("INSERT INTO rb_test (v) VALUES ($1)", &[&"rolled_back"])
        .await
        .unwrap();
    tx.rollback().await.unwrap();

    let rows = client.query("SELECT v FROM rb_test", &[]).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get::<_, &str>("v"), "kept");
}

#[tokio::test]
async fn null_handling() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.unwrap();

    let rows = client
        .query("SELECT NULL::TEXT AS nullable", &[])
        .await
        .unwrap();

    let value: Option<&str> = rows[0].get("nullable");
    assert!(value.is_none());
}

#[tokio::test]
async fn query_opt() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.unwrap();

    let some = client.query_opt("SELECT 1", &[]).await.unwrap();
    assert!(some.is_some());

    let none = client.query_opt("SELECT 1 WHERE false", &[]).await.unwrap();
    assert!(none.is_none());
}

#[tokio::test]
async fn execute_row_count() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.unwrap();

    client
        .execute("CREATE TEMP TABLE cnt_test (n INT)", &[])
        .await
        .unwrap();

    let inserted = client
        .execute("INSERT INTO cnt_test SELECT generate_series(1,5)", &[])
        .await
        .unwrap();
    assert_eq!(inserted, 5);

    let deleted = client
        .execute("DELETE FROM cnt_test WHERE n > 3", &[])
        .await
        .unwrap();
    assert_eq!(deleted, 2);
}

#[tokio::test]
async fn json_roundtrip() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.unwrap();

    let input = serde_json::json!({"key": "value", "num": 123});
    let rows = client
        .query("SELECT $1::JSONB AS data", &[&input])
        .await
        .unwrap();

    let output: serde_json::Value = rows[0].get("data");
    assert_eq!(output, input);
}

#[tokio::test]
async fn migrations_apply_and_track() {
    let test_db = test_db().await;
    let db = test_db.db;
    db.migrate().await.expect("migrations failed");

    let client = db.get().await.expect("failed to get connection");

    let table_exists = client
        .query_one(
            "SELECT to_regclass('public.users') IS NOT NULL AS exists",
            &[],
        )
        .await
        .expect("query failed")
        .get::<_, bool>("exists");
    assert!(table_exists);

    let applied = client
        .query_one("SELECT COUNT(*)::int AS count FROM _migrations", &[])
        .await
        .expect("query failed")
        .get::<_, i32>("count");
    assert!(applied >= 1);

    db.migrate().await.expect("migrations failed");

    let applied_after = client
        .query_one("SELECT COUNT(*)::int AS count FROM _migrations", &[])
        .await
        .expect("query failed")
        .get::<_, i32>("count");
    assert_eq!(applied_after, applied);
}
