use anyhow::Context;

/// Runtime configuration for the standalone dopewars host, read from the
/// environment. Mirrors the nethack host's config minus the per-player save
/// knobs dopewars doesn't have: there is no `-u` playname and no per-player
/// `HOME`, just the one shared high-score file.
pub struct Config {
    /// Path to the dopewars binary.
    pub bin: String,
    /// The single, shared high-score file passed to every child as `-f`. Lives
    /// on the host's PVC so the leaderboard survives pod restarts and is shared
    /// across all players (the dopewars analog of nethack's shared playground).
    pub score_file: String,
    /// Shared secret. The single authorized client key is derived from this; it
    /// must match late-ssh's `LATE_DOPEWARS_SECRET`.
    pub secret: String,
    /// Address to bind the SSH listener to.
    pub listen_addr: String,
    /// Port to bind the SSH listener to.
    pub port: u16,
    /// SSH inactivity timeout in seconds.
    pub idle_timeout: u64,
}

fn optional(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.is_empty())
}

fn optional_parse<T: std::str::FromStr>(key: &str, default: T) -> anyhow::Result<T>
where
    T::Err: std::fmt::Display,
{
    match optional(key) {
        Some(v) => v
            .parse()
            .map_err(|e| anyhow::anyhow!("{key} is invalid: {e}")),
        None => Ok(default),
    }
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let secret =
            optional("LATE_DOPEWARS_SECRET").context("LATE_DOPEWARS_SECRET must be set")?;
        Ok(Self {
            bin: optional("LATE_DOPEWARS_BIN")
                .unwrap_or_else(|| "/usr/games/dopewars".to_string()),
            score_file: optional("LATE_DOPEWARS_SCORE_FILE")
                .unwrap_or_else(|| "/var/lib/late-dopewars/dopewars.sco".to_string()),
            secret,
            listen_addr: optional("LATE_DOPEWARS_LISTEN_ADDR")
                .unwrap_or_else(|| "0.0.0.0".to_string()),
            port: optional_parse("LATE_DOPEWARS_PORT", 2324)?,
            idle_timeout: optional_parse("LATE_DOPEWARS_IDLE_TIMEOUT", 3600)?,
        })
    }
}
