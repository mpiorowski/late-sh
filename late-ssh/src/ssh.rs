use anyhow::{Context, Result};
use getrandom::SysRng;
use late_core::MutexRecover;
use late_core::models::{
    artboard_ban::ArtboardBan,
    article_read::ArticleRead,
    server_ban::ServerBan,
    user::{User, UserParams, extract_theme_id},
};
use russh::keys::{PrivateKey, signature::rand_core::UnwrapErr};
use russh::server::{Auth, Msg, Session};
use russh::{Channel, ChannelId, MethodKind, MethodSet, keys};
use serde_json::{Value, json};
#[cfg(unix)]
use std::fs::Permissions;
use std::net::{IpAddr, SocketAddr};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Mutex as TokioMutex, OwnedSemaphorePermit};
use tokio::task::JoinSet;
use tokio::time::timeout;

use crate::app::activity::event::ActivityEvent;
use crate::app::files::terminal_image::terminal_name_for_log;
use crate::app::{
    common::theme,
    state::{App, SessionConfig},
};
use crate::authz::Permissions as AuthzPermissions;
use crate::metrics::{self, SshRejectReason};
use crate::proxy_protocol;
use crate::render_signal::RenderSignal;
use crate::session_bootstrap::{ArcadeSessionPreloads, load_arcade_session_preloads};
use crate::state::{ActiveSession, State};
use crate::terminal_probe::{self, PROBE_QUERY, ProbeExpiry, ProbeTally, TerminalProbe, TripTally};
use crate::terminal_size::clamp_terminal_size;
use crate::usernames;

static FRAME_DROP_COUNT: AtomicU64 = AtomicU64::new(0);
use crate::config::PROXY_HEADER_TIMEOUT;
const CLI_MODE_ENV: &str = "LATE_CLI_MODE";
const CLI_TOKEN_PREFIX: &str = "LATE_SESSION_TOKEN=";
const CLI_TOKEN_REQUEST: &str = "late-cli-token-v1";
const AUTH_SETUP_BANNER: &str = "\r\nlate.sh requires SSH public-key auth.\r\n\
New here? Install the companion CLI:\r\n\
  curl -fsSL https://cli.late.sh/install.sh | bash\r\n\
  late\r\n\
Or create a key manually with:\r\n\
  ssh-keygen -t ed25519 -C late.sh\r\n\
  ssh late.sh\r\n";
const EXIT_MESSAGE: &str = "\r\nStay late. Code safe. ✨\r\n";
const INPUT_QUEUE_CAP: usize = 256;

/// Minimum wall-clock gap between any two consecutive renders. Bounds the
/// per-session render rate so that keystroke floods or other signal sources
/// can't drive renders faster than this.
const MIN_RENDER_GAP: Duration = Duration::from_millis(15);
/// Max bytes handed to russh without the client returning window credit
/// before the session counts as stalled and rendering pauses. Must sit well
/// above a normal SSH channel window (~2 MB) plus a burst of full-repaint
/// frames, or healthy-but-bursty sessions would false-positive.
const OUTPUT_BUDGET_BYTES: u64 = 32 * 1024 * 1024;
/// How long a session may stay over [`OUTPUT_BUDGET_BYTES`] before we
/// disconnect it instead of waiting for SSH keepalive to reap it.
const OUTPUT_STALL_DISCONNECT: Duration = Duration::from_secs(30);
/// The window one `late_ssh_session_output_bytes_per_second` sample averages.
const OUTPUT_RATE_WINDOW: Duration = Duration::from_secs(10);
/// Longest a quitting session waits for its in-flight probe to be answered
/// ([`settle_terminal_probe`]). Covers a slow link; a terminal that never
/// answers costs its user this much on quit, only until the probe retires.
const PROBE_SETTLE_MAX: Duration = Duration::from_millis(500);
/// Most queued input messages one render pass hands to the app before it
/// draws what it has ([`drain_input`]).
const INPUT_DRAIN_MAX_MESSAGES: usize = 64;
/// Wall time spent in `App::handle_input` after which one render pass stops
/// draining and draws what it has. Checked between messages, so one slow
/// message (a big paste) can run past it; a key flood then costs its own
/// session a frame of lag instead of holding the tokio worker for the whole
/// queue.
const INPUT_DRAIN_MAX_TIME: Duration = Duration::from_millis(4);
/// A pass that holds the app lock longer than the hot tick is one the user
/// can feel; it is warned about, once per [`SLOW_PASS_WARN_EVERY`].
const SLOW_PASS: Duration = crate::app::tick::HOT_TICK;
const SLOW_PASS_WARN_EVERY: Duration = Duration::from_secs(60);

/// Tracks bytes handed to russh for the app channel versus window credit the
/// client sends back. russh queues writes beyond the client's SSH channel
/// window in an uncapped internal buffer, so a client that stops reading
/// turns the render loop into an unbounded memory sink unless we stop
/// feeding it (2026-07-22 OOM, `infra/RUNBOOK.md` incident log).
struct OutputBudget {
    /// Bytes sent since the last proof that russh's pending queue was empty.
    outstanding: AtomicU64,
}

impl OutputBudget {
    fn new() -> Self {
        Self {
            outstanding: AtomicU64::new(0),
        }
    }

    fn record_sent(&self, bytes: usize) {
        self.outstanding.fetch_add(bytes as u64, Ordering::Relaxed);
    }

    /// Feed from `Handler::window_adjusted`. russh flushes its pending queue
    /// before invoking the handler and only reports `new_size > 0` when that
    /// queue fully drained, so a positive window proves the backlog is empty.
    /// A zero window means the client granted credit but the backlog remains.
    fn on_window_adjusted(&self, new_size: u32) {
        if new_size > 0 {
            self.outstanding.store(0, Ordering::Relaxed);
        }
    }

    fn outstanding(&self) -> u64 {
        self.outstanding.load(Ordering::Relaxed)
    }

    fn over_budget(&self) -> bool {
        self.outstanding() > OUTPUT_BUDGET_BYTES
    }
}

#[derive(Clone)]
struct Server {
    state: State,
}

struct ClientHandler {
    /// Core state
    state: State,
    user: Option<User>,
    is_new_user: bool,
    /// Fingerprint of the key this connection authenticated with. Per-device
    /// settings key off it, so it must be the key actually used rather than
    /// `users.fingerprint` (which is only the account's first key).
    auth_fingerprint: Option<String>,

    /// Connection metadata
    transport_peer_addr: Option<std::net::SocketAddr>,
    peer_addr: Option<std::net::SocketAddr>,
    peer_ip: Option<IpAddr>,
    _conn_permit: OwnedSemaphorePermit,
    per_ip_incremented: bool,
    active_user_incremented: bool,

    /// Activity feed
    activity_feed_rx: Option<tokio::sync::broadcast::Receiver<ActivityEvent>>,

    /// Session bindings
    channel: Option<Channel<Msg>>,
    app_channel_id: Option<ChannelId>,
    app: Option<Arc<TokioMutex<crate::app::state::App>>>,
    /// Signaled by input/resize paths to request an immediate (world-stateless)
    /// render, so typed characters echo without waiting for the next world tick.
    render_signal: Option<Arc<RenderSignal>>,
    output_budget: Arc<OutputBudget>,
    /// Set when the shell starts; the render loop sends its queries and
    /// `data` feeds it the replies (`terminal_probe.rs`).
    terminal_probe: Option<Arc<TerminalProbe>>,
    input_tx: Option<tokio::sync::mpsc::Sender<Vec<u8>>>,
    input_rx: Option<tokio::sync::mpsc::Receiver<Vec<u8>>>,
    cli_mode: bool,
    /// The connection ran the `late-cli-token-v1` exec: the native and
    /// openssh CLI modes identify themselves this way and never send
    /// `LATE_CLI_MODE`.
    cli_token_served: bool,
    /// The resume point a CLI carried back when it moved here, off a draining
    /// pod or on a restart from the quit modal (`app/handoff`), sent as an env
    /// before the pty request.
    resume: Option<crate::app::handoff::state::ResumePoint>,
    /// `TERM` from the pty request, cleaned for the session wire summary.
    term: Option<String>,
    terminal_env_hints: Vec<(String, String)>,
    session_token: Option<String>,
    session_rx: Option<tokio::sync::mpsc::Receiver<crate::session::SessionMessage>>,

    /// Session start marks for `metrics::record_session_start`: the accept,
    /// then the key accepted, then (once the `App` is built) the marks the
    /// render loop reports on its first drawn frame.
    connected_at: Instant,
    authed_at: Option<Instant>,
    session_start: Option<SessionStart>,
}

/// The session's start, carried into the render loop and reported once,
/// on the first frame actually drawn.
struct SessionStart {
    connected_at: Instant,
    authed_at: Instant,
    app_ready_at: Instant,
    user: metrics::SessionUser,
    tenure: metrics::Tenure,
}

impl SessionStart {
    fn record_first_frame(self) {
        let drawn_at = Instant::now();
        let stages = [
            (
                metrics::SessionStartStage::Auth,
                self.authed_at - self.connected_at,
            ),
            (
                metrics::SessionStartStage::Bootstrap,
                self.app_ready_at - self.authed_at,
            ),
            (
                metrics::SessionStartStage::FirstFrame,
                drawn_at - self.app_ready_at,
            ),
            (
                metrics::SessionStartStage::Total,
                drawn_at - self.connected_at,
            ),
        ];
        for (stage, took) in stages {
            metrics::record_session_start(stage, self.user, self.tenure, took.as_secs_f64());
        }
    }
}

pub fn load_or_generate_key(state: &State) -> anyhow::Result<PrivateKey> {
    use russh::keys::ssh_key::LineEnding;
    let path = &state.config.server_key_path;

    if path.exists() {
        let key = russh::keys::load_secret_key(path, None)?;
        tracing::info!(path = %path.display(), "loaded existing server key");
        Ok(key)
    } else {
        let key = PrivateKey::random(&mut UnwrapErr(SysRng), russh::keys::Algorithm::Ed25519)?;
        let key_data = key.to_openssh(LineEnding::LF)?;
        std::fs::write(path, key_data.as_bytes())?;
        #[cfg(unix)]
        if let Err(e) = std::fs::set_permissions(path, Permissions::from_mode(0o600)) {
            tracing::warn!(path = %path.display(), error = ?e, "failed to set permissions on server key");
        }
        tracing::info!(path = %path.display(), "generated new server key");
        Ok(key)
    }
}

