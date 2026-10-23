//! Per-shell-channel proxy task: dial late-ssh `/tunnel` and pump
//! bytes between the user's SSH channel and the WebSocket.
//!
//! Inbound side (SSH → WS) goes through a single `SshInputEvent`
//! mpsc fed by the russh handler callbacks (`Handler::data` for
//! Bytes, `Handler::window_change_request` for Resize). Russh
//! dispatches both serially in its per-connection task, so the
//! queue carries them in SSH-wire order; the proxy emits WS Binary
//! and Text frames in that order, and WebSocket preserves frame
//! ordering on the wire (RFC 6455). This is what makes coordinate-
//! sensitive features (mouse SGR reports, paste runs, block
//! selections) safe across resizes.
//!
//! Outbound side (WS → SSH) writes binary payloads through a
//! `ChannelTx` writer; we drop the channel's read half entirely so
//! russh's internal data queue stays empty and never stalls.
//!
//! Reconnect: on an abnormal transport drop (1006-equivalent), a
//! late-private retryable WS close (4100-4199), or a transient dial error
//! (TCP I/O, HTTP 5xx), we redial with exponential backoff and a
//! 30-second total budget. Terminal close codes and HTTP 4xx responses end
//! the session.
//!
//! See `devdocs/BASTION-FEATURE.md` for the current contract.

