use anyhow::Context;
use ipnet::IpNet;
use late_core::db::DbConfig;
use std::path::PathBuf;

use crate::app::voice::svc::VoiceConfig;

#[derive(Clone, Debug)]
pub struct AiConfig {
    pub enabled: bool,
    pub api_key: Option<String>,
    pub model: String,
}

#[derive(Clone, Debug)]
pub struct WebTunnelConfig {
    pub token: String,
    pub username: String,
    pub fingerprint: String,
}

/// Embedded ircd settings; see devdocs/FRD-IRCD.md. All env vars are optional
/// so environments without `LATE_IRC_*` settings are unaffected until the
/// listener is explicitly enabled. The root Makefile opts local dev in.
#[derive(Clone, Debug)]
pub struct IrcConfig {
    pub enabled: bool,
    pub port: u16,
    pub tls_cert_path: Option<PathBuf>,
    pub tls_key_path: Option<PathBuf>,
    pub max_conns_global: usize,
    pub max_conns_per_user: usize,
    pub max_auth_failures_per_ip: usize,
    pub auth_failure_window_secs: u64,
}

impl Default for IrcConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            port: 6667,
            tls_cert_path: None,
            tls_key_path: None,
            max_conns_global: 200,
            max_conns_per_user: 3,
            max_auth_failures_per_ip: 20,
            auth_failure_window_secs: 300,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Config {
    pub ssh_port: u16,
    pub api_port: u16,
    pub icecast_url: String,
    pub web_url: String,
    pub open_access: bool,
    pub force_admin: bool,
    pub db: DbConfig,
    pub max_conns_global: usize,
    pub max_conns_per_ip: usize,
    pub ssh_idle_timeout: u64,
    pub server_key_path: PathBuf,
    pub allowed_origins: Vec<String>,
    pub frame_drop_log_every: u64,
    pub ssh_max_attempts_per_ip: usize,
    pub ssh_rate_limit_window_secs: u64,
    pub ssh_proxy_protocol: bool,
    pub ssh_proxy_trusted_cidrs: Vec<IpNet>,
    pub ws_pair_max_attempts_per_ip: usize,
    pub ws_pair_rate_limit_window_secs: u64,
    pub web_tunnel: WebTunnelConfig,
    pub ai: AiConfig,
    pub youtube_api_key: Option<String>,
    pub voice: VoiceConfig,
    pub irc: IrcConfig,
    pub rebels_enabled: bool,
    pub rebels_host: String,
    pub rebels_port: u16,
    pub rebels_secret: String,
    pub nethack_enabled: bool,
    pub nethack_bin: String,
    pub nethack_data_dir: String,
}

fn required(key: &str) -> anyhow::Result<String> {
    std::env::var(key).with_context(|| format!("{key} must be set"))
}

fn required_parse<T: std::str::FromStr>(key: &str) -> anyhow::Result<T>
where
    T::Err: std::fmt::Display,
{
    required(key)?
        .parse()
        .map_err(|e| anyhow::anyhow!("{key} invalid: {e}"))
}

fn required_bool(key: &str) -> anyhow::Result<bool> {
    let v = required(key)?;
    Ok(v == "1" || v.eq_ignore_ascii_case("true"))
}

fn parse_bool(key: &str, v: &str) -> anyhow::Result<bool> {
    match v.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        _ => anyhow::bail!("{key} invalid: expected boolean"),
    }
}

fn optional(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.trim().is_empty())
}

fn optional_bool(key: &str, default: bool) -> anyhow::Result<bool> {
    match optional(key) {
        Some(value) => parse_bool(key, &value),
        None => Ok(default),
    }
}

fn optional_parse<T: std::str::FromStr>(key: &str, default: T) -> anyhow::Result<T>
where
    T::Err: std::fmt::Display,
{
    match optional(key) {
        Some(value) => value
            .parse()
            .map_err(|e| anyhow::anyhow!("{key} invalid: {e}")),
        None => Ok(default),
    }
}

