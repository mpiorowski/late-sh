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

/// The late-bashquest host is a trusted, late.sh-owned service reached over
/// the internal network. We accept any server host key and rely on the
/// derived shared-secret credentials for auth (same policy as the
/// dopewars/DCSS/nethack doors).
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

/// Per-session proxy to the late-bashquest SSH host. Owns a background task
/// that runs the bidirectional bridge; the foreground holds a shared vt100
/// screen and a status flag updated by that task.
///
/// Same transport shape as the dopewars/DCSS doors, but identity carries real
/// weight here: the SSH username is the account's claimed arcade handle,
/// which the host hands to bashquest.sh as `BASHQUEST_AUTOLOGIN` so the
/// player's save resumes automatically (see `door::arcade::HandleFlow`,
/// threaded in by `state.rs`).
pub struct BashquestProcess {
    cmd_tx: mpsc::Sender<OutboundCommand>,
    task: JoinHandle<()>,
    parser: Arc<Mutex<vt100::Parser>>,
    status: Arc<Mutex<ProxyStatus>>,
    graduation: Arc<Mutex<Option<GraduationRecord>>>,
}

/// A verified graduation observed on the wire: late-bashquest only ever sends
/// this marker after independently confirming bashquest.sh wrote a
/// certificate file on its own PVC, which only its own `graduation_ceremony`
/// can do. See [`extract_marker`] for the format and the anti-forgery
/// reasoning.
pub struct GraduationRecord {
    pub handle: String,
    pub certificate: Vec<u8>,
}

pub struct ProcessConfig {
    pub host: String,
    pub port: u16,
    pub secret: String,
    /// The account's claimed arcade handle, sent as the SSH username; the
    /// host re-sanitizes it and hands it to bashquest.sh as
    /// `BASHQUEST_AUTOLOGIN`.
    pub playname: String,
    pub cols: u16,
    pub rows: u16,
    pub term: String,
    /// Render-loop wakeup. The reader task pokes it on new remote output so
    /// the embedded game repaints promptly. `None` on headless/test paths.
    pub repaint: Option<Arc<RenderSignal>>,
}