use anyhow::Context;
use futures_util::{SinkExt, StreamExt};
use late_core::tunnel_protocol::{
    ControlFrame, SshInputEvent, TUNNEL_CLOSE_ABNORMAL, TUNNEL_CLOSE_RECONNECT_REQUESTED,
    TUNNEL_CLOSE_SESSION_ENDED,
};
use russh::Channel;
use russh::server::Msg;
use std::pin::Pin;
use std::time::Duration;
use tokio::io::{AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::time::{Instant, MissedTickBehavior, timeout};
use tokio_tungstenite::tungstenite;
use tokio_tungstenite::tungstenite::protocol::Message as WsMessage;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

use crate::handshake::{HandshakeContext, build_request};

/// Bound on the input-event mpsc that carries Bytes + Resize from the
/// russh handler callbacks to the proxy task. Sized for a brief stall
/// in the proxy's WS write path under typical keystroke rates;
/// resize events are sparse and just ride along.
pub const INPUT_QUEUE_CAP: usize = 256;

/// Initial wait before the first reconnect attempt.
const BACKOFF_INITIAL: Duration = Duration::from_millis(100);

/// Cap on a single backoff sleep — after several failures we don't
/// want to wait minutes between attempts.
const BACKOFF_MAX: Duration = Duration::from_secs(5);

/// Total time budget across all reconnect attempts within a single
/// disconnection event. Reset only after actual backend output arrives.
const RECONNECT_BUDGET: Duration = Duration::from_secs(30);

/// How long a reconnect window has to stretch before the bastion
/// writes the plain-text "reconnecting…" message into the user's SSH
/// stream. Below this, the gap is invisible — the new TUI's setup
/// sequences cleanly overwrite a brief blip.
const INITIAL_MESSAGE_DELAY: Duration = Duration::from_millis(500);

/// Threshold for the escalated "still reconnecting…" message. The
/// total reconnect budget is 30s, so the escalation can sit on screen
/// for the bulk of a long outage.
const ESCALATION_MESSAGE_DELAY: Duration = Duration::from_secs(5);

/// Sent before the first plain-text reconnect message:
/// - `\x1b[?1049l` exits any active alt-screen the previous backend's
///   TUI may have entered.
/// - `\x1b[0m` resets attribute (color, bold, …) state.
/// - `\x1b[2J\x1b[H` clears the screen and homes the cursor so the
///   message lands in a known-clean spot.
///
/// This is the *only* point at which the bastion writes its own bytes
/// to the user's terminal; see devdocs/BASTION-FEATURE.md.
const TERMINAL_RESET: &str = "\x1b[?1049l\x1b[0m\x1b[2J\x1b[H";

/// Initial reconnect message; constants for tunability.
const RECONNECTING_MSG: &str = "reconnecting to late.sh\u{2026}\r\n";

/// Reconnect message for an explicit user-requested reload during drain.
const RELOADING_UPDATE_MSG: &str = "waiting for updated late.sh...\r\n";

/// Escalated reconnect message after `ESCALATION_MESSAGE_DELAY`.
const STILL_RECONNECTING_MSG: &str = "still reconnecting\u{2026}\r\n";
const CONNECTING_MSG: &str = "connecting to late.sh\u{2026}\r\n";
const STILL_CONNECTING_MSG: &str = "still connecting\u{2026}\r\n";
const UNAVAILABLE_MSG: &str = "late.sh is unavailable. Please try again shortly.\r\n";
const SESSION_FAILED_MSG: &str =
    "late.sh could not start your session. Please try again later.\r\n";
const ACCESS_REFUSED_MSG: &str = "Access to late.sh was refused.\r\n";
const ACCESS_ENDED_MSG: &str = "Your access to this late.sh session has ended.\r\n";
const RATE_LIMITED_MSG: &str = "Too many connections to late.sh. Please try again shortly.\r\n";

const SHUTDOWN_MSG: &str = "late.sh bastion is restarting; disconnecting.\r\n";

/// Cadence of bastion → backend WS Pings. Tungstenite on the backend
/// auto-replies with Pong, so each ping doubles as a "is the pod
/// alive" probe and a NAT keepalive (the latter mostly irrelevant on
/// in-cluster Service routing, but cheap insurance).
const PING_INTERVAL: Duration = Duration::from_secs(2);

/// If we go this long without *any* inbound frame from the backend
/// (Pong, Binary, Text, …), assume the pod has wedged and break the
/// pump as if we'd seen an abnormal close. The reconnect loop will
/// redial against whatever pod the Service is now pointing to.
///
/// Dimensioned for an in-cluster (bastion → service-ssh-internal) hop:
/// Applied after initial output, excluding time blocked writing to the SSH
/// client. Startup has its own ready deadline and output its own stall bound.
const SILENCE_THRESHOLD: Duration = Duration::from_secs(5);
const EXEC_RESPONSE_TIMEOUT: Duration = Duration::from_secs(10);
const DIAL_TIMEOUT: Duration = Duration::from_secs(5);
const WS_WRITE_TIMEOUT: Duration = Duration::from_secs(5);
const SSH_WRITE_TIMEOUT: Duration = late_core::tunnel_protocol::TUNNEL_OUTPUT_STALL_TIMEOUT;

type TunnelWs = WebSocketStream<MaybeTlsStream<TcpStream>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecTunnelResponse {
    pub stdout: String,
    pub stderr: String,
    pub exit_status: u32,
}

impl ExecTunnelResponse {
    pub fn failure(stderr: impl Into<String>) -> Self {
        Self {
            stdout: String::new(),
            stderr: stderr.into(),
            exit_status: 1,
        }
    }
}

pub struct TunnelConnection {
    ws_url: String,
    secret: String,
    ctx: HandshakeContext,
    ws: TunnelWs,
}

impl TunnelConnection {
    pub async fn connect(
        ws_url: String,
        secret: String,
        ctx: HandshakeContext,
        shutdown: late_core::shutdown::CancellationToken,
    ) -> anyhow::Result<Self> {
        let session_log_id = late_core::tunnel_protocol::session_log_id(&ctx.session_id);
        let req =
            build_request(&ws_url, &secret, &ctx).context("failed to build /tunnel handshake")?;
        let (ws, response) = tokio::select! {
            result = timeout(DIAL_TIMEOUT, tokio_tungstenite::connect_async(req)) => {
                result.context("tunnel websocket connect timed out")?.context("failed to connect tunnel websocket")?
            }
            _ = shutdown.cancelled() => {
                anyhow::bail!("bastion shutdown before tunnel connected");
            }
        };
        tracing::info!(
            %session_log_id,
            status = %response.status(),
            reconnect_reason = ?ctx.reconnect_reason,
            "tunnel ws upgraded"
        );
        Ok(Self {
            ws_url,
            secret,
            ctx,
            ws,
        })
    }

    pub fn update_context(&mut self, ctx: HandshakeContext) {
        self.ctx = ctx;
    }

    pub async fn exec(
        &mut self,
        command: String,
        shutdown: late_core::shutdown::CancellationToken,
    ) -> anyhow::Result<ExecTunnelResponse> {
        let session_log_id = late_core::tunnel_protocol::session_log_id(&self.ctx.session_id);
        let exec_id = uuid::Uuid::now_v7().to_string();
        let frame = ControlFrame::ExecRequest {
            id: exec_id.clone(),
            command: command.clone(),
        }
        .to_json()
        .context("failed to encode tunnel exec request")?;
        tokio::select! {
            result = timeout(WS_WRITE_TIMEOUT, self.ws.send(WsMessage::Text(frame.into()))) => {
                result.context("tunnel exec send timed out")?.context("failed to send tunnel exec request")?;
            }
            _ = shutdown.cancelled() => anyhow::bail!("bastion shutdown during exec send"),
        }

        let response = timeout(EXEC_RESPONSE_TIMEOUT, async {
            loop {
                let msg = tokio::select! {
                    msg = self.ws.next() => msg,
                    _ = shutdown.cancelled() => {
                        return Ok(ExecTunnelResponse::failure("bastion shutdown before exec completed"));
                    }
                };
                let Some(msg) = msg else {
                    anyhow::bail!("tunnel websocket ended before exec response");
                };
                match msg.context("tunnel exec websocket receive failed")? {
                    WsMessage::Text(text) => match ControlFrame::from_json(text.as_str()) {
                        Ok(ControlFrame::ExecResponse {
                            id,
                            stdout,
                            stderr,
                            exit_status,
                        }) if id == exec_id => {
                            return Ok(ExecTunnelResponse {
                                stdout,
                                stderr,
                                exit_status,
                            });
                        }
                        Ok(other) => {
                            tracing::debug!(?other, "ignoring non-matching tunnel exec control frame");
                        }
                        Err(err) => {
                            tracing::warn!(error = ?err, payload = %text.as_str(), "bad tunnel exec control frame");
                        }
                    },
                    WsMessage::Close(frame) => {
                        anyhow::bail!("tunnel websocket closed before exec response: {frame:?}");
                    }
                    WsMessage::Binary(_) => {
                        tracing::debug!("ignoring binary frame during tunnel exec");
                    }
                    WsMessage::Ping(_) | WsMessage::Pong(_) | WsMessage::Frame(_) => {}
                }
            }
        })
        .await
        .context("timed out waiting for tunnel exec response")??;

        tracing::debug!(%session_log_id, command = %command, "tunnel exec completed");
        Ok(response)
    }

    pub async fn run_shell(
        self,
        channel: Channel<Msg>,
        input_rx: mpsc::Receiver<SshInputEvent>,
        shutdown: late_core::shutdown::CancellationToken,
    ) -> anyhow::Result<()> {
        let Self {
            ws_url,
            secret,
            ctx,
            ws,
        } = self;
        run_shell_loop(channel, ws_url, secret, ctx, input_rx, shutdown, Some(ws)).await
    }
}

/// How a `connect_async` failure should be handled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DialOutcome {
    /// TCP I/O error or HTTP 5xx — likely a backend mid-restart.
    Retryable,
    /// HTTP 4xx (auth/protocol) or any other non-recoverable error
    /// — retrying with the same handshake will keep failing.
    Terminal,
}

