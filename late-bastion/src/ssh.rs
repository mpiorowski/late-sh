//! russh server for `late-bastion`.
//!
//! Phase 3: on `pty-req` + `shell`, hand the SSH channel to a per-shell
//! proxy task that dials late-ssh `/tunnel` and pumps bytes between the
//! two transports. The russh callback layer here is responsible only
//! for stashing handshake-relevant state across SSH protocol events
//! (auth → channel open → pty → shell → window-change), not for any
//! protocol-aware byte handling.
//!
//! Subsequent phase (per `PERSISTENT-CONNECTION-GATEWAY.md` §10):
//!   - Phase 4: detect WS close codes, draw plain-text "reconnecting…"
//!     message into the SSH channel, redial with backoff.

use anyhow::Result;
use getrandom::SysRng;
use russh::keys::{PrivateKey, signature::rand_core::UnwrapErr};
use russh::server::{Auth, Msg, Session};
use russh::{Channel, ChannelId};
#[cfg(unix)]
use std::fs::Permissions;
use std::net::SocketAddr;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::{OwnedSemaphorePermit, mpsc};
use tokio::task::JoinSet;
use uuid::Uuid;

use crate::config::Config;
use crate::handshake::HandshakeContext;
use crate::proxy::{RESIZE_QUEUE_CAP, run_session};

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

/// Per-connection handler.
///
/// Bastion is intentionally minimal: no DB lookup, no per-IP
/// enforcement, no protocol-aware logic on the byte stream. Late-ssh
/// handles all of that downstream of the WS handshake. The state we
/// hold is just what's needed to compose a `HandshakeContext` once
/// `shell_request` fires — gathered across the russh callbacks (auth,
/// channel-open, pty, window-change) as they arrive.
pub struct ClientHandler {
    config: Arc<Config>,
    peer_addr: Option<SocketAddr>,
    login_username: Option<String>,
    fingerprint: Option<String>,
    term: Option<String>,
    /// Latest known PTY dimensions. Updated on `pty_request` and on
    /// every `window_change_request` (a resize CAN fire between the
    /// pty and shell requests).
    cols: u16,
    rows: u16,
    over_limit: bool,
    _permit: Option<OwnedSemaphorePermit>,
    channel: Option<Channel<Msg>>,
    /// Sender feeding `proxy::run_session`'s resize-event mpsc. Stays
    /// `None` until `shell_request` spawns the proxy task.
    resize_tx: Option<mpsc::Sender<(u16, u16)>>,
}

impl russh::server::Server for Server {
    type Handler = ClientHandler;

    fn new_client(&mut self, peer_addr: Option<SocketAddr>) -> ClientHandler {
        let permit = self.conn_limit.clone().try_acquire_owned().ok();
        let over_limit = permit.is_none();
        if over_limit {
            tracing::warn!(
                cap = self.config.max_conns_global,
                "global connection cap reached; rejecting new client"
            );
        }
        ClientHandler {
            config: self.config.clone(),
            peer_addr,
            login_username: None,
            fingerprint: None,
            term: None,
            cols: 0,
            rows: 0,
            over_limit,
            _permit: permit,
            channel: None,
            resize_tx: None,
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
        tracing::info!(user, %fingerprint, "auth_publickey accepted");
        self.login_username = Some(user.to_string());
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
        self.term = Some(term.to_string());
        self.cols = col_width.try_into().unwrap_or(u16::MAX);
        self.rows = row_height.try_into().unwrap_or(u16::MAX);
        Ok(())
    }

    async fn shell_request(
        &mut self,
        channel_id: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        if let Err(e) = session.channel_success(channel_id) {
            tracing::warn!(error = ?e, "channel_success failed");
        }

        let Some(channel) = self.channel.take() else {
            tracing::warn!(?channel_id, "shell_request without an open channel");
            let _ = session.handle().close(channel_id).await;
            return Ok(());
        };
        let Some(fingerprint) = self.fingerprint.clone() else {
            tracing::warn!(?channel_id, "shell_request before auth_publickey");
            let _ = session.handle().close(channel_id).await;
            return Ok(());
        };
        let username = self.login_username.clone().unwrap_or_default();
        let term = self.term.clone().unwrap_or_else(|| "xterm-256color".into());
        let Some(peer_addr) = self.peer_addr else {
            tracing::warn!(?channel_id, "shell_request without a peer address");
            let _ = session.handle().close(channel_id).await;
            return Ok(());
        };

        let ctx = HandshakeContext {
            fingerprint,
            username,
            peer_ip: peer_addr.ip(),
            term,
            cols: self.cols,
            rows: self.rows,
            reconnect: false,
            session_id: Uuid::now_v7().to_string(),
        };

        let (resize_tx, resize_rx) = mpsc::channel::<(u16, u16)>(RESIZE_QUEUE_CAP);
        self.resize_tx = Some(resize_tx);

        let ws_url = self.config.backend_tunnel_url.clone();
        let secret = self.config.backend_shared_secret.clone();
        let handle = session.handle();
        let session_id = ctx.session_id.clone();

        tracing::info!(
            ?channel_id,
            session_id = %ctx.session_id,
            username = %ctx.username,
            fingerprint = %ctx.fingerprint,
            peer_ip = %ctx.peer_ip,
            cols = ctx.cols,
            rows = ctx.rows,
            "shell_request — spawning tunnel proxy"
        );

        tokio::spawn(async move {
            if let Err(e) = run_session(channel, ws_url, secret, ctx, resize_rx).await {
                tracing::warn!(error = ?e, session_id = %session_id, "tunnel proxy session failed");
            }
            // Either path (Ok or Err): drop the SSH channel by closing
            // it from the russh-handle side too, in case dropping the
            // ChannelStream wasn't enough to flush the close to the
            // user's client (russh holds an internal sender keyed on
            // ChannelId).
            let _ = handle.eof(channel_id).await;
            let _ = handle.close(channel_id).await;
        });

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
        let cols: u16 = col_width.try_into().unwrap_or(u16::MAX);
        let rows: u16 = row_height.try_into().unwrap_or(u16::MAX);
        self.cols = cols;
        self.rows = rows;

        if let Some(tx) = &self.resize_tx {
            // try_send: russh awaits this callback and we don't want
            // to backpressure the russh task on a slow consumer. Cap=4
            // is plenty for human-driven resize events; on overflow we
            // drop the stale event and let the next one win.
            if let Err(e) = tx.try_send((cols, rows)) {
                tracing::debug!(error = ?e, cols, rows, "resize event dropped");
            }
        } else {
            // Resize landed before shell_request started the proxy.
            // The latest values are already stashed in self.{cols,rows}
            // and will be picked up by build_request when shell_request
            // fires.
            tracing::debug!(cols, rows, "window_change before shell_request — stashed");
        }
        Ok(())
    }

    async fn data(
        &mut self,
        _channel: ChannelId,
        _data: &[u8],
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        // Intentional no-op. russh delivers each ChannelMsg::Data to
        // both the handler callback AND the per-channel queue read by
        // `Channel::into_stream` — the proxy task consumes bytes via
        // the latter, so handling them here would double-process.
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
