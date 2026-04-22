use anyhow::{Context, Result};
use late_core::MutexRecover;
use late_core::models::user::{User, UserParams, extract_theme_id};
use russh::keys::PrivateKey;
use russh::server::{Auth, Msg, Session};
use russh::*;
use serde_json::{Value, json};
#[cfg(unix)]
use std::fs::Permissions;
use std::net::{IpAddr, SocketAddr};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{self, Duration, Instant};
use tokio::io::AsyncReadExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Mutex as TokioMutex, Notify, OwnedSemaphorePermit};
use tokio::task::JoinSet;
use tokio::time::{MissedTickBehavior, timeout};

use crate::app::state::{App, SessionConfig};
use crate::metrics;
use crate::state::{ActivityEvent, State};

static FRAME_DROP_COUNT: AtomicU64 = AtomicU64::new(0);
const PROXY_V1_MAX_LEN: usize = 108;
const PROXY_HEADER_TIMEOUT: Duration = Duration::from_millis(250);
const CLI_MODE_ENV: &str = "LATE_CLI_MODE";
const CLI_TOKEN_PREFIX: &str = "LATE_SESSION_TOKEN=";
const CLI_TOKEN_REQUEST: &str = "late-cli-token-v1";
const EXIT_MESSAGE: &str = "\r\nStay late. Code safe. ✨\r\n";

/// World tick advances animations, game clocks, splash timer, visualizer
/// decay, etc. Keeps the rate users see animations at before this commit.
const WORLD_TICK_INTERVAL: Duration = Duration::from_millis(66);
/// Minimum wall-clock gap between any two consecutive renders. Bounds the
/// per-session render rate so that keystroke floods or other signal sources
/// can't drive renders faster than this.
const MIN_RENDER_GAP: Duration = Duration::from_millis(15);

/// Paired "there is unrendered input" flag + wakeup. `Notify` is just the
/// alarm clock; `dirty` is the source of truth. `dirty` is written under the
/// app mutex so any render that subsequently grabs the same mutex either
/// already covered the input or will see `dirty = true` and know to render.
/// Using `Notify` alone leaves a stored permit after a batched render,
/// causing one spurious identical frame per typing burst.
struct RenderSignal {
    dirty: AtomicBool,
    notify: Notify,
}

impl RenderSignal {
    fn new() -> Self {
        Self {
            dirty: AtomicBool::new(false),
            notify: Notify::new(),
        }
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

    /// Connection metadata
    transport_peer_addr: Option<std::net::SocketAddr>,
    peer_addr: Option<std::net::SocketAddr>,
    peer_ip: Option<IpAddr>,
    _conn_permit: Option<OwnedSemaphorePermit>,
    per_ip_incremented: bool,
    active_user_incremented: bool,
    over_limit: bool,

    /// Activity feed
    activity_feed_rx: Option<tokio::sync::broadcast::Receiver<ActivityEvent>>,

    /// Session bindings
    channel: Option<Channel<Msg>>,
    app: Option<Arc<TokioMutex<crate::app::state::App>>>,
    /// Signaled by input/resize paths to request an immediate (world-stateless)
    /// render, so typed characters echo without waiting for the next world tick.
    render_signal: Option<Arc<RenderSignal>>,
    cli_mode: bool,
    session_token: Option<String>,
    session_rx: Option<tokio::sync::mpsc::Receiver<crate::session::SessionMessage>>,
}

pub fn load_or_generate_key(state: &State) -> anyhow::Result<PrivateKey> {
    use russh::keys::ssh_key::LineEnding;
    let path = &state.config.server_key_path;

    if path.exists() {
        let key = russh::keys::load_secret_key(path, None)?;
        tracing::info!(path = %path.display(), "loaded existing server key");
        Ok(key)
    } else {
        let key = PrivateKey::random(&mut rand_core::OsRng, russh::keys::Algorithm::Ed25519)?;
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
        keys,
        window_size: 8 * 1024 * 1024, // 8MB window size
        event_buffer_size: 128,
        ..Default::default()
    };
    let config = Arc::new(config);

    let addr = socket.local_addr()?;
    tracing::info!(address = %addr, "ssh server listening");

    let server = Server { state };
    let mut session_tasks = JoinSet::new();
    if server.state.config.ssh_proxy_protocol
        && server.state.config.ssh_proxy_trusted_cidrs.is_empty()
    {
        tracing::warn!(
            "ssh proxy protocol is enabled but LATE_SSH_PROXY_TRUSTED_CIDRS is empty; \
             proxy headers will be rejected"
        );
    }

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
                                return;
                            }
                        };

                    let handler = server.new_client_with_addrs(Some(transport_peer_addr), proxied_addr);
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