fn classify_dial_err(err: &tungstenite::Error) -> DialOutcome {
    match err {
        tungstenite::Error::Io(_) => DialOutcome::Retryable,
        tungstenite::Error::Http(resp) => {
            if resp.status().is_server_error() {
                DialOutcome::Retryable
            } else {
                DialOutcome::Terminal
            }
        }
        // Url, Tls, Capacity, Protocol, Utf8, …: configuration or
        // protocol-shape issues that won't fix themselves.
        _ => DialOutcome::Terminal,
    }
}

fn terminal_dial_message(err: &tungstenite::Error) -> &'static str {
    match err {
        tungstenite::Error::Http(response) => match response.status().as_u16() {
            401 => UNAVAILABLE_MSG,
            403 => ACCESS_REFUSED_MSG,
            429 => RATE_LIMITED_MSG,
            _ => SESSION_FAILED_MSG,
        },
        _ => SESSION_FAILED_MSG,
    }
}

fn terminal_close_message(code: u16) -> Option<&'static str> {
    match code {
        1000 | TUNNEL_CLOSE_SESSION_ENDED => None,
        4001 | late_core::tunnel_protocol::TUNNEL_CLOSE_BANNED => Some(ACCESS_ENDED_MSG),
        _ => Some(SESSION_FAILED_MSG),
    }
}

