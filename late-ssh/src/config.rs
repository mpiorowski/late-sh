use anyhow::Context;
use ipnet::IpNet;
use late_core::db::DbConfig;
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct AiConfig {
    pub enabled: bool,
    pub api_key: Option<String>,
    pub model: String,
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
    pub liquidsoap_addr: String,
    pub frame_drop_log_every: u64,
    pub vote_switch_interval_secs: u64,
    pub ssh_max_attempts_per_ip: usize,
    pub ssh_rate_limit_window_secs: u64,
    pub ssh_proxy_protocol: bool,
    pub ssh_proxy_trusted_cidrs: Vec<IpNet>,
    pub ws_pair_max_attempts_per_ip: usize,
    pub ws_pair_rate_limit_window_secs: u64,
    pub ai: AiConfig,
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
            liquidsoap_addr = %self.liquidsoap_addr,
            web_url = %self.web_url,
            "audio: Icecast status endpoint, Liquidsoap telnet, web pairing URL"
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
            vote_switch_secs = self.vote_switch_interval_secs,
            frame_drop_log_every = self.frame_drop_log_every,
            "tuning: genre vote round duration, render frame-drop log throttle"
        );
        tracing::info!(
            ai_enabled = self.ai.enabled,
            ai_model = %self.ai.model,
            has_key = self.ai.api_key.is_some(),
            "ai: @bot chat responder model and status"
        );
    }

    pub fn from_env() -> anyhow::Result<Self> {
        let ai_key_str = required("LATE_AI_API_KEY")?;
        let ai_api_key = if ai_key_str.is_empty() {
            None
        } else {
            Some(ai_key_str)
        };

        let db = DbConfig {
            host: required("LATE_DB_HOST")?,
            port: required_parse("LATE_DB_PORT")?,
            user: required("LATE_DB_USER")?,
            password: required("LATE_DB_PASSWORD")?,
            dbname: required("LATE_DB_NAME")?,
            max_pool_size: required_parse("LATE_DB_POOL_SIZE")?,
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
            liquidsoap_addr: required("LATE_LIQUIDSOAP_ADDR")?,
            frame_drop_log_every: required_parse("LATE_FRAME_DROP_LOG_EVERY")?,
            vote_switch_interval_secs: required_parse("LATE_VOTE_SWITCH_INTERVAL_SECS")?,
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
            ai: AiConfig {
                enabled: required_bool("LATE_AI_ENABLED")?,
                api_key: ai_api_key,
                model: required("LATE_AI_MODEL")?,
            },
        })
    }
}