impl Server {
    fn new_client_with_addrs(
        &self,
        transport_peer_addr: Option<SocketAddr>,
        proxied_addr: Option<SocketAddr>,
    ) -> ClientHandler {
        metrics::record_ssh_connection();
        let permit = self.state.conn_limit.clone().try_acquire_owned().ok();
        let mut over_limit = permit.is_none();
        let effective_peer_addr = proxied_addr.or(transport_peer_addr);
        let peer_ip = effective_peer_addr.map(|addr| addr.ip());
        let mut per_ip_incremented = false;

        if over_limit {
            tracing::info!(
                ?transport_peer_addr,
                ?effective_peer_addr,
                "connection limit reached, rejecting new client"
            );
        } else if let Some(ip) = peer_ip {
            if !self.state.ssh_attempt_limiter.allow(ip) {
                over_limit = true;
                tracing::warn!(
                    ?ip,
                    max_attempts = self.state.ssh_attempt_limiter.max_attempts(),
                    window_secs = self.state.ssh_attempt_limiter.window_secs(),
                    "ssh rate limit exceeded for peer ip"
                );
            }

            let mut counts = self.state.conn_counts.lock_recover();
            if !over_limit {
                let count = counts.entry(ip).or_insert(0);
                if *count >= self.state.config.max_conns_per_ip {
                    over_limit = true;
                    tracing::warn!(
                        ?ip,
                        limit = self.state.config.max_conns_per_ip,
                        "per-ip limit reached, rejecting new client"
                    );
                } else {
                    *count += 1;
                    per_ip_incremented = true;
                }
            }
        }

        tracing::debug!(
            ?transport_peer_addr,
            ?effective_peer_addr,
            "new client connection"
        );
        ClientHandler {
            state: self.state.clone(),
            user: None,
            is_new_user: false,
            activity_feed_rx: None,
            transport_peer_addr,
            peer_addr: effective_peer_addr,
            peer_ip,
            _conn_permit: permit,
            per_ip_incremented,
            active_user_incremented: false,
            over_limit,
            channel: None,
            app: None,
            render_signal: None,
            cli_mode: false,
            session_token: None,
            session_rx: None,
        }
    }
}

impl russh::server::Server for Server {
    type Handler = ClientHandler;

