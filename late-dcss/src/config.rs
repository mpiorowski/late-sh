use anyhow::Context;

/// Runtime configuration for the standalone DCSS host, read from the
/// environment. Mirrors the nethack host's knobs: crawl keys per-player saves by
/// the `-name` argument inside a shared `HOME` (`data_dir`), so the shape is the
/// same shared playground.
pub struct Config {
    /// Path to the crawl console binary.
    pub bin: String,
    /// `HOME` for each child. crawl writes everything under `$HOME/.crawl`
    /// (saves keyed by `-name`, shared scores/logfile/milestones, morgues), so
    /// this is the persistent playground (the PVC in prod).
    pub data_dir: String,
    /// Shared secret. The single authorized client key is derived from this; it
    /// must match late-ssh's `LATE_DCSS_SECRET`.
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
        let secret = optional("LATE_DCSS_SECRET").context("LATE_DCSS_SECRET must be set")?;
        Ok(Self {
            bin: optional("LATE_DCSS_BIN").unwrap_or_else(|| "/usr/games/crawl".to_string()),
            data_dir: optional("LATE_DCSS_DATA_DIR")
                .unwrap_or_else(|| "/var/lib/late-dcss".to_string()),
            secret,
            listen_addr: optional("LATE_DCSS_LISTEN_ADDR")
                .unwrap_or_else(|| "0.0.0.0".to_string()),
            port: optional_parse("LATE_DCSS_PORT", 2325)?,
            idle_timeout: optional_parse("LATE_DCSS_IDLE_TIMEOUT", 3600)?,
        })
    }
}
