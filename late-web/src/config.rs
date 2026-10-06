//! All configuration lives here, keyed by `LATE_ENV`, mirroring
//! late-ssh/src/config.rs. The only env reads are `LATE_ENV` itself and
//! secrets (database credentials); everything else is a literal in the
//! profile functions below.

use anyhow::Context;
use late_core::db::DbConfig;

/// Which environment this process runs as, from `LATE_ENV`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Env {
    /// Local docker compose stack (`make start`).
    Dev,
    /// Second local compose instance (`make start-instance2`).
    Dev2,
    /// The k8s cluster deployed from `infra/`.
    Prod,
}

impl Env {
    fn from_process_env() -> anyhow::Result<Self> {
        match required("LATE_ENV")?.as_str() {
            "dev" => Ok(Self::Dev),
            "dev2" => Ok(Self::Dev2),
            "prod" => Ok(Self::Prod),
            other => anyhow::bail!("LATE_ENV invalid: '{other}' (expected dev, dev2, or prod)"),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Dev => "dev",
            Self::Dev2 => "dev2",
            Self::Prod => "prod",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Config {
    pub env: Env,
    pub port: u16,
    pub ssh_internal_url: String,
    pub audio_base_url: String,
    pub db: DbConfig,
}

/// Read a required env value; empty or whitespace-only counts as unset.
fn required(key: &str) -> anyhow::Result<String> {
    let value = std::env::var(key)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
    value.with_context(|| format!("{key} must be set"))
}

fn db(host: &str) -> anyhow::Result<DbConfig> {
    Ok(DbConfig {
        host: host.to_string(),
        port: 5432,
        user: required("LATE_DB_USER")?,
        password: required("LATE_DB_PASSWORD")?,
        dbname: required("LATE_DB_NAME")?,
        max_pool_size: 16,
    })
}

impl Config {
    pub fn load() -> anyhow::Result<Self> {
        match Env::from_process_env()? {
            Env::Dev => Ok(Self {
                env: Env::Dev,
                port: 3000,
                ssh_internal_url: "http://service-ssh:4001".to_string(),
                audio_base_url: "http://icecast:8000".to_string(),
                db: db("postgres")?,
            }),
            Env::Dev2 => Ok(Self {
                env: Env::Dev2,
                port: 3001,
                ssh_internal_url: "http://service-ssh:4001".to_string(),
                audio_base_url: "http://icecast:8000".to_string(),
                db: db("postgres")?,
            }),
            Env::Prod => Ok(Self {
                env: Env::Prod,
                port: 3000,
                ssh_internal_url: "http://service-ssh-sv:4000".to_string(),
                audio_base_url: "http://icecast-sv:8000".to_string(),
                db: db("postgres-rw")?,
            }),
        }
    }

    /// Log the full configuration at startup with human-readable descriptions.
    pub fn log_startup(&self) {
        tracing::info!(env = self.env.as_str(), "profile: active LATE_ENV profile");
        tracing::info!(
            port = self.port,
            "network: HTTP listener port for the web server"
        );
        tracing::info!(
            ssh_internal = %self.ssh_internal_url,
            "ssh: internal API for now-playing, status, and listen state"
        );
        tracing::info!(
            audio_url = %self.audio_base_url,
            "audio: upstream Icecast URL proxied via /stream with silent-frame keepalive"
        );
        tracing::info!(
            db_host = %self.db.host,
            db_port = self.db.port,
            db_name = %self.db.dbname,
            pool_size = self.db.max_pool_size,
            "database: Postgres connection target and pool size"
        );
    }
}