    fn new_client(&mut self, peer_addr: Option<std::net::SocketAddr>) -> ClientHandler {
        self.new_client_with_addrs(peer_addr, peer_addr)
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

    read_proxy_v1_client_addr(stream, PROXY_HEADER_TIMEOUT).await
}

fn is_trusted_proxy_peer(state: &State, ip: IpAddr) -> bool {
    state
        .config
        .ssh_proxy_trusted_cidrs
        .iter()
        .any(|cidr| cidr.contains(&ip))
}

async fn read_proxy_v1_client_addr(
    stream: &mut TcpStream,
    timeout_duration: Duration,
) -> Result<Option<SocketAddr>> {
    let mut line = Vec::with_capacity(PROXY_V1_MAX_LEN);
    let mut byte = [0u8; 1];

    let read_future = async {
        while line.len() < PROXY_V1_MAX_LEN {
            stream.read_exact(&mut byte).await?;
            line.push(byte[0]);
            if line.len() >= 2 && line[line.len() - 2..] == *b"\r\n" {
                return parse_proxy_v1_addr(&line);
            }
        }
        anyhow::bail!(
            "proxy protocol v1 header exceeded {} bytes",
            PROXY_V1_MAX_LEN
        );
    };

    match timeout(timeout_duration, read_future).await {
        Ok(Ok(addr)) => Ok(addr),
        Ok(Err(e)) => Err(e.context("failed to read proxy protocol header")),
        Err(_) => anyhow::bail!("timed out waiting for proxy protocol header"),
    }
}

fn parse_proxy_v1_addr(line: &[u8]) -> Result<Option<SocketAddr>> {
    let text = std::str::from_utf8(line).context("proxy v1 header is not valid UTF-8")?;
    let text = text
        .strip_suffix("\r\n")
        .ok_or_else(|| anyhow::anyhow!("proxy v1 header missing CRLF terminator"))?;
    let parts: Vec<&str> = text.split_whitespace().collect();
    if parts.len() < 2 || parts[0] != "PROXY" {
        anyhow::bail!("proxy v1 header malformed");
    }
    match parts[1] {
        "UNKNOWN" => Ok(None),
        "TCP4" | "TCP6" => {
            if parts.len() != 6 {
                anyhow::bail!("proxy v1 TCP header has unexpected field count");
            }
            let src_ip: IpAddr = parts[2]
                .parse()
                .with_context(|| format!("invalid proxy v1 source IP '{}'", parts[2]))?;
            let src_port: u16 = parts[4]
                .parse()
                .with_context(|| format!("invalid proxy v1 source port '{}'", parts[4]))?;
            Ok(Some(SocketAddr::new(src_ip, src_port)))
        }
        fam => anyhow::bail!("unsupported proxy v1 protocol family '{fam}'"),
    }
}

impl Drop for ClientHandler {
    fn drop(&mut self) {
        if self.app.is_none()
            && let Some(token) = self.session_token.clone()
        {
            let registry = self.state.session_registry.clone();
            tokio::spawn(async move {
                registry.unregister(&token).await;
            });
        }

        if self.active_user_incremented
            && let Some(user) = self.user.as_ref()
        {
            metrics::add_ssh_session(-1);
            let mut active_users = self.state.active_users.lock_recover();

            if let Some(active) = active_users.get_mut(&user.id) {
                if active.connection_count <= 1 {
                    active_users.remove(&user.id);
                } else {
                    active.connection_count -= 1;
                }
            }
        }

        if self.over_limit || !self.per_ip_incremented {
            return;
        }
        let Some(ip) = self.peer_ip else {
            return;
        };
        let mut counts = self.state.conn_counts.lock_recover();
        if let Some(count) = counts.get_mut(&ip) {
            if *count <= 1 {
                counts.remove(&ip);
            } else {
                *count -= 1;
            }
        }
    }
}

impl ClientHandler {
    async fn ensure_cli_session(&mut self) -> Result<String> {
        if let Some(token) = self.session_token.clone() {
            return Ok(token);
        }

        let session_token = crate::session::new_session_token();
        let (session_tx, session_rx) = tokio::sync::mpsc::channel(64);
        self.state
            .session_registry
            .register(session_token.clone(), session_tx)
            .await;
        self.session_token = Some(session_token.clone());
        self.session_rx = Some(session_rx);
        Ok(session_token)
    }
}

impl russh::server::Handler for ClientHandler {
    type Error = anyhow::Error;

