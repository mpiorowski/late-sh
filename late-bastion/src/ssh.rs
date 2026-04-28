//! Minimal russh server skeleton for `late-bastion`.
//!
//! Phase 1 scope: load/generate host key, listen on the configured port,
//! accept SSH connections, log channel events. **No proxy logic yet** —
//! shell requests are answered with a stub message and a clean close.
//!
//! Subsequent phases (per `PERSISTENT-CONNECTION-GATEWAY.md` §10) will:
//!   - Phase 3: dial late-ssh `/tunnel`, byte-pump bidirectionally, forward
//!     `window-change` requests as WS `resize` text frames.
//!   - Phase 4: detect WS close codes, draw plain-text "reconnecting…"
//!     message into the SSH channel, redial with backoff.

use anyhow::Result;
use getrandom::SysRng;
use russh::keys::{PrivateKey, signature::rand_core::UnwrapErr};
use russh::server::{Auth, Msg, Session};
use russh::{Channel, ChannelId};
#[cfg(unix)]
use std::fs::Permissions;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::OwnedSemaphorePermit;
use tokio::task::JoinSet;

use crate::config::Config;

const STUB_BANNER: &str = "\r\n  late-bastion stub: tunnel not yet wired (Phase 3).\r\n\r\n";

/// Server-wide state shared across accepted connections.
#[derive(Clone)]
pub struct Server {
    config: Arc<Config>,
    conn_limit: Arc<tokio::sync::Semaphore>,
}

impl Server {
    pub fn new(config: Arc<Config>) -> Self {
        let conn_limit = Arc::new(tokio::sync::Semaphore::new(config.max_conns_global));
        Server { config, conn_limit }
    }
}

/// Per-connection handler. Bastion is intentionally minimal: no DB lookup,
/// no per-IP enforcement, no protocol-aware logic. Late-ssh handles all of
/// that downstream of the WS handshake.
pub struct ClientHandler {
    fingerprint: Option<String>,
    over_limit: bool,
    _permit: Option<OwnedSemaphorePermit>,
    channel: Option<Channel<Msg>>,
}

impl russh::server::Server for Server {
    type Handler = ClientHandler;

    fn new_client(&mut self, _peer_addr: Option<std::net::SocketAddr>) -> ClientHandler {
        let permit = self.conn_limit.clone().try_acquire_owned().ok();
        let over_limit = permit.is_none();
        if over_limit {
            tracing::warn!(
                cap = self.config.max_conns_global,
                "global connection cap reached; rejecting new client"
            );
        }
        ClientHandler {
            fingerprint: None,
            over_limit,
            _permit: permit,
            channel: None,
        }
    }
}

impl russh::server::Handler for ClientHandler {
    type Error = anyhow::Error;

    async fn auth_publickey(
        &mut self,
        user: &str,
        key: &russh::keys::PublicKey,
    ) -> Result<Auth, Self::Error> {
        if self.over_limit {
            return Ok(Auth::reject());
        }
        let fingerprint = key.fingerprint(russh::keys::HashAlg::Sha256).to_string();
        tracing::info!(user, %fingerprint, "auth_publickey accepted (stub)");
        self.fingerprint = Some(fingerprint);
        Ok(Auth::Accept)
    }

    async fn channel_open_session(
        &mut self,
        channel: Channel<Msg>,
        _session: &mut Session,
    ) -> Result<bool, Self::Error> {
        if self.over_limit {
            return Ok(false);
        }
        tracing::debug!(channel_id = ?channel.id(), "session channel opened");
        self.channel = Some(channel);
        Ok(true)
    }

    async fn pty_request(
        &mut self,
        _channel: ChannelId,
        term: &str,
        col_width: u32,
        row_height: u32,
        _pix_width: u32,
        _pix_height: u32,
        _modes: &[(russh::Pty, u32)],
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        tracing::info!(term, col_width, row_height, "pty_request");
        Ok(())
    }

