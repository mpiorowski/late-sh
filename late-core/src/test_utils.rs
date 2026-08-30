use crate::db::{Db, DbConfig};
use crate::models::user::{User, UserParams};
use anyhow::{Context, Result, bail};
use std::time::{Duration, Instant};
use tokio_postgres::{Client, NoTls};

const TEST_DB_POOL_SIZE: usize = 8;

/// Set `LATE_TEST_DB_TEMPLATE=0` to force every test database through the plain
/// `CREATE DATABASE` + `migrate()` path. An escape hatch, not a tuning knob: the
/// templated path already falls back on its own if anything goes wrong.
const TEMPLATE_ENV: &str = "LATE_TEST_DB_TEMPLATE";

/// How long a process waits on whichever peer is building the template before
/// giving up and migrating its own database. Building it is one migration
/// replay — well under a second locally, a few seconds on a cold CI runner — so
/// this is pure headroom: it exists so that a wedged builder degrades the suite
/// to "slow" rather than "hung".
const TEMPLATE_WAIT: Duration = Duration::from_secs(120);
const TEMPLATE_POLL: Duration = Duration::from_millis(50);

/// Attempts at `CREATE DATABASE ... TEMPLATE` before falling back to migrating.
const TEMPLATE_CREATE_ATTEMPTS: u32 = 3;

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

/// The server test databases are provisioned on, minus the database name.
#[derive(Clone)]
struct Server {
    host: String,
    port: u16,
    user: String,
    password: String,
}

impl Server {
    fn parse(url: &str) -> Self {
        let config: tokio_postgres::Config = url.parse().expect("parse TEST_DATABASE_URL");

        Self {
            host: config
                .get_hosts()
                .first()
                .map(|h| match h {
                    tokio_postgres::config::Host::Tcp(s) => s.clone(),
                    _ => "127.0.0.1".to_string(),
                })
                .unwrap_or_else(|| "127.0.0.1".to_string()),
            port: config.get_ports().first().copied().unwrap_or(5432),
            user: config.get_user().unwrap_or("postgres").to_string(),
            password: config
                .get_password()
                .map(|p| String::from_utf8_lossy(p).to_string())
                .unwrap_or_else(|| "postgres".to_string()),
        }
    }

    fn db_config(&self, dbname: String, max_pool_size: usize) -> DbConfig {
        DbConfig {
            host: self.host.clone(),
            port: self.port,
            user: self.user.clone(),
            password: self.password.clone(),
            dbname,
            max_pool_size,
        }
    }

    /// A single unpooled connection, so callers own its exact lifetime — which
    /// is what makes the session advisory lock below safe.
    async fn connect(&self, dbname: &str) -> Result<Client> {
        let Self {
            host,
            port,
            user,
            password,
        } = self;
        let (client, conn) = tokio_postgres::connect(
            &format!("host={host} port={port} user={user} password={password} dbname={dbname}"),
            NoTls,
        )
        .await
        .with_context(|| format!("connect to database {dbname}"))?;
        tokio::spawn(conn);
        Ok(client)
    }
}

/// Connect to an already-running postgres and create a unique database.
async fn test_db_external(url: &str) -> TestDb {
    let server = Server::parse(url);

    // Each test gets its own database to avoid conflicts.
    let db_name = format!("test_{}", uuid::Uuid::now_v7().to_string().replace('-', ""));

    // Connect to the default database to create our test database.
    let admin = server
        .connect("postgres")
        .await
        .expect("connect to admin postgres");

    // Copying a pre-migrated template beats replaying every migration, and the
    // suite provisions hundreds of these. Any trouble falls back to the plain
    // path: a test must never fail merely because templating was unavailable.
    let templated = templating_enabled()
        && match provision_from_template(&admin, &server, &db_name).await {
            Ok(()) => true,
            Err(e) => {
                eprintln!("late-core test_db: templating unavailable, migrating instead ({e:#})");
                false
            }
        };

    if !templated {
        admin
            .batch_execute(&format!("CREATE DATABASE \"{db_name}\""))
            .await
            .expect("create test database");
    }
    drop(admin);

    let db = Db::new(&server.db_config(db_name, TEST_DB_POOL_SIZE)).expect("create db");
    if !templated {
        // A templated database is a copy of an already-migrated one, its
        // `_migrations` bookkeeping included, so there is nothing left to apply.
        db.migrate().await.expect("migrate db");
    }

    TestDb { db }
}