pub async fn run(
    addr: &str,
    port: u16,
    state: State,
    shutdown: Option<late_core::shutdown::CancellationToken>,
) -> anyhow::Result<()> {
    let socket = TcpListener::bind((addr, port)).await?;
    run_with_listener(socket, state, shutdown).await
}

pub async fn run_with_listener(
    socket: TcpListener,
    state: State,
    shutdown: Option<late_core::shutdown::CancellationToken>,
) -> anyhow::Result<()> {
    let shutdown = shutdown.unwrap_or_default();
    let keys = vec![load_or_generate_key(&state)?];
    let config = russh::server::Config {
        inactivity_timeout: Some(std::time::Duration::from_secs(
            state.config.ssh_idle_timeout,
        )),
        auth_rejection_time: std::time::Duration::from_secs(3),
        // Don't penalize the client's initial `none`-auth probe; only delay
        // repeated failures. Without this, every connection waits 3s before
        // the real publickey auth is even attempted.
        auth_rejection_time_initial: Some(std::time::Duration::ZERO),
        keys,
        // Offer `none` only. russh advertises zlib by default, and clients
        // that ask for it (`ssh -C`, `Compression yes`) have been dropping
        // mid-session with "closed by remote host" on the first keystroke
        // that moves a character. A TUI frame stream is small and already
        // poorly compressible, so there is nothing to win here and a live
        // disconnect to lose.
        preferred: russh::Preferred {
            compression: std::borrow::Cow::Borrowed(&[russh::compression::NONE]),
            ..russh::Preferred::DEFAULT
        },
        window_size: 8 * 1024 * 1024, // 8MB window size
        event_buffer_size: 128,
        nodelay: true,
        keepalive_interval: Some(std::time::Duration::from_secs(30)),
        keepalive_max: 3,
        ..Default::default()
    };
    let config = Arc::new(config);

    let addr = socket.local_addr()?;
    tracing::info!(address = %addr, "ssh server listening");

    let server = Server { state };
    let mut session_tasks = JoinSet::new();

    loop {
        tokio::select! {
            accept_result = socket.accept() => {
                let (mut tcp, transport_peer_addr) = accept_result?;
                let config = Arc::clone(&config);
                let server = server.clone();
                session_tasks.spawn(async move {
                    if config.nodelay
                        && let Err(e) = tcp.set_nodelay(true)
                    {
                        tracing::warn!(error = ?e, "set_nodelay failed");
                    }

                    let proxied_addr =
                        match resolve_proxied_client_addr(&server.state, &mut tcp, transport_peer_addr)
                            .await
                        {
                            Ok(addr) => addr,
                            Err(err) => {
                                tracing::warn!(
                                    ?transport_peer_addr,
                                    error = ?err,
                                    "failed to resolve proxy protocol header; dropping connection"
                                );
                                metrics::record_ssh_connection_rejected(SshRejectReason::ProxyHeader);
                                return;
                            }
                        };

                    // Admission runs before the SSH handshake so a refused
                    // socket costs nothing past this line: `tcp` drops on
                    // return, no permit, no russh task, no buffers. A flood
                    // that held its connections open through the handshake
                    // is what exhausted the global permits on 2026-09-03.
                    let effective_peer_addr = proxied_addr.or(Some(transport_peer_addr));
                    let admission = match server.admit(effective_peer_addr) {
                        Ok(admission) => admission,
                        Err(reason) => {
                            // `ip` stays a dedicated field: the incident
                            // queries in CONTEXT.md group rejections by it.
                            let ip = effective_peer_addr.map(|addr| addr.ip());
                            match reason {
                                SshRejectReason::RateLimited => tracing::warn!(
                                    ?ip,
                                    ?transport_peer_addr,
                                    max_attempts = server.state.ssh_attempt_limiter.max_attempts(),
                                    window_secs = server.state.ssh_attempt_limiter.window_secs(),
                                    "ssh rate limit exceeded for peer ip"
                                ),
                                SshRejectReason::PerIpLimit => tracing::warn!(
                                    ?ip,
                                    ?transport_peer_addr,
                                    limit = server.state.config.max_conns_per_ip,
                                    "per-ip limit reached, rejecting new client"
                                ),
                                SshRejectReason::GlobalLimit => tracing::info!(
                                    ?transport_peer_addr,
                                    ?effective_peer_addr,
                                    "connection limit reached, rejecting new client"
                                ),
                                SshRejectReason::ProxyHeader => {
                                    unreachable!("admit never reports a proxy header failure")
                                }
                            }
                            metrics::record_ssh_connection_rejected(reason);
                            return;
                        }
                    };

                    let handler = server.new_client(Some(transport_peer_addr), proxied_addr, admission);
                    match russh::server::run_stream(config, tcp, handler).await {
                        Ok(session) => {
                            if let Err(err) = session.await {
                                tracing::debug!(error = ?err, "ssh session ended with error");
                            }
                        }
                        Err(err) => {
                            tracing::debug!(error = ?err, "failed to initialize ssh session");
                        }
                    }
                });
            }
            _ = shutdown.cancelled() => {
                tracing::info!("ssh shutdown requested, stopping accept loop");
                break;
            }
        }
    }

    drop(socket); // Immediately stop accepting and reject new TCP connections

    if !session_tasks.is_empty() {
        tracing::info!("waiting for active ssh sessions to drain");
        while let Some(join_result) = session_tasks.join_next().await {
            if let Err(err) = join_result {
                tracing::debug!(error = ?err, "ssh session task failed while draining");
            }
        }
    }

    Ok(())
}

/// What an admitted connection holds for its lifetime. Released by
/// `ClientHandler::drop`.
struct Admission {
    permit: OwnedSemaphorePermit,
    per_ip_incremented: bool,
}

impl Server {
    /// Decide whether a fresh TCP connection may start the SSH handshake.
    /// Cheap per-IP checks run first so a rate-limited peer never touches the
    /// global semaphore; the permit is taken last and only kept on success.
    fn admit(&self, effective_peer_addr: Option<SocketAddr>) -> Result<Admission, SshRejectReason> {
        metrics::record_ssh_connection();
        let peer_ip = effective_peer_addr.map(|addr| addr.ip());
        let mut per_ip_incremented = false;

        if let Some(ip) = peer_ip {
            if !self.state.ssh_attempt_limiter.allow(ip) {
                return Err(SshRejectReason::RateLimited);
            }
            let mut counts = self.state.conn_counts.lock_recover();
            let count = counts.entry(ip).or_insert(0);
            if *count >= self.state.config.max_conns_per_ip {
                return Err(SshRejectReason::PerIpLimit);
            }
            *count += 1;
            per_ip_incremented = true;
        }

        match self.state.conn_limit.clone().try_acquire_owned() {
            Ok(permit) => Ok(Admission {
                permit,
                per_ip_incremented,
            }),
            Err(_) => {
                if let Some(ip) = peer_ip {
                    release_per_ip_slot(&self.state, ip);
                }
                Err(SshRejectReason::GlobalLimit)
            }
        }
    }

    fn new_client(
        &self,
        transport_peer_addr: Option<SocketAddr>,
        proxied_addr: Option<SocketAddr>,
        admission: Admission,
    ) -> ClientHandler {
        let effective_peer_addr = proxied_addr.or(transport_peer_addr);
        let peer_ip = effective_peer_addr.map(|addr| addr.ip());

        tracing::debug!(
            ?transport_peer_addr,
            ?effective_peer_addr,
            "new client connection"
        );
        ClientHandler {
            state: self.state.clone(),
            user: None,
            is_new_user: false,
            auth_fingerprint: None,
            activity_feed_rx: None,
            transport_peer_addr,
            peer_addr: effective_peer_addr,
            peer_ip,
            _conn_permit: admission.permit,
            per_ip_incremented: admission.per_ip_incremented,
            active_user_incremented: false,
            channel: None,
            app_channel_id: None,
            app: None,
            render_signal: None,
            output_budget: Arc::new(OutputBudget::new()),
            terminal_probe: None,
            input_tx: None,
            input_rx: None,
            cli_mode: false,
            cli_token_served: false,
            resume: None,
            term: None,
            terminal_env_hints: Vec::new(),
            session_token: None,
            session_rx: None,
            connected_at: Instant::now(),
            authed_at: None,
            session_start: None,
        }
    }
}

fn release_per_ip_slot(state: &State, ip: IpAddr) {
    let mut counts = state.conn_counts.lock_recover();
    if let Some(count) = counts.get_mut(&ip) {
        if *count <= 1 {
            counts.remove(&ip);
        } else {
            *count -= 1;
        }
    }
}

async fn resolve_proxied_client_addr(
    state: &State,
    stream: &mut TcpStream,
    transport_peer_addr: SocketAddr,
) -> Result<Option<SocketAddr>> {
    if !state.config.ssh_proxy_protocol {
        return Ok(None);
    }

    if !is_trusted_proxy_peer(state, transport_peer_addr.ip()) {
        return Ok(None);
    }

    proxy_protocol::read_v1_client_addr(stream, PROXY_HEADER_TIMEOUT).await
}

fn is_trusted_proxy_peer(state: &State, ip: IpAddr) -> bool {
    proxy_protocol::is_trusted_peer(ip, &state.config.ssh_proxy_trusted_cidrs)
}

impl Drop for ClientHandler {
    fn drop(&mut self) {
        if self.app.is_none()
            && let Some(token) = self.session_token.clone()
        {
            // No App was ever built, so `Drop for App` will not run: retire
            // the paired registry's token-scoped state from here instead.
            self.state.paired_client_registry.forget_session(&token);
            let registry = self.state.session_registry.clone();
            tokio::spawn(async move {
                registry.unregister(&token).await;
            });
        }

        if self.active_user_incremented
            && let Some(user) = self.user.as_ref()
        {
            metrics::add_ssh_session(-1);
            let user_id = user.id;
            let mut became_offline = false;
            let mut active_users = self.state.active_users.lock_recover();

            if let Some(active) = active_users.get_mut(&user_id) {
                if let Some(token) = self.session_token.as_ref() {
                    active.sessions.retain(|session| session.token != *token);
                }
                if active.connection_count <= 1 {
                    active_users.remove(&user_id);
                    became_offline = true;
                } else {
                    active.connection_count -= 1;
                }
            }
            if became_offline {
                self.state
                    .leaderboard_service
                    .online_user_disconnected(user_id);
            }
            drop(active_users);
        }

        if !self.per_ip_incremented {
            return;
        }
        let Some(ip) = self.peer_ip else {
            return;
        };
        release_per_ip_slot(&self.state, ip);
    }
}

