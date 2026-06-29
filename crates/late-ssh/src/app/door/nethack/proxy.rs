use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result};
use russh::client::{self, Config, Handler};
use russh::keys::PublicKey;
use russh::{ChannelMsg, Disconnect};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::timeout;

use super::identity::derive_client_key;
use crate::render_signal::RenderSignal;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProxyStatus {
    Connecting,
    Running,
    Closed,
}

const SETUP_TIMEOUT: Duration = Duration::from_secs(15);

/// The late-nethack host is a trusted, late.sh-owned service reached over the
/// internal network. We accept any server host key and rely on the derived
/// shared-secret credentials for auth (same policy as the rebels door).
struct AcceptAnyHostKey;

impl Handler for AcceptAnyHostKey {
    type Error = russh::Error;

    async fn check_server_key(&mut self, _key: &PublicKey) -> Result<bool, Self::Error> {
        Ok(true)
    }
}

enum OutboundCommand {
    Input(Vec<u8>),
    Resize { cols: u16, rows: u16 },
}

/// Per-session proxy to the late-nethack SSH host. Owns a background task that
/// runs the bidirectional bridge; the foreground holds a shared vt100 screen and
/// a status flag updated by that task.
///
/// This is the network-proxied twin of the rebels door (`RebelsProxy`): same
/// vt100 model and transport, but the target is late.sh's own NetHack host and
/// the SSH username carries the account-derived `-u` playname.
pub struct NethackProcess {
    cmd_tx: mpsc::Sender<OutboundCommand>,
    task: JoinHandle<()>,
    parser: Arc<Mutex<vt100::Parser>>,
    status: Arc<Mutex<ProxyStatus>>,
}

pub struct ProcessConfig {
    pub host: String,
    pub port: u16,
    pub secret: String,
    pub user_id: uuid::Uuid,
    pub cols: u16,
    pub rows: u16,
    pub term: String,
    /// Render-loop wakeup. The reader task pokes it on new remote output so the
    /// embedded game repaints promptly. `None` on headless/test paths.
    pub repaint: Option<Arc<RenderSignal>>,
}

impl NethackProcess {
    pub fn spawn(cfg: ProcessConfig) -> Self {
        let (cmd_tx, cmd_rx) = mpsc::channel::<OutboundCommand>(256);
        let parser = Arc::new(Mutex::new(vt100::Parser::new(cfg.rows, cfg.cols, 0)));
        let status = Arc::new(Mutex::new(ProxyStatus::Connecting));

        let task_parser = parser.clone();
        let task_status = status.clone();
        // Wake the render loop when the connection closes so the foreground runs
        // `tick()`, sees `Closed`, and repaints the launcher. Without this the
        // screen freezes on the last game frame (e.g. right after `S` saves).
        let exit_repaint = cfg.repaint.clone();
        let task = tokio::spawn(async move {
            if let Err(e) = run_bridge(cfg, cmd_rx, task_parser, task_status.clone()).await {
                tracing::warn!(error = ?e, "nethack proxy bridge ended with error");
            }
            *task_status.lock().expect("status mutex") = ProxyStatus::Closed;
            if let Some(sig) = &exit_repaint {
                sig.wake();
            }
        });

        Self {
            cmd_tx,
            task,
            parser,
            status,
        }
    }

    pub fn status(&self) -> ProxyStatus {
        *self.status.lock().expect("status mutex")
    }

    pub fn is_running(&self) -> bool {
        self.status() == ProxyStatus::Running
    }

    pub fn send_input(&self, bytes: Vec<u8>) {
        let _ = self.cmd_tx.try_send(OutboundCommand::Input(bytes));
    }

    pub fn resize(&self, cols: u16, rows: u16) {
        // Clamp to >=1: a tiny client can shrink the content area to zero, and a
        // 0-sized vt100 grid is invalid.
        let cols = cols.max(1);
        let rows = rows.max(1);
        self.parser
            .lock()
            .expect("parser mutex")
            .screen_mut()
            .set_size(rows, cols);
        let _ = self.cmd_tx.try_send(OutboundCommand::Resize { cols, rows });
    }

    /// Run a closure against the current screen (avoids cloning the grid).
    pub fn with_screen<R>(&self, f: impl FnOnce(&vt100::Screen) -> R) -> R {
        let guard = self.parser.lock().expect("parser mutex");
        f(guard.screen())
    }
}

