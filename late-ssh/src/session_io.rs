//! Frame-sink abstraction shared by the russh and `/tunnel` session
//! paths.
//!
//! The render loop in `ssh.rs::run_session` only needs two things from
//! its transport: push a `Vec<u8>` of PTY output, and close cleanly.
//! Both russh `Handle::data` and an axum WebSocket can implement that,
//! so we model exactly that surface and nothing more.
//!
//! Drop-on-timeout accounting (the per-frame "did the byte go out?"
//! counter and `force_full_repaint` logic) lives at the render-loop layer
//! — the sink only reports whether each individual write completed.

use axum::extract::ws::{CloseFrame, Message};
use russh::ChannelId;
use russh::server::Handle;
use std::future::Future;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::timeout;

/// Per-frame send timeout. Matches the value `render_once` and
/// `clean_disconnect` used inline before the seam refactor.
const FRAME_SEND_TIMEOUT: Duration = Duration::from_millis(50);

/// Transport surface used by `run_session`'s render loop.
///
/// Implementors handle a single user session. `send_frame` is called once
/// per ratatui frame and once per pending terminal command; `eof_close`
/// is called at most once per session, on graceful or error shutdown.
pub trait FrameSink: Send + Sync {
    /// Push PTY output bytes to the wire. Returns `Ok(true)` on success,
    /// `Ok(false)` when the write was dropped (caller increments its
    /// drop counter), and `Err(_)` for a terminal transport failure
    /// (caller exits the render loop).
    fn send_frame(&self, bytes: Vec<u8>) -> impl Future<Output = anyhow::Result<bool>> + Send;

    /// Best-effort EOF + close. Never errors; the session is going away.
    fn eof_close(&self, close_code: u16) -> impl Future<Output = ()> + Send;
}

/// `FrameSink` over a russh server `Handle` + `ChannelId` pair. Owns its
/// handle so the render loop can move it into a `tokio::spawn`.
pub struct RusshFrameSink {
    handle: Handle,
    channel_id: ChannelId,
}

impl RusshFrameSink {
    pub fn new(handle: Handle, channel_id: ChannelId) -> Self {
        Self { handle, channel_id }
    }
}

impl FrameSink for RusshFrameSink {
    async fn send_frame(&self, bytes: Vec<u8>) -> anyhow::Result<bool> {
        match timeout(FRAME_SEND_TIMEOUT, self.handle.data(self.channel_id, bytes)).await {
            Ok(Ok(())) => Ok(true),
            Ok(Err(_)) => Err(anyhow::anyhow!(
                "russh handle.data failed (channel closed or session torn down)"
            )),
            Err(_) => Ok(false),
        }
    }

    async fn eof_close(&self, _close_code: u16) {
        let _ = self.handle.eof(self.channel_id).await;
        let _ = self.handle.close(self.channel_id).await;
    }
}

/// `FrameSink` over an axum WebSocket, fed via a bounded mpsc that a
/// separate writer task drains. The handler that owns the WS splits it,
/// spawns the writer task, and hands the mpsc `Sender` here.
///
/// The mpsc is bounded so backpressure surfaces as `Ok(false)` (drop +
/// repaint) instead of unbounded buffering — same shape as the russh
/// path's 50ms `Handle::data` timeout.
pub struct WsFrameSink {
    tx: mpsc::Sender<Message>,
}

impl WsFrameSink {
    pub fn new(tx: mpsc::Sender<Message>) -> Self {
        Self { tx }
    }
}

impl FrameSink for WsFrameSink {
    async fn send_frame(&self, bytes: Vec<u8>) -> anyhow::Result<bool> {
        match timeout(
            FRAME_SEND_TIMEOUT,
            self.tx.send(Message::Binary(bytes.into())),
        )
        .await
        {
            Ok(Ok(())) => Ok(true),
            Ok(Err(_)) => Err(anyhow::anyhow!(
                "ws sink writer task dropped (channel closed)"
            )),
            Err(_) => Ok(false),
        }
    }

    async fn eof_close(&self, close_code: u16) {
        let frame = CloseFrame {
            code: close_code,
            reason: "session ended".into(),
        };
        let _ = self.tx.send(Message::Close(Some(frame))).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use late_core::tunnel_protocol::{
        TUNNEL_CLOSE_RECONNECT_REQUESTED, TUNNEL_CLOSE_SESSION_ENDED,
    };

    #[tokio::test]
    async fn ws_sink_sends_binary_frame() {
        let (tx, mut rx) = mpsc::channel(4);
        let sink = WsFrameSink::new(tx);

        assert!(matches!(sink.send_frame(b"hello".to_vec()).await, Ok(true)));

        match rx.recv().await {
            Some(Message::Binary(bytes)) => assert_eq!(bytes.as_ref(), b"hello"),
            other => panic!("expected Binary, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn ws_sink_send_errors_when_receiver_dropped() {
        let (tx, rx) = mpsc::channel(1);
        drop(rx);
        let sink = WsFrameSink::new(tx);

        assert!(sink.send_frame(b"x".to_vec()).await.is_err());
    }

    #[tokio::test]
    async fn ws_sink_send_returns_false_on_timeout() {
        // Capacity 1, never drained → second send waits past FRAME_SEND_TIMEOUT.
        let (tx, _rx) = mpsc::channel(1);
        let sink = WsFrameSink::new(tx);

        assert!(matches!(sink.send_frame(b"a".to_vec()).await, Ok(true)));
        // Second send should hit the timeout branch.
        assert!(matches!(sink.send_frame(b"b".to_vec()).await, Ok(false)));
    }

    #[tokio::test]
    async fn ws_sink_eof_close_sends_terminal_session_ended() {
        let (tx, mut rx) = mpsc::channel(2);
        let sink = WsFrameSink::new(tx);

        sink.eof_close(TUNNEL_CLOSE_SESSION_ENDED).await;

        match rx.recv().await {
            Some(Message::Close(Some(frame))) => {
                assert_eq!(frame.code, TUNNEL_CLOSE_SESSION_ENDED);
            }
            other => panic!("expected Close, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn ws_sink_eof_close_uses_requested_code() {
        let (tx, mut rx) = mpsc::channel(2);
        let sink = WsFrameSink::new(tx);

        sink.eof_close(TUNNEL_CLOSE_RECONNECT_REQUESTED).await;

        match rx.recv().await {
            Some(Message::Close(Some(frame))) => {
                assert_eq!(frame.code, TUNNEL_CLOSE_RECONNECT_REQUESTED);
            }
            other => panic!("expected Close, got {other:?}"),
        }
    }
}