    #[tracing::instrument(skip(self, key), fields(peer = ?self.peer_addr, transport = ?self.transport_peer_addr))]
    async fn auth_publickey(
        &mut self,
        user: &str,
        key: &russh::keys::PublicKey,
    ) -> Result<Auth, Self::Error> {
        tracing::debug!(user, "public key auth accepted");
        if self.over_limit {
            tracing::debug!(user, "connection over limit, rejecting auth");
            return Ok(reject_publickey_only());
        }
        if !self.state.config.open_access {
            tracing::debug!(user, "open access disabled, rejecting public key auth");
            return Ok(reject_publickey_only());
        }
        let fingerprint = key.fingerprint(keys::HashAlg::Sha256).to_string();
        let (user, is_new_user) =
            match crate::ssh::ensure_user(&self.state, user, &fingerprint).await {
                Ok(pair) => pair,
                Err(e) => {
                    tracing::warn!(error = ?e, "failed to ensure user, rejecting auth");
                    return Ok(reject_publickey_only());
                }
            };
        self.is_new_user = is_new_user;
        if !self.active_user_incremented {
            let mut active_users = self.state.active_users.lock_recover();

            if let Some(active) = active_users.get_mut(&user.id) {
                active.connection_count += 1;
                active.username = user.username.clone();
                active.last_login_at = std::time::Instant::now();
            } else {
                active_users.insert(
                    user.id,
                    crate::state::ActiveUser {
                        username: user.username.clone(),
                        connection_count: 1,
                        last_login_at: std::time::Instant::now(),
                    },
                );
            }
            self.active_user_incremented = true;
            metrics::add_ssh_session(1);
        }

        let username = user.username.clone();

        tracing::info!(
            username = %username,
            fingerprint = %fingerprint,
            "user connected"
        );

        self.user = Some(user);
        self.activity_feed_rx = Some(self.state.activity_feed.subscribe());
        let _ = self.state.activity_feed.send(ActivityEvent {
            username,
            action: "joined".to_string(),
            at: time::Instant::now(),
        });
        Ok(Auth::Accept)
    }

    #[tracing::instrument(skip(self, _response), fields(peer = ?self.peer_addr, transport = ?self.transport_peer_addr))]
    async fn auth_keyboard_interactive(
        &mut self,
        user: &str,
        _submethods: &str,
        _response: Option<russh::server::Response<'_>>,
    ) -> Result<Auth, Self::Error> {
        tracing::debug!(
            user,
            "keyboard-interactive auth rejected: public key auth is required"
        );
        Ok(reject_publickey_only())
    }

    #[tracing::instrument(skip(self, _password), fields(peer = ?self.peer_addr, transport = ?self.transport_peer_addr))]
    async fn auth_password(&mut self, user: &str, _password: &str) -> Result<Auth, Self::Error> {
        tracing::debug!(user, "password auth rejected: public key auth is required");
        Ok(reject_publickey_only())
    }

    #[tracing::instrument(skip(self, channel, _session), fields(peer = ?self.peer_addr, transport = ?self.transport_peer_addr))]
    async fn channel_open_session(
        &mut self,
        channel: Channel<Msg>,
        _session: &mut Session,
    ) -> Result<bool, Self::Error> {
        tracing::debug!("session channel opened");
        if self.over_limit {
            tracing::debug!("connection over limit, rejecting channel open");
            return Ok(false);
        }
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
        let session_token = self.ensure_cli_session().await?;
        let session_rx = self
            .session_rx
            .take()
            .ok_or_else(|| anyhow::anyhow!("cli session receiver missing during pty request"))?;

        let article_service = self.state.article_service.clone();
        let vote_service = self.state.vote_service.clone();
        let chat_service = self.state.chat_service.clone();
        let profile_service = self.state.profile_service.clone();
        let twenty_forty_eight_service = self.state.twenty_forty_eight_service.clone();
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

        let my_vote = match self.state.vote_service.get_user_vote(user_id).await {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(error = ?e, "failed to get user vote");
                None
            }
        };

        let initial_2048_game = match self
            .state
            .twenty_forty_eight_service
            .load_game(user_id)
            .await
        {
            Ok(g) => g,
            Err(e) => {
                tracing::warn!(error = ?e, "failed to load 2048 game state");
                None
            }
        };
        let initial_2048_high_score = match self
            .state
            .twenty_forty_eight_service
            .load_high_score(user_id)
            .await
        {
            Ok(score) => score,
            Err(e) => {
                tracing::warn!(error = ?e, "failed to load 2048 high score");
                None
            }
        };
        let initial_tetris_game = match self.state.tetris_service.load_game(user_id).await {
            Ok(game) => game,
            Err(e) => {
                tracing::warn!(error = ?e, "failed to load tetris game state");
                None
            }
        };
        let initial_tetris_high_score =
            match self.state.tetris_service.load_high_score(user_id).await {
                Ok(score) => score,
                Err(e) => {
                    tracing::warn!(error = ?e, "failed to load tetris high score");
                    None
                }
            };

