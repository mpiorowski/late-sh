use anyhow::Context;

/// Runtime configuration for the standalone Usurper host, read from the
/// environment. Unlike the roguelike hosts (per-player saves inside a shared
/// HOME), Usurper is one shared persistent world: every session runs in the
/// same `game_dir` and the game's own data files (`DATA/USERS.DAT`, keyed by
/// the player name from the per-session dropfile) hold all state.
pub(crate) struct Config {
    /// Path to the USURPER.EXE binary (statically linked, from the image).
    pub(crate) bin: String,
    /// The writable game tree the children run in (their working directory).
    /// The game resolves everything relative to it: DATA/, TEXT/, NODE/,
    /// SCORES/, DOCS/, USURPER.CFG. Backed by a PVC in prod.
    pub(crate) game_dir: String,
    /// Read-only seed template baked into the image; files missing from
    /// `game_dir` are copied from here at boot.
    pub(crate) seed_dir: String,
    /// Shared secret. The single authorized client key is derived from this; it
    /// must match late-ssh's `LATE_USURPER_SECRET`.
    pub(crate) secret: String,
    /// Address to bind the SSH listener to.
    pub(crate) listen_addr: String,
    /// Port to bind the SSH listener to.
    pub(crate) port: u16,
    /// SSH inactivity timeout in seconds.
    pub(crate) idle_timeout: u64,
    /// Maximum concurrent sessions (the game's "nodes"). Each session gets a
    /// node number 1..=max_nodes; when all are taken, new connections are
    /// turned away with a message.
    pub(crate) max_nodes: u16,
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
        let secret = optional("LATE_USURPER_SECRET").context("LATE_USURPER_SECRET must be set")?;
        Ok(Self {
            bin: optional("LATE_USURPER_BIN")
                .unwrap_or_else(|| "/opt/usurper/bin/USURPER.EXE".to_string()),
            game_dir: optional("LATE_USURPER_GAME_DIR")
                .unwrap_or_else(|| "/var/lib/late-usurper".to_string()),
            seed_dir: optional("LATE_USURPER_SEED_DIR")
                .unwrap_or_else(|| "/opt/usurper/seed".to_string()),
            secret,
            listen_addr: optional("LATE_USURPER_LISTEN_ADDR")
                .unwrap_or_else(|| "0.0.0.0".to_string()),
            port: optional_parse("LATE_USURPER_PORT", 2326)?,
            idle_timeout: optional_parse("LATE_USURPER_IDLE_TIMEOUT", 3600)?,
            max_nodes: optional_parse("LATE_USURPER_MAX_NODES", 10)?,
        })
    }
}
