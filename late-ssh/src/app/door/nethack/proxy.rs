use std::sync::{Arc, Mutex};

use anyhow::Result;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::render_signal::RenderSignal;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProxyStatus {
    Starting,
    Running,
    Closed,
}

enum OutboundCommand {
    Input(Vec<u8>),
    Resize { cols: u16, rows: u16 },
}

/// Per-session host for a local NetHack process. Owns a background task that
/// runs the child on a PTY and bridges its terminal into a shared vt100 screen;
/// the foreground reads that screen and a status flag updated by the task.
///
/// This is the local-process twin of `door::rebels::proxy::RebelsProxy`: same
/// vt100 model, but the transport is an `openpty`-spawned child rather than an
/// outbound SSH connection.
pub struct NethackProcess {
    cmd_tx: mpsc::Sender<OutboundCommand>,
    task: JoinHandle<()>,
    parser: Arc<Mutex<vt100::Parser>>,
    status: Arc<Mutex<ProxyStatus>>,
}

pub struct ProcessConfig {
    /// Path to the nethack binary (e.g. `/usr/games/nethack`).
    pub bin: String,
    /// late.sh-owned playground / home for the child (`HOME`). Saves and bones
    /// live under the install's playground; per-player saves are keyed by name.
    pub data_dir: String,
    /// In-game player name, passed as `-u`. Already sanitized to be PTY-safe.
    pub playname: String,
    pub cols: u16,
    pub rows: u16,
    pub term: String,
    /// Render-loop wakeup. The reader pokes it on new output so the embedded
    /// game repaints promptly. `None` on headless/test paths.
    pub repaint: Option<Arc<RenderSignal>>,
}