fn templating_enabled() -> bool {
    !matches!(
        std::env::var(TEMPLATE_ENV).as_deref(),
        Ok("0" | "false" | "off" | "no")
    )
}

/// Create `db_name` as a copy of the migrated template, building that template
/// first if this is the process that got there first.
async fn provision_from_template(admin: &Client, server: &Server, db_name: &str) -> Result<()> {
    let template = format!("late_tmpl_{}", crate::db::migrations_fingerprint());

    if !database_exists(admin, &template).await? {
        ensure_template(server, &template).await?;
    }

    let sql = format!("CREATE DATABASE \"{db_name}\" TEMPLATE \"{template}\"");
    let mut attempts = TEMPLATE_CREATE_ATTEMPTS;
    loop {
        match admin.batch_execute(&sql).await {
            Ok(()) => return Ok(()),
            Err(e) => {
                attempts -= 1;
                if attempts == 0 {
                    // A failed CREATE DATABASE rolls its own catalog row back,
                    // but drop defensively so the caller's fallback cannot trip
                    // over a half-created name.
                    let _ = admin
                        .batch_execute(&format!("DROP DATABASE IF EXISTS \"{db_name}\""))
                        .await;
                    return Err(anyhow::Error::new(e))
                        .with_context(|| format!("copy template {template} into {db_name}"));
                }
                tokio::time::sleep(TEMPLATE_POLL).await;
            }
        }
    }
}

/// Build the template unless somebody already has, waiting out whichever
/// process is currently doing it.
///
/// The coordination is a Postgres session advisory lock rather than anything
/// in-process, because nextest runs every test in its own process: "set this up
/// once, before the concurrent work starts" has to be agreed through the only
/// thing all those processes share, which is the server itself. The lock sits on
/// a dedicated connection so a builder that panics or gets killed releases it
/// when its backend exits, instead of wedging every other process.
async fn ensure_template(server: &Server, template: &str) -> Result<()> {
    let client = server.connect("postgres").await?;
    let key = advisory_key(template);
    let deadline = Instant::now() + TEMPLATE_WAIT;

    loop {
        let acquired: bool = client
            .query_one("SELECT pg_try_advisory_lock($1)", &[&key])
            .await
            .context("take template build lock")?
            .get(0);
        if acquired {
            break;
        }
        // Somebody else is building it. Watch for the finished article rather
        // than for the lock: the moment it appears we are done, lock or no lock.
        if database_exists(&client, template).await? {
            return Ok(());
        }
        if Instant::now() >= deadline {
            bail!("timed out waiting for another process to build template {template}");
        }
        tokio::time::sleep(TEMPLATE_POLL).await;
    }

    let built = build_template(&client, server, template).await;
    let _ = client
        .execute("SELECT pg_advisory_unlock($1)", &[&key])
        .await;
    built
}

/// Migrate a fresh database and publish it as the template. Caller holds the
/// build lock.
async fn build_template(admin: &Client, server: &Server, template: &str) -> Result<()> {
    // Re-check under the lock: the peer we queued behind may have just finished.
    if database_exists(admin, template).await? {
        return Ok(());
    }

    // Migrate under a scratch name and rename only on success, so a builder
    // killed mid-migration leaves behind a scratch database rather than a
    // half-migrated one under the name every other process trusts. Existence of
    // the final name means "finished", full stop.
    let scratch = format!(
        "{template}_b{}",
        &uuid::Uuid::now_v7().simple().to_string()[..12]
    );
    admin
        .batch_execute(&format!("CREATE DATABASE \"{scratch}\""))
        .await
        .with_context(|| format!("create scratch database {scratch}"))?;

    // Migrated through the ordinary path, so the template is exactly what the
    // fallback would have produced — and the pool is dropped before we seal it.
    let migrated = async {
        let db = Db::new(&server.db_config(scratch.clone(), 1))?;
        db.migrate().await
    }
    .await;

    let published = match migrated {
        Ok(()) => publish_template(admin, &scratch, template).await,
        Err(e) => Err(e).with_context(|| format!("migrate template {template}")),
    };
    if published.is_err() {
        let _ = admin
            .batch_execute(&format!("DROP DATABASE IF EXISTS \"{scratch}\""))
            .await;
    }
    published
}

