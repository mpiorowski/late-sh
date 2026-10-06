use anyhow::Context;

/// Runtime configuration for the standalone BashQuest host, read from the
/// environment. Shape matches the DCSS host: bashquest.sh keys saves by
/// `BASHQUEST_AUTOLOGIN` (the arcade handle) inside a shared `HOME`
/// (`data_dir`), so every child shares one persistent playground, not a
/// per-session scratch dir. That is deliberate: bashquest.sh's own leaderboard
/// only means something if every player's `users.db`/save lives in the same
/// place.
pub(crate) struct Config {
    /// Path to bashquest.sh (executable, `#!/bin/bash` shebang).
    pub(crate) bin: String,
    /// `HOME` for every child. bashquest.sh keeps everything under
    /// `$HOME/.bashquest` (the shared `users.db` and every player's
    /// `<name>.save`), so this is the persistent playground (the PVC in
    /// prod).
    pub(crate) data_dir: String,
    /// Shared secret. The single authorized client key is derived from this;
    /// it must match late-ssh's `LATE_BASHQUEST_SECRET`.
    pub(crate) secret: String,
    /// Address to bind the SSH listener to.
    pub(crate) listen_addr: String,
    /// Port to bind the SSH listener to.
    pub(crate) port: u16,
    /// SSH inactivity timeout in seconds.
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
            optional("LATE_BASHQUEST_SECRET").context("LATE_BASHQUEST_SECRET must be set")?;
        Ok(Self {
            bin: optional("LATE_BASHQUEST_BIN")
                .unwrap_or_else(|| "/usr/local/bin/bashquest.sh".to_string()),
            data_dir: optional("LATE_BASHQUEST_DATA_DIR")
                .unwrap_or_else(|| "/var/lib/late-bashquest".to_string()),
            secret,
            listen_addr: optional("LATE_BASHQUEST_LISTEN_ADDR")
                .unwrap_or_else(|| "0.0.0.0".to_string()),
            port: optional_parse("LATE_BASHQUEST_PORT", 2330)?,
            idle_timeout: optional_parse("LATE_BASHQUEST_IDLE_TIMEOUT", 3600)?,
        })
    }
}
