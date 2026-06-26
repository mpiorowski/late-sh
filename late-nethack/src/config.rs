use anyhow::Context;

/// Runtime configuration for the standalone NetHack host, read from the
/// environment. Mirrors the knobs the door used to own inside late-ssh, plus the
/// SSH listener settings.
pub struct Config {
    /// Path to the nethack binary.
    pub bin: String,
    /// `HOME` for each child (its `.nethackrc` lives here).
    pub data_dir: String,
    /// Shared secret. The single authorized client key is derived from this; it
    /// must match late-ssh's `LATE_NETHACK_SECRET`.
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
        let secret = optional("LATE_NETHACK_SECRET").context("LATE_NETHACK_SECRET must be set")?;
        Ok(Self {
            bin: optional("LATE_NETHACK_BIN").unwrap_or_else(|| "/usr/games/nethack".to_string()),
            data_dir: optional("LATE_NETHACK_DATA_DIR")
                .unwrap_or_else(|| "/var/lib/late-nethack".to_string()),
            secret,
            listen_addr: optional("LATE_NETHACK_LISTEN_ADDR")
                .unwrap_or_else(|| "0.0.0.0".to_string()),
            port: optional_parse("LATE_NETHACK_PORT", 2323)?,
            idle_timeout: optional_parse("LATE_NETHACK_IDLE_TIMEOUT", 3600)?,
        })
    }
}
