use anyhow::Context;

#[derive(Clone, Debug)]
pub struct Config {
    pub port: u16,
    pub ssh_internal_url: String,
    pub ssh_public_url: String,
    pub audio_base_url: String,
    pub web_tunnel_token: String,
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
            token_len = self.web_tunnel_token.len(),
            "web-tunnel: browser TUI page"
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
        let web_tunnel_token =
            std::env::var("LATE_WEB_TUNNEL_TOKEN").context("LATE_WEB_TUNNEL_TOKEN must be set")?;
        if web_tunnel_token.trim().is_empty() {
            anyhow::bail!("LATE_WEB_TUNNEL_TOKEN must not be empty");
        }

        Ok(Self {
            port,
            ssh_internal_url,
            ssh_public_url,
            audio_base_url,
            web_tunnel_token,
        })
    }
}