impl ClientHandler {
    async fn ensure_cli_session(&mut self) -> Result<String> {
        if let Some(token) = self.session_token.clone() {
            return Ok(token);
        }

        let user_id =
            self.user.as_ref().map(|u| u.id).ok_or_else(|| {
                anyhow::anyhow!("cli session requested before user authenticated")
            })?;

        let session_token = crate::session::new_session_token();
        let (session_tx, session_rx) = tokio::sync::mpsc::channel(64);
        self.state
            .session_registry
            .register(
                session_token.clone(),
                session_tx,
                user_id,
                self.auth_fingerprint.clone(),
            )
            .await;
        self.session_token = Some(session_token.clone());
        self.session_rx = Some(session_rx);
        Ok(session_token)
    }

    /// Settled by the time the shell starts: the token exec and the
    /// `LATE_CLI_MODE` env request both arrive before the shell request.
    fn session_client(&self) -> metrics::SessionClient {
        session_client(self.cli_mode, self.cli_token_served)
    }

    fn track_active_session_token(&self, session_token: &str) {
        let Some(user) = self.user.as_ref() else {
            return;
        };
        let mut active_users = self.state.active_users.lock_recover();
        let Some(active) = active_users.get_mut(&user.id) else {
            return;
        };
        if active
            .sessions
            .iter()
            .any(|session| session.token == session_token)
        {
            return;
        }
        active.sessions.push(ActiveSession {
            token: session_token.to_string(),
            fingerprint: Some(user.fingerprint.clone()),
            peer_ip: self.peer_ip,
            away: false,
        });
    }
}

impl russh::server::Handler for ClientHandler {
    type Error = anyhow::Error;

    async fn authentication_banner(&mut self) -> Result<Option<String>, Self::Error> {
        Ok(Some(AUTH_SETUP_BANNER.to_string()))
    }

    #[tracing::instrument(skip(self, ssh_user, key), fields(peer = ?self.peer_addr, transport = ?self.transport_peer_addr))]
    async fn auth_publickey(
        &mut self,
        ssh_user: &str,
        key: &russh::keys::PublicKey,
    ) -> Result<Auth, Self::Error> {
        tracing::debug!("public key auth accepted");
        if !self.state.config.open_access {
            tracing::debug!("open access disabled, rejecting public key auth");
            return Ok(reject_publickey_only());
        }
        let fingerprint = key.fingerprint(keys::HashAlg::Sha256).to_string();
        let client = match self.state.db.get().await {
            Ok(client) => client,
            Err(e) => {
                tracing::warn!(error = ?e, "failed to check server ban, rejecting auth");
                return Ok(reject_publickey_only());
            }
        };
        match has_active_server_ban_before_user_lookup(&client, &fingerprint, self.peer_ip).await {
            Ok(true) => {
                tracing::warn!(
                    fingerprint = %fingerprint,
                    peer_ip = ?self.peer_ip,
                    "active server ban rejected SSH auth"
                );
                return Ok(reject_publickey_only());
            }
            Ok(false) => {}
            Err(e) => {
                tracing::warn!(error = ?e, "failed to check server ban, rejecting auth");
                return Ok(reject_publickey_only());
            }
        }
        match User::find_by_fingerprint(&client, &fingerprint).await {
            Ok(Some(existing_user)) => {
                match ServerBan::find_active_for_user_id(&client, existing_user.id).await {
                    Ok(Some(_)) => {
                        tracing::warn!(
                            username = %existing_user.username,
                            fingerprint = %fingerprint,
                            peer_ip = ?self.peer_ip,
                            "active server ban rejected SSH auth"
                        );
                        return Ok(reject_publickey_only());
                    }
                    Ok(None) => {}
                    Err(e) => {
                        tracing::warn!(error = ?e, "failed to check server ban, rejecting auth");
                        return Ok(reject_publickey_only());
                    }
                }
            }
            Ok(None) => {}
            Err(e) => {
                tracing::warn!(error = ?e, "failed to look up user before ban check, rejecting auth");
                return Ok(reject_publickey_only());
            }
        }
        drop(client);
        let (user, is_new_user) = match crate::ssh::ensure_user(&self.state, &fingerprint).await {
            Ok(pair) => pair,
            Err(e) => {
                tracing::warn!(error = ?e, "failed to ensure user, rejecting auth");
                return Ok(reject_publickey_only());
            }
        };
        self.is_new_user = is_new_user;
        // `ssh invite-<code>@late.sh` names an inviter, read once: on the
        // connect that created the account. Any other login name (plain
        // `ssh late.sh` sends the local $USER) is ignored, as it always was.
        if is_new_user && let Some(code) = crate::app::referral::state::ssh_invite_code(ssh_user) {
            self.state.referral_service.attach_task(
                user.id,
                code,
                late_core::models::referral::ReferralSource::Ssh,
            );
        }
        if !self.active_user_incremented {
            let mut active_users = self.state.active_users.lock_recover();

            let became_online = if let Some(active) = active_users.get_mut(&user.id) {
                active.connection_count += 1;
                active.username = user.username.clone();
                active.fingerprint = Some(fingerprint.clone());
                active.audio_source = late_core::models::user::extract_audio_source(&user.settings);
                active.last_login_at = std::time::Instant::now();
                false
            } else {
                active_users.insert(
                    user.id,
                    crate::state::ActiveUser {
                        username: user.username.clone(),
                        fingerprint: Some(fingerprint.clone()),
                        audio_source: late_core::models::user::extract_audio_source(&user.settings),
                        sessions: Vec::new(),
                        connection_count: 1,
                        last_login_at: std::time::Instant::now(),
                    },
                );
                true
            };
            if became_online {
                self.state
                    .leaderboard_service
                    .online_user_connected(user.id);
            }
            drop(active_users);
            self.active_user_incremented = true;
            metrics::add_ssh_session(1);
        }
        crate::usernames::upsert(
            &self.state.username_directory,
            user.id,
            user.username.clone(),
        );

        let username = user.username.clone();

        tracing::info!(
            username = %username,
            fingerprint = %fingerprint,
            "user connected"
        );

        self.user = Some(user);
        self.auth_fingerprint = Some(fingerprint);
        // Subscribed here, before `pty_request` publishes this session's own
        // join, so the session hears it.
        self.activity_feed_rx = Some(self.state.activity_feed.subscribe());
        self.authed_at = Some(Instant::now());
        Ok(Auth::Accept)
    }

    #[tracing::instrument(skip(self, _user, _response), fields(peer = ?self.peer_addr, transport = ?self.transport_peer_addr))]
    async fn auth_keyboard_interactive(
        &mut self,
        _user: &str,
        _submethods: &str,
        _response: Option<russh::server::Response<'_>>,
    ) -> Result<Auth, Self::Error> {
        tracing::debug!("keyboard-interactive auth rejected: public key auth is required");
        Ok(reject_publickey_only())
    }

    #[tracing::instrument(skip(self, _user, _password), fields(peer = ?self.peer_addr, transport = ?self.transport_peer_addr))]
    async fn auth_password(&mut self, _user: &str, _password: &str) -> Result<Auth, Self::Error> {
        tracing::debug!("password auth rejected: public key auth is required");
        Ok(reject_publickey_only())
    }

    #[tracing::instrument(skip(self, channel, _session), fields(peer = ?self.peer_addr, transport = ?self.transport_peer_addr))]
    async fn channel_open_session(
        &mut self,
        channel: Channel<Msg>,
        _session: &mut Session,
    ) -> Result<bool, Self::Error> {
        tracing::debug!("session channel opened");
        self.channel = Some(channel);
        Ok(true)
    }

    #[tracing::instrument(skip(self, session, _modes), fields(peer = ?self.peer_addr, transport = ?self.transport_peer_addr))]
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
        tracing::debug!(term, col_width, row_height, "pty requested");
        self.term = Some(terminal_name_for_log(term));
        let terminal_size = clamp_terminal_size(col_width, row_height);
        if terminal_size.clamped {
            tracing::warn!(
                term,
                reported_cols = col_width,
                reported_rows = row_height,
                cols = terminal_size.cols,
                rows = terminal_size.rows,
                "clamped oversized pty dimensions"
            );
        }
        let session_token = self.ensure_cli_session().await?;
        self.track_active_session_token(&session_token);
        let session_rx = self
            .session_rx
            .take()
            .ok_or_else(|| anyhow::anyhow!("cli session receiver missing during pty request"))?;

        let article_service = self.state.article_service.clone();
        let chat_service = self.state.chat_service.clone();
        let profile_service = self.state.profile_service.clone();
        let twenty_forty_eight_service = self.state.twenty_forty_eight_service.clone();
        let le_word_service = self.state.le_word_service.clone();
        let sudoku_service = self.state.sudoku_service.clone();
        let nonogram_service = self.state.nonogram_service.clone();
        let solitaire_service = self.state.solitaire_service.clone();
        let nonogram_library = self.state.nonogram_library.clone();

        let user = match self.user.as_ref() {
            Some(user) => user,
            None => {
                tracing::error!("pty request without authenticated user");
                return Err(anyhow::anyhow!("unauthenticated pty request"));
            }
        };

        let user_id = user.id;
        // The login event waits for the pty, the first point that knows
        // whether this session is one a drain handoff moved off another pod
        // (`LATE_RESUME`, read in `env_request`). That one already joined
        // there and comes in quietly.
        let resume = self.resume.take();
        if resume.is_none() {
            let _ = self
                .state
                .activity_feed
                .send(ActivityEvent::joined(user_id, user.username.clone()));
        }
        let permissions = AuthzPermissions::new(
            user.is_admin || self.state.config.force_admin,
            user.is_moderator,
        );

