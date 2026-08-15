//! All configuration lives here, keyed by `LATE_ENV`.
//!
//! The only values read from the process environment are `LATE_ENV` itself
//! and secrets (credentials, API keys, shared identity secrets). Everything
//! else is a literal in the profile functions below: `dev()` mirrors the
//! local compose stack, `prod()` mirrors the k8s deployment. Changing a
//! non-secret value means editing this file and deploying, on purpose.

use anyhow::Context;
use ipnet::IpNet;
use late_core::db::DbConfig;
use std::path::PathBuf;

use crate::app::voice::svc::VoiceConfig;

/// Which environment this process runs as, from `LATE_ENV`.
///
/// Adding a variant forces every `match` below to spell out a complete
/// profile for it; there is no fallback environment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Env {
    /// Local docker compose stack (`make start`).
    Dev,
    /// Second local compose instance (`make start-instance2`); identical to
    /// `Dev` except for the ports the Makefile also overrides compose-side.
    Dev2,
    /// The k8s cluster deployed from `infra/`.
    Prod,
}

impl Env {
    fn from_process_env() -> anyhow::Result<Self> {
        match required("LATE_ENV")?.as_str() {
            "dev" => Ok(Self::Dev),
            "dev2" => Ok(Self::Dev2),
            "prod" => Ok(Self::Prod),
            other => anyhow::bail!("LATE_ENV invalid: '{other}' (expected dev, dev2, or prod)"),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Dev => "dev",
            Self::Dev2 => "dev2",
            Self::Prod => "prod",
        }
    }
}

#[derive(Clone, Debug)]
pub struct AiConfig {
    pub enabled: bool,
    pub api_key: Option<String>,
}

/// Embedded ircd settings; see devdocs/FRD-IRCD.md.
#[derive(Clone, Debug)]
pub struct IrcConfig {
    pub enabled: bool,
    pub port: u16,
    pub tls_cert_path: Option<PathBuf>,
    pub tls_key_path: Option<PathBuf>,
    pub proxy_protocol: bool,
    pub proxy_trusted_cidrs: Vec<IpNet>,
    pub max_conns_global: usize,
    pub max_conns_per_user: usize,
    pub max_auth_failures_per_ip: usize,
    pub auth_failure_window_secs: u64,
}

/// Size cap for images this server will upload or fetch for rendering.
/// Applies in every environment, including ones without upload storage.
pub const MAX_IMAGE_BYTES: usize = 10 * 1024 * 1024;

/// How long a listener waits for a PROXY protocol header before giving up.
/// Shared by the SSH and IRC accept paths.
pub const PROXY_HEADER_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(250);

/// S3/R2 object storage for image uploads. `Config.files` is `None` when the
/// environment has no upload storage; every feature that uploads checks that
/// instead of probing env vars.
#[derive(Clone, Debug)]
pub struct FilesConfig {
    pub endpoint: String,
    pub bucket: String,
    pub public_base_url: String,
    pub region: String,
    pub access_key_id: String,
    pub secret_access_key: String,
}

#[derive(Clone, Debug)]
pub struct Config {
    pub env: Env,
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
    pub frame_drop_log_every: u64,
    pub ssh_max_attempts_per_ip: usize,
    pub ssh_rate_limit_window_secs: u64,
    pub ssh_proxy_protocol: bool,
    pub ssh_proxy_trusted_cidrs: Vec<IpNet>,
    pub ws_pair_max_attempts_per_ip: usize,
    pub ws_pair_rate_limit_window_secs: u64,
    pub ai: AiConfig,
    pub youtube_api_key: Option<String>,
    pub voice: VoiceConfig,
    pub irc: IrcConfig,
    pub files: Option<FilesConfig>,
    pub rebels_enabled: bool,
    pub rebels_host: String,
    pub rebels_port: u16,
    pub rebels_secret: String,
    pub nethack_enabled: bool,
    pub nethack_host: String,
    pub nethack_port: u16,
    pub nethack_secret: String,
    /// DCSS door game: reached over SSH like nethack. `enabled` gates only
    /// the client; the host (`late-dcss`) is deployed unconditionally.
    pub dcss_enabled: bool,
    pub dcss_host: String,
    pub dcss_port: u16,
    pub dcss_secret: String,
    /// Brogue door game: reached over SSH like dcss. `enabled` gates only
    /// the client; the host (`late-brogue`) is deployed unconditionally.
    pub brogue_enabled: bool,
    pub brogue_host: String,
    pub brogue_port: u16,
    pub brogue_secret: String,
    /// Usurper door game: reached over SSH like nethack. `enabled` gates only
    /// the client; the host (`late-usurper`) is deployed unconditionally.
    pub usurper_enabled: bool,
    pub usurper_host: String,
    pub usurper_port: u16,
    pub usurper_secret: String,
    /// dopewars door game: reached over SSH like nethack. `enabled` gates only
    /// the client; the host (`late-dopewars`) is deployed unconditionally.
    pub dopewars_enabled: bool,
    pub dopewars_host: String,
    pub dopewars_port: u16,
    pub dopewars_secret: String,
    /// CodeKeep: The Pale door game (host `late-codekeep`).
    pub codekeep_enabled: bool,
    pub codekeep_host: String,
    pub codekeep_port: u16,
    pub codekeep_secret: String,
}