impl BashquestProcess {
    pub fn spawn(cfg: ProcessConfig) -> Self {
        let (cmd_tx, cmd_rx) = mpsc::channel::<OutboundCommand>(256);
        let parser = Arc::new(Mutex::new(vt100::Parser::new(cfg.rows, cfg.cols, 0)));
        let status = Arc::new(Mutex::new(ProxyStatus::Connecting));
        let graduation = Arc::new(Mutex::new(None));

        let task_parser = parser.clone();
        let task_status = status.clone();
        let task_graduation = graduation.clone();
        // Wake the render loop when the connection closes so the foreground
        // runs `tick()`, sees `Closed`, and repaints the launcher. Without
        // this the screen freezes on the last game frame.
        let exit_repaint = cfg.repaint.clone();
        let task = tokio::spawn(async move {
            if let Err(e) = run_bridge(
                cfg,
                cmd_rx,
                task_parser,
                task_status.clone(),
                task_graduation,
            )
            .await
            {
                tracing::warn!(error = ?e, "bashquest proxy bridge ended with error");
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
            graduation,
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
        // Clamp to >=1: a tiny client can shrink the content area to zero, and
        // a 0-sized vt100 grid is invalid.
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

    /// Take a verified graduation observed on the wire since the last call,
    /// if any. Clears it, so a caller that records it to the database will
    /// only ever see it once per process.
    pub fn take_graduation(&self) -> Option<GraduationRecord> {
        self.graduation.lock().expect("graduation mutex").take()
    }
}

impl Drop for BashquestProcess {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn run_bridge(
    cfg: ProcessConfig,
    mut cmd_rx: mpsc::Receiver<OutboundCommand>,
    parser: Arc<Mutex<vt100::Parser>>,
    status: Arc<Mutex<ProxyStatus>>,
    graduation: Arc<Mutex<Option<GraduationRecord>>>,
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
    .context("bashquest outbound connect timed out")?
    .with_context(|| format!("connecting to {}:{}", cfg.host, cfg.port))?;

    // Authenticate with the shared-secret-derived key; the username carries
    // the account's arcade handle, which the host hands to bashquest.sh as
    // BASHQUEST_AUTOLOGIN.
    let key =
        russh::keys::PrivateKeyWithHashAlg::new(Arc::new(derive_client_key(&cfg.secret)), None);
    let auth = timeout(
        SETUP_TIMEOUT,
        session.authenticate_publickey(cfg.playname.as_str(), key),
    )
    .await
    .context("bashquest outbound authenticate_publickey timed out")?
    .context("outbound authenticate_publickey failed")?;
    if !auth.success() {
        anyhow::bail!("bashquest host rejected derived credentials");
    }

    let mut outbound = timeout(SETUP_TIMEOUT, session.channel_open_session())
        .await
        .context("bashquest outbound channel_open_session timed out")?
        .context("channel_open_session failed")?;
    timeout(
        SETUP_TIMEOUT,
        outbound.request_pty(true, &cfg.term, cfg.cols as u32, cfg.rows as u32, 0, 0, &[]),
    )
    .await
    .context("bashquest outbound request_pty timed out")?
    .context("request_pty failed")?;
    timeout(SETUP_TIMEOUT, outbound.request_shell(true))
        .await
        .context("bashquest outbound request_shell timed out")?
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
                        match extract_marker(&data) {
                            Some((record, span)) => {
                                *graduation.lock().expect("graduation mutex") = Some(record);
                                // Strip the marker out so it never corrupts the
                                // rendered game screen; feed whatever real
                                // output surrounds it (there is normally none --
                                // the host sends this as its own final message).
                                let mut rest = data[..span.start].to_vec();
                                rest.extend_from_slice(&data[span.end..]);
                                if !rest.is_empty() {
                                    parser.lock().expect("parser mutex").process(&rest);
                                }
                            }
                            None => {
                                parser.lock().expect("parser mutex").process(&data);
                            }
                        }
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

/// Tag prefixing the graduation marker. MUST stay byte-identical to
/// late-bashquest's `host::CERT_MARKER_TAG` -- a NUL byte, which bashquest.sh
/// never legitimately prints (its own output is plain ANSI/UTF-8 text) and
/// which the host only ever sends via a direct, host-originated
/// `handle.data()` call made *after* the child has already exited, never as
/// forwarded child/pty output. Nothing the player types or pastes reaches
/// that call site, so this cannot be spoofed from inside the game.
const CERT_MARKER_TAG: &[u8] = b"\x00BQCERT\x01";

/// Length of the blake3 hex digest embedded right after the tag.
const DIGEST_HEX_LEN: usize = 64;

/// Look for a complete `TAG <64-hex-digest> \x01 <handle> \x01 <certificate> \x00`
/// marker anywhere in `data`. Returns the parsed record and the byte range it
/// occupies (to strip out of what gets fed to the vt100 parser).
///
/// Verifies the embedded digest against the extracted certificate bytes
/// before returning anything, so a marker split across an unlucky chunk
/// boundary is dropped rather than recorded with truncated content -- this
/// only needs to be correct when the whole marker lands in one chunk (which
/// it always does in practice: the host writes it in a single call
/// immediately before closing the channel), not perfectly robust to
/// adversarial fragmentation.
fn extract_marker(data: &[u8]) -> Option<(GraduationRecord, std::ops::Range<usize>)> {
    let tag_pos = data
        .windows(CERT_MARKER_TAG.len())
        .position(|w| w == CERT_MARKER_TAG)?;
    let after_tag = tag_pos + CERT_MARKER_TAG.len();
    let digest_end = after_tag.checked_add(DIGEST_HEX_LEN)?;
    let digest_hex = data.get(after_tag..digest_end)?;

    let mut pos = digest_end;
    if data.get(pos)? != &0x01 {
        return None;
    }
    pos += 1;

    let handle_start = pos;
    let handle_end = handle_start + data[handle_start..].iter().position(|&b| b == 0x01)?;
    let handle = String::from_utf8(data[handle_start..handle_end].to_vec()).ok()?;

    let cert_start = handle_end + 1;
    let cert_end = cert_start + data[cert_start..].iter().position(|&b| b == 0x00)?;
    let certificate = data[cert_start..cert_end].to_vec();

    let expected_digest = blake3::hash(&certificate).to_hex();
    if expected_digest.as_bytes() != digest_hex {
        return None;
    }

    Some((
        GraduationRecord {
            handle,
            certificate,
        },
        tag_pos..cert_end + 1,
    ))
}

#[cfg(test)]
#[path = "proxy_test.rs"]
mod proxy_test;
