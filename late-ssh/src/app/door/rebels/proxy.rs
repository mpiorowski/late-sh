use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result};
use russh::client::{self, Config, Handler};
use russh::keys::PublicKey;
use russh::{ChannelMsg, Disconnect};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::timeout;

use super::identity::derive_identity;
use crate::render_signal::RenderSignal;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProxyStatus {
    Connecting,
    Running,
    Closed,
}

const SETUP_TIMEOUT: Duration = Duration::from_secs(15);

/// The Rebels target is a public SSH service. For now we intentionally accept
/// any server host key and rely on the derived per-user credentials for auth.
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

/// Per-session proxy to the rebels SSH server. Owns a background task that runs
/// the bidirectional bridge; the foreground holds a shared vt100 screen and a
/// status flag updated by that task.
pub struct RebelsProxy {
    cmd_tx: mpsc::Sender<OutboundCommand>,
    task: JoinHandle<()>,
    parser: Arc<Mutex<vt100::Parser>>,
    status: Arc<Mutex<ProxyStatus>>,
}

pub struct ProxyConfig {
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

impl RebelsProxy {
    pub fn connect(cfg: ProxyConfig) -> Self {
        let (cmd_tx, cmd_rx) = mpsc::channel::<OutboundCommand>(256);
        let parser = Arc::new(Mutex::new(vt100::Parser::new(cfg.rows, cfg.cols, 0)));
        let status = Arc::new(Mutex::new(ProxyStatus::Connecting));

        let task_parser = parser.clone();
        let task_status = status.clone();
        let task = tokio::spawn(async move {
            if let Err(e) = run_bridge(cfg, cmd_rx, task_parser, task_status.clone()).await {
                tracing::warn!(error = ?e, "rebels proxy bridge ended with error");
            }
            *task_status.lock().expect("status mutex") = ProxyStatus::Closed;
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

impl Drop for RebelsProxy {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn run_bridge(
    cfg: ProxyConfig,
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
    .context("rebels outbound connect timed out")?
    .with_context(|| format!("connecting to {}:{}", cfg.host, cfg.port))?;

    // Mirror frittura-ssh-hub/src/ssh/bridge.rs: authenticate with a derived
    // Ed25519 key via publickey.
    let id = derive_identity(&cfg.secret, cfg.user_id);
    let key = russh::keys::PrivateKeyWithHashAlg::new(Arc::new(id.key), None);
    let auth = timeout(
        SETUP_TIMEOUT,
        session.authenticate_publickey(id.username.as_str(), key),
    )
    .await
    .context("rebels outbound authenticate_publickey timed out")?
    .context("outbound authenticate_publickey failed")?;
    if !auth.success() {
        anyhow::bail!("rebels rejected derived credentials");
    }

    let mut outbound = timeout(SETUP_TIMEOUT, session.channel_open_session())
        .await
        .context("rebels outbound channel_open_session timed out")?
        .context("channel_open_session failed")?;
    timeout(
        SETUP_TIMEOUT,
        outbound.request_pty(true, &cfg.term, cfg.cols as u32, cfg.rows as u32, 0, 0, &[]),
    )
    .await
    .context("rebels outbound request_pty timed out")?
    .context("request_pty failed")?;
    timeout(SETUP_TIMEOUT, outbound.request_shell(true))
        .await
        .context("rebels outbound request_shell timed out")?
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