        let ArcadeSessionPreloads {
            initial_2048_game,
            initial_2048_high_score,
            initial_tetris_game,
            initial_tetris_high_score,
            initial_snake_game,
            initial_snake_high_score,
            initial_traffic_track_scores,
            initial_traffic_high_score,
            initial_le_word_today,
            initial_rubiks_cube_game,
            initial_sliding_puzzle_games,
            initial_sudoku_games,
            initial_nonogram_games,
            initial_solitaire_games,
            initial_minesweeper_games,
            initial_netwalk_games,
            initial_chess_puzzle_today,
            initial_chess_puzzle_games,
            initial_word_wheel_today,
            initial_quartets_today,
        } = load_arcade_session_preloads(&self.state, user_id).await;
        let initial_bonsai_tree = match self.state.bonsai_service.ensure_tree(user_id).await {
            Ok(tree) => Some(tree),
            Err(e) => {
                tracing::warn!(error = ?e, "failed to load/create bonsai tree");
                None
            }
        };
        let initial_bonsai_decay_protection =
            match self.state.bonsai_service.decay_protection(user_id).await {
                Ok(protection) => protection,
                Err(e) => {
                    tracing::warn!(error = ?e, "failed to load bonsai decay protection");
                    None
                }
            };
        let shop_snapshot_rx = self.state.shop_service.subscribe_snapshot(user_id);
        // Primes the per-user snapshot channel subscribed above; the session
        // reads everything it needs from that channel.
        if let Err(e) = self.state.shop_service.refresh_user(user_id).await {
            tracing::warn!(error = ?e, "failed to refresh shop snapshot");
        }
        let initial_pet = match self.state.pet_service.ensure_pet(user_id).await {
            Ok(pet) => Some(pet),
            Err(e) => {
                tracing::warn!(error = ?e, "failed to load/create pet companion");
                None
            }
        };
        let initial_aquarium_care = match self.state.aquarium_service.bootstrap(user_id).await {
            Ok(care) => care,
            Err(e) => {
                tracing::warn!(error = ?e, "failed to load aquarium care");
                Default::default()
            }
        };

