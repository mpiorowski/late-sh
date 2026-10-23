//! russh server for `late-bastion`.
//!
//! On `pty-req` + `shell`, hand the SSH channel to a per-shell
//! proxy task that dials late-ssh `/tunnel` and pumps bytes between the
//! two transports. The russh callback layer here is responsible only
//! for stashing handshake-relevant state across SSH protocol events
//! (auth → channel open → pty → shell → window-change), not for any
//! protocol-aware byte handling. Bounded pre-shell exec and reconnects
//! reuse this SSH connection; see `devdocs/BASTION-FEATURE.md`.

use anyhow::Result;
use getrandom::SysRng;
use late_core::tunnel_protocol::{SshInputEvent, is_tunnel_env_allowed};
use russh::keys::{PrivateKey, signature::rand_core::UnwrapErr};
use russh::server::{Auth, Msg, Session};
use russh::{Channel, ChannelId, Sig};
#[cfg(unix)]
use std::fs::Permissions;
use std::net::{IpAddr, SocketAddr};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{OwnedSemaphorePermit, mpsc};
use tokio::task::JoinSet;
use uuid::Uuid;

use crate::admission::{IpLimits, IpPermit};
use crate::config::Config;
use crate::handshake::HandshakeContext;
use crate::proxy::{INPUT_QUEUE_CAP, TunnelConnection, run_session};

/// How long to wait for an upstream proxy to finish writing its PROXY v1
/// header. Mirrors the late-ssh `:2222` listener's value (250ms).
pub const PROXY_HEADER_TIMEOUT: Duration = Duration::from_millis(250);
const AUTH_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_ENV_VALUE_BYTES: usize = 4096;

/// Server-wide state shared across accepted connections.
#[derive(Clone)]
pub struct Server {
    config: Arc<Config>,
    conn_limit: Arc<tokio::sync::Semaphore>,
    ip_limits: IpLimits,
    shutdown: late_core::shutdown::CancellationToken,
}

impl Server {
    pub fn new(config: Arc<Config>, shutdown: late_core::shutdown::CancellationToken) -> Self {
        let conn_limit = Arc::new(tokio::sync::Semaphore::new(config.max_conns_global));
        let ip_limits = IpLimits::new(
            config.max_conns_per_ip,
            config.max_attempts_per_ip,
            config.rate_limit_window_secs,
        );
        Server {
            config,
            conn_limit,
            ip_limits,
            shutdown,
        }
    }
}

/// Per-connection handler.
///
/// Bastion has no DB lookup or protocol-aware logic on the byte stream.
/// Transport admission runs before SSH negotiation; late-ssh handles account
/// admission downstream of the WS handshake. The state we
/// hold is just what's needed to compose a `HandshakeContext` once
/// `shell_request` fires — gathered across the russh callbacks (auth,
/// channel-open, pty, window-change) as they arrive.
pub struct ClientHandler {
    config: Arc<Config>,
    shutdown: late_core::shutdown::CancellationToken,
    authenticated: late_core::shutdown::CancellationToken,
    peer_addr: Option<SocketAddr>,
    login_username: Option<String>,
    fingerprint: Option<String>,
    term: Option<String>,
    /// Stable id for this user SSH connection. Reused by pre-shell exec
    /// and the later shell tunnel so the backend can associate them.
    session_id: String,
    /// Latest known PTY dimensions. Updated on `pty_request` and on
    /// every `window_change_request` (a resize CAN fire between the
    /// pty and shell requests).
    cols: u16,
    rows: u16,
    env: Vec<(String, String)>,
    over_limit: bool,
    _ip_permit: Option<IpPermit>,
    _permit: Option<OwnedSemaphorePermit>,
    channel: Option<Channel<Msg>>,
    /// Sender feeding `proxy::run_session`'s ordered input-event
    /// mpsc. Both `Handler::data` (Bytes) and
    /// `Handler::window_change_request` (Resize) push here so the
    /// proxy sees them in russh's dispatch order — not muxed by a
    /// `tokio::select!` between two queues. Stays `None` until
    /// `shell_request` spawns the proxy task.
    input_tx: Option<mpsc::Sender<SshInputEvent>>,
    /// Backend `/tunnel` WebSocket for this user SSH connection before
    /// the interactive shell has started. A late-cli exec can initialize
    /// over it, and the later shell setup reuses the same socket.
    tunnel: Option<TunnelConnection>,
    /// Best-effort label for stdin bytes sent on an exec channel. MVP execs do
    /// not support stdin, but rogue clients may still send data.
    active_exec_command: Option<String>,
}

