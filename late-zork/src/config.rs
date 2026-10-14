use anyhow::Context;

pub(crate) struct Config {
    /// Independently built GPL interpreter executable.
    pub(crate) bin: String,
    pub(crate) story_dir: String,
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
        let secret = optional("LATE_ZORK_SECRET").context("LATE_ZORK_SECRET must be set")?;
        Ok(Self {
            bin: optional("LATE_ZORK_BIN").unwrap_or_else(|| "/usr/games/frotz".to_string()),
            story_dir: optional("LATE_ZORK_STORY_DIR")
                .unwrap_or_else(|| "/usr/share/late-zork".to_string()),
            data_dir: optional("LATE_ZORK_DATA_DIR")
                .unwrap_or_else(|| "/var/lib/late-zork".to_string()),
            secret,
            listen_addr: optional("LATE_ZORK_LISTEN_ADDR").unwrap_or_else(|| "0.0.0.0".to_string()),
            port: optional_parse("LATE_ZORK_PORT", 2331)?,
            idle_timeout: optional_parse("LATE_ZORK_IDLE_TIMEOUT", 3600)?,
        })
    }
}