        // Ensure the user's chip balance row exists.
        let initial_chip_balance = match self.state.chip_service.ensure_chips(user_id).await {
            Ok(chips) => chips.balance,
            Err(e) => {
                tracing::warn!(error = ?e, "failed to ensure chip balance");
                0
            }
        };
        let quest_snapshot_rx = self.state.quest_service.subscribe_snapshot(user_id);
        if let Err(e) = self.state.quest_service.refresh_user(user_id).await {
            tracing::warn!(error = ?e, "failed to refresh quest snapshot");
        }
        let initial_ultimate_cooldowns =
            match self.state.ultimate_service.list_cooldowns(user_id).await {
                Ok(cooldowns) => cooldowns,
                Err(e) => {
                    tracing::warn!(error = ?e, "failed to load ultimate cooldowns");
                    Vec::new()
                }
            };
        let artboard_ban = match self.state.db.get().await {
            Ok(client) => match ArtboardBan::find_active_for_user(&client, user_id).await {
                Ok(ban) => ban,
                Err(e) => {
                    tracing::warn!(error = ?e, "failed to check artboard ban status");
                    None
                }
            },
            Err(e) => {
                tracing::warn!(error = ?e, "failed to get db client for artboard ban check");
                None
            }
        };
        let key_fingerprint = self.auth_fingerprint.clone();
        let device = crate::session_bootstrap::load_device_state(
            &self.state,
            user_id,
            key_fingerprint.as_deref(),
        )
        .await;
        // The door: the day's wall piece for those who chose Always View on
        // the Artboard content disclaimer (on this device), else the cup.
        let splash_piece = self
            .state
            .gallery_service
            .splash_piece_for_login(device.shows_artboard_disclaimer(user))
            .await;
        let initial_door_rcs = match self.state.door_rc_service.list(user_id).await {
            Ok(rcs) => rcs,
            Err(e) => {
                tracing::warn!(error = ?e, "failed to load door rc files");
                Vec::new()
            }
        };
        let (input_tx, input_rx) = tokio::sync::mpsc::channel(INPUT_QUEUE_CAP);
        let first_contact_gate =
            crate::app::deadchannel::haunt::svc::bootstrap_gate(&self.state, user).await;
        let mut app = crate::app::state::App::new(SessionConfig {
            // Terminal / layout
            cols: terminal_size.cols,
            rows: terminal_size.rows,
            term: term.to_string(),

            // Services / data sources
            audio_service: self.state.audio_service.clone(),
            voice_service: self.state.voice_service.clone(),
            stream_service: self.state.stream_service.clone(),
            chat_service,
            translation_service: self.state.translation_service.clone(),
            summary_service: self.state.summary_service.clone(),
            paper_service: self.state.paper_service.clone(),
            herald_service: self.state.herald_service.clone(),
            jobs_service: self.state.jobs_service.clone(),
            notification_service: self.state.notification_service.clone(),
            article_service,
            calendar_service: self.state.calendar_service.clone(),
            feed_service: self.state.feed_service.clone(),
            cyberspace_service: self.state.cyberspace_service.clone(),
            showcase_service: self.state.showcase_service.clone(),
            work_service: self.state.work_service.clone(),
            profile_service,
            twenty_forty_eight_service,
            initial_2048_game,
            initial_2048_high_score,
            tetris_service: self.state.tetris_service.clone(),
            snake_service: self.state.snake_service.clone(),
            traffic_service: self.state.traffic_service.clone(),
            rubiks_cube_service: self.state.rubiks_cube_service.clone(),
            initial_rubiks_cube_game,
            sliding_puzzle_service: self.state.sliding_puzzle_service.clone(),
            initial_sliding_puzzle_games,
            netwalk_service: self.state.netwalk_service.clone(),
            initial_netwalk_games,
            chess_puzzle_service: self.state.chess_puzzle_service.clone(),
            initial_chess_puzzle_today,
            initial_chess_puzzle_games,
            word_wheel_service: self.state.word_wheel_service.clone(),
            initial_word_wheel_today,
            quartets_service: self.state.quartets_service.clone(),
            initial_quartets_today,
            initial_tetris_game,
            initial_snake_game,
            initial_tetris_high_score,
            initial_snake_high_score,
            initial_traffic_track_scores,
            initial_traffic_high_score,
            le_word_service,
            initial_le_word_today,
            sudoku_service,
            initial_sudoku_games,
            nonogram_service,
            initial_nonogram_games,
            solitaire_service,
            initial_solitaire_games,
            minesweeper_service: self.state.minesweeper_service.clone(),
            initial_minesweeper_games,
            lateania_service: self.state.lateania_service.clone(),
            greendragon_service: self.state.greendragon_service.clone(),
            darkroom_service: self.state.darkroom_service.clone(),
            arcade_handle_service: self.state.arcade_handle_service.clone(),
            live_games: self.state.live_games.clone(),
            door_rc_service: self.state.door_rc_service.clone(),
            games_hub_service: self.state.games_hub_service.clone(),
            initial_door_rcs,
            daily_service: self.state.daily_service.clone(),
            realm_service: self.state.realm_service.clone(),
            house_registry: self.state.house_registry.clone(),
            dartboard_server: self.state.dartboard_server.clone(),
            dartboard_provenance: self.state.dartboard_provenance.clone(),
            artboard_snapshot_service: crate::app::artboard::svc::ArtboardSnapshotService::new(
                self.state.db.clone(),
            ),
            gallery_service: self.state.gallery_service.clone(),
            splash_piece,
            username: user.username.clone(),
            bonsai_service: self.state.bonsai_service.clone(),
            fight_service: crate::app::deadchannel::fight::svc::FightService::new(
                self.state.db.clone(),
                self.state.chat_service.clone(),
                self.state.chip_service.clone(),
            ),
            tailor_service: crate::app::deadchannel::tailor::svc::TailorService::new(
                self.state.db.clone(),
            ),
            poster_service: crate::app::deadchannel::poster::svc::PosterService::new(
                self.state.db.clone(),
            ),
            guide_service: crate::app::deadchannel::guide::svc::GuideService::new(
                self.state.db.clone(),
            ),
            lounge_service: self.state.lounge_service.clone(),
            initial_bonsai_tree,
            initial_bonsai_decay_protection,
            pet_service: self.state.pet_service.clone(),
            initial_pet,
            aquarium_service: self.state.aquarium_service.clone(),
            initial_aquarium_care,
            quest_service: self.state.quest_service.clone(),
            quest_snapshot_rx,
            shop_service: self.state.shop_service.clone(),
            shop_snapshot_rx,
            ultimate_service: self.state.ultimate_service.clone(),
            initial_ultimate_cooldowns,
            nonogram_library,
            chip_service: self.state.chip_service.clone(),
            initial_chip_balance,
            leaderboard_rx: Some(self.state.leaderboard_service.subscribe()),

            // Session / connection
            web_url: self.state.config.web_url.clone(),
            rebels_enabled: self.state.config.rebels_enabled,
            rebels_host: self.state.config.rebels_host.clone(),
            rebels_port: self.state.config.rebels_port,
            rebels_secret: self.state.config.rebels_secret.clone(),
            nethack_enabled: self.state.config.nethack_enabled,
            nethack_host: self.state.config.nethack_host.clone(),
            nethack_port: self.state.config.nethack_port,
            nethack_secret: self.state.config.nethack_secret.clone(),
            nethack_activity: Some(
                crate::app::activity::publisher::ActivityPublisher::new(
                    self.state.db.clone(),
                    self.state.activity_feed.clone(),
                )
                .with_username_directory(self.state.username_directory.clone()),
            ),
            dcss_enabled: self.state.config.dcss_enabled,
            dcss_host: self.state.config.dcss_host.clone(),
            dcss_port: self.state.config.dcss_port,
            dcss_secret: self.state.config.dcss_secret.clone(),
            brogue_enabled: self.state.config.brogue_enabled,
            brogue_host: self.state.config.brogue_host.clone(),
            brogue_port: self.state.config.brogue_port,
            brogue_secret: self.state.config.brogue_secret.clone(),
            usurper_enabled: self.state.config.usurper_enabled,
            usurper_host: self.state.config.usurper_host.clone(),
            usurper_port: self.state.config.usurper_port,
            usurper_secret: self.state.config.usurper_secret.clone(),
            dopewars_enabled: self.state.config.dopewars_enabled,
            dopewars_host: self.state.config.dopewars_host.clone(),
            dopewars_port: self.state.config.dopewars_port,
            dopewars_secret: self.state.config.dopewars_secret.clone(),
            bashquest_enabled: self.state.config.bashquest_enabled,
            bashquest_host: self.state.config.bashquest_host.clone(),
            bashquest_port: self.state.config.bashquest_port,
            bashquest_secret: self.state.config.bashquest_secret.clone(),
            bashquest_awards: Some(crate::app::door::bashquest::graduate::BashquestAwards::new(
                self.state.db.clone(),
            )),
            codekeep_enabled: self.state.config.codekeep_enabled,
            codekeep_host: self.state.config.codekeep_host.clone(),
            codekeep_port: self.state.config.codekeep_port,
            codekeep_secret: self.state.config.codekeep_secret.clone(),
            zork_enabled: self.state.config.zork_enabled,
            zork_host: self.state.config.zork_host.clone(),
            zork_port: self.state.config.zork_port,
            zork_secret: self.state.config.zork_secret.clone(),
            session_token,
            session_registry: Some(self.state.session_registry.clone()),
            paired_client_registry: Some(self.state.paired_client_registry.clone()),
            session_rx: Some(session_rx),
            now_playing_rx: Some(self.state.now_playing_rx.clone()),
            radio_meta_rx: Some(self.state.radio_meta_rx.clone()),
            active_users: Some(self.state.active_users.clone()),
            drunk_map: self.state.drunk_map.clone(),
            nightcap_house: Some(self.state.nightcap_house.clone()),
            mention_ladders: self.state.mention_ladders.clone(),
            files: self.state.config.files.clone(),
            scratchpad_registry: Some(self.state.scratchpad_registry.clone()),
            clubhouse_tutorial_done: late_core::models::user::extract_clubhouse_tutorial_done(
                &user.settings,
            ),
            first_contact: crate::app::deadchannel::haunt::state::FirstContactMarks::from_settings(
                &user.settings,
            ),
            first_contact_gate,
            runner_looks_rx: self.state.runner_looks.subscribe(),
            presence: self.state.presence.clone(),
            zen_layout: device
                .zen_layout
                .clone()
                .or_else(|| late_core::models::user::extract_zen_layout(&user.settings)),
            home_layout: device
                .home_layout
                .clone()
                .or_else(|| late_core::models::user::extract_home_layout(&user.settings)),
            key_fingerprint,
            key_layout: device.layout,
            key_artboard_disclaimer: device.artboard_disclaimer,
            key_left_at: device.left_at,
            username_directory: Some(self.state.username_directory.clone()),
            flair_directory: Some(self.state.flair_directory.clone()),
            pot_service: Some(self.state.pot_service.clone()),
            referral_service: self.state.referral_service.clone(),
            newcomer_clock: crate::app::referral::state::NewcomerClock::new(
                user.created,
                chrono::Utc::now(),
            ),
            account_created: user.created,
            activity_feed_rx: self.activity_feed_rx.take(),
            user_id,
            permissions,
            artboard_banned: artboard_ban.is_some(),
            artboard_ban_expires_at: artboard_ban.and_then(|ban| ban.expires_at),

            is_new_user: self.is_new_user,
            landing_page: late_core::models::user::extract_landing_page(&user.settings),
            paper_at_login: late_core::models::user::extract_paper_at_login(&user.settings),

            // Display config
            initial_theme_id: late_ssh_theme_id(&user.settings),
            initial_interaction_mode: late_core::models::user::extract_interaction_mode(
                &user.settings,
            ),
            initial_audio_source: late_core::models::user::extract_audio_source(&user.settings),
            initial_radio_station: late_core::models::user::extract_radio_station(&user.settings),
            initial_radio_slots: late_core::models::user::extract_radio_slots(&user.settings),

            // Server state
            is_draining: self.state.is_draining.clone(),
            handoff_idle: self.state.config.handoff_idle,
            resume,
        })
        .context("failed to initialize app for PTY session")?;
        // Counted once the session stands, so a move whose new session failed
        // to start stays in `sent` plus `requested` minus `resumed`.
        if let Some(point) = resume {
            metrics::record_drain_handoff(metrics::HandoffStage::Resumed);
            tracing::info!(
                user_id = %user_id,
                page = ?point.page,
                "resuming a session its cli moved here"
            );
        }
        for (name, value) in &self.terminal_env_hints {
            app.apply_terminal_env_hint(name, value);
        }
        let tenure = app.tenure();
        self.app = Some(Arc::new(TokioMutex::new(app)));
        self.session_start = Some(SessionStart {
            connected_at: self.connected_at,
            authed_at: self
                .authed_at
                .expect("a pty session is built only after its key was accepted"),
            app_ready_at: Instant::now(),
            user: match self.is_new_user {
                true => metrics::SessionUser::New,
                false => metrics::SessionUser::Returning,
            },
            tenure,
        });
        self.input_tx = Some(input_tx);
        self.input_rx = Some(input_rx);
        match session.channel_success(channel) {
            Ok(()) => tracing::debug!("pty channel_success sent"),
            Err(e) => tracing::error!(error = ?e, "pty channel_success failed"),
        }
        Ok(())
    }

    #[tracing::instrument(skip(self, session), fields(peer = ?self.peer_addr, transport = ?self.transport_peer_addr))]
    async fn env_request(
        &mut self,
        channel: ChannelId,
        variable_name: &str,
        variable_value: &str,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        if variable_name == CLI_MODE_ENV {
            self.cli_mode = matches!(variable_value, "1" | "true" | "TRUE" | "yes" | "YES");
            tracing::debug!(
                cli_mode = self.cli_mode,
                "updated cli mode from env request"
            );
        } else if variable_name == crate::app::handoff::state::RESUME_ENV {
            // `Resumed` waits for `pty_request` to build the session.
            self.resume = crate::app::handoff::state::ResumePoint::parse(variable_value);
            match self.resume {
                Some(_) => {}
                None => {
                    metrics::record_drain_handoff(metrics::HandoffStage::Unparsed);
                    tracing::warn!(
                        len = variable_value.len(),
                        "unreadable resume point; starting a fresh session"
                    );
                }
            }
        } else if crate::app::files::terminal_image::protocol_from_env_hint(
            variable_name,
            variable_value,
        )
        .is_some()
        {
            self.terminal_env_hints
                .push((variable_name.to_string(), variable_value.to_string()));
            if let Some(app) = self.app.as_ref() {
                app.lock()
                    .await
                    .apply_terminal_env_hint(variable_name, variable_value);
            }
        }
        match session.channel_success(channel) {
            Ok(()) => tracing::debug!(variable_name, "env channel_success sent"),
            Err(e) => tracing::error!(error = ?e, variable_name, "env channel_success failed"),
        }
        Ok(())
    }

    #[tracing::instrument(skip(self, data, session), fields(peer = ?self.peer_addr, transport = ?self.transport_peer_addr, len = data.len()))]
    async fn exec_request(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        let command = String::from_utf8_lossy(data);
        let preview: String = command.chars().take(128).collect();
        if command.trim() == CLI_TOKEN_REQUEST {
            tracing::info!("serving cli token exec request");
            self.cli_token_served = true;
            match session.channel_success(channel) {
                Ok(()) => tracing::debug!("exec token channel_success sent"),
                Err(e) => tracing::error!(error = ?e, "exec token channel_success failed"),
            }

            let token = self.ensure_cli_session().await?;
            let payload = serde_json::to_vec(&json!({ "session_token": token }))
                .context("failed to encode cli token exec response")?;

            if let Some(chan) = self.channel.take() {
                // `channel_open_session` populates `self.channel` immediately before this
                // `exec_request`, so for the token handshake this slot should hold the exec
                // channel we are replying on. The fallback below writes via `Session` if that
                // invariant ever stops holding.
                chan.data(payload.as_slice()).await?;
                let _ = chan.exit_status(0).await;
                let _ = chan.eof().await;
                let _ = chan.close().await;
            } else {
                session
                    .data(channel, payload)
                    .context("failed to send cli token exec response")?;
                if let Err(e) = session.eof(channel) {
                    tracing::error!(error = ?e, "exec token eof failed");
                }
                if let Err(e) = session.close(channel) {
                    tracing::error!(error = ?e, "exec token close failed");
                }
            }
            return Ok(());
        }

        tracing::info!(
            command = %preview,
            "rejecting exec request; only interactive shell is supported"
        );
        if let Err(e) = session.channel_failure(channel) {
            tracing::error!(error = ?e, "exec channel_failure failed");
        }
        if let Err(e) = session.close(channel) {
            tracing::error!(error = ?e, "exec channel close failed");
        }
        Ok(())
    }

    #[tracing::instrument(skip(self, session), fields(peer = ?self.peer_addr, transport = ?self.transport_peer_addr))]
    async fn shell_request(
        &mut self,
        channel: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        tracing::debug!("shell requested");
        match session.channel_success(channel) {
            Ok(()) => tracing::debug!("shell channel_success sent"),
            Err(e) => tracing::error!(error = ?e, "shell channel_success failed"),
        }
        if let (Some(chan), Some(app)) = (self.channel.take(), self.app.as_ref()) {
            let mut input_rx = self.input_rx.take().ok_or_else(|| {
                anyhow::anyhow!("session input receiver missing during shell request")
            })?;
            let channel_id = chan.id();
            self.app_channel_id = Some(channel_id);
            let handle = session.handle();

            if self.cli_mode
                && let Some(token) = self.session_token.as_ref()
            {
                let banner = format!("{CLI_TOKEN_PREFIX}{token}\r\n");
                let _ = timeout(
                    Duration::from_millis(50),
                    handle.data(channel_id, banner.into_bytes()),
                )
                .await;
            }

            // Keyboard-only sessions never turn on mouse reporting, so the
            // terminal keeps its own selection/copy.
            let mouse_on = app.lock().await.interaction_mode.mouse_enabled();
            let init = App::enter_alt_screen(mouse_on);
            let _ = timeout(Duration::from_millis(50), handle.data(channel_id, init)).await;

            let app = Arc::clone(app);
            let signal = Arc::new(RenderSignal::new());
            self.render_signal = Some(Arc::clone(&signal));
            let probe = Arc::new(TerminalProbe::new(Instant::now()));
            self.terminal_probe = Some(Arc::clone(&probe));
            let ctx = RenderContext {
                handle,
                channel_id,
                frame_drop_log_every: self.state.config.frame_drop_log_every,
                signal: Arc::clone(&signal),
                budget: Arc::clone(&self.output_budget),
                client: self.session_client(),
                probe,
            };
            app.lock().await.set_repaint_signal(Arc::clone(&signal));
            let mut session_start = self.session_start.take();
            let user = self
                .user
                .as_ref()
                .expect("the app is built only for an authenticated user");
            let who = SessionWho {
                user_id: user.id,
                username: user.username.clone(),
                client: ctx.client,
                term: self
                    .term
                    .clone()
                    .expect("the app is built by the pty request, which records TERM"),
            };
            tokio::spawn(async move {
                let started = Instant::now();
                let mut output = OutputTally::default();
                let mut previous_render: Option<Instant> = None;
                let mut input_pending = false;
                let mut stalled_since: Option<Instant> = None;
                // Local skip-ratio feel: drawn vs skipped-clean passes,
                // debug-logged every 5s (RUST_LOG=late_ssh=debug). The OTel
                // counters carry the same split in prod.
                let mut stats_drawn: u64 = 0;
                let mut stats_skipped: u64 = 0;
                let mut stats_since = Instant::now();
                // Bytes this session wrote since the window opened, for the
                // per-session output rate.
                let mut window_bytes: usize = 0;
                let mut window_since = Instant::now();
                // When this session last logged a slow render pass.
                let mut slow_pass_warned_at: Option<Instant> = None;
                // The world tick is adaptive: each pass reports how soon the
                // next one is needed (`App::wake_hint`), so idle sessions
                // sleep at the idle floor while animated screens keep the
                // classic 66ms cadence. The first deadline is immediate so
                // the session paints without waiting a tick.
                let mut world_deadline = Instant::now();
                let ended = loop {
                    let advance_world = match next_render_action(
                        world_deadline,
                        &ctx.signal,
                        &mut input_pending,
                        previous_render,
                    )
                    .await
                    {
                        RenderAction::AdvanceWorld => true,
                        RenderAction::Render => false,
                        RenderAction::Skip => continue,
                    };
                    // Output-budget guard: while the client sits on more
                    // unacked bytes than the budget, rendering pauses
                    // entirely. Ratatui's diff state does not advance during
                    // the pause, so resuming needs no forced repaint.
                    if ctx.budget.over_budget() {
                        let since = match stalled_since {
                            Some(since) => since,
                            None => {
                                let now = Instant::now();
                                stalled_since = Some(now);
                                tracing::warn!(
                                    outstanding = ctx.budget.outstanding(),
                                    "pausing renders, ssh output backlog over budget"
                                );
                                now
                            }
                        };
                        metrics::record_render_stall_skip();
                        if since.elapsed() >= OUTPUT_STALL_DISCONNECT {
                            tracing::warn!(
                                outstanding = ctx.budget.outstanding(),
                                stalled_secs = since.elapsed().as_secs(),
                                "disconnecting session, ssh output stalled past budget"
                            );
                            metrics::record_render_stall_disconnect();
                            let _ = ctx.handle.eof(ctx.channel_id).await;
                            let _ = ctx.handle.close(ctx.channel_id).await;
                            break SessionEnd::Stalled;
                        }
                        // Re-poll the budget at the hot cadence; leaving the
                        // deadline in the past would spin the loop.
                        world_deadline = Instant::now() + crate::app::tick::HOT_TICK;
                        continue;
                    }
                    if let Some(since) = stalled_since.take() {
                        tracing::info!(
                            stalled_ms = since.elapsed().as_millis() as u64,
                            "ssh output backlog cleared, resuming renders"
                        );
                    }
                    match render_once(&app, &mut input_rx, &ctx, advance_world).await {
                        Ok(outcome) => {
                            previous_render = Some(Instant::now());
                            if let Some(pass) = outcome.pass
                                && slow_pass_due(pass.took, slow_pass_warned_at, Instant::now())
                            {
                                slow_pass_warned_at = Some(Instant::now());
                                tracing::warn!(
                                    username = %who.username,
                                    screen = ?pass.screen,
                                    pass_ms = pass.took.as_millis() as u64,
                                    "slow render pass"
                                );
                            }
                            if outcome.drew
                                && let Some(start) = session_start.take()
                            {
                                start.record_first_frame();
                            }
                            if outcome.drew {
                                stats_drawn += 1;
                            } else {
                                stats_skipped += 1;
                            }
                            if stats_since.elapsed() >= Duration::from_secs(5) {
                                tracing::debug!(
                                    drawn = stats_drawn,
                                    skipped_clean = stats_skipped,
                                    "render stats, last 5s"
                                );
                                stats_drawn = 0;
                                stats_skipped = 0;
                                stats_since = Instant::now();
                            }
                            output.bytes += outcome.bytes_sent as u64;
                            if outcome.drew {
                                output.frames += 1;
                            }
                            window_bytes += outcome.bytes_sent;
                            let window = window_since.elapsed();
                            if window >= OUTPUT_RATE_WINDOW {
                                let rate = window_bytes as f64 / window.as_secs_f64();
                                metrics::record_session_output_rate(ctx.client, rate);
                                output.peak_bytes_per_second =
                                    output.peak_bytes_per_second.max(rate);
                                window_bytes = 0;
                                window_since = Instant::now();
                            }
                            if outcome.should_quit {
                                tracing::debug!("app requested quit, closing connection");
                                settle_terminal_probe(&ctx.probe).await;
                                clean_disconnect(&ctx.handle, ctx.channel_id).await;
                                break SessionEnd::Closed;
                            }
                            drive_terminal_probe(&ctx, outcome.bytes_sent).await;
                            if advance_world {
                                world_deadline = Instant::now() + outcome.wake_hint;
                            } else {
                                // An input render can shrink the cadence
                                // (post-input hot window) but never delays a
                                // world tick that was already due sooner.
                                world_deadline =
                                    world_deadline.min(Instant::now() + outcome.wake_hint);
                            }
                        }
                        Err(err) => {
                            tracing::debug!(error = ?err, "error rendering frame, stopping render loop");
                            let exit = App::leave_alt_screen();
                            let _ = timeout(
                                Duration::from_millis(50),
                                ctx.handle.data(ctx.channel_id, exit),
                            )
                            .await;
                            let _ = ctx.handle.eof(ctx.channel_id).await;
                            let _ = ctx.handle.close(ctx.channel_id).await;
                            break SessionEnd::Failed;
                        }
                    }
                };
                let (terminal, screen, tenure) = {
                    let app = app.lock().await;
                    (
                        app.terminal_version().map(str::to_owned),
                        app.screen,
                        app.tenure(),
                    )
                };
                metrics::record_session_end(tenure, screen);
                log_wire_summary(
                    &who,
                    ended,
                    started.elapsed(),
                    &output,
                    ctx.probe.tally(),
                    terminal.as_deref(),
                );
            });
        }
        Ok(())
    }

    async fn window_adjusted(
        &mut self,
        channel: ChannelId,
        new_size: u32,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        if self.app_channel_id == Some(channel) {
            self.output_budget.on_window_adjusted(new_size);
        }
        Ok(())
    }

    #[tracing::instrument(skip(self, data, _session), fields(peer = ?self.peer_addr, len = data.len()))]
    async fn data(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        tracing::debug!(len = data.len(), "received input data");
        if self.app_channel_id != Some(channel) {
            tracing::debug!(?channel, "ignoring input from non-app channel");
            return Ok(());
        }
        if self.app.is_none() {
            return Ok(());
        }
        let Some(input_tx) = self.input_tx.as_ref() else {
            return Ok(());
        };
        // Probe replies are the terminal talking, not the user: they never
        // reach the input queue (`terminal_probe.rs`).
        let (input, replies) = terminal_probe::strip_replies(data);
        if replies > 0
            && let Some(probe) = self.terminal_probe.as_ref()
            && let Some(sample) = probe.answered(Instant::now())
        {
            let client = self.session_client();
            metrics::record_terminal_probe(client, metrics::TerminalProbeOutcome::Answered);
            metrics::record_terminal_rtt(client, sample.backlog, sample.rtt.as_secs_f64());
        }
        if input.is_empty() {
            return Ok(());
        }
        match input_tx.try_reserve() {
            Ok(permit) => {
                permit.send(input.into_owned());
            }
            Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
                tracing::warn!(
                    queue_cap = INPUT_QUEUE_CAP,
                    "session input queue full; dropping inbound ssh input"
                );
                return Ok(());
            }
            Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
                tracing::debug!("session input queue closed; dropping inbound ssh input");
                return Ok(());
            }
        }
        if let Some(signal) = self.render_signal.as_ref() {
            signal.wake();
        }
        Ok(())
    }

    #[tracing::instrument(skip(self, _session), fields(peer = ?self.peer_addr, transport = ?self.transport_peer_addr))]
    async fn channel_eof(
        &mut self,
        channel: ChannelId,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        tracing::debug!(?channel, "client sent channel EOF");
        if self.app_channel_id == Some(channel)
            && let Some(app) = self.app.as_ref()
        {
            let mut app = app.lock().await;
            app.running = false;
        }
        Ok(())
    }

    #[tracing::instrument(skip(self, _session), fields(peer = ?self.peer_addr, transport = ?self.transport_peer_addr))]
    async fn channel_close(
        &mut self,
        channel: ChannelId,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        tracing::debug!(?channel, "client closed channel");
        if self.app_channel_id == Some(channel)
            && let Some(app) = self.app.as_ref()
        {
            let mut app = app.lock().await;
            app.running = false;
            self.app_channel_id = None;
        }
        Ok(())
    }

    #[tracing::instrument(skip(self, _session), fields(peer = ?self.peer_addr, transport = ?self.transport_peer_addr))]
    async fn window_change_request(
        &mut self,
        _channel: ChannelId,
        col_width: u32,
        row_height: u32,
        _pix_width: u32,
        _pix_height: u32,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        tracing::debug!(col_width, row_height, "window resize");
        let terminal_size = clamp_terminal_size(col_width, row_height);
        if terminal_size.clamped {
            tracing::warn!(
                reported_cols = col_width,
                reported_rows = row_height,
                cols = terminal_size.cols,
                rows = terminal_size.rows,
                "clamped oversized window resize"
            );
        }
        let Some(app) = self.app.as_ref() else {
            return Ok(());
        };
        {
            let mut app = app.lock().await;
            if let Err(e) = app.resize(terminal_size.cols, terminal_size.rows) {
                tracing::error!(error = ?e, "error resizing app");
            }
            if let Some(signal) = self.render_signal.as_ref() {
                signal.dirty.store(true, Ordering::Release);
            }
        }
        if let Some(signal) = self.render_signal.as_ref() {
            signal.notify.notify_one();
        }
        Ok(())
    }
}

