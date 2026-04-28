use anyhow::Context;
use ipnet::IpNet;
use std::path::PathBuf;

/// Runtime configuration for `late-bastion`.
///
/// Kept intentionally lean — the bastion has no DB, no service deps, no
/// per-user state, and no ban logic. See `PERSISTENT-CONNECTION-GATEWAY.md`
/// §5 for the "intentionally minimal" principle.
#[derive(Clone, Debug)]
pub struct Config {
    /// SSH listener port (default 5222 during dual-path rollout; eventually
    /// fronts NGINX `:22` after cutover).
    pub ssh_port: u16,

    /// SSH host key path. Loaded via `load_or_generate_key`; persisted to
    /// disk on first generation. In production this is mounted from a K8s
    /// Secret.
    pub host_key_path: PathBuf,

    /// `russh` inactivity timeout (seconds). Kicks idle SSH sessions.
    pub ssh_idle_timeout: u64,

    /// Backend `/tunnel` URL the bastion dials per shell session, e.g.
    /// `ws://service-ssh-internal:4001/tunnel`.
    pub backend_tunnel_url: String,

    /// Pre-shared secret sent on the WS upgrade as `X-Late-Secret`. Backend
    /// constant-time compares.
    pub backend_shared_secret: String,

    /// Global cap on simultaneous SSH connections. OOM guard. No per-IP cap
    /// at the bastion (per Q8 — that stays at late-ssh keyed on
    /// `X-Late-Peer-IP`).
    pub max_conns_global: usize,

    /// Whether NGINX is fronting the bastion with PROXY v1. When true, the
    /// bastion reads the PROXY v1 header from each accepted TCP connection
    /// to recover the real client IP (which it forwards to late-ssh via
    /// `X-Late-Peer-IP`).
    pub proxy_protocol: bool,

    /// CIDRs we accept PROXY v1 headers from. Defends against header
    /// spoofing in case the listener gets exposed to non-NGINX traffic.
    pub proxy_trusted_cidrs: Vec<IpNet>,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let ssh_port = required_parse("LATE_BASTION_SSH_PORT")?;
        let host_key_path = PathBuf::from(required("LATE_BASTION_HOST_KEY_PATH")?);
        let ssh_idle_timeout = required_parse("LATE_BASTION_SSH_IDLE_TIMEOUT")?;
        let backend_tunnel_url = required("LATE_BASTION_BACKEND_TUNNEL_URL")?;
        let backend_shared_secret = required("LATE_BASTION_SHARED_SECRET")?;
        let max_conns_global = required_parse("LATE_BASTION_MAX_CONNS_GLOBAL")?;
        let proxy_protocol = required_bool("LATE_BASTION_PROXY_PROTOCOL")?;
        let proxy_trusted_cidrs = parse_cidrs(&required("LATE_BASTION_PROXY_TRUSTED_CIDRS")?)?;

        Ok(Config {
            ssh_port,
            host_key_path,
            ssh_idle_timeout,
            backend_tunnel_url,
            backend_shared_secret,
            max_conns_global,
            proxy_protocol,
            proxy_trusted_cidrs,
        })
    }

    pub fn log_startup(&self) {
        tracing::info!(
            ssh_port = self.ssh_port,
            host_key_path = %self.host_key_path.display(),
            idle_timeout_secs = self.ssh_idle_timeout,
            "bastion: SSH listener config",
        );
        tracing::info!(
            backend_tunnel_url = %self.backend_tunnel_url,
            "bastion: backend /tunnel target",
        );
        tracing::info!(
            max_global = self.max_conns_global,
            proxy_protocol = self.proxy_protocol,
            trusted_cidrs = ?self.proxy_trusted_cidrs,
            "bastion: limits and proxy config",
        );
    }
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

fn parse_cidrs(raw: &str) -> anyhow::Result<Vec<IpNet>> {
    raw.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| {
            s.parse::<IpNet>()
                .with_context(|| format!("invalid CIDR '{s}'"))
        })
        .collect()
}