/// Close everything attached to `scratch`, make it unconnectable, and rename it
/// into place as `template`.
async fn publish_template(admin: &Client, scratch: &str, template: &str) -> Result<()> {
    // `ALTER DATABASE ... RENAME` and `CREATE DATABASE ... TEMPLATE` both refuse
    // to run while any backend is attached to the database in question, and the
    // migration pool above closes its connections asynchronously. Force them
    // shut and wait for the count to reach zero rather than hoping the pool's
    // drop has already landed.
    disconnect_all(admin, scratch).await?;

    // Sealed before it is published, so the template is never connectable under
    // its final name. That demotes "source database is being accessed by other
    // users" — the error that makes naive templating flaky — from unlikely to
    // unreachable, because nothing can attach to it in the first place.
    admin
        .batch_execute(&format!(
            "ALTER DATABASE \"{scratch}\" WITH ALLOW_CONNECTIONS false"
        ))
        .await
        .with_context(|| format!("seal template {scratch}"))?;

    if let Err(e) = admin
        .batch_execute(&format!(
            "ALTER DATABASE \"{scratch}\" RENAME TO \"{template}\""
        ))
        .await
    {
        // Lost a race with a builder that was not holding our lock. Its template
        // is as good as ours, so drop ours and use theirs.
        if !database_exists(admin, template).await.unwrap_or(false) {
            return Err(anyhow::Error::new(e))
                .with_context(|| format!("publish template {template}"));
        }
        let _ = admin
            .batch_execute(&format!("DROP DATABASE IF EXISTS \"{scratch}\""))
            .await;
    }

    Ok(())
}

/// Terminate every other backend attached to `dbname` and wait for the count to
/// reach zero.
async fn disconnect_all(admin: &Client, dbname: &str) -> Result<()> {
    const ATTEMPTS: u32 = 100;
    const POLL: Duration = Duration::from_millis(20);

    for _ in 0..ATTEMPTS {
        let remaining: i64 = admin
            .query_one(
                "SELECT count(*) FROM pg_stat_activity
                 WHERE datname = $1 AND pid <> pg_backend_pid()",
                &[&dbname],
            )
            .await
            .with_context(|| format!("count backends on {dbname}"))?
            .get(0);
        if remaining == 0 {
            return Ok(());
        }
        admin
            .execute(
                "SELECT pg_terminate_backend(pid) FROM pg_stat_activity
                 WHERE datname = $1 AND pid <> pg_backend_pid()",
                &[&dbname],
            )
            .await
            .with_context(|| format!("terminate backends on {dbname}"))?;
        tokio::time::sleep(POLL).await;
    }

    bail!("connections to {dbname} would not close")
}

async fn database_exists(client: &Client, name: &str) -> Result<bool> {
    Ok(client
        .query_opt("SELECT 1 FROM pg_database WHERE datname = $1", &[&name])
        .await
        .with_context(|| format!("look up database {name}"))?
        .is_some())
}

/// Advisory-lock key for a template name. Derived from the name — and so from
/// the migration fingerprint in it — so two different migration sets never
/// serialize against each other.
fn advisory_key(template: &str) -> i64 {
    use sha2::{Digest, Sha256};

    let digest = Sha256::digest(template.as_bytes());
    i64::from_le_bytes(digest[..8].try_into().expect("sha256 yields 32 bytes"))
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

/// Test-only clock control: move every crown reign back one UTC month, as if
/// the month had rolled over under it. The reign stays open on purpose: the
/// crown emptying at the boundary is a read-time rule with no sweeper behind
/// it, and this is what lets a test stand on the far side of the rollover.
pub async fn roll_crown_reigns_back_a_month(client: &tokio_postgres::Client) {
    let updated = client
        .execute(
            "UPDATE crown_reigns
             SET month = (month - interval '1 month')::date,
                 taken_at = taken_at - interval '1 month'",
            &[],
        )
        .await
        .expect("roll crown reigns back a month");
    assert!(
        updated > 0,
        "roll_crown_reigns_back_a_month matched no reign"
    );
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
            &format!("UPDATE {table} SET created = now() + interval '1 second' WHERE {filter_sql}"),
            params,
        )
        .await
        .expect("bump created past now");
    assert!(updated > 0, "bump_created_past_now matched no rows");
}
