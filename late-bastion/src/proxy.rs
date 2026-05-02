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
//! See `devdocs/LATE-CONNECTION-BASTION.md` §4–§5.

use anyhow::Context;
use futures_util::{SinkExt, StreamExt};
use late_core::tunnel_protocol::{
    ControlFrame, SshInputEvent, TUNNEL_CLOSE_ABNORMAL, TUNNEL_CLOSE_RECONNECT_REQUESTED,
};
use russh::Channel;
use russh::server::Msg;
use std::pin::Pin;
use std::time::{Duration, Instant};
use tokio::io::{AsyncWrite, AsyncWriteExt};
use tokio::sync::mpsc;
use tokio::time::{MissedTickBehavior, timeout};
use tokio_tungstenite::tungstenite;
use tokio_tungstenite::tungstenite::protocol::Message as WsMessage;

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
/// disconnection event. Reset on every successful upgrade.
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
/// to the user's terminal (per devdocs/LATE-CONNECTION-BASTION.md §5).
const TERMINAL_RESET: &str = "\x1b[?1049l\x1b[0m\x1b[2J\x1b[H";

/// Initial reconnect message; constants for tunability.
const RECONNECTING_MSG: &str = "reconnecting to late.sh\u{2026}\r\n";

/// Reconnect message for an explicit user-requested reload during drain.
const RELOADING_UPDATE_MSG: &str = "waiting for updated late.sh...\r\n";