/// How the inner pump loop ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PumpOutcome {
    /// User-side SSH channel closed (EOF or read error). Done.
    SshClosed,
    /// WS closed or transport failed for a reason that warrants reconnecting.
    Retryable(u16),
    /// WS closed for a reason that means the user's SSH session
    /// should end (banned, kicked, protocol error, …).
    Terminal,
}

fn classify_close_code(code: u16) -> PumpOutcome {
    // Per the close-code policy in devdocs/BASTION-FEATURE.md.
    match code {
        TUNNEL_CLOSE_ABNORMAL => PumpOutcome::Retryable(TUNNEL_CLOSE_ABNORMAL),
        4100..=4199 => PumpOutcome::Retryable(code),
        // Conservative default: an unknown code is more likely a
        // misbehaving backend than a transient blip. End the session
        // so we don't retry into a loop on a code we don't understand.
        _ => PumpOutcome::Terminal,
    }
}

fn initial_reconnect_message_delay(reason: Option<u16>) -> Duration {
    if matches!(reason, Some(TUNNEL_CLOSE_RECONNECT_REQUESTED)) {
        Duration::ZERO
    } else {
        INITIAL_MESSAGE_DELAY
    }
}

fn initial_reconnect_message(reason: Option<u16>) -> &'static str {
    if matches!(reason, Some(TUNNEL_CLOSE_RECONNECT_REQUESTED)) {
        RELOADING_UPDATE_MSG
    } else {
        RECONNECTING_MSG
    }
}

/// Exponential backoff bounded by a wall-clock budget.
///
/// The ceiling doubles from 100ms to 5s; each delay is randomized between
/// half that ceiling and the ceiling to spread simultaneous reconnects.
/// `next_delay()` returns `None` once the cumulative wall-clock time
/// since construction exceeds `RECONNECT_BUDGET`. Reset by re-creating.
struct ReconnectBackoff {
    attempt: u32,
    started: Instant,
    budget: Duration,
}

impl ReconnectBackoff {
    fn new(budget: Duration) -> Self {
        Self {
            attempt: 0,
            started: Instant::now(),
            budget,
        }
    }

    fn next_delay(&mut self) -> Option<Duration> {
        if self.started.elapsed() >= self.budget {
            return None;
        }
        let factor = 1u32.checked_shl(self.attempt).unwrap_or(u32::MAX);
        let scaled = BACKOFF_INITIAL
            .checked_mul(factor)
            .unwrap_or(BACKOFF_MAX)
            .min(BACKOFF_MAX);
        self.attempt = self.attempt.saturating_add(1);
        // Entropy failure falls back to the full delay, preserving the budget.
        let sample = f64::from(getrandom::u32().unwrap_or(u32::MAX)) / f64::from(u32::MAX);
        let jittered = scaled.mul_f64(0.5 + sample * 0.5);
        Some(jittered.min(self.budget.saturating_sub(self.started.elapsed())))
    }
}

/// Run the proxy session for a single shell channel. Returns when
/// the user's SSH side ends, the backend signals a terminal close,
/// or the reconnect budget is exhausted on a sustained outage.
///
/// `ws_url` is the full backend URL (e.g.
/// `ws://service-ssh-internal:4001/tunnel`). `secret` is the value
/// for `X-Late-Secret`. `ctx` carries the per-session handshake fields;
/// `ctx.session_id` is reused across redials, and `ctx.reconnect_reason`
/// is set to the close code that triggered each redial.
pub async fn run_session(
    channel: Channel<Msg>,
    ws_url: String,
    secret: String,
    ctx: HandshakeContext,
    input_rx: mpsc::Receiver<SshInputEvent>,
    shutdown: late_core::shutdown::CancellationToken,
) -> anyhow::Result<()> {
    run_shell_loop(channel, ws_url, secret, ctx, input_rx, shutdown, None).await
}

/// Consume input from a disconnected TUI generation, keeping only its latest size.
fn discard_stale_input(rx: &mut mpsc::Receiver<SshInputEvent>, ctx: &mut HandshakeContext) {
    while let Ok(event) = rx.try_recv() {
        remember_size(event, ctx);
    }
}

fn remember_size(event: SshInputEvent, ctx: &mut HandshakeContext) {
    if let SshInputEvent::Resize { cols, rows } = event {
        ctx.cols = cols;
        ctx.rows = rows;
    }
}