impl Config {
    /// Log the full configuration at startup with human-readable descriptions.
    pub fn log_startup(&self) {
        tracing::info!(
            ssh_port = self.ssh_port,
            api_port = self.api_port,
            open_access = self.open_access,
            force_admin = self.force_admin,
            "network: SSH listener port, internal API port, open-access auth mode, dev force-admin"
        );
        tracing::info!(
            db_host = %self.db.host,
            db_port = self.db.port,
            db_name = %self.db.dbname,
            pool_size = self.db.max_pool_size,
            "database: Postgres connection target and pool size"
        );
        tracing::info!(
            icecast_url = %self.icecast_url,
            web_url = %self.web_url,
            "audio: Icecast status endpoint and web pairing URL"
        );
        tracing::info!(
            max_global = self.max_conns_global,
            max_per_ip = self.max_conns_per_ip,
            idle_timeout_secs = self.ssh_idle_timeout,
            "limits: max simultaneous SSH sessions (global / per-IP), idle disconnect"
        );
        tracing::info!(
            ssh_max_attempts = self.ssh_max_attempts_per_ip,
            ssh_window_secs = self.ssh_rate_limit_window_secs,
            ws_max_attempts = self.ws_pair_max_attempts_per_ip,
            ws_window_secs = self.ws_pair_rate_limit_window_secs,
            "rate-limits: SSH auth attempts and WS pair attempts per IP per window"
        );
        tracing::info!(
            proxy_protocol = self.ssh_proxy_protocol,
            trusted_cidrs = ?self.ssh_proxy_trusted_cidrs,
            "proxy: PROXY protocol for real client IP behind load balancer"
        );
        tracing::info!(
            frame_drop_log_every = self.frame_drop_log_every,
            "tuning: render frame-drop log throttle"
        );
        tracing::info!(
            ai_enabled = self.ai.enabled,
            ai_model = %self.ai.model,
            has_key = self.ai.api_key.is_some(),
            "ai: @bot chat responder model and status"
        );
        tracing::info!(
            has_key = self.youtube_api_key.is_some(),
            "youtube: Data API validation key status"
        );
        tracing::info!(
            enabled = self.voice.enabled,
            livekit_url = ?self.voice.livekit_url,
            room = %self.voice.room_name,
            has_key = self.voice.api_key.is_some(),
            "voice: LiveKit RTC status"
        );
        tracing::info!(
            username = %self.web_tunnel.username,
            token_len = self.web_tunnel.token.len(),
            "web-tunnel: browser TUI display route"
        );
        tracing::info!(
            enabled = self.irc.enabled,
            port = self.irc.port,
            tls = self.irc.tls_cert_path.is_some(),
            max_global = self.irc.max_conns_global,
            max_per_user = self.irc.max_conns_per_user,
            "irc: embedded ircd listener status"
        );
        tracing::info!(
            enabled = self.rebels_enabled,
            host = %self.rebels_host,
            port = self.rebels_port,
            has_secret = !self.rebels_secret.is_empty(),
            "rebels: Rebels in the Sky door-game proxy target and status"
        );
        tracing::info!(
            enabled = self.nethack_enabled,
            bin = %self.nethack_bin,
            data_dir = %self.nethack_data_dir,
            "nethack: local NetHack door-game status"
        );
    }

