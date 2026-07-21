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

/// The late-dopewars host is a trusted, late.sh-owned service reached over the
/// internal network. We accept any server host key and rely on the derived
/// shared-secret credentials for auth (same policy as the nethack/rebels doors).
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

/// Per-session proxy to the late-dopewars SSH host. Owns a background task that
/// runs the bidirectional bridge; the foreground holds a shared vt100 screen and
/// a status flag updated by that task.
///
/// This is the network-proxied twin of the nethack door (`NethackProcess`): same
/// vt100 model and transport, but the target is late.sh's own dopewars host.
/// dopewars single-player takes no `-u` name, so the SSH username carries no
/// identity (only an account-derived label for host-side log correlation); the
/// player types their handle in-game and it lands in the shared high-score table.
pub struct DopewarsProcess {
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

impl DopewarsProcess {
    pub fn spawn(cfg: ProcessConfig) -> Self {
        let (cmd_tx, cmd_rx) = mpsc::channel::<OutboundCommand>(256);
        let parser = Arc::new(Mutex::new(vt100::Parser::new(cfg.rows, cfg.cols, 0)));
        let status = Arc::new(Mutex::new(ProxyStatus::Connecting));

        let task_parser = parser.clone();
        let task_status = status.clone();
        // Wake the render loop when the connection closes so the foreground runs
        // `tick()`, sees `Closed`, and repaints the launcher. Without this the
        // screen freezes on the last game frame.
        let exit_repaint = cfg.repaint.clone();
        let task = tokio::spawn(async move {
            if let Err(e) = run_bridge(cfg, cmd_rx, task_parser, task_status.clone()).await {
                tracing::warn!(error = ?e, "dopewars proxy bridge ended with error");
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

impl Drop for DopewarsProcess {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// An opaque, account-derived label sent as the SSH username. dopewars uses no
/// `-u` name, so this is not an identity the host acts on -- it only correlates
/// host-side logs to an account. Derived from the immutable user id (trailing
/// hex, since our UUIDv7 ids have a low-entropy leading timestamp) so it is
/// stable and PTY/SSH-safe.
pub fn dopewars_session_label(user_id: uuid::Uuid) -> String {
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
    .context("dopewars outbound connect timed out")?
    .with_context(|| format!("connecting to {}:{}", cfg.host, cfg.port))?;

    // Authenticate with the shared-secret-derived key; the username is only an
    // account-correlation label (the host takes no `-u` name from it).
    let username = dopewars_session_label(cfg.user_id);
    let key =
        russh::keys::PrivateKeyWithHashAlg::new(Arc::new(derive_client_key(&cfg.secret)), None);
    let auth = timeout(
        SETUP_TIMEOUT,
        session.authenticate_publickey(username.as_str(), key),
    )
    .await
    .context("dopewars outbound authenticate_publickey timed out")?
    .context("outbound authenticate_publickey failed")?;
    if !auth.success() {
        anyhow::bail!("dopewars host rejected derived credentials");
    }

    let mut outbound = timeout(SETUP_TIMEOUT, session.channel_open_session())
        .await
        .context("dopewars outbound channel_open_session timed out")?
        .context("channel_open_session failed")?;
    timeout(
        SETUP_TIMEOUT,
        outbound.request_pty(true, &cfg.term, cfg.cols as u32, cfg.rows as u32, 0, 0, &[]),
    )
    .await
    .context("dopewars outbound request_pty timed out")?
    .context("request_pty failed")?;
    timeout(SETUP_TIMEOUT, outbound.request_shell(true))
        .await
        .context("dopewars outbound request_shell timed out")?
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
