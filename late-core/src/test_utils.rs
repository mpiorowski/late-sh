use crate::db::{Db, DbConfig};
use crate::models::user::{User, UserParams};
use tokio_postgres::NoTls;

const TEST_DB_POOL_SIZE: usize = 8;

pub struct TestDb {
    pub db: Db,
}

pub async fn test_db() -> TestDb {
    let url = std::env::var("TEST_DATABASE_URL").expect(
        "TEST_DATABASE_URL must be set for DB integration tests. \
         Run `make check`, or start Postgres and export a URL like \
         `host=127.0.0.1 port=5433 user=postgres password=postgres dbname=postgres`.",
    );
    test_db_external(&url).await
}

/// Connect to an already-running postgres and create a unique database.
async fn test_db_external(url: &str) -> TestDb {
    let admin_config: tokio_postgres::Config = url.parse().expect("parse TEST_DATABASE_URL");

    let host = admin_config
        .get_hosts()
        .first()
        .map(|h| match h {
            tokio_postgres::config::Host::Tcp(s) => s.clone(),
            _ => "127.0.0.1".to_string(),
        })
        .unwrap_or_else(|| "127.0.0.1".to_string());
    let port = admin_config.get_ports().first().copied().unwrap_or(5432);
    let user = admin_config.get_user().unwrap_or("postgres").to_string();
    let password = admin_config
        .get_password()
        .map(|p| String::from_utf8_lossy(p).to_string())
        .unwrap_or_else(|| "postgres".to_string());

    // Each test gets its own database to avoid conflicts.
    let db_name = format!("test_{}", uuid::Uuid::now_v7().to_string().replace('-', ""));

    // Connect to the default database to create our test database.
    let admin_conn_str =
        format!("host={host} port={port} user={user} password={password} dbname=postgres");
    let (client, conn) = tokio_postgres::connect(&admin_conn_str, NoTls)
        .await
        .expect("connect to admin postgres");
    tokio::spawn(conn);
    client
        .batch_execute(&format!("CREATE DATABASE \"{db_name}\""))
        .await
        .expect("create test database");
    drop(client);

    let config = DbConfig {
        host,
        port,
        user,
        password,
        dbname: db_name,
        max_pool_size: TEST_DB_POOL_SIZE,
    };

    let db = Db::new(&config).expect("create db");
    db.migrate().await.expect("migrate db");

    TestDb { db }
}

/// Create a user for integration tests. Returns the `User`.
pub async fn create_test_user(db: &Db, username: &str) -> User {
    let client = db.get().await.expect("db client");
    let username = User::next_available_username(&client, username)
        .await
        .expect("next available username");
    User::create(
        &client,
        UserParams {
            fingerprint: format!("fp-{}", uuid::Uuid::now_v7()),
            username,
            settings: serde_json::json!({}),
        },
    )
    .await
    .expect("create user")
}

/// Test-only clock control: push matching rows' `created` one second past
/// `now()`, so `created > <cursor taken now>` comparisons are decisive
/// instead of racing the clock's microsecond resolution. Tests must not
/// hand-roll this UPDATE; this helper is the one place that fudges `created`.
pub async fn bump_created_past_now(
    client: &tokio_postgres::Client,
    table: &str,
    filter_sql: &str,
    params: &[&(dyn tokio_postgres::types::ToSql + Sync)],
) {
    let updated = client
        .execute(
            &format!(
                "UPDATE {table} SET created = now() + interval '1 second' WHERE {filter_sql}"
            ),
            params,
        )
        .await
        .expect("bump created past now");
    assert!(updated > 0, "bump_created_past_now matched no rows");
}