impl Server {
    /// Build a [`ClientHandler`] given both the transport peer address
    /// and the optional proxied address (resolved via PROXY v1 by the
    /// accept loop). The handler's effective `peer_addr` — the value
    /// the bastion forwards as `X-Late-Peer-IP` — is the proxied
    /// address when present and trusted, falling back to the transport
    /// peer otherwise.
    pub fn new_client_with_addrs(
        &self,
        transport_peer_addr: Option<SocketAddr>,
        proxied_peer_addr: Option<SocketAddr>,
    ) -> ClientHandler {
        self.new_client_with_permit(
            transport_peer_addr,
            proxied_peer_addr,
            self.try_global_permit(),
        )
    }

    fn try_global_permit(&self) -> Option<OwnedSemaphorePermit> {
        let permit = self.conn_limit.clone().try_acquire_owned().ok();
        if permit.is_none() {
            tracing::warn!(
                cap = self.config.max_conns_global,
                "global connection cap reached; rejecting new client"
            );
        }
        permit
    }

    fn new_client_with_permit(
        &self,
        transport_peer_addr: Option<SocketAddr>,
        proxied_peer_addr: Option<SocketAddr>,
        permit: Option<OwnedSemaphorePermit>,
    ) -> ClientHandler {
        let peer_addr = proxied_peer_addr.or(transport_peer_addr);
        let ip_permit = peer_addr.filter(|_| permit.is_some()).and_then(|peer| {
            match self.ip_limits.admit(peer.ip()) {
                Ok(permit) => Some(permit),
                Err(reason) => {
                    tracing::warn!(ip = %peer.ip(), ?reason, "bastion pre-auth admission rejected");
                    None
                }
            }
        });
        let over_limit = permit.is_none() || ip_permit.is_none();
        ClientHandler {
            config: self.config.clone(),
            shutdown: self.shutdown.child_token(),
            authenticated: late_core::shutdown::CancellationToken::new(),
            peer_addr,
            login_username: None,
            fingerprint: None,
            term: None,
            session_id: Uuid::now_v7().to_string(),
            cols: 0,
            rows: 0,
            env: Vec::new(),
            over_limit,
            _ip_permit: ip_permit,
            _permit: permit,
            channel: None,
            input_tx: None,
            tunnel: None,
            active_exec_command: None,
        }
    }
}

impl ClientHandler {
    fn handshake_context(&self) -> Option<HandshakeContext> {
        let fingerprint = self.fingerprint.clone()?;
        let peer_addr = self.peer_addr?;
        Some(HandshakeContext {
            fingerprint,
            username: self.login_username.clone().unwrap_or_default(),
            peer_ip: peer_addr.ip(),
            term: self.term.clone().unwrap_or_else(|| "xterm-256color".into()),
            cols: self.cols,
            rows: self.rows,
            reconnect_reason: None,
            session_id: self.session_id.clone(),
            env: self.env.clone(),
        })
    }
}

impl Drop for ClientHandler {
    fn drop(&mut self) {
        // A TCP disconnect must also stop a proxy that is dialing or blocked
        // on transport output, rather than leaving it alive until a retry.
        self.shutdown.cancel();
    }
}

impl russh::server::Server for Server {
    type Handler = ClientHandler;