/// Who a session's wire summary is about, settled when the shell starts.
struct SessionWho {
    user_id: uuid::Uuid,
    username: String,
    client: metrics::SessionClient,
    term: String,
}

/// What a session's render loop wrote, for its wire summary.
#[derive(Default)]
struct OutputTally {
    bytes: u64,
    frames: u64,
    /// The busiest [`OUTPUT_RATE_WINDOW`]; a session shorter than one
    /// window reads 0.
    peak_bytes_per_second: f64,
}

/// How a session's render loop ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SessionEnd {
    /// The app quit: the user left, or the client closed the channel.
    Closed,
    /// The output backlog stayed over budget past [`OUTPUT_STALL_DISCONNECT`].
    Stalled,
    /// A frame could not be rendered or written.
    Failed,
}

impl SessionEnd {
    fn label(self) -> &'static str {
        match self {
            Self::Closed => "closed",
            Self::Stalled => "stalled",
            Self::Failed => "failed",
        }
    }
}

/// One line per session at its end, so who lagged and why is a log query
/// (SCALE.md, Pain Point 11). A slow `rtt_fastest_ms` is distance: the
/// user's network. Trips fine at `none` but slow at `large` are bandwidth:
/// our bytes outrun their link. A high `peak_bytes_per_sec` is a heavy
/// screen. Lost or retired probes are a terminal that ignores DSR, or a link
/// that stalled. Times are whole milliseconds; a bucket's avg and max read 0
/// when its count is 0.
fn log_wire_summary(
    who: &SessionWho,
    ended: SessionEnd,
    lived: Duration,
    output: &OutputTally,
    probes: ProbeTally,
    terminal: Option<&str>,
) {
    let whole_ms = |duration: Duration| duration.as_millis() as u64;
    let avg_ms = |trips: TripTally| trips.mean().map_or(0, whole_ms);
    tracing::info!(
        user_id = %who.user_id,
        username = %who.username,
        client = who.client.label(),
        term = %who.term,
        terminal = terminal.unwrap_or("unknown"),
        ended = ended.label(),
        secs = lived.as_secs(),
        bytes = output.bytes,
        frames = output.frames,
        mean_bytes_per_sec = (output.bytes as f64 / lived.as_secs_f64()) as u64,
        peak_bytes_per_sec = output.peak_bytes_per_second as u64,
        probes_answered = probes.answered(),
        probes_lost = probes.lost,
        probe_retired = probes.retired,
        rtt_fastest_ms = probes.fastest().map_or(0, whole_ms),
        rtt_none_n = probes.none.count,
        rtt_none_avg_ms = avg_ms(probes.none),
        rtt_none_max_ms = whole_ms(probes.none.slowest),
        rtt_small_n = probes.small.count,
        rtt_small_avg_ms = avg_ms(probes.small),
        rtt_small_max_ms = whole_ms(probes.small.slowest),
        rtt_medium_n = probes.medium.count,
        rtt_medium_avg_ms = avg_ms(probes.medium),
        rtt_medium_max_ms = whole_ms(probes.medium.slowest),
        rtt_large_n = probes.large.count,
        rtt_large_avg_ms = avg_ms(probes.large),
        rtt_large_max_ms = whole_ms(probes.large.slowest),
        "session wire summary"
    );
}