        let initial_sudoku_games = match self.state.sudoku_service.load_games(user_id).await {
            Ok(g) => g,
            Err(e) => {
                tracing::warn!(error = ?e, "failed to load sudoku game states");
                Vec::new()
            }
        };
        let initial_nonogram_games = match self.state.nonogram_service.load_games(user_id).await {
            Ok(games) => games,
            Err(e) => {
                tracing::warn!(error = ?e, "failed to load nonogram game states");
                Vec::new()
            }
        };
        let initial_solitaire_games = match self.state.solitaire_service.load_games(user_id).await {
            Ok(games) => games,
            Err(e) => {
                tracing::warn!(error = ?e, "failed to load solitaire game states");
                Vec::new()
            }
        };
        let initial_minesweeper_games =
            match self.state.minesweeper_service.load_games(user_id).await {
                Ok(games) => games,
                Err(e) => {
                    tracing::warn!(error = ?e, "failed to load minesweeper game states");
                    Vec::new()
                }
            };
        let initial_bonsai_tree = match self.state.bonsai_service.ensure_tree(user_id).await {
            Ok(tree) => Some(tree),
            Err(e) => {
                tracing::warn!(error = ?e, "failed to load/create bonsai tree");
                None
            }
        };

        // Grant daily chip stipend on login
        let initial_chip_balance = match self.state.chip_service.ensure_chips(user_id).await {
            Ok(chips) => chips.balance,
            Err(e) => {
                tracing::warn!(error = ?e, "failed to grant daily chip stipend");
                0
            }
        };

        let app = crate::app::state::App::new(SessionConfig {
            // Terminal / layout
            cols: col_width as u16,
            rows: row_height as u16,

            // Services / data sources
            vote_service,
            chat_service,
            notification_service: self.state.notification_service.clone(),
            article_service,
            profile_service,
            twenty_forty_eight_service,
            initial_2048_game,
            initial_2048_high_score,
            tetris_service: self.state.tetris_service.clone(),
            initial_tetris_game,
            initial_tetris_high_score,
            sudoku_service,
            initial_sudoku_games,
            nonogram_service,
            initial_nonogram_games,
            solitaire_service,
            initial_solitaire_games,
            minesweeper_service: self.state.minesweeper_service.clone(),
            initial_minesweeper_games,
            blackjack_service: self.state.blackjack_service.clone(),
            bonsai_service: self.state.bonsai_service.clone(),
            initial_bonsai_tree,
            nonogram_library,
            initial_chip_balance,
            leaderboard_rx: Some(self.state.leaderboard_service.subscribe()),

            // Session / connection
            web_url: self.state.config.web_url.clone(),
            session_token,
            session_registry: Some(self.state.session_registry.clone()),
            paired_client_registry: Some(self.state.paired_client_registry.clone()),
            web_chat_registry: Some(self.state.web_chat_registry.clone()),
            session_rx: Some(session_rx),
            now_playing_rx: Some(self.state.now_playing_rx.clone()),
            active_users: Some(self.state.active_users.clone()),
            activity_feed_rx: self.activity_feed_rx.take(),
            user_id,
            is_admin: user.is_admin || self.state.config.force_admin,

            // Voting
            my_vote,
            is_new_user: self.is_new_user,

            // Display config
            initial_theme_id: late_ssh_theme_id(&user.settings),

            // Server state
            is_draining: self.state.is_draining.clone(),
        })
        .context("failed to initialize app for PTY session")?;
        self.app = Some(Arc::new(TokioMutex::new(app)));
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
            let channel_id = chan.id();
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

            let init = App::enter_alt_screen();
            let _ = timeout(Duration::from_millis(50), handle.data(channel_id, init)).await;