    /// Trait entrypoint used when no PROXY v1 resolution is happening
    /// in front of us (e.g. tests that don't go through `run`). Treats
    /// the transport peer as the effective peer.
    fn new_client(&mut self, peer_addr: Option<SocketAddr>) -> ClientHandler {
        self.new_client_with_addrs(peer_addr, None)
    }
}

/// Whether the bastion will read a PROXY v1 header from a connection
/// originating at `ip`. Pure-logic; used by the accept loop.
pub fn is_trusted_proxy_peer(config: &Config, ip: IpAddr) -> bool {
    config
        .proxy_trusted_cidrs
        .iter()
        .any(|cidr| cidr.contains(&ip))
}

/// If PROXY v1 parsing is enabled and the transport peer is in the
/// trusted CIDR list, read the PROXY v1 header off the front of the
/// stream and return the asserted source address. Otherwise return
/// `Ok(None)`.
///
/// Errors here are fatal for the connection: a misconfigured upstream
/// or a malformed header is far more likely than a benign client that
/// happens to mimic the proxy header, so dropping the connection is
/// the safe call. `PROXY UNKNOWN` is also rejected: falling back to a
/// trusted proxy's transport address would attribute every user to it.
pub async fn resolve_proxied_client_addr(
    config: &Config,
    stream: &mut TcpStream,
    transport_peer_addr: SocketAddr,
) -> Result<Option<SocketAddr>> {
    if !config.proxy_protocol {
        return Ok(None);
    }
    if !is_trusted_proxy_peer(config, transport_peer_addr.ip()) {
        return Ok(None);
    }
    let client_addr =
        late_core::proxy_protocol::read_proxy_v1_client_addr(stream, PROXY_HEADER_TIMEOUT)
            .await?
            .ok_or_else(|| anyhow::anyhow!("trusted proxy did not provide a client address"))?;
    Ok(Some(client_addr))
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
        self.authenticated.cancel();
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
        if self.input_tx.is_some() || self.channel.is_some() {
            tracing::warn!(
                channel_id = ?channel.id(),
                shell_active = self.input_tx.is_some(),
                pending_channel = self.channel.is_some(),
                "rejecting extra session channel"
            );
            return Ok(false);
        }
        tracing::debug!(channel_id = ?channel.id(), "session channel opened");
        self.channel = Some(channel);
        Ok(true)
    }

    async fn pty_request(
        &mut self,
        channel: ChannelId,
        term: &str,
        col_width: u32,
        row_height: u32,
        _pix_width: u32,
        _pix_height: u32,
        _modes: &[(russh::Pty, u32)],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        tracing::info!(term, col_width, row_height, "pty_request");
        self.term = Some(term.to_string());
        self.cols = col_width.try_into().unwrap_or(u16::MAX);
        self.rows = row_height.try_into().unwrap_or(u16::MAX);
        if let Err(e) = session.channel_success(channel) {
            tracing::warn!(error = ?e, "pty channel_success failed");
        }
        Ok(())
    }

    async fn x11_request(
        &mut self,
        channel: ChannelId,
        _single_connection: bool,
        _x11_auth_protocol: &str,
        _x11_auth_cookie: &str,
        _x11_screen_number: u32,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        fail_channel_request(session, channel, "x11");
        Ok(())
    }

    async fn env_request(
        &mut self,
        channel: ChannelId,
        variable_name: &str,
        variable_value: &str,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        if is_tunnel_env_allowed(variable_name) {
            if variable_value.len() > MAX_ENV_VALUE_BYTES {
                fail_channel_request(session, channel, "oversized env");
                return Ok(());
            }
            let variable_name = variable_name.trim();
            // Store one bounded value per whitelisted name. Repeated pre-shell
            // requests must not grow connection memory or replay stale hints.
            if let Some((_, value)) = self.env.iter_mut().find(|(name, _)| name == variable_name) {
                *value = variable_value.to_string();
            } else {
                self.env
                    .push((variable_name.to_string(), variable_value.to_string()));
            }
            tracing::debug!(variable_name, "stored whitelisted env request");
        } else {
            tracing::debug!(variable_name, "ignoring non-whitelisted env request");
        }
        if let Err(e) = session.channel_success(channel) {
            tracing::warn!(error = ?e, variable_name, "env channel_success failed");
        }
        Ok(())
    }

    async fn exec_request(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        let command = String::from_utf8_lossy(data).trim().to_string();
        if command.is_empty() {
            fail_channel_request(session, channel, "exec");
            return Ok(());
        }
        if self.input_tx.is_some() {
            tracing::warn!(
                ?channel,
                command = %command,
                "exec_request after shell tunnel is active; closing channel"
            );
            fail_channel_request(session, channel, "exec");
            let _ = session.handle().close(channel).await;
            return Ok(());
        }
        self.active_exec_command = Some(command.clone());
        if let Err(e) = session.channel_success(channel) {
            tracing::warn!(error = ?e, ?channel, "exec channel_success failed");
        }

        let Some(chan) = self.channel.take() else {
            tracing::warn!(?channel, "exec_request without an open channel");
            let _ = session.handle().close(channel).await;
            return Ok(());
        };
        let response = match self.handshake_context() {
            Some(ctx) => {
                let mut connect_failure = None;
                if let Some(tunnel) = self.tunnel.as_mut() {
                    tunnel.update_context(ctx.clone());
                } else {
                    match TunnelConnection::connect(
                        self.config.backend_tunnel_url.clone(),
                        self.config.backend_shared_secret.clone(),
                        ctx,
                        self.shutdown.clone(),
                    )
                    .await
                    {
                        Ok(tunnel) => {
                            self.tunnel = Some(tunnel);
                        }
                        Err(err) => {
                            tracing::warn!(error = ?err, command = %command, "tunnel connect for exec failed");
                            connect_failure = Some(return_exec_failure("tunnel exec failed"));
                        }
                    }
                }

                if let Some(response) = connect_failure {
                    response
                } else {
                    match self.tunnel.as_mut() {
                        Some(tunnel) => {
                            match tunnel.exec(command.clone(), self.shutdown.clone()).await {
                                Ok(response) => response,
                                Err(err) => {
                                    tracing::warn!(error = ?err, command = %command, "tunnel exec failed");
                                    self.tunnel = None;
                                    return_exec_failure("tunnel exec failed")
                                }
                            }
                        }
                        None => return_exec_failure("tunnel exec failed"),
                    }
                }
            }
            None => {
                tracing::warn!(?channel, "exec_request before auth/peer metadata");
                return_exec_failure("exec request missing authenticated session metadata")
            }
        };

        if !response.stdout.is_empty() {
            chan.data(response.stdout.as_bytes()).await?;
        }
        if !response.stderr.is_empty() {
            chan.extended_data(1, response.stderr.as_bytes()).await?;
        }
        let _ = chan.exit_status(response.exit_status).await;
        let _ = chan.eof().await;
        let _ = chan.close().await;
        self.active_exec_command = None;
        Ok(())
    }

    async fn subsystem_request(
        &mut self,
        channel: ChannelId,
        _name: &str,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        fail_channel_request(session, channel, "subsystem");
        Ok(())
    }

    async fn shell_request(
        &mut self,
        channel_id: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        if self.input_tx.is_some() {
            tracing::warn!(
                ?channel_id,
                "duplicate shell_request while shell tunnel is active"
            );
            fail_channel_request(session, channel_id, "shell");
            let _ = session.handle().close(channel_id).await;
            return Ok(());
        }
        if let Err(e) = session.channel_success(channel_id) {
            tracing::warn!(error = ?e, "channel_success failed");
        }

        let Some(channel) = self.channel.take() else {
            tracing::warn!(?channel_id, "shell_request without an open channel");
            let _ = session.handle().close(channel_id).await;
            return Ok(());
        };
        let Some(ctx) = self.handshake_context() else {
            tracing::warn!(?channel_id, "shell_request before auth/peer metadata");
            let _ = session.handle().close(channel_id).await;
            return Ok(());
        };

        let (input_tx, input_rx) = mpsc::channel::<SshInputEvent>(INPUT_QUEUE_CAP);
        self.input_tx = Some(input_tx);

        let ws_url = self.config.backend_tunnel_url.clone();
        let secret = self.config.backend_shared_secret.clone();
        let handle = session.handle();
        let session_log_id = late_core::tunnel_protocol::session_log_id(&ctx.session_id);
        let shutdown = self.shutdown.clone();
        let mut existing_tunnel = self.tunnel.take();
        if let Some(tunnel) = existing_tunnel.as_mut() {
            tunnel.update_context(ctx.clone());
        }

        tracing::info!(
            ?channel_id,
            %session_log_id,
            username = %ctx.username,
            fingerprint = %ctx.fingerprint,
            peer_ip = %ctx.peer_ip,
            cols = ctx.cols,
            rows = ctx.rows,
            reused_tunnel = existing_tunnel.is_some(),
            "shell_request — spawning tunnel proxy"
        );

        tokio::spawn(async move {
            let result = match existing_tunnel {
                Some(tunnel) => tunnel.run_shell(channel, input_rx, shutdown).await,
                None => run_session(channel, ws_url, secret, ctx, input_rx, shutdown).await,
            };
            if let Err(e) = result {
                tracing::warn!(error = ?e, %session_log_id, "tunnel proxy session failed");
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

    async fn signal(
        &mut self,
        channel: ChannelId,
        _signal: Sig,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        fail_channel_request(session, channel, "signal");
        Ok(())
    }

    async fn tcpip_forward(
        &mut self,
        address: &str,
        port: &mut u32,
        _session: &mut Session,
    ) -> Result<bool, Self::Error> {
        tracing::debug!(address, port = *port, "tcpip-forward rejected");
        Ok(false)
    }

    async fn cancel_tcpip_forward(
        &mut self,
        address: &str,
        port: u32,
        _session: &mut Session,
    ) -> Result<bool, Self::Error> {
        tracing::debug!(address, port, "cancel-tcpip-forward rejected");
        Ok(false)
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

        if let Some(tx) = &self.input_tx {
            // Preserve byte/resize ordering under backpressure. A stalled
            // consumer closes the connection rather than losing a resize.
            tokio::select! {
                result = tokio::time::timeout(Duration::from_secs(2), tx.send(SshInputEvent::Resize { cols, rows })) => {
                    result.map_err(|_| anyhow::anyhow!("resize input stalled"))??;
                }
                _ = self.shutdown.cancelled() => {},
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
        data: &[u8],
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        // Push inbound PTY bytes onto the same FIFO that
        // window_change_request feeds, so the proxy task emits WS
        // Binary and Text frames in russh's dispatch order. Bypassing
        // `Channel::into_stream`'s read half (which we drop in
        // run_session) is what makes the ordering durable: that
        // queue and resize_tx used to be muxed by `tokio::select!`,
        // which has no fairness guarantee.
        let Some(tx) = &self.input_tx else {
            if let Some(command) = self.active_exec_command.as_ref() {
                tracing::warn!(
                    bytes = data.len(),
                    command = %command,
                    "discarding stdin bytes on exec"
                );
            } else {
                tracing::debug!(
                    bytes = data.len(),
                    "discarding ssh data before shell tunnel is active"
                );
            }
            return Ok(());
        };
        tokio::select! {
            result = tokio::time::timeout(Duration::from_secs(2), tx.send(SshInputEvent::Bytes(data.to_vec()))) => {
                result.map_err(|_| anyhow::anyhow!("byte input stalled"))??;
            }
            _ = self.shutdown.cancelled() => {},
        }
        Ok(())
    }

    async fn channel_eof(
        &mut self,
        _channel: ChannelId,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        // Drop the input sender so the proxy's input_rx.recv()
        // returns None and the session loop ends as SshClosed.
        self.input_tx = None;
        Ok(())
    }

    async fn channel_close(
        &mut self,
        _channel: ChannelId,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.input_tx = None;
        Ok(())
    }
}

fn return_exec_failure(stderr: impl Into<String>) -> crate::proxy::ExecTunnelResponse {
    crate::proxy::ExecTunnelResponse::failure(stderr)
}

fn fail_channel_request(session: &mut Session, channel: ChannelId, request: &'static str) {
    tracing::debug!(?channel, request, "unsupported channel request rejected");
    if let Err(e) = session.channel_failure(channel) {
        tracing::debug!(error = ?e, ?channel, request, "channel_failure failed");
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
    run_with_listener(listener, config, shutdown).await
}

/// Run the production accept loop on an already-bound listener. Tests use
/// ephemeral ports without replacing admission or shutdown handling.
pub async fn run_with_listener(
    listener: TcpListener,
    config: Arc<Config>,
    shutdown: late_core::shutdown::CancellationToken,
) -> Result<()> {
    let host_key = load_or_generate_key(&config.host_key_path)?;
    let russh_config = Arc::new(russh::server::Config {
        inactivity_timeout: Some(Duration::from_secs(config.ssh_idle_timeout)),
        auth_rejection_time: Duration::from_secs(3),
        // Don't penalize the client's initial `none`-auth probe; only delay
        // repeated failures. Without this, every connection waits 3s before
        // the real publickey auth is even attempted.
        auth_rejection_time_initial: Some(Duration::ZERO),
        keys: vec![host_key],
        window_size: 8 * 1024 * 1024,
        event_buffer_size: 128,
        nodelay: true,
        keepalive_interval: Some(Duration::from_secs(30)),
        keepalive_max: 3,
        ..Default::default()
    });

    let server = Server::new(config.clone(), shutdown.clone());
    let addr = listener.local_addr()?;
    tracing::info!(address = %addr, "bastion ssh server listening");

    if config.proxy_protocol && config.proxy_trusted_cidrs.is_empty() {
        tracing::warn!(
            "bastion proxy protocol is enabled but LATE_BASTION_PROXY_TRUSTED_CIDRS is empty; \
             proxy headers will be rejected"
        );
    }

    let mut tasks: JoinSet<()> = JoinSet::new();
    let mut cleanup = tokio::time::interval(Duration::from_secs(60));

    loop {
        tokio::select! {
            accept_result = listener.accept() => {
                let (mut tcp, transport_peer_addr) = match accept_result {
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
                let server = server.clone();
                let config = Arc::clone(&config);
                // Bound even clients that never finish their PROXY header.
                let Some(permit) = server.try_global_permit() else { continue; };
                tasks.spawn(async move {
                    let proxied_addr =
                        match resolve_proxied_client_addr(&config, &mut tcp, transport_peer_addr).await {
                            Ok(addr) => addr,
                            Err(err) => {
                                tracing::warn!(
                                    ?transport_peer_addr,
                                    error = ?err,
                                    "failed to resolve proxy protocol header; dropping connection"
                                );
                                return;
                            }
                        };
                    let handler = server.new_client_with_permit(Some(transport_peer_addr), proxied_addr, Some(permit));
                    // Reject before russh allocates handshake state or sends its banner.
                    if handler.over_limit { return; }
                    let authenticated = handler.authenticated.clone();
                    let connection_shutdown = handler.shutdown.clone();
                    let auth_deadline = tokio::time::Instant::now() + AUTH_TIMEOUT;
                    let mut session = tokio::select! {
                        result = russh::server::run_stream(russh_config, tcp, handler) => match result {
                            Ok(session) => session,
                            Err(err) => { tracing::debug!(?err, "ssh session init failed"); return; }
                        },
                        _ = connection_shutdown.cancelled() => return,
                        _ = tokio::time::sleep_until(auth_deadline) => return,
                    };
                    let handle = session.handle();
                    tokio::select! {
                        result = &mut session => { if let Err(err) = result { tracing::debug!(?err, "ssh session ended"); } return; },
                        _ = authenticated.cancelled() => {},
                        _ = connection_shutdown.cancelled() => {},
                        _ = tokio::time::sleep_until(auth_deadline) => { tracing::debug!("ssh authentication timed out"); },
                    }
                    if authenticated.is_cancelled() && !connection_shutdown.is_cancelled() {
                        tokio::select! {
                            _ = &mut session => return,
                            _ = connection_shutdown.cancelled() => {},
                        }
                    }
                    connection_shutdown.cancel();
                    let _ = tokio::time::timeout(Duration::from_secs(1), handle.disconnect(russh::Disconnect::ByApplication, "bastion connection ended".into(), "".into())).await;
                    let _ = tokio::time::timeout(Duration::from_secs(1), session).await;
                });
            }
            _ = cleanup.tick() => server.ip_limits.cleanup(),
            result = tasks.join_next(), if !tasks.is_empty() => {
                if let Some(Err(err)) = result { tracing::debug!(?err, "bastion client task failed"); }
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

#[cfg(test)]
mod tests {
    use super::*;
    use ipnet::IpNet;
    use std::path::PathBuf;

    fn config_with(proxy_protocol: bool, cidrs: &[&str]) -> Config {
        Config {
            ssh_port: 0,
            host_key_path: PathBuf::from("/tmp/unused"),
            ssh_idle_timeout: 60,
            backend_tunnel_url: "ws://localhost:0/tunnel".to_string(),
            backend_shared_secret: "x".to_string(),
            max_conns_global: 1,
            max_conns_per_ip: 3,
            max_attempts_per_ip: 30,
            rate_limit_window_secs: 60,
            proxy_protocol,
            proxy_trusted_cidrs: cidrs
                .iter()
                .map(|s| s.parse::<IpNet>().expect("cidr"))
                .collect(),
        }
    }

    #[test]
    fn untrusted_peer_when_cidr_list_empty() {
        let cfg = config_with(true, &[]);
        assert!(!is_trusted_proxy_peer(
            &cfg,
            "10.0.0.1".parse::<IpAddr>().unwrap()
        ));
    }

    #[test]
    fn trusted_peer_inside_cidr() {
        let cfg = config_with(true, &["10.42.0.0/16"]);
        assert!(is_trusted_proxy_peer(
            &cfg,
            "10.42.7.5".parse::<IpAddr>().unwrap()
        ));
    }

    #[test]
    fn untrusted_peer_outside_cidr() {
        let cfg = config_with(true, &["10.42.0.0/16"]);
        assert!(!is_trusted_proxy_peer(
            &cfg,
            "192.0.2.5".parse::<IpAddr>().unwrap()
        ));
    }

    #[test]
    fn ipv6_cidr_match() {
        let cfg = config_with(true, &["2001:db8::/32"]);
        assert!(is_trusted_proxy_peer(
            &cfg,
            "2001:db8::1".parse::<IpAddr>().unwrap()
        ));
        assert!(!is_trusted_proxy_peer(
            &cfg,
            "2001:dead::1".parse::<IpAddr>().unwrap()
        ));
    }

    #[tokio::test]
    async fn client_ip_untrusted_proxy_header_is_not_consumed() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let config = config_with(true, &["10.42.0.0/16"]);
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mut client = TcpStream::connect(listener.local_addr().unwrap())
            .await
            .unwrap();
        let header = b"PROXY TCP4 203.0.113.99 127.0.0.1 12345 5222\r\n";
        client.write_all(header).await.unwrap();
        let (mut stream, transport_peer) = listener.accept().await.unwrap();
        assert_eq!(
            resolve_proxied_client_addr(&config, &mut stream, transport_peer)
                .await
                .unwrap(),
            None
        );
        let mut remaining = vec![0; header.len()];
        tokio::time::timeout(Duration::from_secs(2), stream.read_exact(&mut remaining))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(remaining, header);
    }
}