impl Drop for NethackProcess {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// Build the NetHack `-u` player name for an account. Derived **only** from the
/// immutable user id, never the mutable username: the name keys the player's
/// save/bones, so deriving it from the username would orphan a character whenever
/// the user renames. It must be unique per account, stable forever, and PTY-safe.
///
/// We take the TRAILING hex of the id, not the leading hex: our ids are UUIDv7,
/// whose leading 48 bits are a millisecond timestamp (low entropy for
/// same-moment signups), while the tail is random. `late_` + 24 hex chars (29
/// total) stays under NetHack's name cap (`PL_NSIZ` 32, i.e. 31 usable) and is
/// collision-free in practice. (Cost: bones/ghost names are opaque, not the
/// username.) This is sent as the SSH username; the host re-sanitizes it (it
/// keeps `_`) before passing it to `-u`.
pub fn nethack_playname(user_id: uuid::Uuid) -> String {
    let simple = user_id.simple().to_string(); // 32 lowercase hex
    format!("late_{}", &simple[simple.len() - 24..])
}

async fn run_bridge(
    cfg: ProcessConfig,
    mut cmd_rx: mpsc::Receiver<OutboundCommand>,
    parser: Arc<Mutex<vt100::Parser>>,
    status: Arc<Mutex<ProxyStatus>>,
) -> Result<()> {
    let config = Arc::new(Config {
        inactivity_timeout: Some(Duration::from_secs(3600)),
        ..Default::default()
    });

    let mut session = timeout(
        SETUP_TIMEOUT,
        client::connect(config, (cfg.host.as_str(), cfg.port), AcceptAnyHostKey),
    )
    .await
    .context("nethack outbound connect timed out")?
    .with_context(|| format!("connecting to {}:{}", cfg.host, cfg.port))?;

    // Authenticate with the shared-secret-derived key; the username carries the
    // account-derived playname (the host uses it as `-u`).
    let username = nethack_playname(cfg.user_id);
    let key =
        russh::keys::PrivateKeyWithHashAlg::new(Arc::new(derive_client_key(&cfg.secret)), None);
    let auth = timeout(
        SETUP_TIMEOUT,
        session.authenticate_publickey(username.as_str(), key),
    )
    .await
    .context("nethack outbound authenticate_publickey timed out")?
    .context("outbound authenticate_publickey failed")?;
    if !auth.success() {
        anyhow::bail!("nethack host rejected derived credentials");
    }

    let mut outbound = timeout(SETUP_TIMEOUT, session.channel_open_session())
        .await
        .context("nethack outbound channel_open_session timed out")?
        .context("channel_open_session failed")?;
    timeout(
        SETUP_TIMEOUT,
        outbound.request_pty(true, &cfg.term, cfg.cols as u32, cfg.rows as u32, 0, 0, &[]),
    )
    .await
    .context("nethack outbound request_pty timed out")?
    .context("request_pty failed")?;
    timeout(SETUP_TIMEOUT, outbound.request_shell(true))
        .await
        .context("nethack outbound request_shell timed out")?
        .context("request_shell failed")?;

    *status.lock().expect("status mutex") = ProxyStatus::Running;

    loop {
        tokio::select! {
            cmd = cmd_rx.recv() => {
                match cmd {
                    Some(OutboundCommand::Input(bytes)) => {
                        if outbound.data(&bytes[..]).await.is_err() {
                            break;
                        }
                    }
                    Some(OutboundCommand::Resize { cols, rows }) => {
                        let _ = outbound
                            .window_change(cols as u32, rows as u32, 0, 0)
                            .await;
                    }
                    None => break, // proxy dropped
                }
            }
            msg = outbound.wait() => {
                let Some(msg) = msg else { break };
                match msg {
                    ChannelMsg::Data { data } | ChannelMsg::ExtendedData { data, .. } => {
                        parser.lock().expect("parser mutex").process(&data);
                        if let Some(sig) = &cfg.repaint {
                            sig.wake();
                        }
                    }
                    ChannelMsg::Eof | ChannelMsg::Close | ChannelMsg::ExitStatus { .. } => break,
                    _ => {}
                }
            }
        }
    }

    let _ = outbound.close().await;
    let _ = session
        .disconnect(Disconnect::ByApplication, "", "en")
        .await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn playname_is_account_derived_and_pty_safe() {
        let id = uuid::Uuid::from_u128(0x1234_5678_9abc_def0_1122_3344_5566_7788);
        let name = nethack_playname(id);
        assert!(name.starts_with("late_"));
        // trailing 24 hex of the id
        assert!(name.ends_with(&id.simple().to_string()[8..]));
        assert!(name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'));
        // within NetHack's PL_NSIZ (32 -> 31 usable)
        assert!(name.len() <= 31, "playname {name} too long: {}", name.len());
    }

    #[test]
    fn playname_is_stable_per_account() {
        let id = uuid::Uuid::from_u128(0x1234_5678_9abc_def0_1122_3344_5566_7788);
        // No username input -> a rename cannot change the save identity.
        assert_eq!(nethack_playname(id), nethack_playname(id));
    }

    #[test]
    fn playname_distinguishes_accounts() {
        let a = uuid::Uuid::from_u128(1);
        let b = uuid::Uuid::from_u128(2);
        assert_ne!(nethack_playname(a), nethack_playname(b));
    }
}