    pub fn from_env() -> anyhow::Result<Self> {
        let ai_enabled = required_bool("LATE_AI_ENABLED")?;
        let ai_api_key = if ai_enabled {
            Some(
                optional("LATE_AI_API_KEY")
                    .context("LATE_AI_API_KEY must be set when LATE_AI_ENABLED is true")?,
            )
        } else {
            optional("LATE_AI_API_KEY")
        };

        let db = DbConfig {
            host: required("LATE_DB_HOST")?,
            port: required_parse("LATE_DB_PORT")?,
            user: required("LATE_DB_USER")?,
            password: required("LATE_DB_PASSWORD")?,
            dbname: required("LATE_DB_NAME")?,
            max_pool_size: required_parse("LATE_DB_POOL_SIZE")?,
        };
        let web_tunnel_token = required("LATE_WEB_TUNNEL_TOKEN")?;
        if web_tunnel_token.trim().is_empty() {
            anyhow::bail!("LATE_WEB_TUNNEL_TOKEN must not be empty");
        }
        let voice = if optional_bool("LATE_VOICE_ENABLED", false)? {
            VoiceConfig::enabled(
                required("LATE_LIVEKIT_URL")?,
                required("LATE_LIVEKIT_API_KEY")?,
                required("LATE_LIVEKIT_API_SECRET")?,
                optional("LATE_VOICE_ROOM").unwrap_or_else(|| "late-voice".to_string()),
            )?
        } else {
            VoiceConfig::disabled()
        };

        let rebels_enabled = optional_bool("LATE_REBELS_ENABLED", true)?;
        let rebels_secret = if rebels_enabled {
            optional("LATE_REBELS_SECRET")
                .context("LATE_REBELS_SECRET must be set when LATE_REBELS_ENABLED is true")?
        } else {
            optional("LATE_REBELS_SECRET").unwrap_or_default()
        };

        Ok(Self {
            ssh_port: required_parse("LATE_SSH_PORT")?,
            api_port: required_parse("LATE_API_PORT")?,
            icecast_url: required("LATE_ICECAST_URL")?,
            web_url: required("LATE_WEB_URL")?,
            open_access: required_bool("LATE_SSH_OPEN")?,
            force_admin: required_bool("LATE_FORCE_ADMIN")?,
            db,
            max_conns_global: required_parse("LATE_MAX_CONNS_GLOBAL")?,
            max_conns_per_ip: required_parse("LATE_MAX_CONNS_PER_IP")?,
            ssh_idle_timeout: required_parse("LATE_SSH_IDLE_TIMEOUT")?,
            server_key_path: PathBuf::from(required("LATE_SSH_KEY_PATH")?),
            allowed_origins: required("LATE_ALLOWED_ORIGINS")?
                .split(',')
                .map(|s| s.trim().to_string())
                .collect(),
            frame_drop_log_every: required_parse("LATE_FRAME_DROP_LOG_EVERY")?,
            ssh_max_attempts_per_ip: required_parse("LATE_SSH_MAX_ATTEMPTS_PER_IP")?,
            ssh_rate_limit_window_secs: required_parse("LATE_SSH_RATE_LIMIT_WINDOW_SECS")?,
            ssh_proxy_protocol: required_bool("LATE_SSH_PROXY_PROTOCOL")?,
            ssh_proxy_trusted_cidrs: required("LATE_SSH_PROXY_TRUSTED_CIDRS")?
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| {
                    s.parse::<IpNet>().map_err(|e| {
                        anyhow::anyhow!("LATE_SSH_PROXY_TRUSTED_CIDRS invalid entry '{s}': {e}")
                    })
                })
                .collect::<anyhow::Result<Vec<_>>>()?,
            ws_pair_max_attempts_per_ip: required_parse("LATE_WS_PAIR_MAX_ATTEMPTS_PER_IP")?,
            ws_pair_rate_limit_window_secs: required_parse("LATE_WS_PAIR_RATE_LIMIT_WINDOW_SECS")?,
            web_tunnel: WebTunnelConfig {
                token: web_tunnel_token,
                username: optional("LATE_WEB_TUNNEL_USERNAME")
                    .unwrap_or_else(|| "web-demo".to_string()),
                fingerprint: optional("LATE_WEB_TUNNEL_FINGERPRINT")
                    .unwrap_or_else(|| "web-tunnel-demo".to_string()),
            },
            ai: AiConfig {
                enabled: ai_enabled,
                api_key: ai_api_key,
                model: required("LATE_AI_MODEL")?,
            },
            youtube_api_key: optional("LATE_YOUTUBE_API_KEY"),
            voice,
            irc: {
                let defaults = IrcConfig::default();
                let enabled = optional_bool("LATE_IRC_ENABLED", defaults.enabled)?;
                let tls_cert_path = optional("LATE_IRC_TLS_CERT").map(PathBuf::from);
                let tls_key_path = optional("LATE_IRC_TLS_KEY").map(PathBuf::from);
                if enabled {
                    match (&tls_cert_path, &tls_key_path) {
                        (Some(_), Some(_)) | (None, None) => {}
                        (Some(_), None) => {
                            anyhow::bail!(
                                "LATE_IRC_TLS_KEY must be set when LATE_IRC_TLS_CERT is set"
                            );
                        }
                        (None, Some(_)) => {
                            anyhow::bail!(
                                "LATE_IRC_TLS_CERT must be set when LATE_IRC_TLS_KEY is set"
                            );
                        }
                    }
                }
                let default_port = if enabled && tls_cert_path.is_some() {
                    6697
                } else {
                    defaults.port
                };
                IrcConfig {
                    enabled,
                    port: optional_parse("LATE_IRC_PORT", default_port)?,
                    tls_cert_path,
                    tls_key_path,
                    max_conns_global: optional_parse(
                        "LATE_IRC_MAX_CONNS_GLOBAL",
                        defaults.max_conns_global,
                    )?,
                    max_conns_per_user: optional_parse(
                        "LATE_IRC_MAX_CONNS_PER_USER",
                        defaults.max_conns_per_user,
                    )?,
                    max_auth_failures_per_ip: optional_parse(
                        "LATE_IRC_MAX_AUTH_FAILURES_PER_IP",
                        defaults.max_auth_failures_per_ip,
                    )?,
                    auth_failure_window_secs: optional_parse(
                        "LATE_IRC_AUTH_FAILURE_WINDOW_SECS",
                        defaults.auth_failure_window_secs,
                    )?,
                }
            },
            rebels_enabled,
            rebels_host: optional("LATE_REBELS_HOST").unwrap_or_else(|| "frittura.org".to_string()),
            rebels_port: optional_parse("LATE_REBELS_PORT", 3788)?,
            rebels_secret,
            nethack_enabled: optional_bool("LATE_NETHACK_ENABLED", false)?,
            nethack_bin: optional("LATE_NETHACK_BIN")
                .unwrap_or_else(|| "/usr/games/nethack".to_string()),
            nethack_data_dir: optional("LATE_NETHACK_DATA_DIR")
                .unwrap_or_else(|| "/var/lib/late-nethack".to_string()),
        })
    }
}