async fn notice(writer: &mut Pin<Box<dyn AsyncWrite + Send>>, message: &str) {
    // Notices must never hold teardown hostage to a client with a zero SSH window.
    let _ = timeout(
        Duration::from_millis(250),
        writer.write_all(message.as_bytes()),
    )
    .await;
}

#[derive(Default)]
struct ConnectionNotices {
    initial: bool,
    escalation: bool,
}

impl ConnectionNotices {
    async fn show(
        &mut self,
        writer: &mut Pin<Box<dyn AsyncWrite + Send>>,
        reset: &str,
        is_redial: bool,
        reason: Option<u16>,
        elapsed: Duration,
    ) {
        let delay = if is_redial {
            initial_reconnect_message_delay(reason)
        } else {
            INITIAL_MESSAGE_DELAY
        };
        if !self.initial && elapsed >= delay {
            let message = if is_redial {
                initial_reconnect_message(reason)
            } else {
                CONNECTING_MSG
            };
            notice(writer, &format!("{reset}{message}")).await;
            self.initial = true;
        }
        if !self.escalation && elapsed >= ESCALATION_MESSAGE_DELAY {
            notice(
                writer,
                if is_redial {
                    STILL_RECONNECTING_MSG
                } else {
                    STILL_CONNECTING_MSG
                },
            )
            .await;
            self.escalation = true;
        }
    }
}

