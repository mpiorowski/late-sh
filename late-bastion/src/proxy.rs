//! Per-shell-channel proxy task: dial late-ssh `/tunnel` and pump
//! bytes between the user's SSH channel and the WebSocket.
//!
//! Phase 3 scope: dial once, pump until either side closes, then drop
//! both halves so the SSH channel terminates cleanly. No reconnect
//! loop and no plain-text "reconnecting…" message — those are Phase 4.
//! See `PERSISTENT-CONNECTION-GATEWAY.md` §10.

use anyhow::Context;
use futures_util::{SinkExt, StreamExt};
use late_core::tunnel_protocol::ControlFrame;
use russh::Channel;
use russh::server::Msg;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;
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

/// Run the proxy session for a single shell channel. Returns when
/// either the WS or the SSH channel closes; the caller is expected
/// to have spawned this on its own task and has nothing further to do.
///
/// `ws_url` is the full backend URL (e.g.
/// `ws://service-ssh-internal:4001/tunnel`). `secret` is the value
/// for `X-Late-Secret`. `ctx` carries the per-session handshake fields.
pub async fn run_session(
    channel: Channel<Msg>,
    ws_url: String,
    secret: String,
    ctx: HandshakeContext,
    mut resize_rx: mpsc::Receiver<(u16, u16)>,
) -> anyhow::Result<()> {
    let req = build_request(&ws_url, &secret, &ctx).context("failed to build /tunnel handshake")?;

    let session_id = ctx.session_id.clone();

    let (ws, response) = tokio_tungstenite::connect_async(req)
        .await
        .with_context(|| format!("failed to dial {ws_url}"))?;
    tracing::info!(
        session_id = %session_id,
        status = %response.status(),
        "tunnel ws upgraded"
    );

    let (mut ws_sink, mut ws_stream) = ws.split();

    // Take the SSH channel as a single AsyncRead+AsyncWrite stream.
    // Dropping the stream sends EOF/Close to russh's per-channel
    // sender, which closes the SSH channel cleanly when the loop ends.
    let stream = channel.into_stream();
    let (mut ssh_reader, mut ssh_writer) = tokio::io::split(stream);

    let mut ssh_buf = vec![0u8; SSH_READ_BUF];

    loop {
        tokio::select! {
            // SSH (user) → WS (backend) — opaque binary frames.
            n = ssh_reader.read(&mut ssh_buf) => {
                match n {
                    Ok(0) => {
                        tracing::debug!(session_id = %session_id, "ssh reader EOF");
                        break;
                    }
                    Ok(n) => {
                        if let Err(e) = ws_sink
                            .send(WsMessage::Binary(ssh_buf[..n].to_vec().into()))
                            .await
                        {
                            tracing::debug!(error = ?e, session_id = %session_id, "ws send (binary) failed");
                            break;
                        }
                    }
                    Err(e) => {
                        tracing::debug!(error = ?e, session_id = %session_id, "ssh read failed");
                        break;
                    }
                }
            }

            // WS (backend) → SSH (user) — opaque binary; ignore text/ping.
            msg = ws_stream.next() => {
                match msg {
                    None => {
                        tracing::debug!(session_id = %session_id, "ws stream ended");
                        break;
                    }
                    Some(Ok(WsMessage::Binary(bytes))) => {
                        if let Err(e) = ssh_writer.write_all(&bytes).await {
                            tracing::debug!(error = ?e, session_id = %session_id, "ssh write failed");
                            break;
                        }
                    }
                    Some(Ok(WsMessage::Close(frame))) => {
                        tracing::info!(
                            session_id = %session_id,
                            code = ?frame.as_ref().map(|f| u16::from(f.code)),
                            reason = frame.as_ref().map(|f| f.reason.as_str()),
                            "ws close received"
                        );
                        break;
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
                        tracing::debug!(error = ?e, session_id = %session_id, "ws recv error");
                        break;
                    }
                }
            }

            // Local resize events (from window_change_request) → WS text.
            resize = resize_rx.recv() => {
                match resize {
                    Some((cols, rows)) => {
                        let frame = match (ControlFrame::Resize { cols, rows }).to_json() {
                            Ok(s) => s,
                            Err(e) => {
                                tracing::warn!(error = ?e, "encode resize");
                                continue;
                            }
                        };
                        if let Err(e) = ws_sink.send(WsMessage::Text(frame.into())).await {
                            tracing::debug!(error = ?e, session_id = %session_id, "ws send (resize) failed");
                            break;
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
    }

    // Best-effort tidy-up. Any error here is uninteresting; both sides
    // are already on their way out.
    let _ = ws_sink.close().await;
    let _ = ssh_writer.shutdown().await;

    tracing::info!(session_id = %session_id, "tunnel proxy session ended");
    Ok(())
}