/// Escalated reconnect message after `ESCALATION_MESSAGE_DELAY`.
const STILL_RECONNECTING_MSG: &str = "still reconnecting\u{2026}\r\n";

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
/// healthy RTT is sub-ms, even hot-loop event-loop stalls won't hit
/// 5s. The user's `ssh` ↔ bastion leg has its own SSH-level
/// keepalive separately and isn't affected by this threshold.
const SILENCE_THRESHOLD: Duration = Duration::from_secs(5);

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
    // Per devdocs/LATE-CONNECTION-BASTION.md §4 close-codes table.
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
/// Schedule (from `BACKOFF_INITIAL` 100ms, `BACKOFF_MAX` 5s):
///   100ms, 200ms, 400ms, 800ms, 1.6s, 3.2s, 5s, 5s, 5s, …
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
        Some(scaled)
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
    mut ctx: HandshakeContext,
    mut input_rx: mpsc::Receiver<SshInputEvent>,
    shutdown: late_core::shutdown::CancellationToken,
) -> anyhow::Result<()> {
    let session_id = ctx.session_id.clone();

    // Split the channel into halves; we only keep the writer.
    //
    // Inbound bytes come through `input_rx`, which is fed by
    // `Handler::data` in russh's per-connection task — the same task
    // that fires `Handler::window_change_request` (which also feeds
    // `input_rx`). russh dispatches both callbacks serially, so the
    // queue carries data and resize in their original SSH wire order.
    //
    // Dropping the read half is safe: russh's internal `chan.send`
    // for ChannelMsg::Data does `unwrap_or(())` on a closed receiver,
    // so the channel queue stays empty and there's no flow-control
    // stall. We were only using that path to read bytes, and
    // `Handler::data` gives us the same bytes in better-defined order.
    let (read_half, write_half) = channel.split();
    drop(read_half);
    // `make_writer()` returns `impl AsyncWrite + 'static`, but its
    // poll_write keeps an in-flight permit future across yields, so
    // it's not Unpin. Box::pin to satisfy the Unpin bound on
    // AsyncWriteExt's helper methods.
    let mut ssh_writer: Pin<Box<dyn AsyncWrite + Send + 'static>> =
        Box::pin(write_half.make_writer());

    // Becomes true after the first successful pump ends with a
    // retryable close. Gates the plain-text reconnect message: a long
    // gap on the *first* dial isn't a reconnect, so we don't talk
    // about reconnecting then.
    let mut is_redial = false;

    'session: loop {
        // === Dial with exponential backoff ===
        let mut backoff = ReconnectBackoff::new(RECONNECT_BUDGET);
        // Per-window flags so escalation only fires once per gap.
        let mut wrote_initial_message = false;
        let mut wrote_escalation_message = false;

        let (mut ws_sink, mut ws_stream) = 'dial: loop {
            let req = build_request(&ws_url, &secret, &ctx)
                .context("failed to build /tunnel handshake")?;

            let dial = tokio::select! {
                result = tokio_tungstenite::connect_async(req) => result,
                _ = shutdown.cancelled() => {
                    let payload = format!("{TERMINAL_RESET}{SHUTDOWN_MSG}");
                    let _ = ssh_writer.write_all(payload.as_bytes()).await;
                    let _ = ssh_writer.flush().await;
                    return finish(ssh_writer, &session_id, "shutdown during dial").await;
                }
            };

            match dial {
                Ok((ws, response)) => {
                    tracing::info!(
                        session_id = %session_id,
                        status = %response.status(),
                        reconnect_reason = ?ctx.reconnect_reason,
                        "tunnel ws upgraded"
                    );
                    break 'dial ws.split();
                }
                Err(e) => match classify_dial_err(&e) {
                    DialOutcome::Terminal => {
                        tracing::info!(
                            error = ?e,
                            session_id = %session_id,
                            "tunnel dial: terminal error; ending session"
                        );
                        return finish(ssh_writer, &session_id, "dial terminal").await;
                    }
                    DialOutcome::Retryable => {
                        // Once the gap stretches past the visibility
                        // thresholds, write the user-facing message —
                        // but only for *re*dials, not for the initial
                        // dial (where there's no prior TUI to clear
                        // and no continuity to explain).
                        if is_redial {
                            let elapsed = backoff.started.elapsed();
                            if !wrote_initial_message
                                && elapsed >= initial_reconnect_message_delay(ctx.reconnect_reason)
                            {
                                let payload = format!(
                                    "{TERMINAL_RESET}{}",
                                    initial_reconnect_message(ctx.reconnect_reason)
                                );
                                let _ = ssh_writer.write_all(payload.as_bytes()).await;
                                let _ = ssh_writer.flush().await;
                                wrote_initial_message = true;
                            }
                            if !wrote_escalation_message && elapsed >= ESCALATION_MESSAGE_DELAY {
                                let _ = ssh_writer
                                    .write_all(STILL_RECONNECTING_MSG.as_bytes())
                                    .await;
                                let _ = ssh_writer.flush().await;
                                wrote_escalation_message = true;
                            }
                        }

                        let Some(delay) = backoff.next_delay() else {
                            tracing::warn!(
                                session_id = %session_id,
                                "tunnel reconnect budget exhausted; ending session"
                            );
                            return finish(ssh_writer, &session_id, "budget exhausted").await;
                        };
                        tracing::debug!(
                            error = ?e,
                            session_id = %session_id,
                            delay_ms = delay.as_millis() as u64,
                            "tunnel dial retryable; sleeping"
                        );
                        tokio::select! {
                            _ = tokio::time::sleep(delay) => {}
                            _ = shutdown.cancelled() => {
                                let payload = format!("{TERMINAL_RESET}{SHUTDOWN_MSG}");
                                let _ = ssh_writer.write_all(payload.as_bytes()).await;
                                let _ = ssh_writer.flush().await;
                                return finish(ssh_writer, &session_id, "shutdown during backoff").await;
                            }
                        }
                    }
                },
            }
        };

        // === Pump bytes until either side ends ===
        // Liveness tracking: any inbound WS frame counts as a sign the
        // backend is alive. We also actively probe with WS Pings every
        // PING_INTERVAL so a wedged backend (auto-pong loop stuck) is
        // detectable even when the user is idle.
        let mut last_inbound = Instant::now();
        let mut ping_interval = tokio::time::interval(PING_INTERVAL);
        ping_interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
        // Don't pre-tick: the first tick fires immediately, sending a
        // probe Ping to validate the connection is fully up.

        let outcome = loop {
            tokio::select! {
                _ = shutdown.cancelled() => {
                    let payload = format!("{TERMINAL_RESET}{SHUTDOWN_MSG}");
                    let _ = ssh_writer.write_all(payload.as_bytes()).await;
                    let _ = ssh_writer.flush().await;
                    break PumpOutcome::Terminal;
                }

                // SSH (user) → WS (backend). Single FIFO carrying
                // both Bytes and Resize in russh's dispatch order;
                // emitted as WS Binary or WS Text accordingly. WS
                // frames preserve order on the wire (RFC 6455), so
                // the backend reads them back in the same order.
                event = input_rx.recv() => {
                    let Some(event) = event else {
                        tracing::debug!(session_id = %session_id, "ssh input channel closed");
                        break PumpOutcome::SshClosed;
                    };
                    match event {
                        SshInputEvent::Bytes(bytes) => {
                            if let Err(e) = ws_sink
                                .send(WsMessage::Binary(bytes.into()))
                                .await
                            {
                                tracing::debug!(error = ?e, session_id = %session_id, "ws send (binary) failed; treating as retryable");
                                break PumpOutcome::Retryable(TUNNEL_CLOSE_ABNORMAL);
                            }
                        }
                        SshInputEvent::Resize { cols, rows } => {
                            // Stash the latest dimensions on the
                            // handshake context so the next reconnect
                            // upgrades with the current PTY size.
                            ctx.cols = cols;
                            ctx.rows = rows;
                            let frame = match (ControlFrame::Resize { cols, rows }).to_json() {
                                Ok(s) => s,
                                Err(e) => {
                                    tracing::warn!(error = ?e, "encode resize");
                                    continue;
                                }
                            };
                            if let Err(e) = ws_sink.send(WsMessage::Text(frame.into())).await {
                                tracing::debug!(error = ?e, session_id = %session_id, "ws send (resize) failed; treating as retryable");
                                break PumpOutcome::Retryable(TUNNEL_CLOSE_ABNORMAL);
                            }
                        }
                    }
                }

                // WS (backend) → SSH (user) — opaque binary; ignore text/ping.
                msg = ws_stream.next() => {
                    // Any successfully-parsed inbound frame (Pong,
                    // Binary, Text, Ping, Frame) means the backend is
                    // still talking; reset the silence countdown.
                    if matches!(msg, Some(Ok(_))) {
                        last_inbound = Instant::now();
                    }
                    match msg {
                        None => {
                            tracing::debug!(session_id = %session_id, "ws stream ended without close frame");
                            // 1006-equivalent: transport dropped us.
                            break PumpOutcome::Retryable(TUNNEL_CLOSE_ABNORMAL);
                        }
                        Some(Ok(WsMessage::Binary(bytes))) => {
                            if let Err(e) = ssh_writer.write_all(&bytes).await {
                                tracing::debug!(error = ?e, session_id = %session_id, "ssh write failed");
                                break PumpOutcome::SshClosed;
                            }
                        }
                        Some(Ok(WsMessage::Close(frame))) => {
                            let code = frame.as_ref().map(|f| u16::from(f.code));
                            let outcome = match code {
                                Some(c) => classify_close_code(c),
                                None => PumpOutcome::Terminal,
                            };
                            tracing::info!(
                                session_id = %session_id,
                                code = ?code,
                                reason = frame.as_ref().map(|f| f.reason.as_str()),
                                ?outcome,
                                "ws close received"
                            );
                            break outcome;
                        }
                        Some(Ok(WsMessage::Ping(_) | WsMessage::Pong(_) | WsMessage::Frame(_))) => {
                            // tungstenite handles ping/pong automatically.
                        }
                        Some(Ok(WsMessage::Text(t))) => {
                            // Backend → bastion text vocabulary is reserved for
                            // future control frames; ignore unknown variants.
                            tracing::debug!(session_id = %session_id, payload = %t.as_str(), "ws text frame ignored");
                        }
                        Some(Err(e)) => {
                            tracing::debug!(error = ?e, session_id = %session_id, "ws recv error; treating as retryable");
                            break PumpOutcome::Retryable(TUNNEL_CLOSE_ABNORMAL);
                        }
                    }
                }

                // Liveness probe + dead-backend detector.
                _ = ping_interval.tick() => {
                    if last_inbound.elapsed() > SILENCE_THRESHOLD {
                        tracing::warn!(
                            session_id = %session_id,
                            silence_ms = last_inbound.elapsed().as_millis() as u64,
                            "tunnel ws silent past threshold; treating as dead backend"
                        );
                        break PumpOutcome::Retryable(TUNNEL_CLOSE_ABNORMAL);
                    }
                    if let Err(e) = ws_sink.send(WsMessage::Ping(Default::default())).await {
                        tracing::debug!(error = ?e, session_id = %session_id, "ws send (ping) failed; treating as retryable");
                        break PumpOutcome::Retryable(TUNNEL_CLOSE_ABNORMAL);
                    }
                }
            }
        };

        // Tidy the WS halves regardless of outcome — both go out of
        // scope with this iteration.
        let _ = timeout(Duration::from_millis(500), ws_sink.close()).await;
        drop(ws_stream);

        match outcome {
            PumpOutcome::SshClosed => {
                return finish(ssh_writer, &session_id, "ssh closed").await;
            }
            PumpOutcome::Terminal => {
                return finish(ssh_writer, &session_id, "ws terminal close").await;
            }
            PumpOutcome::Retryable(reason) => {
                ctx.reconnect_reason = Some(reason);
                is_redial = true;
                continue 'session;
            }
        }
    }
}

