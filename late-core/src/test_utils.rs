use crate::db::{Db, DbConfig};
use crate::models::user::{User, UserParams};
use testcontainers::{
    ContainerAsync, GenericImage, ImageExt,
    core::{IntoContainerPort, WaitFor},
    runners::AsyncRunner,
};
use tokio::time::{Duration, sleep};
use tokio_postgres::NoTls;

pub struct TestDb {
    /// Holds the testcontainers container alive (local dev).
    /// `None` when using an external postgres (CI).
    _container: Option<ContainerAsync<GenericImage>>,
    pub db: Db,
}

pub async fn test_db() -> TestDb {
    if let Ok(url) = std::env::var("TEST_DATABASE_URL") {
        return test_db_external(&url).await;
    }
    test_db_container().await
}

/// CI path: connect to an already-running postgres, create a unique database.
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
        max_pool_size: 16,
    };

    let db = Db::new(&config).expect("create db");
    db.migrate().await.expect("migrate db");

    TestDb {
        _container: None,
        db,
    }
}

/// Local dev path: spin up a postgres container via testcontainers.
async fn test_db_container() -> TestDb {
    let container = GenericImage::new("postgres", "18-alpine")
        .with_exposed_port(5432.tcp())
        .with_wait_for(WaitFor::message_on_stderr(
            "database system is ready to accept connections",
        ))
        .with_env_var("POSTGRES_USER", "postgres")
        .with_env_var("POSTGRES_PASSWORD", "postgres")
        .with_env_var("POSTGRES_DB", "postgres")
        .start()
        .await
        .unwrap();

    let port = container
        .get_host_port_ipv4(5432.tcp())
        .await
        .expect("failed to map postgres port");
    let config = DbConfig {
        host: "127.0.0.1".to_string(),
        port,
        user: "postgres".to_string(),
        password: "postgres".to_string(),
        dbname: "postgres".to_string(),
        max_pool_size: 16,
    };

    wait_for_db(&config).await;

    let db = Db::new(&config).expect("create db");
    db.migrate().await.expect("migrate db");

    TestDb {
        _container: Some(container),
        db,
    }
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

async fn wait_for_db(config: &DbConfig) {
    let conn_str = format!(
        "host={} port={} user={} password={} dbname={}",
        config.host, config.port, config.user, config.password, config.dbname
    );
    for _ in 0..50 {
        match tokio_postgres::connect(&conn_str, NoTls).await {
            Ok((client, connection)) => {
                tokio::spawn(connection);
                if client.simple_query("SELECT 1").await.is_ok() {
                    return;
                }
            }
            Err(e) => tracing::debug!(%e, "postgres not ready"),
        }
        sleep(Duration::from_millis(100)).await;
    }
    panic!("postgres did not become ready in time");
}
