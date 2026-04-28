//! Frame-sink abstraction shared by the russh and (eventually) `/tunnel`
//! session paths.
//!
//! The render loop in `ssh.rs::run_session` only needs two things from
//! its transport: push a `Vec<u8>` of PTY output, and close cleanly.
//! Both russh `Handle::data` and a future WebSocket sink can implement
//! that, so we model exactly that surface and nothing more.
//!
//! Drop-on-timeout accounting (the per-frame "did the byte go out?"
//! counter and `force_full_repaint` logic) lives at the render-loop layer
//! — the sink only reports whether each individual write completed.

use russh::ChannelId;
use russh::server::Handle;
use std::future::Future;
use std::time::Duration;
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
    fn eof_close(&self) -> impl Future<Output = ()> + Send;
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

    async fn eof_close(&self) {
        let _ = self.handle.eof(self.channel_id).await;
        let _ = self.handle.close(self.channel_id).await;
    }
}