/// Read a required env value; empty or whitespace-only counts as unset.
fn required(key: &str) -> anyhow::Result<String> {
    let value = std::env::var(key)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
    value.with_context(|| format!("{key} must be set"))
}

/// Read an optional env value; empty or whitespace-only counts as unset.
/// Used only for personal dev keys that opt optional integrations in.
fn optional(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// The production R2 bucket. Prod always carries it; dev opts in by setting
/// both R2 credentials, so local uploads land in the same bucket prod serves.
fn prod_files(access_key_id: String, secret_access_key: String) -> FilesConfig {
    FilesConfig {
        endpoint: "https://8ecfba101ed3834cf19fd86e68fc325b.r2.cloudflarestorage.com".to_string(),
        bucket: "late-sh-r-files".to_string(),
        public_base_url: "https://files.late.sh".to_string(),
        region: "auto".to_string(),
        access_key_id,
        secret_access_key,
    }
}

/// Dev opt-in for uploads: both credentials present enables the prod bucket,
/// both absent disables uploads, a half-set pair is a startup error.
pub(crate) fn dev_files(
    access_key_id: Option<String>,
    secret_access_key: Option<String>,
) -> anyhow::Result<Option<FilesConfig>> {
    match (access_key_id, secret_access_key) {
        (Some(access_key_id), Some(secret_access_key)) => {
            Ok(Some(prod_files(access_key_id, secret_access_key)))
        }
        (None, None) => Ok(None),
        (Some(_), None) => {
            anyhow::bail!(
                "LATE_FILES_S3_ACCESS_KEY_ID is set without LATE_FILES_S3_SECRET_ACCESS_KEY"
            )
        }
        (None, Some(_)) => {
            anyhow::bail!(
                "LATE_FILES_S3_SECRET_ACCESS_KEY is set without LATE_FILES_S3_ACCESS_KEY_ID"
            )
        }
    }
}

fn parse_cidrs(label: &str, value: &str) -> anyhow::Result<Vec<IpNet>> {
    value
        .split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(|entry| {
            entry
                .parse::<IpNet>()
                .map_err(|error| anyhow::anyhow!("{label} invalid entry '{entry}': {error}"))
        })
        .collect()
}

/// Ports that differ between the two local compose instances; everything else
/// in the dev profile is identical. The compose-side half (host port mapping)
/// lives in the root .env.dev / .env.dev2 templates and must stay in sync.
struct DevInstance {
    ssh_port: u16,
    api_port: u16,
    irc_port: u16,
    web_port: u16,
    /// Host-side LiveKit port. Client-facing only: browsers and the CLI
    /// reach LiveKit through the compose port mapping, while this process
    /// reaches it by service name inside the instance's own network.
    livekit_host_port: u16,
}

impl Config {
    pub fn load() -> anyhow::Result<Self> {
        let config = match Env::from_process_env()? {
            Env::Dev => Self::dev(
                Env::Dev,
                DevInstance {
                    ssh_port: 2222,
                    api_port: 4001,
                    irc_port: 6667,
                    web_port: 3000,
                    livekit_host_port: 7880,
                },
            )?,
            Env::Dev2 => Self::dev(
                Env::Dev2,
                DevInstance {
                    ssh_port: 2223,
                    api_port: 4001,
                    irc_port: 6668,
                    web_port: 3001,
                    livekit_host_port: 7883,
                },
            )?,
            Env::Prod => Self::prod()?,
        };
        config.validate()?;
        Ok(config)
    }

    /// Local docker compose stack. Secrets come from `.env`, copied from the
    /// committed `.env.dev` / `.env.dev2` templates (shared with the
    /// door-game and LiveKit containers), or from a personal `.env.local`.
    fn dev(env: Env, instance: DevInstance) -> anyhow::Result<Self> {
        Ok(Self {
            env,
            ssh_port: instance.ssh_port,
            api_port: instance.api_port,
            icecast_url: "http://icecast:8000".to_string(),
            web_url: format!("http://localhost:{}", instance.web_port),
            open_access: true,
            force_admin: true,
            db: DbConfig {
                host: "postgres".to_string(),
                port: 5432,
                user: required("LATE_DB_USER")?,
                password: required("LATE_DB_PASSWORD")?,
                dbname: required("LATE_DB_NAME")?,
                max_pool_size: 16,
            },
            max_conns_global: 10_000,
            max_conns_per_ip: 3,
            ssh_idle_timeout: 3600,
            server_key_path: PathBuf::from("/app/server_key"),
            frame_drop_log_every: 100,
            ssh_max_attempts_per_ip: 30,
            ssh_rate_limit_window_secs: 60,
            ssh_proxy_protocol: false,
            ssh_proxy_trusted_cidrs: Vec::new(),
            ws_pair_max_attempts_per_ip: 30,
            ws_pair_rate_limit_window_secs: 60,
            ai: AiConfig {
                enabled: true,
                api_key: Some(required("LATE_AI_API_KEY")?),
            },
            // Personal opt-in: link validation stays off without a key.
            youtube_api_key: optional("LATE_YOUTUBE_API_KEY"),
            voice: VoiceConfig::enabled(
                // Client-facing: browsers and the CLI run on the host.
                format!("ws://localhost:{}", instance.livekit_host_port),
                // Server-to-server: this process runs in a container, where
                // localhost is itself and not LiveKit.
                "http://livekit:7880".to_string(),
                required("LATE_LIVEKIT_API_KEY")?,
                required("LATE_LIVEKIT_API_SECRET")?,
                "late-voice".to_string(),
            )?,
            irc: IrcConfig {
                enabled: true,
                port: instance.irc_port,
                tls_cert_path: None,
                tls_key_path: None,
                proxy_protocol: false,
                proxy_trusted_cidrs: Vec::new(),
                max_conns_global: 200,
                max_conns_per_user: 3,
                max_auth_failures_per_ip: 20,
                auth_failure_window_secs: 300,
            },
            // Personal opt-in: setting both R2 credentials in .env.local
            // points uploads at the prod bucket; without them, upload
            // features report "disabled".
            files: dev_files(
                optional("LATE_FILES_S3_ACCESS_KEY_ID"),
                optional("LATE_FILES_S3_SECRET_ACCESS_KEY"),
            )?,
            rebels_enabled: true,
            rebels_host: "frittura.org".to_string(),
            rebels_port: 3788,
            rebels_secret: required("LATE_REBELS_SECRET")?,
            nethack_enabled: true,
            nethack_host: "service-nethack".to_string(),
            nethack_port: 2323,
            nethack_secret: required("LATE_NETHACK_SECRET")?,
            dcss_enabled: true,
            dcss_host: "service-dcss".to_string(),
            dcss_port: 2325,
            dcss_secret: required("LATE_DCSS_SECRET")?,
            brogue_enabled: true,
            brogue_host: "service-brogue".to_string(),
            brogue_port: 2327,
            brogue_secret: required("LATE_BROGUE_SECRET")?,
            usurper_enabled: true,
            usurper_host: "service-usurper".to_string(),
            usurper_port: 2326,
            usurper_secret: required("LATE_USURPER_SECRET")?,
            dopewars_enabled: true,
            dopewars_host: "service-dopewars".to_string(),
            dopewars_port: 2324,
            dopewars_secret: required("LATE_DOPEWARS_SECRET")?,
            codekeep_enabled: true,
            codekeep_host: "service-codekeep".to_string(),
            codekeep_port: 2328,
            codekeep_secret: required("LATE_CODEKEEP_SECRET")?,
        })
    }

    /// The k8s deployment. Secrets come from the env vars `infra/service-ssh.tf`
    /// injects out of cluster secrets; every other value is pinned here.
    fn prod() -> anyhow::Result<Self> {
        Ok(Self {
            env: Env::Prod,
            ssh_port: 2222,
            api_port: 4000,
            icecast_url: "http://icecast-sv:8000".to_string(),
            web_url: "https://late.sh".to_string(),
            open_access: true,
            force_admin: false,
            db: DbConfig {
                // CloudNativePG: rw service is stable, credentials are
                // operator-generated and injected from the postgres-app secret.
                host: "postgres-rw".to_string(),
                port: 5432,
                user: required("LATE_DB_USER")?,
                password: required("LATE_DB_PASSWORD")?,
                dbname: required("LATE_DB_NAME")?,
                max_pool_size: 16,
            },
            max_conns_global: 1000,
            max_conns_per_ip: 3,
            ssh_idle_timeout: 3600,
            server_key_path: PathBuf::from("/app/keys/server_key"),
            frame_drop_log_every: 100,
            ssh_max_attempts_per_ip: 30,
            ssh_rate_limit_window_secs: 60,
            // ingress-nginx terminates the edge and prepends PROXY v1; trust
            // only the pod network and the node itself.
            ssh_proxy_protocol: true,
            ssh_proxy_trusted_cidrs: parse_cidrs(
                "prod ssh proxy trusted cidrs",
                "10.42.0.0/16,46.62.210.86/32",
            )?,
            ws_pair_max_attempts_per_ip: 30,
            ws_pair_rate_limit_window_secs: 60,
            ai: AiConfig {
                enabled: true,
                api_key: Some(required("LATE_AI_API_KEY")?),
            },
            youtube_api_key: Some(required("LATE_YOUTUBE_API_KEY")?),
            voice: VoiceConfig::enabled(
                // Client-facing: the public signaling endpoint through ingress.
                "wss://rtc.late.sh".to_string(),
                // Server-to-server: stay inside the cluster instead of
                // looping back out through the public ingress.
                "http://livekit-sv".to_string(),
                required("LATE_LIVEKIT_API_KEY")?,
                required("LATE_LIVEKIT_API_SECRET")?,
                "late-voice".to_string(),
            )?,
            irc: IrcConfig {
                enabled: true,
                port: 6697,
                tls_cert_path: Some(PathBuf::from("/etc/irc-tls/tls.crt")),
                tls_key_path: Some(PathBuf::from("/etc/irc-tls/tls.key")),
                proxy_protocol: true,
                proxy_trusted_cidrs: parse_cidrs(
                    "prod irc proxy trusted cidrs",
                    "10.42.0.0/16,46.62.210.86/32",
                )?,
                max_conns_global: 200,
                max_conns_per_user: 3,
                max_auth_failures_per_ip: 20,
                auth_failure_window_secs: 300,
            },
            files: Some(prod_files(
                required("LATE_FILES_S3_ACCESS_KEY_ID")?,
                required("LATE_FILES_S3_SECRET_ACCESS_KEY")?,
            )),
            rebels_enabled: true,
            rebels_host: "frittura.org".to_string(),
            rebels_port: 3788,
            rebels_secret: required("LATE_REBELS_SECRET")?,
            nethack_enabled: true,
            nethack_host: "late-nethack-sv".to_string(),
            nethack_port: 2323,
            nethack_secret: required("LATE_NETHACK_SECRET")?,
            dcss_enabled: true,
            dcss_host: "late-dcss-sv".to_string(),
            dcss_port: 2325,
            dcss_secret: required("LATE_DCSS_SECRET")?,
            brogue_enabled: true,
            brogue_host: "late-brogue-sv".to_string(),
            brogue_port: 2327,
            brogue_secret: required("LATE_BROGUE_SECRET")?,
            usurper_enabled: true,
            usurper_host: "late-usurper-sv".to_string(),
            usurper_port: 2326,
            usurper_secret: required("LATE_USURPER_SECRET")?,
            dopewars_enabled: true,
            dopewars_host: "late-dopewars-sv".to_string(),
            dopewars_port: 2324,
            dopewars_secret: required("LATE_DOPEWARS_SECRET")?,
            codekeep_enabled: true,
            codekeep_host: "late-codekeep-sv".to_string(),
            codekeep_port: 2328,
            codekeep_secret: required("LATE_CODEKEEP_SECRET")?,
        })
    }

    /// Cross-field invariants every profile must satisfy. Profiles are code,
    /// so a violation is an authoring bug; failing at startup keeps it from
    /// shipping quietly.
    pub(crate) fn validate(&self) -> anyhow::Result<()> {
        if self.ssh_proxy_protocol && self.ssh_proxy_trusted_cidrs.is_empty() {
            anyhow::bail!("ssh proxy protocol requires non-empty trusted cidrs");
        }
        if self.irc.enabled {
            if self.irc.proxy_protocol && self.irc.proxy_trusted_cidrs.is_empty() {
                anyhow::bail!("irc proxy protocol requires non-empty trusted cidrs");
            }
            match (&self.irc.tls_cert_path, &self.irc.tls_key_path) {
                (Some(_), Some(_)) | (None, None) => {}
                (Some(_), None) => anyhow::bail!("irc tls cert is set without a key"),
                (None, Some(_)) => anyhow::bail!("irc tls key is set without a cert"),
            }
        }
        if self.ai.enabled && self.ai.api_key.as_deref().is_none_or(str::is_empty) {
            anyhow::bail!("ai is enabled without an api key");
        }
        let door_secrets = [
            ("rebels", self.rebels_enabled, &self.rebels_secret),
            ("nethack", self.nethack_enabled, &self.nethack_secret),
            ("dcss", self.dcss_enabled, &self.dcss_secret),
            ("brogue", self.brogue_enabled, &self.brogue_secret),
            ("usurper", self.usurper_enabled, &self.usurper_secret),
            ("dopewars", self.dopewars_enabled, &self.dopewars_secret),
            ("codekeep", self.codekeep_enabled, &self.codekeep_secret),
        ];
        for (name, enabled, secret) in door_secrets {
            if enabled && secret.is_empty() {
                anyhow::bail!("{name} is enabled without a shared secret");
            }
        }
        Ok(())
    }

    /// Log the full configuration at startup with human-readable descriptions.
    pub fn log_startup(&self) {
        tracing::info!(env = self.env.as_str(), "profile: active LATE_ENV profile");
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
            "audio: Icecast status endpoint and public web URL"
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
            ai_model = crate::app::ai::svc::AI_MODEL,
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
            livekit_api_url = ?self.voice.livekit_api_url,
            room = %self.voice.room_name,
            has_key = self.voice.api_key.is_some(),
            "voice: LiveKit RTC status"
        );
        tracing::info!(
            enabled = self.irc.enabled,
            port = self.irc.port,
            tls = self.irc.tls_cert_path.is_some(),
            proxy_protocol = self.irc.proxy_protocol,
            proxy_trusted_cidrs = ?self.irc.proxy_trusted_cidrs,
            max_global = self.irc.max_conns_global,
            max_per_user = self.irc.max_conns_per_user,
            "irc: embedded ircd listener status"
        );
        tracing::info!(
            enabled = self.files.is_some(),
            bucket = self.files.as_ref().map(|f| f.bucket.as_str()),
            public_base_url = self.files.as_ref().map(|f| f.public_base_url.as_str()),
            "files: S3/R2 image upload storage status"
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
            host = %self.nethack_host,
            port = self.nethack_port,
            has_secret = !self.nethack_secret.is_empty(),
            "nethack: NetHack door-game host (late-nethack) target and status"
        );
        tracing::info!(
            enabled = self.dcss_enabled,
            host = %self.dcss_host,
            port = self.dcss_port,
            has_secret = !self.dcss_secret.is_empty(),
            "dcss: DCSS door-game host (late-dcss) target and status"
        );
        tracing::info!(
            enabled = self.brogue_enabled,
            host = %self.brogue_host,
            port = self.brogue_port,
            has_secret = !self.brogue_secret.is_empty(),
            "brogue: Brogue door-game host (late-brogue) target and status"
        );
        tracing::info!(
            enabled = self.usurper_enabled,
            host = %self.usurper_host,
            port = self.usurper_port,
            has_secret = !self.usurper_secret.is_empty(),
            "usurper: Usurper door-game host (late-usurper) target and status"
        );
        tracing::info!(
            enabled = self.dopewars_enabled,
            host = %self.dopewars_host,
            port = self.dopewars_port,
            has_secret = !self.dopewars_secret.is_empty(),
            "dopewars: dopewars door-game host (late-dopewars) target and status"
        );
        tracing::info!(
            enabled = self.codekeep_enabled,
            host = %self.codekeep_host,
            port = self.codekeep_port,
            has_secret = !self.codekeep_secret.is_empty(),
            "codekeep: CodeKeep door-game host (late-codekeep) target and status"
        );
    }
}
