use anyhow::Context;

pub(crate) struct Config {
    /// Executable wrapper for the pinned CodeKeep package.
    pub(crate) bin: String,
    /// Root containing one stable HOME per late.sh account.
    pub(crate) data_dir: String,
    pub(crate) secret: String,
    pub(crate) listen_addr: String,
    pub(crate) port: u16,
    pub(crate) idle_timeout: u64,
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
    pub(crate) fn from_env() -> anyhow::Result<Self> {
        let secret =
            optional("LATE_CODEKEEP_SECRET").context("LATE_CODEKEEP_SECRET must be set")?;
        Ok(Self {
            bin: optional("LATE_CODEKEEP_BIN")
                .unwrap_or_else(|| "/usr/local/bin/codekeep".to_string()),
            data_dir: optional("LATE_CODEKEEP_DATA_DIR")
                .unwrap_or_else(|| "/var/lib/late-codekeep".to_string()),
            secret,
            listen_addr: optional("LATE_CODEKEEP_LISTEN_ADDR")
                .unwrap_or_else(|| "0.0.0.0".to_string()),
            port: optional_parse("LATE_CODEKEEP_PORT", 2328)?,
            idle_timeout: optional_parse("LATE_CODEKEEP_IDLE_TIMEOUT", 3600)?,
        })
    }
}