/// Common cleanup: shut down the SSH writer half (sends EOF/Close to
/// russh, terminating the user's channel cleanly) and log.
async fn finish<W: AsyncWriteExt + Unpin>(
    mut ssh_writer: W,
    session_id: &str,
    why: &'static str,
) -> anyhow::Result<()> {
    let _ = ssh_writer.shutdown().await;
    tracing::info!(session_id = %session_id, reason = why, "tunnel proxy session ended");
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
    fn backoff_schedule_doubles_then_caps() {
        let mut b = ReconnectBackoff::new(Duration::from_secs(60));
        assert_eq!(b.next_delay(), Some(Duration::from_millis(100)));
        assert_eq!(b.next_delay(), Some(Duration::from_millis(200)));
        assert_eq!(b.next_delay(), Some(Duration::from_millis(400)));
        assert_eq!(b.next_delay(), Some(Duration::from_millis(800)));
        assert_eq!(b.next_delay(), Some(Duration::from_millis(1600)));
        assert_eq!(b.next_delay(), Some(Duration::from_millis(3200)));
        // Hits cap.
        assert_eq!(b.next_delay(), Some(BACKOFF_MAX));
        assert_eq!(b.next_delay(), Some(BACKOFF_MAX));
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
        assert_eq!(b.next_delay(), Some(BACKOFF_MAX));
    }
}