/// What the render loop should do next.
#[derive(Debug, PartialEq, Eq)]
enum RenderAction {
    /// World tick fired — advance animations and render.
    AdvanceWorld,
    /// Input throttle elapsed — render without advancing world time.
    Render,
    /// No render this iteration; loop back to waiting.
    Skip,
}

/// Picks the next action for the render loop. Three wake sources are polled
/// `biased` so world tick wins on ties (avoids starving animations under a
/// keystroke flood):
///
/// - `sleep_until(world_deadline)`: the adaptive world tick came due
///   (`App::wake_hint` set the deadline); advance animations + render +
///   ship frame.
/// - `sleep_until(prev + MIN_RENDER_GAP)`: the throttle window for a
///   previously-noticed input has elapsed; render without advancing world
///   time. Only armed when `input_pending` is true.
/// - `signal.notify.notified()`: input/resize happened. Arm the throttle iff
///   `dirty` is actually set — a stored permit from input already covered by
///   an earlier render has `dirty == false` and is silently eaten here.
async fn next_render_action(
    world_deadline: Instant,
    signal: &RenderSignal,
    input_pending: &mut bool,
    previous_render: Option<Instant>,
) -> RenderAction {
    tokio::select! {
        biased;
        _ = tokio::time::sleep_until(world_deadline.into()) => {
            // A world-tick render also satisfies any pending input render.
            *input_pending = false;
            RenderAction::AdvanceWorld
        }
        _ = tokio::time::sleep_until(
            previous_render
                .map(|t| t + MIN_RENDER_GAP)
                .unwrap_or_else(Instant::now)
                .into(),
        ), if *input_pending => {
            *input_pending = false;
            RenderAction::Render
        }
        _ = signal.notify.notified(), if !*input_pending => {
            if signal.dirty.load(Ordering::Acquire) {
                *input_pending = true;
            }
            RenderAction::Skip
        }
    }
}

/// Everything about a session's output channel that stays fixed for the
/// lifetime of its render loop.
struct RenderContext {
    handle: russh::server::Handle,
    channel_id: ChannelId,
    frame_drop_log_every: u64,
    signal: Arc<RenderSignal>,
    budget: Arc<OutputBudget>,
    client: metrics::SessionClient,
    probe: Arc<TerminalProbe>,
}

/// What one render pass produced: whether the app asked to quit, how soon
/// the next world tick is needed (`App::wake_hint` read under the same
/// lock, after any draw, so it sees the slots the draw recorded), whether
/// the pass drew a frame or skipped clean (feeds the loop's debug stats
/// line), the bytes it handed to the channel (frame plus terminal
/// commands; feeds the output rate and the probe's backlog label), and how
/// long it held the app lock (`None` on quit; feeds the slow-pass warning).
struct RenderOutcome {
    should_quit: bool,
    wake_hint: Duration,
    drew: bool,
    bytes_sent: usize,
    pass: Option<RenderPass>,
}

impl RenderOutcome {
    fn quit() -> Self {
        Self {
            should_quit: true,
            // Unused: the loop breaks on quit.
            wake_hint: crate::app::tick::IDLE_TICK,
            drew: false,
            bytes_sent: 0,
            pass: None,
        }
    }
}

/// How long one pass held the app lock (input drain, tick, draw) and the
/// screen it ended on.
#[derive(Debug, Clone, Copy)]
struct RenderPass {
    took: Duration,
    screen: crate::app::common::primitives::Screen,
}

/// Where the session was when a pass drew, for attributing the bytes it
/// sent (`late_ssh_output_bytes_total`).
#[derive(Debug, Clone, Copy)]
struct OutputSpot {
    screen: crate::app::common::primitives::Screen,
    place: metrics::Place,
    presence: metrics::Presence,
}

impl OutputSpot {
    fn record(self, kind: metrics::OutputKind, bytes: usize) {
        metrics::record_output_bytes(self.screen, self.place, self.presence, kind, bytes);
    }
}

/// Whether a pass this slow is worth a warning: it ran past [`SLOW_PASS`]
/// and the session has not warned within [`SLOW_PASS_WARN_EVERY`].
fn slow_pass_due(took: Duration, last_warned: Option<Instant>, now: Instant) -> bool {
    if took <= SLOW_PASS {
        return false;
    }
    match last_warned {
        None => true,
        Some(at) => now.duration_since(at) >= SLOW_PASS_WARN_EVERY,
    }
}

/// How one pass's input drain ended.
#[derive(Debug, PartialEq, Eq)]
enum InputDrain {
    /// Nothing was queued.
    Empty,
    /// Every queued message was handled.
    Drained,
    /// The budget ran out with input still queued. The signal is marked dirty
    /// and woken, so the next pass picks the rest up after the render gap.
    BudgetHit,
    /// The app stopped running partway through.
    Quit,
}

/// Hands queued input to `handle` until the queue runs dry, `handle` reports
/// the app stopped (it returns whether the app is still running), or the
/// pass's budget is spent: `max_messages` handled or `max_time` spent
/// handling them, whichever comes first. At least one message is handled per
/// call, so a pass always makes progress.
fn drain_input(
    input_rx: &mut tokio::sync::mpsc::Receiver<Vec<u8>>,
    signal: &RenderSignal,
    max_messages: usize,
    max_time: Duration,
    mut handle: impl FnMut(&[u8]) -> bool,
) -> InputDrain {
    let started = Instant::now();
    let mut handled = 0;
    loop {
        let spent = handled >= max_messages || started.elapsed() >= max_time;
        if handled > 0 && spent {
            if input_rx.is_empty() {
                return InputDrain::Drained;
            }
            signal.wake();
            return InputDrain::BudgetHit;
        }
        let Ok(data) = input_rx.try_recv() else {
            return match handled {
                0 => InputDrain::Empty,
                _ => InputDrain::Drained,
            };
        };
        handled += 1;
        if !handle(&data) {
            return InputDrain::Quit;
        }
    }
}