    async fn shell_request(
        &mut self,
        channel: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        tracing::info!(?channel, "shell_request — writing stub banner and closing");
        if let Err(e) = session.channel_success(channel) {
            tracing::warn!(error = ?e, "channel_success failed");
        }
        let handle = session.handle();
        let _ = handle.data(channel, STUB_BANNER.as_bytes().to_vec()).await;
        // Close the channel cleanly. EOF first so the client flushes.
        let _ = handle.eof(channel).await;
        let _ = handle.close(channel).await;
        Ok(())
    }

    async fn window_change_request(
        &mut self,
        _channel: ChannelId,
        col_width: u32,
        row_height: u32,
        _pix_width: u32,
        _pix_height: u32,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        tracing::debug!(col_width, row_height, "window_change_request");
        Ok(())
    }

    async fn data(
        &mut self,
        _channel: ChannelId,
        _data: &[u8],
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        // Stub: discard input. Phase 3 forwards into the WS binary frame stream.
        Ok(())
    }
}

pub fn load_or_generate_key(path: &std::path::Path) -> Result<PrivateKey> {
    use russh::keys::ssh_key::LineEnding;

    if path.exists() {
        let key = russh::keys::load_secret_key(path, None)?;
        tracing::info!(path = %path.display(), "loaded existing host key");
        Ok(key)
    } else {
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent).ok();
        }
        let key = PrivateKey::random(&mut UnwrapErr(SysRng), russh::keys::Algorithm::Ed25519)?;
        let key_data = key.to_openssh(LineEnding::LF)?;
        std::fs::write(path, key_data.as_bytes())?;
        #[cfg(unix)]
        if let Err(e) = std::fs::set_permissions(path, Permissions::from_mode(0o600)) {
            tracing::warn!(path = %path.display(), error = ?e, "failed to set permissions on host key");
        }
        tracing::info!(path = %path.display(), "generated new host key");
        Ok(key)
    }
}

pub async fn run(
    config: Arc<Config>,
    shutdown: late_core::shutdown::CancellationToken,
) -> Result<()> {
    let listener = TcpListener::bind(("0.0.0.0", config.ssh_port)).await?;
    let host_key = load_or_generate_key(&config.host_key_path)?;
    let russh_config = Arc::new(russh::server::Config {
        inactivity_timeout: Some(Duration::from_secs(config.ssh_idle_timeout)),
        auth_rejection_time: Duration::from_secs(3),
        keys: vec![host_key],
        window_size: 8 * 1024 * 1024,
        event_buffer_size: 128,
        nodelay: true,
        keepalive_interval: Some(Duration::from_secs(30)),
        keepalive_max: 3,
        ..Default::default()
    });

    let server = Server::new(config.clone());
    let addr = listener.local_addr()?;
    tracing::info!(address = %addr, "bastion ssh server listening");

    let mut tasks: JoinSet<()> = JoinSet::new();

    loop {
        tokio::select! {
            accept_result = listener.accept() => {
                let (tcp, peer_addr) = match accept_result {
                    Ok(pair) => pair,
                    Err(e) => {
                        tracing::warn!(error = ?e, "accept failed");
                        continue;
                    }
                };
                if russh_config.nodelay
                    && let Err(e) = tcp.set_nodelay(true)
                {
                    tracing::warn!(error = ?e, "set_nodelay failed");
                }
                let russh_config = Arc::clone(&russh_config);
                let mut server = server.clone();
                tasks.spawn(async move {
                    let handler = russh::server::Server::new_client(&mut server, Some(peer_addr));
                    match russh::server::run_stream(russh_config, tcp, handler).await {
                        Ok(session) => {
                            if let Err(e) = session.await {
                                tracing::debug!(error = ?e, ?peer_addr, "ssh session ended with error");
                            }
                        }
                        Err(e) => {
                            tracing::debug!(error = ?e, ?peer_addr, "ssh session init failed");
                        }
                    }
                });
            }
            _ = shutdown.cancelled() => {
                tracing::info!("bastion shutdown requested, stopping accept loop");
                break;
            }
        }
    }

    drop(listener);

    if !tasks.is_empty() {
        tracing::info!("waiting for active bastion sessions to drain");
        while let Some(join_result) = tasks.join_next().await {
            if let Err(e) = join_result {
                tracing::debug!(error = ?e, "session task failed while draining");
            }
        }
    }

    Ok(())
}