async fn run_shell_loop(
    channel: Channel<Msg>,
    ws_url: String,
    secret: String,
    mut ctx: HandshakeContext,
    mut input_rx: mpsc::Receiver<SshInputEvent>,
    shutdown: late_core::shutdown::CancellationToken,
    mut initial_ws: Option<TunnelWs>,
) -> anyhow::Result<()> {
    let session_log_id = late_core::tunnel_protocol::session_log_id(&ctx.session_id);
    let (read_half, write_half) = channel.split();
    drop(read_half);
    // Unlike Handle::data, ChannelTx obeys the remote SSH window. Blocking
    // here bounds output memory; the timeout/cancellation bounds its lifetime.
    let mut ssh_writer: Pin<Box<dyn AsyncWrite + Send>> = Box::pin(write_half.make_writer());
    let terminal_reset = format!("{}{TERMINAL_RESET}", late_core::terminal::RESET_MODES);
    let mut gap = ReconnectBackoff::new(RECONNECT_BUDGET);
    let mut is_redial = false;
    let mut retry_delay = Duration::ZERO;
    let mut notices = ConnectionNotices::default();
    let mut exit_status = 1;
    let mut closing_message = Some(UNAVAILABLE_MSG);

    'session: loop {
        if is_redial {
            discard_stale_input(&mut input_rx, &mut ctx);
        }
        if input_rx.is_closed() && input_rx.is_empty() {
            closing_message = None;
            break;
        }
        let ws = if let Some(ws) = initial_ws.take() {
            ws
        } else {
            loop {
                // A stalled handshake, retry sleep, or peer that keeps accepting
                // and dropping cannot extend the overall 30s outage budget.
                let deadline = gap.started + gap.budget;
                if Instant::now() >= deadline {
                    break 'session;
                }
                let req = build_request(&ws_url, &secret, &ctx)?;
                let dial = async move {
                    tokio::time::sleep(retry_delay).await;
                    timeout(DIAL_TIMEOUT, tokio_tungstenite::connect_async(req))
                        .await
                        .unwrap_or_else(|_| {
                            Err(tungstenite::Error::Io(std::io::Error::new(
                                std::io::ErrorKind::TimedOut,
                                "tunnel dial timed out",
                            )))
                        })
                };
                tokio::pin!(dial);
                let mut messages = tokio::time::interval(Duration::from_millis(100));
                let result = loop {
                    tokio::select! {
                        biased;
                        _ = shutdown.cancelled() => {
                            notice(&mut ssh_writer, &format!("{terminal_reset}{SHUTDOWN_MSG}")).await;
                            closing_message = None;
                            break 'session;
                        }
                        _ = tokio::time::sleep_until(deadline) => break 'session,
                        result = &mut dial => break result,
                        event = input_rx.recv() => match event {
                            Some(event) => remember_size(event, &mut ctx),
                            None => { closing_message = None; break 'session; },
                        },
                        _ = messages.tick() => {
                            notices.show(&mut ssh_writer, &terminal_reset, is_redial, ctx.reconnect_reason, gap.started.elapsed()).await;
                        }
                    }
                };
                match result {
                    Ok((ws, _)) => break ws,
                    Err(err) if classify_dial_err(&err) == DialOutcome::Retryable => {
                        let Some(delay) = gap.next_delay() else {
                            break 'session;
                        };
                        retry_delay = delay;
                    }
                    Err(err) => {
                        if let tungstenite::Error::Http(response) = &err
                            && matches!(response.status().as_u16(), 401 | 403)
                        {
                            tracing::warn!(status = %response.status(), ?err, %session_log_id, "tunnel access rejected; check backend credentials and access configuration");
                        } else {
                            tracing::info!(?err, %session_log_id, "terminal tunnel dial failure");
                        }
                        closing_message = Some(terminal_dial_message(&err));
                        break 'session;
                    }
                }
            }
        };
        if is_redial {
            discard_stale_input(&mut input_rx, &mut ctx);
        }
        let (mut sink, mut stream) = ws.split();
        let mut setup = vec![ControlFrame::Pty {
            term: ctx.term.clone(),
            cols: ctx.cols,
            rows: ctx.rows,
        }];
        setup.extend(
            ctx.env
                .iter()
                .cloned()
                .map(|(name, value)| ControlFrame::Env { name, value }),
        );
        setup.push(ControlFrame::ShellStart);
        for frame in setup {
            let send = timeout(
                WS_WRITE_TIMEOUT,
                sink.send(WsMessage::Text(frame.to_json()?.into())),
            );
            let sent = tokio::select! {
                result = send => matches!(result, Ok(Ok(()))),
                _ = shutdown.cancelled() => { closing_message = Some(SHUTDOWN_MSG); break 'session; },
            };
            if !sent {
                ctx.reconnect_reason = Some(TUNNEL_CLOSE_ABNORMAL);
                is_redial = true;
                retry_delay = gap.next_delay().unwrap_or(Duration::ZERO);
                continue 'session;
            }
        }
        let mut last_inbound = Instant::now();
        let mut ready = false;
        let mut ping = tokio::time::interval(PING_INTERVAL);
        ping.set_missed_tick_behavior(MissedTickBehavior::Delay);
        let outcome = loop {
            tokio::select! {
                _ = shutdown.cancelled() => {
                    notice(&mut ssh_writer, &format!("{terminal_reset}{SHUTDOWN_MSG}")).await;
                    closing_message = None;
                    break PumpOutcome::Terminal;
                }
                _ = tokio::time::sleep_until(gap.started + gap.budget), if !ready => break PumpOutcome::Terminal,
                msg = stream.next() => {
                    if matches!(msg, Some(Ok(_))) { last_inbound = Instant::now(); }
                    match msg {
                        Some(Ok(WsMessage::Binary(bytes))) => {
                            if !ready {
                                if is_redial {
                                    let before = (ctx.cols, ctx.rows);
                                    discard_stale_input(&mut input_rx, &mut ctx);
                                    if before != (ctx.cols, ctx.rows) {
                                        let frame = ControlFrame::Resize { cols: ctx.cols, rows: ctx.rows }.to_json()?;
                                        if !matches!(timeout(WS_WRITE_TIMEOUT, sink.send(WsMessage::Text(frame.into()))).await, Ok(Ok(()))) {
                                            break PumpOutcome::Retryable(TUNNEL_CLOSE_ABNORMAL);
                                        }
                                    }
                                }
                                ready = true;
                            }
                            let sent = tokio::select! {
                                result = timeout(SSH_WRITE_TIMEOUT, ssh_writer.write_all(&bytes)) => matches!(result, Ok(Ok(()))),
                                _ = shutdown.cancelled() => false,
                            };
                            if !sent { break PumpOutcome::SshClosed; }
                            // The receive loop could not poll the backend while
                            // the SSH window was full. Start a fresh liveness
                            // interval after this successful output progress.
                            last_inbound = Instant::now();
                        }
                        Some(Ok(WsMessage::Close(frame))) => {
                            if let Some(frame) = frame.as_ref()
                                && matches!(classify_close_code(frame.code.into()), PumpOutcome::Terminal)
                            {
                                closing_message = terminal_close_message(frame.code.into());
                            }
                            if frame.as_ref().is_some_and(|frame| matches!(u16::from(frame.code), 1000 | TUNNEL_CLOSE_SESSION_ENDED)) {
                                exit_status = 0;
                            }
                            break frame.map_or(PumpOutcome::Terminal, |frame| classify_close_code(frame.code.into()));
                        },
                        None | Some(Err(_)) => break PumpOutcome::Retryable(TUNNEL_CLOSE_ABNORMAL),
                        _ => {},
                    }
                }
                event = input_rx.recv() => {
                    let Some(event) = event else { break PumpOutcome::SshClosed; };
                    let frame = match event {
                        SshInputEvent::Bytes(bytes) => {
                            if is_redial && !ready { continue; }
                            WsMessage::Binary(bytes.into())
                        },
                        SshInputEvent::Resize { cols, rows } => {
                            ctx.cols = cols; ctx.rows = rows;
                            WsMessage::Text(ControlFrame::Resize { cols, rows }.to_json()?.into())
                        }
                    };
                    let sent = tokio::select! {
                        result = timeout(WS_WRITE_TIMEOUT, sink.send(frame)) => matches!(result, Ok(Ok(()))),
                        _ = shutdown.cancelled() => { closing_message = Some(SHUTDOWN_MSG); break PumpOutcome::Terminal; },
                    };
                    if !sent { break PumpOutcome::Retryable(TUNNEL_CLOSE_ABNORMAL); }
                }
                _ = ping.tick() => {
                    if !ready {
                        notices.show(&mut ssh_writer, &terminal_reset, is_redial, ctx.reconnect_reason, gap.started.elapsed()).await;
                    }
                    if ready && last_inbound.elapsed() > SILENCE_THRESHOLD { break PumpOutcome::Retryable(TUNNEL_CLOSE_ABNORMAL); }
                    let sent = tokio::select! {
                        result = timeout(WS_WRITE_TIMEOUT, sink.send(WsMessage::Ping(Default::default()))) => matches!(result, Ok(Ok(()))),
                        _ = shutdown.cancelled() => { closing_message = Some(SHUTDOWN_MSG); break PumpOutcome::Terminal; },
                    };
                    if !sent { break PumpOutcome::Retryable(TUNNEL_CLOSE_ABNORMAL); }
                }
            }
        };
        let _ = timeout(Duration::from_millis(250), sink.close()).await;
        match outcome {
            PumpOutcome::SshClosed => {
                exit_status = 0;
                closing_message = None;
                break;
            }
            PumpOutcome::Terminal => break,
            PumpOutcome::Retryable(reason) => {
                if ready {
                    gap = ReconnectBackoff::new(RECONNECT_BUDGET);
                    notices = ConnectionNotices::default();
                }
                ctx.reconnect_reason = Some(reason);
                is_redial = true;
                let Some(delay) = gap.next_delay() else {
                    break;
                };
                retry_delay = if reason == TUNNEL_CLOSE_RECONNECT_REQUESTED {
                    Duration::ZERO
                } else {
                    delay
                };
            }
        }
    }
    // OpenSSH (including ControlMaster clients) treats a missing exit-status
    // as 255 even after a normal app quit. Send status before EOF and Close.
    let _ = timeout(
        Duration::from_millis(250),
        write_half.exit_status(exit_status),
    )
    .await;
    finish(ssh_writer, &session_log_id, "proxy ended", closing_message).await?;
    let _ = timeout(Duration::from_millis(250), write_half.close()).await;
    Ok(())
}

/// Common cleanup: shut down the SSH writer half (sends EOF/Close to
/// russh, terminating the user's channel cleanly) and log.
async fn finish<W: AsyncWriteExt + Unpin>(
    mut ssh_writer: W,
    session_log_id: &str,
    why: &'static str,
    message: Option<&str>,
) -> anyhow::Result<()> {
    // Restore modes even when the backend disappeared before it could send its
    // own teardown. Do not clear the main screen or erase a farewell message.
    let restore = format!(
        "{}\x1b[?1049l{}",
        late_core::terminal::RESET_MODES,
        message.unwrap_or_default()
    );
    let _ = timeout(
        Duration::from_millis(250),
        ssh_writer.write_all(restore.as_bytes()),
    )
    .await;
    let _ = timeout(Duration::from_millis(250), ssh_writer.shutdown()).await;
    tracing::info!(%session_log_id, reason = why, "tunnel proxy session ended");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio_tungstenite::tungstenite::http::{Response, StatusCode};

    fn http_err(status: StatusCode) -> tungstenite::Error {
        // tungstenite::Error::Http carries Response<Option<Vec<u8>>>.
        // The typed builder only works on Response<()>, so build via
        // the generic constructor and stuff in the status afterwards.
        let mut resp: Response<Option<Vec<u8>>> = Response::new(None);
        *resp.status_mut() = status;
        tungstenite::Error::Http(Box::new(resp))
    }

    #[test]
    fn dial_classifier_io_is_retryable() {
        let err = tungstenite::Error::Io(std::io::Error::other("connection refused"));
        assert_eq!(classify_dial_err(&err), DialOutcome::Retryable);
    }

    #[test]
    fn dial_classifier_5xx_is_retryable() {
        for s in [
            StatusCode::INTERNAL_SERVER_ERROR,
            StatusCode::BAD_GATEWAY,
            StatusCode::SERVICE_UNAVAILABLE,
            StatusCode::GATEWAY_TIMEOUT,
        ] {
            assert_eq!(
                classify_dial_err(&http_err(s)),
                DialOutcome::Retryable,
                "status {s}"
            );
        }
    }

    #[test]
    fn dial_classifier_4xx_is_terminal() {
        for s in [
            StatusCode::BAD_REQUEST,
            StatusCode::UNAUTHORIZED,
            StatusCode::FORBIDDEN,
            StatusCode::TOO_MANY_REQUESTS,
        ] {
            assert_eq!(
                classify_dial_err(&http_err(s)),
                DialOutcome::Terminal,
                "status {s}"
            );
        }
    }

    #[test]
    fn close_code_classifier_matches_design() {
        assert_eq!(
            classify_close_code(TUNNEL_CLOSE_ABNORMAL),
            PumpOutcome::Retryable(TUNNEL_CLOSE_ABNORMAL)
        );
        assert_eq!(classify_close_code(4100), PumpOutcome::Retryable(4100));
        assert_eq!(classify_close_code(4199), PumpOutcome::Retryable(4199));

        // Terminal: backend told us to give up, or emitted an unknown signal.
        assert_eq!(classify_close_code(1000), PumpOutcome::Terminal);
        assert_eq!(classify_close_code(1001), PumpOutcome::Terminal);
        assert_eq!(classify_close_code(4000), PumpOutcome::Terminal);
        assert_eq!(classify_close_code(4001), PumpOutcome::Terminal);
        assert_eq!(classify_close_code(4002), PumpOutcome::Terminal);
        assert_eq!(classify_close_code(4003), PumpOutcome::Terminal);

        // Unknown codes default to terminal so we don't loop on a
        // misbehaving backend.
        assert_eq!(classify_close_code(1011), PumpOutcome::Terminal);
        assert_eq!(classify_close_code(4999), PumpOutcome::Terminal);
    }

    #[test]
    fn backoff_jitter_stays_within_doubling_then_capped_bounds() {
        let mut b = ReconnectBackoff::new(Duration::from_secs(60));
        for ceiling_ms in [100, 200, 400, 800, 1600, 3200, 5000, 5000] {
            let ceiling = Duration::from_millis(ceiling_ms);
            let delay = b.next_delay().unwrap();
            assert!((ceiling / 2..=ceiling).contains(&delay), "{delay:?}");
        }
    }

    #[test]
    fn backoff_returns_none_after_budget() {
        // Zero-budget backoff returns None on the first call without
        // having to actually sleep.
        let mut b = ReconnectBackoff::new(Duration::ZERO);
        assert_eq!(b.next_delay(), None);
    }

    #[test]
    fn backoff_attempt_doesnt_overflow() {
        let mut b = ReconnectBackoff::new(Duration::from_secs(60));
        // Bump attempt high enough that 1u32 << attempt would panic.
        b.attempt = 40;
        // Doesn't panic; clamps at the cap.
        assert!((BACKOFF_MAX / 2..=BACKOFF_MAX).contains(&b.next_delay().unwrap()));
    }
}