async fn render_once(
    app: &Arc<TokioMutex<crate::app::state::App>>,
    input_rx: &mut tokio::sync::mpsc::Receiver<Vec<u8>>,
    ctx: &RenderContext,
    advance_world: bool,
) -> anyhow::Result<RenderOutcome> {
    let reason = match advance_world {
        true => metrics::RenderReason::WorldTick,
        false => metrics::RenderReason::Input,
    };
    let (frame, terminal_commands, mut wake_hint, pass, spot) = {
        let mut app = app.lock().await;
        let locked_at = Instant::now();
        if !app.running {
            return Ok(RenderOutcome::quit());
        }
        // Clear `dirty` before draining the queued input so any input arriving
        // during this render flips it back to `true` and schedules another
        // pass instead of being erased by this batch. Remember the old value:
        // it is one of the gate's changed sources.
        let mut changed = ctx.signal.dirty.swap(false, Ordering::AcqRel);
        let drain = drain_input(
            input_rx,
            &ctx.signal,
            INPUT_DRAIN_MAX_MESSAGES,
            INPUT_DRAIN_MAX_TIME,
            |data| {
                app.handle_input(data);
                app.running
            },
        );
        match drain {
            InputDrain::Empty => {}
            InputDrain::Drained => changed = true,
            InputDrain::BudgetHit => {
                metrics::record_render_input_budget_hit();
                changed = true;
            }
            InputDrain::Quit => return Ok(RenderOutcome::quit()),
        }
        if advance_world {
            changed |= app.tick();
        }
        // Dirty gate: a world tick that changed nothing render-visible skips
        // the draw entirely. Ratatui's diff state does not advance on a skip,
        // so the next real frame needs no forced repaint.
        if !changed {
            metrics::record_render_skipped_clean();
            let pass = RenderPass {
                took: locked_at.elapsed(),
                screen: app.screen,
            };
            metrics::record_render_pass(reason, pass.screen, pass.took.as_secs_f64());
            return Ok(RenderOutcome {
                should_quit: false,
                wake_hint: app.wake_hint(),
                drew: false,
                bytes_sent: 0,
                pass: Some(pass),
            });
        }
        metrics::record_render(reason);
        let frame = app.render().context("rendering frame")?;
        let terminal_commands = std::mem::take(&mut app.pending_terminal_commands);
        let wake_hint = app.wake_hint();
        let pass = RenderPass {
            took: locked_at.elapsed(),
            screen: app.screen,
        };
        metrics::record_render_pass(reason, pass.screen, pass.took.as_secs_f64());
        let spot = OutputSpot {
            screen: app.screen,
            place: app.attention_place(),
            presence: app.attention_presence(),
        };
        (frame, terminal_commands, wake_hint, pass, spot)
    };

    let frame_len = frame.len();
    let mut bytes_sent = 0;
    let frame_sent = match timeout(
        Duration::from_millis(50),
        ctx.handle.data(ctx.channel_id, frame),
    )
    .await
    {
        Ok(Ok(())) => {
            ctx.budget.record_sent(frame_len);
            metrics::record_frame_bytes(reason, frame_len);
            spot.record(metrics::OutputKind::Frame, frame_len);
            bytes_sent += frame_len;
            true
        }
        Ok(Err(err)) => {
            return Err(anyhow::anyhow!(
                "render_once: handle send failed: {:?}",
                err
            ));
        }
        Err(_) => {
            let drops = FRAME_DROP_COUNT.fetch_add(1, Ordering::Relaxed) + 1;
            metrics::record_render_frame_drop();
            if drops.is_multiple_of(ctx.frame_drop_log_every) {
                tracing::debug!(drops, "frame drops (handle busy)");
            }
            false
        }
    };

    if !frame_sent {
        // `app.render()` already advanced ratatui's diff buffers. If the SSH
        // write is dropped, force the next successful frame to repaint from a
        // blank previous buffer so old terminal cells cannot leak through.
        // Deliberately no notify: the repaint rides the next world tick,
        // pinned to the hot cadence (15fps retry) instead of re-arming the
        // input path against a handle that is already busy.
        let mut app = app.lock().await;
        app.force_full_repaint();
        ctx.signal.dirty.store(true, Ordering::Release);
        wake_hint = crate::app::tick::HOT_TICK;
    }

    for command in terminal_commands {
        let command_len = command.len();
        match timeout(
            Duration::from_millis(50),
            ctx.handle.data(ctx.channel_id, command),
        )
        .await
        {
            Ok(Ok(())) => {
                ctx.budget.record_sent(command_len);
                spot.record(metrics::OutputKind::Command, command_len);
                bytes_sent += command_len;
            }
            Ok(Err(err)) => {
                return Err(anyhow::anyhow!(
                    "render_once: terminal command send failed: {:?}",
                    err
                ));
            }
            Err(_) => {
                let drops = FRAME_DROP_COUNT.fetch_add(1, Ordering::Relaxed) + 1;
                metrics::record_render_frame_drop();
                if drops.is_multiple_of(ctx.frame_drop_log_every) {
                    tracing::debug!(drops, "frame drops (handle busy)");
                }
                // A dropped command is most often one chunk of an image
                // transmission, and the placement diff already believes that
                // image is on screen: nothing would ever resend it. Treat it
                // like a dropped frame, so the next frame re-emits every
                // raster from a clean slate, and abandon the rest of this
                // batch rather than piling more chunks onto a busy handle.
                // The output-budget guard in the render loop keeps a link
                // that stays busy from turning this retry into a flood.
                let mut app = app.lock().await;
                app.force_full_repaint();
                ctx.signal.dirty.store(true, Ordering::Release);
                wake_hint = crate::app::tick::HOT_TICK;
                break;
            }
        }
    }

    Ok(RenderOutcome {
        should_quit: false,
        wake_hint,
        drew: true,
        bytes_sent,
        pass: Some(pass),
    })
}

/// Expires a lost probe, then sends the next one if it is due, labelled by
/// what the session wrote in the second before it, this pass's
/// `bytes_written` included (`terminal_probe.rs`). The probe only keeps
/// time; its telemetry lives here and in the `data` handler.
async fn drive_terminal_probe(ctx: &RenderContext, bytes_written: usize) {
    let now = Instant::now();
    match ctx.probe.expire(now) {
        None => {}
        Some(ProbeExpiry::Unanswered) => {
            metrics::record_terminal_probe(ctx.client, metrics::TerminalProbeOutcome::Unanswered);
        }
        Some(ProbeExpiry::Retired) => {
            metrics::record_terminal_probe(ctx.client, metrics::TerminalProbeOutcome::Unanswered);
            tracing::info!("terminal never answered the round-trip probe; probing stopped");
        }
    }
    if !ctx.probe.try_start(now, bytes_written) {
        return;
    }
    match timeout(
        Duration::from_millis(50),
        ctx.handle.data(ctx.channel_id, PROBE_QUERY.to_vec()),
    )
    .await
    {
        Ok(Ok(())) => ctx.budget.record_sent(PROBE_QUERY.len()),
        // A busy or closed channel: the query never left, so nothing is in
        // flight. A closed one ends the loop on the next frame send.
        Ok(Err(_)) | Err(_) => ctx.probe.cancel(Instant::now()),
    }
}

/// A probe still in flight when the session quits would be answered after
/// the user's ssh exits, and the answer would land in their shell as stray
/// characters. Hold the goodbye until the answer is in, briefly.
async fn settle_terminal_probe(probe: &TerminalProbe) {
    let deadline = Instant::now() + PROBE_SETTLE_MAX;
    while probe.in_flight() && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// What the session's terminal sits behind, for the `client` metric label.
/// The old subprocess CLI mode announces itself with the `LATE_CLI_MODE` env;
/// the native and openssh modes run the token exec instead. Either marks a
/// CLI.
pub(crate) fn session_client(cli_mode: bool, cli_token_served: bool) -> metrics::SessionClient {
    match cli_mode || cli_token_served {
        true => metrics::SessionClient::Cli,
        false => metrics::SessionClient::Ssh,
    }
}

async fn clean_disconnect(handle: &russh::server::Handle, channel_id: ChannelId) {
    let exit = App::leave_alt_screen();
    let _ = timeout(Duration::from_millis(50), handle.data(channel_id, exit)).await;
    let _ = timeout(
        Duration::from_millis(50),
        handle.data(channel_id, EXIT_MESSAGE.as_bytes().to_vec()),
    )
    .await;
    let _ = handle.eof(channel_id).await;
    let _ = handle.close(channel_id).await;
}

// Updated helper to take State
/// Returns `(user, is_new)` — `is_new` is true when the user was just created.
async fn ensure_user(state: &State, fingerprint: &str) -> Result<(User, bool)> {
    tracing::debug!(fingerprint, "ensuring user exists");
    let client = state.db.get().await?;
    let row = User::find_by_fingerprint(&client, fingerprint).await?;
    let (user, is_new_user) = match row {
        Some(row) => {
            if let Err(e) = User::update_last_seen(&mut row.clone(), &client).await {
                tracing::warn!(error = ?e, "failed to update last_seen for user");
            }
            if let Err(e) =
                late_core::models::user_ssh_key::UserSshKey::ensure(&client, row.id, fingerprint)
                    .await
            {
                tracing::warn!(error = ?e, "failed to ensure ssh key for user");
            }
            (row, false)
        }
        None => {
            // Bounded retry: next_generated_username draws from a 213k-combination
            // space against a fresh occupied snapshot, so a unique violation means
            // a concurrent signup claimed the name between select and insert. A
            // few attempts converge; if they somehow do not, reject auth rather
            // than spin the loop against the DB on the auth hot path.
            const MAX_USERNAME_ATTEMPTS: usize = 5;
            let mut created = None;
            for _ in 0..MAX_USERNAME_ATTEMPTS {
                let username = usernames::next_generated_username(&client).await?;
                match User::create(
                    &client,
                    UserParams {
                        fingerprint: fingerprint.to_string(),
                        username,
                        settings: json!({}),
                    },
                )
                .await
                {
                    Ok(user) => {
                        created = Some(user);
                        break;
                    }
                    Err(error) if is_username_unique_violation(&error) => {
                        tracing::debug!("generated username was claimed concurrently; retrying");
                    }
                    Err(error) => return Err(error),
                }
            }
            let user = created.ok_or_else(|| {
                anyhow::anyhow!("could not allocate a unique username after retries")
            })?;
            late_core::models::user_ssh_key::UserSshKey::ensure(&client, user.id, fingerprint)
                .await?;
            match state.chat_service.auto_join_public_rooms(user.id).await {
                Ok(joined) => {
                    tracing::debug!(
                        user_id = %user.id,
                        joined,
                        "seeded auto-join chat rooms for newly created user"
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        user_id = %user.id,
                        error = ?e,
                        "failed to seed auto-join chat rooms for newly created user"
                    );
                }
            }
            // A first login never shows the back catalog as unread: the new
            // user has read the feed as it stands.
            if let Err(e) = ArticleRead::mark_feed_read(&client, user.id).await {
                tracing::warn!(
                    user_id = %user.id,
                    error = ?e,
                    "failed to seed news reads for newly created user"
                );
            }
            (user, true)
        }
    };

    Ok((user, is_new_user))
}

fn is_username_unique_violation(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        cause
            .downcast_ref::<tokio_postgres::Error>()
            .and_then(tokio_postgres::Error::as_db_error)
            .is_some_and(|db_error| {
                db_error.code() == &tokio_postgres::error::SqlState::UNIQUE_VIOLATION
                    && db_error.constraint() == Some("idx_users_username_lower")
            })
    })
}

async fn has_active_server_ban_before_user_lookup(
    client: &tokio_postgres::Client,
    fingerprint: &str,
    peer_ip: Option<IpAddr>,
) -> Result<bool> {
    if let Some(peer_ip) = peer_ip
        && ServerBan::find_active_for_ip_address(client, &peer_ip.to_string())
            .await?
            .is_some()
    {
        return Ok(true);
    }
    Ok(ServerBan::find_active_for_fingerprint(client, fingerprint)
        .await?
        .is_some())
}

fn late_ssh_theme_id(settings: &Value) -> String {
    extract_theme_id(settings).unwrap_or_else(|| theme::DEFAULT_ID.to_string())
}

fn reject_publickey_only() -> Auth {
    Auth::Reject {
        proceed_with_methods: Some(MethodSet::from(&[MethodKind::PublicKey][..])),
        partial_success: false,
    }
}

#[cfg(test)]
#[path = "ssh_internal_test.rs"]
mod ssh_internal_test;
