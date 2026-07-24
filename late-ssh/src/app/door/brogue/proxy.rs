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

/// The late-brogue host is a trusted, late.sh-owned service reached over the
/// internal network. We accept any server host key and rely on the derived
/// shared-secret credentials for auth (same policy as the dcss door).
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

/// Per-session proxy to the late-brogue SSH host. Owns a background task that
/// runs the bidirectional bridge; the foreground holds a shared vt100 screen and
/// a status flag updated by that task.
///
/// This is the twin of the dcss door's `DcssProcess`: same vt100 model and
/// transport, but the target is late.sh's Brogue host and the SSH username
/// carries the account's arcade handle as the save-directory playname.
pub struct BrogueProcess {
    cmd_tx: mpsc::Sender<OutboundCommand>,
    task: JoinHandle<()>,
    parser: Arc<Mutex<vt100::Parser>>,
    status: Arc<Mutex<ProxyStatus>>,
}

pub struct ProcessConfig {
    pub host: String,
    pub port: u16,
    pub secret: String,
    /// The account's arcade handle, sent as the SSH username; the host
    /// re-sanitizes it and uses it as the per-player save directory name (the
    /// brogue child's cwd), which keys the save. Claimed once and immutable
    /// (`late_core::models::arcade_handle`), so a late.sh rename can never
    /// orphan a character.
    pub playname: String,
    pub cols: u16,
    pub rows: u16,
    pub term: String,
    /// Render-loop wakeup. The reader task pokes it on new remote output so the
    /// embedded game repaints promptly. `None` on headless/test paths.
    pub repaint: Option<Arc<RenderSignal>>,
}

impl BrogueProcess {
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
                tracing::warn!(error = ?e, "brogue proxy bridge ended with error");
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

impl Drop for BrogueProcess {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// Rewrite CSI HVP (`ESC [ Pl ; Pc f`) into CUP (`ESC [ Pl ; Pc H`) so the
/// vt100 parser honors it. brogue's truecolor renderer (`buffer_render_24bit`
/// in term.c, selected by the COLORTERM the host exports) positions the cursor
/// exclusively with the `f` final byte, which the vt100 crate does not
/// implement: every move is silently dropped and the frame smears sequentially
/// across the grid. HVP and CUP are semantically identical, so the rewrite is
/// lossless. Stateful because an escape sequence can be split across SSH data
/// chunks; an unterminated candidate tail is carried into the next call.
struct HvpNormalizer {
    carry: Vec<u8>,
}

/// A real HVP is `ESC [` + short numeric params + `f`; anything longer than
/// this is not one, so flush it verbatim instead of buffering unbounded.
const HVP_CARRY_MAX: usize = 16;

impl HvpNormalizer {
    fn new() -> Self {
        Self { carry: Vec::new() }
    }

    fn feed(&mut self, data: &[u8]) -> Vec<u8> {
        let mut input = std::mem::take(&mut self.carry);
        input.extend_from_slice(data);

        let mut out = Vec::with_capacity(input.len());
        let mut i = 0;
        while i < input.len() {
            if input[i] != 0x1b {
                out.push(input[i]);
                i += 1;
                continue;
            }
            // Candidate CSI: ESC [ digits/; ... final. Walk to the final byte.
            let seq_start = i;
            let mut j = i + 1;
            if j >= input.len() {
                self.carry = input[seq_start..].to_vec();
                break;
            }
            if input[j] != b'[' {
                out.push(input[i]);
                i += 1;
                continue;
            }
            j += 1;
            while j < input.len() && (input[j].is_ascii_digit() || input[j] == b';') {
                j += 1;
            }
            if j >= input.len() {
                // Unterminated numeric CSI at the chunk edge: hold it back if
                // it could still become an HVP, else flush verbatim.
                let tail = &input[seq_start..];
                if tail.len() <= HVP_CARRY_MAX {
                    self.carry = tail.to_vec();
                } else {
                    out.extend_from_slice(tail);
                }
                break;
            }
            if input[j] == b'f' && j - seq_start <= HVP_CARRY_MAX {
                out.extend_from_slice(&input[seq_start..j]);
                out.push(b'H');
            } else {
                out.extend_from_slice(&input[seq_start..=j]);
            }
            i = j + 1;
        }
        out
    }
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
    .context("brogue outbound connect timed out")?
    .with_context(|| format!("connecting to {}:{}", cfg.host, cfg.port))?;

    // Authenticate with the shared-secret-derived key; the username carries the
    // account's arcade handle (the host uses it as the save-directory name).
    let key =
        russh::keys::PrivateKeyWithHashAlg::new(Arc::new(derive_client_key(&cfg.secret)), None);
    let auth = timeout(
        SETUP_TIMEOUT,
        session.authenticate_publickey(cfg.playname.as_str(), key),
    )
    .await
    .context("brogue outbound authenticate_publickey timed out")?
    .context("outbound authenticate_publickey failed")?;
    if !auth.success() {
        anyhow::bail!("brogue host rejected derived credentials");
    }

    let mut outbound = timeout(SETUP_TIMEOUT, session.channel_open_session())
        .await
        .context("brogue outbound channel_open_session timed out")?
        .context("channel_open_session failed")?;
    timeout(
        SETUP_TIMEOUT,
        outbound.request_pty(true, &cfg.term, cfg.cols as u32, cfg.rows as u32, 0, 0, &[]),
    )
    .await
    .context("brogue outbound request_pty timed out")?
    .context("request_pty failed")?;
    timeout(SETUP_TIMEOUT, outbound.request_shell(true))
        .await
        .context("brogue outbound request_shell timed out")?
        .context("request_shell failed")?;

    *status.lock().expect("status mutex") = ProxyStatus::Running;

    let mut norm = HvpNormalizer::new();
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
                        let bytes = norm.feed(&data);
                        parser.lock().expect("parser mutex").process(&bytes);
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
#[path = "proxy_test.rs"]
mod proxy_test;
