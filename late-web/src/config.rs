use anyhow::Context;

#[derive(Clone, Debug)]
pub struct Config {
    pub port: u16,
    pub ssh_internal_url: String,
    pub ssh_public_url: String,
    pub audio_base_url: String,
    pub tunnel_url: String,
    pub tunnel_shared_secret: String,
    pub spectator_username: String,
    pub spectator_fingerprint: String,
    pub spectator_default_cols: u16,
    pub spectator_default_rows: u16,
    pub spectator_max_cols: u16,
    pub spectator_max_rows: u16,
}

impl Config {
    /// Log the full configuration at startup with human-readable descriptions.
    pub fn log_startup(&self) {
        tracing::info!(
            port = self.port,
            "network: HTTP listener port for the web server"
        );
        tracing::info!(
            ssh_internal = %self.ssh_internal_url,
            ssh_public = %self.ssh_public_url,
            "ssh: internal API for now-playing/status, public URL for browser pairing"
        );
        tracing::info!(
            audio_url = %self.audio_base_url,
            "audio: upstream Icecast URL proxied via /stream with silent-frame keepalive"
        );
        tracing::info!(
            tunnel_url = %self.tunnel_url,
            spectator_username = %self.spectator_username,
            default_cols = self.spectator_default_cols,
            default_rows = self.spectator_default_rows,
            max_cols = self.spectator_max_cols,
            max_rows = self.spectator_max_rows,
            "spectate: upstream /tunnel endpoint and terminal size bounds"
        );
    }

    pub fn from_env() -> anyhow::Result<Self> {
        let port = std::env::var("LATE_WEB_PORT")
            .context("LATE_WEB_PORT must be set")?
            .parse()
            .context("LATE_WEB_PORT must be a valid port number")?;

        let ssh_internal_url =
            std::env::var("LATE_SSH_INTERNAL_URL").context("LATE_SSH_INTERNAL_URL must be set")?;

        let ssh_public_url =
            std::env::var("LATE_SSH_PUBLIC_URL").context("LATE_SSH_PUBLIC_URL must be set")?;

        let audio_base_url =
            std::env::var("LATE_AUDIO_URL").context("LATE_AUDIO_URL must be set")?;

        let tunnel_url =
            std::env::var("LATE_WEB_TUNNEL_URL").context("LATE_WEB_TUNNEL_URL must be set")?;
        let tunnel_shared_secret = std::env::var("LATE_WEB_TUNNEL_SHARED_SECRET")
            .context("LATE_WEB_TUNNEL_SHARED_SECRET must be set")?;
        let spectator_username =
            env_or_default("LATE_WEB_SPECTATOR_USERNAME", "spectator".to_string());
        let spectator_fingerprint = env_or_default(
            "LATE_WEB_SPECTATOR_FINGERPRINT",
            "web-spectator:v1".to_string(),
        );
        let spectator_default_cols = parse_env_or_default("LATE_WEB_SPECTATOR_DEFAULT_COLS", 120)?;
        let spectator_default_rows = parse_env_or_default("LATE_WEB_SPECTATOR_DEFAULT_ROWS", 40)?;
        let spectator_max_cols = parse_env_or_default("LATE_WEB_SPECTATOR_MAX_COLS", 300)?;
        let spectator_max_rows = parse_env_or_default("LATE_WEB_SPECTATOR_MAX_ROWS", 100)?;

        Ok(Self {
            port,
            ssh_internal_url,
            ssh_public_url,
            audio_base_url,
            tunnel_url,
            tunnel_shared_secret,
            spectator_username,
            spectator_fingerprint,
            spectator_default_cols,
            spectator_default_rows,
            spectator_max_cols,
            spectator_max_rows,
        })
    }
}

fn env_or_default(name: &str, default: String) -> String {
    std::env::var(name).unwrap_or(default)
}

fn parse_env_or_default<T>(name: &str, default: T) -> anyhow::Result<T>
where
    T: std::str::FromStr + Copy,
    T::Err: std::error::Error + Send + Sync + 'static,
{
    match std::env::var(name) {
        Ok(value) => value
            .parse()
            .with_context(|| format!("{name} must be a valid value")),
        Err(std::env::VarError::NotPresent) => Ok(default),
        Err(err) => Err(err).with_context(|| format!("failed to read {name}")),
    }
}