impl NethackProcess {
    pub fn spawn(cfg: ProcessConfig) -> Self {
        let (cmd_tx, cmd_rx) = mpsc::channel::<OutboundCommand>(256);
        let parser = Arc::new(Mutex::new(vt100::Parser::new(cfg.rows, cfg.cols, 0)));
        let status = Arc::new(Mutex::new(ProxyStatus::Starting));

        let task_parser = parser.clone();
        let task_status = status.clone();
        let task = tokio::spawn(async move {
            if let Err(e) = run_bridge(cfg, cmd_rx, task_parser, task_status.clone()).await {
                tracing::warn!(error = ?e, "nethack bridge ended with error");
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

impl Drop for NethackProcess {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// Keep only PTY-safe characters for the `-u` player name; fall back to a
/// stable account-derived name when nothing usable remains. NetHack caps names
/// at 32 chars.
pub fn sanitize_playname(username: &str, user_id: uuid::Uuid) -> String {
    let cleaned: String = username
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(32)
        .collect();
    if cleaned.is_empty() {
        format!("late{}", &user_id.simple().to_string()[..8])
    } else {
        cleaned
    }
}

#[cfg(unix)]
async fn run_bridge(
    cfg: ProcessConfig,
    cmd_rx: mpsc::Receiver<OutboundCommand>,
    parser: Arc<Mutex<vt100::Parser>>,
    status: Arc<Mutex<ProxyStatus>>,
) -> Result<()> {
    use std::os::fd::AsRawFd;
    use std::process::Stdio;
    use std::{fs, io};

    use anyhow::Context;
    use nix::libc;
    use nix::pty::{Winsize, openpty};
    use nix::unistd::setsid;
    use tokio::process::Command;

    let winsize = Winsize {
        ws_row: cfg.rows.max(1),
        ws_col: cfg.cols.max(1),
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let pty = openpty(Some(&winsize), None).context("failed to allocate nethack pty")?;
    let master = Arc::new(fs::File::from(pty.master));
    let slave = fs::File::from(pty.slave);
    let slave_fd = slave.as_raw_fd();

    let mut cmd = Command::new(&cfg.bin);
    cmd.arg("-u")
        .arg(&cfg.playname)
        .env("TERM", &cfg.term)
        .env("HOME", &cfg.data_dir)
        .env("NETHACKDIR", &cfg.data_dir)
        .env("LINES", cfg.rows.max(1).to_string())
        .env("COLUMNS", cfg.cols.max(1).to_string())
        .stdin(Stdio::from(
            slave.try_clone().context("clone nethack pty slave for stdin")?,
        ))
        .stdout(Stdio::from(
            slave.try_clone().context("clone nethack pty slave for stdout")?,
        ))
        .stderr(Stdio::from(
            slave.try_clone().context("clone nethack pty slave for stderr")?,
        ))
        .kill_on_drop(true);

    // Give the child its own session and make the PTY its controlling terminal,
    // so curses sizing and job control behave (mirrors late-cli/src/ssh.rs).
    unsafe {
        cmd.pre_exec(move || {
            setsid().map_err(|e| io::Error::from_raw_os_error(e as i32))?;
            if libc::ioctl(slave_fd, libc::TIOCSCTTY as _, 0) == -1 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }

    let mut child = cmd
        .spawn()
        .with_context(|| format!("failed to start nethack ({})", cfg.bin))?;
    drop(slave);

    *status.lock().expect("status mutex") = ProxyStatus::Running;

    // Blocking reader: pump child output into the vt100 parser and wake the
    // render loop. Exits on EOF/error once the child or master is gone.
    let reader_master = master.try_clone().context("clone nethack pty master for reader")?;
    let reader_parser = parser.clone();
    let repaint = cfg.repaint.clone();
    let reader = std::thread::spawn(move || {
        use std::io::Read;
        let mut src: &fs::File = &reader_master;
        let mut buf = [0u8; 8192];
        loop {
            match src.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    reader_parser.lock().expect("parser mutex").process(&buf[..n]);
                    if let Some(sig) = &repaint {
                        sig.wake();
                    }
                }
            }
        }
    });

    bridge_loop(cmd_rx, &master, &mut child).await;

    // Dropping the child kills nethack (kill_on_drop); the reader then sees EOF.
    let _ = child.kill().await;
    drop(master);
    let _ = reader.join();
    Ok(())
}

#[cfg(unix)]
async fn bridge_loop(
    mut cmd_rx: mpsc::Receiver<OutboundCommand>,
    master: &std::sync::Arc<std::fs::File>,
    child: &mut tokio::process::Child,
) {
    use std::io::Write;

    loop {
        tokio::select! {
            cmd = cmd_rx.recv() => match cmd {
                Some(OutboundCommand::Input(bytes)) => {
                    let mut sink: &std::fs::File = master;
                    if sink.write_all(&bytes).is_err() {
                        break;
                    }
                }
                Some(OutboundCommand::Resize { cols, rows }) => set_winsize(master, cols, rows),
                None => break, // proxy dropped
            },
            _ = child.wait() => break, // nethack exited (quit, death, crash)
        }
    }
}

/// Push a new window size to the PTY; the kernel signals SIGWINCH to the child's
/// foreground group so curses redraws at the new size.
#[cfg(unix)]
fn set_winsize(master: &std::fs::File, cols: u16, rows: u16) {
    use std::os::fd::AsRawFd;

    use nix::libc;

    let ws = libc::winsize {
        ws_row: rows.max(1),
        ws_col: cols.max(1),
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    unsafe {
        libc::ioctl(master.as_raw_fd(), libc::TIOCSWINSZ, &ws);
    }
}

#[cfg(not(unix))]
async fn run_bridge(
    _cfg: ProcessConfig,
    _cmd_rx: mpsc::Receiver<OutboundCommand>,
    _parser: Arc<Mutex<vt100::Parser>>,
    _status: Arc<Mutex<ProxyStatus>>,
) -> Result<()> {
    anyhow::bail!("nethack door requires a unix host")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_keeps_alphanumerics() {
        let id = uuid::Uuid::nil();
        assert_eq!(sanitize_playname("Mateusz", id), "Mateusz");
        assert_eq!(sanitize_playname("a.b-c_d 1", id), "abcd1");
    }

    #[test]
    fn sanitize_falls_back_for_empty() {
        let id = uuid::Uuid::from_u128(0x1234_5678_9abc_def0_1122_3344_5566_7788);
        let name = sanitize_playname("...", id);
        assert!(name.starts_with("late"));
        assert_eq!(name.len(), 12);
        assert!(name.chars().all(|c| c.is_ascii_alphanumeric()));
    }

    #[test]
    fn sanitize_caps_length() {
        let id = uuid::Uuid::nil();
        let long = "x".repeat(100);
        assert_eq!(sanitize_playname(&long, id).len(), 32);
    }
}