            let app = Arc::clone(app);
            let frame_drop_log_every = self.state.config.frame_drop_log_every;
            let signal = Arc::new(RenderSignal::new());
            self.render_signal = Some(Arc::clone(&signal));
            tokio::spawn(async move {
                let mut world_tick = tokio::time::interval(WORLD_TICK_INTERVAL);
                world_tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
                let mut previous_render: Option<Instant> = None;
                let mut input_pending = false;
                loop {
                    let advance_world = match next_render_action(
                        &mut world_tick,
                        &signal,
                        &mut input_pending,
                        previous_render,
                    )
                    .await
                    {
                        RenderAction::AdvanceWorld => true,
                        RenderAction::Render => false,
                        RenderAction::Skip => continue,
                    };
                    match render_once(
                        &app,
                        &handle,
                        channel_id,
                        frame_drop_log_every,
                        advance_world,
                        &signal,
                    )
                    .await
                    {
                        Ok(should_quit) => {
                            previous_render = Some(Instant::now());
                            if should_quit {
                                tracing::debug!("app requested quit, closing connection");
                                clean_disconnect(&handle, channel_id).await;
                                break;
                            }
                        }
                        Err(err) => {
                            tracing::debug!(error = ?err, "error rendering frame, stopping render loop");
                            let _ = handle.eof(channel_id).await;
                            let _ = handle.close(channel_id).await;
                            break;
                        }
                    }
                }
            });
        }
        Ok(())
    }

    #[tracing::instrument(skip(self, data, session), fields(peer = ?self.peer_addr, len = data.len()))]
    async fn data(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        tracing::debug!(len = data.len(), "received input data");
        let Some(app) = self.app.as_ref() else {
            return Ok(());
        };
        {
            let mut app = app.lock().await;
            app.handle_input(data);
            if !app.running {
                tracing::info!("client requested disconnect");
                clean_disconnect(&session.handle(), channel).await;
                return Ok(());
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

    #[tracing::instrument(skip(self, _session), fields(peer = ?self.peer_addr, transport = ?self.transport_peer_addr))]
    async fn channel_eof(
        &mut self,
        channel: ChannelId,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        tracing::debug!(?channel, "client sent channel EOF");
        if let Some(app) = self.app.as_ref() {
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
        if let Some(app) = self.app.as_ref() {
            let mut app = app.lock().await;
            app.running = false;
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
        let Some(app) = self.app.as_ref() else {
            return Ok(());
        };
        {
            let mut app = app.lock().await;
            if let Err(e) = app.resize(col_width as u16, row_height as u16) {
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
/// - `world_tick`: fires every [`WORLD_TICK_INTERVAL`]; advance animations +
///   render + ship frame.
/// - `sleep_until(prev + MIN_RENDER_GAP)`: the throttle window for a
///   previously-noticed input has elapsed; render without advancing world
///   time. Only armed when `input_pending` is true.
/// - `signal.notify.notified()`: input/resize happened. Arm the throttle iff
///   `dirty` is actually set — a stored permit from input already covered by
///   an earlier render has `dirty == false` and is silently eaten here.
async fn next_render_action(
    world_tick: &mut tokio::time::Interval,
    signal: &RenderSignal,
    input_pending: &mut bool,
    previous_render: Option<Instant>,
) -> RenderAction {
    tokio::select! {
        biased;
        _ = world_tick.tick() => {
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

async fn render_once(
    app: &Arc<TokioMutex<crate::app::state::App>>,
    handle: &russh::server::Handle,
    channel_id: ChannelId,
    frame_drop_log_every: u64,
    advance_world: bool,
    signal: &RenderSignal,
) -> anyhow::Result<bool> {
    let (frame, terminal_commands) = {
        let mut app = app.lock().await;
        if !app.running {
            return Ok(true);
        }
        // Clear `dirty` under the same lock that gates input mutations.
        // Any input arriving after we release the lock will re-set `dirty`
        // and be picked up by a subsequent loop iteration.
        signal.dirty.store(false, Ordering::Release);
        if advance_world {
            app.tick();
        }
        let frame = app.render().context("rendering frame")?;
        let terminal_commands = std::mem::take(&mut app.pending_terminal_commands);
        (frame, terminal_commands)
    };

    match timeout(Duration::from_millis(50), handle.data(channel_id, frame)).await {
        Ok(Ok(())) => {}
        Ok(Err(err)) => {
            return Err(anyhow::anyhow!(
                "render_once: handle send failed: {:?}",
                err
            ));
        }
        Err(_) => {
            let drops = FRAME_DROP_COUNT.fetch_add(1, Ordering::Relaxed) + 1;
            metrics::record_render_frame_drop();
            if drops.is_multiple_of(frame_drop_log_every) {
                tracing::debug!(drops, "frame drops (handle busy)");
            }
        }
    }

    for command in terminal_commands {
        match timeout(Duration::from_millis(50), handle.data(channel_id, command)).await {
            Ok(Ok(())) => {}
            Ok(Err(err)) => {
                return Err(anyhow::anyhow!(
                    "render_once: terminal command send failed: {:?}",
                    err
                ));
            }
            Err(_) => {
                let drops = FRAME_DROP_COUNT.fetch_add(1, Ordering::Relaxed) + 1;
                metrics::record_render_frame_drop();
                if drops.is_multiple_of(frame_drop_log_every) {
                    tracing::debug!(drops, "frame drops (handle busy)");
                }
            }
        }
    }

    Ok(false)
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
async fn ensure_user(state: &State, username: &str, fingerprint: &str) -> Result<(User, bool)> {
    tracing::debug!(username, fingerprint, "ensuring user exists");
    let client = state.db.get().await?;
    let row = User::find_by_fingerprint(&client, fingerprint).await?;
    let (user, is_new_user) = match row {
        Some(row) => {
            if let Err(e) = User::update_last_seen(&mut row.clone(), &client).await {
                tracing::warn!(error = ?e, "failed to update last_seen for user");
            }
            (row, false)
        }
        None => {
            let username = User::next_available_username(&client, username).await?;
            let user = User::create(
                &client,
                UserParams {
                    fingerprint: fingerprint.to_string(),
                    username,
                    settings: json!({}),
                },
            )
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
            (user, true)
        }
    };

    Ok((user, is_new_user))
}

fn late_ssh_theme_id(settings: &Value) -> String {
    extract_theme_id(settings).unwrap_or_else(|| "late".to_string())
}

fn reject_publickey_only() -> Auth {
    Auth::Reject {
        proceed_with_methods: Some(MethodSet::from(&[MethodKind::PublicKey][..])),
        partial_success: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn reject_publickey_only_advertises_only_publickey() {
        match reject_publickey_only() {
            Auth::Reject {
                proceed_with_methods,
                partial_success,
            } => {
                assert_eq!(
                    proceed_with_methods,
                    Some(MethodSet::from(&[MethodKind::PublicKey][..]))
                );
                assert!(!partial_success);
            }
            _ => panic!("expected reject auth"),
        }
    }

    #[test]
    fn parse_proxy_v1_tcp4_source_addr() {
        let line = b"PROXY TCP4 203.0.113.10 10.42.0.76 54231 2222\r\n";
        let addr = parse_proxy_v1_addr(line)
            .expect("parse")
            .expect("source addr");
        assert_eq!(
            addr,
            SocketAddr::from_str("203.0.113.10:54231").expect("socket addr")
        );
    }

    #[test]
    fn parse_proxy_v1_unknown_returns_none() {
        let line = b"PROXY UNKNOWN\r\n";
        let addr = parse_proxy_v1_addr(line).expect("parse");
        assert!(addr.is_none());
    }

    #[test]
    fn parse_proxy_v1_rejects_malformed_header() {
        let line = b"PROXY TCP4 203.0.113.10 10.42.0.76 only-one-port\r\n";
        assert!(parse_proxy_v1_addr(line).is_err());
    }

    #[test]
    fn render_signal_starts_clean() {
        let signal = RenderSignal::new();
        assert!(!signal.dirty.load(Ordering::Acquire));
    }

    /// Core regression test for the stored-permit bug: after a render has
    /// cleared `dirty`, a leftover `Notify` permit must NOT re-arm the
    /// throttle. Otherwise every typing burst ends with a spurious render of
    /// an unchanged frame.
    #[tokio::test]
    async fn stale_permit_does_not_arm_throttle() {
        let signal = RenderSignal::new();
        let mut world_tick = tokio::time::interval(Duration::from_secs(100));
        world_tick.tick().await; // consume immediate first tick

        // A prior input rang the bell and was batched into a render; the
        // render cleared `dirty` but the permit is still sitting here.
        signal.notify.notify_one();
        assert!(!signal.dirty.load(Ordering::Acquire));

        let mut input_pending = false;
        let action = next_render_action(
            &mut world_tick,
            &signal,
            &mut input_pending,
            Some(Instant::now()),
        )
        .await;

        assert_eq!(action, RenderAction::Skip);
        assert!(!input_pending, "stale permit must not arm the throttle");
    }

    #[tokio::test]
    async fn dirty_permit_arms_throttle() {
        let signal = RenderSignal::new();
        let mut world_tick = tokio::time::interval(Duration::from_secs(100));
        world_tick.tick().await;

        signal.dirty.store(true, Ordering::Release);
        signal.notify.notify_one();

        let mut input_pending = false;
        let action = next_render_action(
            &mut world_tick,
            &signal,
            &mut input_pending,
            Some(Instant::now()),
        )
        .await;

        assert_eq!(action, RenderAction::Skip);
        assert!(input_pending, "dirty permit must arm the throttle");
    }

    #[tokio::test]
    async fn throttle_fires_immediately_when_gap_elapsed() {
        let signal = RenderSignal::new();
        let mut world_tick = tokio::time::interval(Duration::from_secs(100));
        world_tick.tick().await;

        let mut input_pending = true;
        // Pretend the last render was a long time ago — the throttle is
        // already satisfied and should resolve without any wait.
        let previous_render = Some(Instant::now() - Duration::from_secs(1));

        let start = Instant::now();
        let action = next_render_action(
            &mut world_tick,
            &signal,
            &mut input_pending,
            previous_render,
        )
        .await;
        let elapsed = start.elapsed();

        assert_eq!(action, RenderAction::Render);
        assert!(!input_pending);
        assert!(
            elapsed < Duration::from_millis(5),
            "should fire immediately, actually waited {elapsed:?}"
        );
    }

    #[tokio::test]
    async fn throttle_waits_for_min_render_gap() {
        let signal = RenderSignal::new();
        let mut world_tick = tokio::time::interval(Duration::from_secs(100));
        world_tick.tick().await;

        let mut input_pending = true;
        let previous_render = Some(Instant::now());

        let start = Instant::now();
        let action = next_render_action(
            &mut world_tick,
            &signal,
            &mut input_pending,
            previous_render,
        )
        .await;
        let elapsed = start.elapsed();

        assert_eq!(action, RenderAction::Render);
        // Generous lower bound — timers can fire a tick or two early.
        assert!(
            elapsed >= Duration::from_millis(10),
            "throttle should wait ~{}ms, waited {:?}",
            MIN_RENDER_GAP.as_millis(),
            elapsed
        );
    }

    #[tokio::test]
    async fn world_tick_fires_when_idle() {
        let signal = RenderSignal::new();
        // Interval's first tick is immediate, so this resolves right away.
        let mut world_tick = tokio::time::interval(Duration::from_secs(100));

        let mut input_pending = false;
        let action = next_render_action(&mut world_tick, &signal, &mut input_pending, None).await;

        assert_eq!(action, RenderAction::AdvanceWorld);
    }

    /// When both the throttle timer and a world tick are ready at the same
    /// instant, `biased` ensures world tick wins so animations aren't
    /// starved under a keystroke flood.
    #[tokio::test]
    async fn world_tick_wins_tie_with_throttle() {
        let signal = RenderSignal::new();
        let mut world_tick = tokio::time::interval(Duration::from_millis(1));
        world_tick.tick().await; // consume immediate first tick
        // Let the next world tick come due.
        tokio::time::sleep(Duration::from_millis(5)).await;

        let mut input_pending = true;
        // Throttle is already satisfied too (previous render long ago).
        let previous_render = Some(Instant::now() - Duration::from_secs(1));

        let action = next_render_action(
            &mut world_tick,
            &signal,
            &mut input_pending,
            previous_render,
        )
        .await;

        assert_eq!(
            action,
            RenderAction::AdvanceWorld,
            "world tick must beat the throttle branch under `biased` select"
        );
        assert!(!input_pending);
    }
}
