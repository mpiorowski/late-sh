//! Per-shell-channel proxy task: dial late-ssh `/tunnel` and pump
//! bytes between the user's SSH channel and the WebSocket.
//!
//! Phase 4 (this commit): outer reconnect loop. The SSH stream is
//! split once and reused across multiple WS sessions; on a retryable
//! WS close (1000/1001/1006) or a transient dial error (TCP I/O,
//! HTTP 5xx), we redial with exponential backoff and a 30-second
//! total budget. Terminal close codes (4001/4002/4003) and HTTP 4xx
//! responses end the session.
//!
//! See `PERSISTENT-CONNECTION-GATEWAY.md` §4–§5.

use anyhow::Context;
use futures_util::{SinkExt, StreamExt};
use late_core::tunnel_protocol::ControlFrame;
use russh::Channel;
use russh::server::Msg;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite;
use tokio_tungstenite::tungstenite::protocol::Message as WsMessage;

use crate::handshake::{HandshakeContext, build_request};

/// Bound on the resize-event mpsc. Resizes are sparse (a human
/// dragging a terminal corner), so a tiny buffer is plenty; if it
/// ever fills up we'd rather drop one stale resize than backpressure
/// russh's handler task.
pub const RESIZE_QUEUE_CAP: usize = 4;

/// Per-iteration buffer size for SSH→WS reads. PTY input is dominated
/// by single keystrokes; even pasted content arrives in modest chunks.
/// Output direction (WS→SSH) doesn't go through this buffer — full
/// frames are written via `write_all`.
const SSH_READ_BUF: usize = 8 * 1024;

/// Initial wait before the first reconnect attempt.
const BACKOFF_INITIAL: Duration = Duration::from_millis(100);

/// Cap on a single backoff sleep — after several failures we don't
/// want to wait minutes between attempts.
const BACKOFF_MAX: Duration = Duration::from_secs(5);

/// Total time budget across all reconnect attempts within a single
/// disconnection event. Reset on every successful upgrade.
const RECONNECT_BUDGET: Duration = Duration::from_secs(30);

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
    /// WS closed for a reason that warrants reconnecting.
    Retryable,
    /// WS closed for a reason that means the user's SSH session
    /// should end (banned, kicked, protocol error, …).
    Terminal,
}

fn classify_close_code(code: u16) -> PumpOutcome {
    // Per PERSISTENT-CONNECTION-GATEWAY.md §4 close-codes table.
    match code {
        1000 | 1001 | 1006 => PumpOutcome::Retryable,
        4001..=4003 => PumpOutcome::Terminal,
        // Conservative default: an unknown code is more likely a
        // misbehaving backend than a transient blip. End the session
        // so we don't retry into a loop on a code we don't understand.
        _ => PumpOutcome::Terminal,
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
/// `ctx.session_id` is reused across redials, and `ctx.reconnect` is
/// flipped to `true` on every dial after the first.
pub async fn run_session(
    channel: Channel<Msg>,
    ws_url: String,
    secret: String,
    mut ctx: HandshakeContext,
    mut resize_rx: mpsc::Receiver<(u16, u16)>,
) -> anyhow::Result<()> {
    let session_id = ctx.session_id.clone();

    // Take the SSH channel as a single AsyncRead+AsyncWrite stream
    // ONCE for the lifetime of the user's SSH connection. The byte
    // halves outlive any single WS session so a backend redeploy
    // doesn't tear down the SSH channel.
    let stream = channel.into_stream();
    let (mut ssh_reader, mut ssh_writer) = tokio::io::split(stream);
    let mut ssh_buf = vec![0u8; SSH_READ_BUF];

    'session: loop {
        // === Dial with exponential backoff ===
        let mut backoff = ReconnectBackoff::new(RECONNECT_BUDGET);
        let (mut ws_sink, mut ws_stream) = 'dial: loop {
            let req = build_request(&ws_url, &secret, &ctx)
                .context("failed to build /tunnel handshake")?;

            match tokio_tungstenite::connect_async(req).await {
                Ok((ws, response)) => {
                    tracing::info!(
                        session_id = %session_id,
                        status = %response.status(),
                        reconnect = ctx.reconnect,
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
                        tokio::time::sleep(delay).await;
                        ctx.reconnect = true;
                    }
                },
            }
        };

        // === Pump bytes until either side ends ===
        let outcome = loop {
            tokio::select! {
                // SSH (user) → WS (backend) — opaque binary frames.
                n = ssh_reader.read(&mut ssh_buf) => {
                    match n {
                        Ok(0) => {
                            tracing::debug!(session_id = %session_id, "ssh reader EOF");
                            break PumpOutcome::SshClosed;
                        }
                        Ok(n) => {
                            if let Err(e) = ws_sink
                                .send(WsMessage::Binary(ssh_buf[..n].to_vec().into()))
                                .await
                            {
                                tracing::debug!(error = ?e, session_id = %session_id, "ws send (binary) failed; treating as retryable");
                                break PumpOutcome::Retryable;
                            }
                        }
                        Err(e) => {
                            tracing::debug!(error = ?e, session_id = %session_id, "ssh read failed");
                            break PumpOutcome::SshClosed;
                        }
                    }
                }

                // WS (backend) → SSH (user) — opaque binary; ignore text/ping.
                msg = ws_stream.next() => {
                    match msg {
                        None => {
                            tracing::debug!(session_id = %session_id, "ws stream ended without close frame");
                            // 1006-equivalent: transport dropped us.
                            break PumpOutcome::Retryable;
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
                                // Bare close without a code: treat as 1000-equivalent.
                                None => PumpOutcome::Retryable,
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
                            break PumpOutcome::Retryable;
                        }
                    }
                }

                // Local resize events (from window_change_request) → WS text.
                // Also tracked in `ctx` so the next reconnect's handshake
                // headers carry the latest PTY size.
                resize = resize_rx.recv() => {
                    match resize {
                        Some((cols, rows)) => {
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
                                break PumpOutcome::Retryable;
                            }
                        }
                        None => {
                            // Sender dropped (handler torn down). The SSH side
                            // is going away too — finish the loop on the next
                            // ssh_reader poll.
                        }
                    }
                }
            }
        };

        // Tidy the WS halves regardless of outcome — both go out of
        // scope with this iteration.
        let _ = ws_sink.close().await;
        drop(ws_stream);

        match outcome {
            PumpOutcome::SshClosed => {
                return finish(ssh_writer, &session_id, "ssh closed").await;
            }
            PumpOutcome::Terminal => {
                return finish(ssh_writer, &session_id, "ws terminal close").await;
            }
            PumpOutcome::Retryable => {
                ctx.reconnect = true;
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
        // Retryable: graceful or transport.
        assert_eq!(classify_close_code(1000), PumpOutcome::Retryable);
        assert_eq!(classify_close_code(1001), PumpOutcome::Retryable);
        assert_eq!(classify_close_code(1006), PumpOutcome::Retryable);

        // Terminal: backend told us to give up.
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
