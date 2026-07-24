use anyhow::Context;

/// Runtime configuration for the standalone Brogue host, read from the
/// environment. Unlike crawl there is no `-name` flag: brogue opens every
/// player file (saves, recordings, high scores, run history) relative to its
/// working directory, so per-player identity is a per-player cwd inside
/// `data_dir` (the dgamelaunch model; see host.rs `player_dir`).
pub struct Config {
    /// Path to the brogue curses binary.
    pub bin: String,
    /// Root of the per-player playground (the PVC in prod). Each child runs
    /// with cwd `data_dir/players/<playname>`, which holds that player's
    /// saves, recordings, high scores, and run history.
    pub data_dir: String,
    /// Shared secret. The single authorized client key is derived from this; it
    /// must match late-ssh's `LATE_BROGUE_SECRET`.
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
        let secret = optional("LATE_BROGUE_SECRET").context("LATE_BROGUE_SECRET must be set")?;
        Ok(Self {
            bin: optional("LATE_BROGUE_BIN").unwrap_or_else(|| "/usr/games/brogue".to_string()),
            data_dir: optional("LATE_BROGUE_DATA_DIR")
                .unwrap_or_else(|| "/var/lib/late-brogue".to_string()),
            secret,
            listen_addr: optional("LATE_BROGUE_LISTEN_ADDR")
                .unwrap_or_else(|| "0.0.0.0".to_string()),
            port: optional_parse("LATE_BROGUE_PORT", 2327)?,
            idle_timeout: optional_parse("LATE_BROGUE_IDLE_TIMEOUT", 3600)?,
        })
    }
}
