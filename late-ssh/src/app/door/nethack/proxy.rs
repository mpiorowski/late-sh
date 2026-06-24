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
    /// late.sh-owned `HOME` for the child, where its `.nethackrc` lives. Saves
    /// and bones live under the nethack install's own playground; per-player
    /// saves there are keyed by the `-u` name.
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
        // Wake the render loop when the process exits so the foreground runs
        // `tick()`, sees `Closed`, and repaints the launcher. Without this the
        // screen freezes on the last game frame (e.g. right after `S` saves).
        let exit_repaint = cfg.repaint.clone();
        let task = tokio::spawn(async move {
            if let Err(e) = run_bridge(cfg, cmd_rx, task_parser, task_status.clone()).await {
                tracing::warn!(error = ?e, "nethack bridge ended with error");
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
        // Clamp to >=1 like the connect and PTY-winsize paths: a tiny client can
        // shrink the content area to zero, and a 0-sized vt100 grid is invalid.
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
/// same-moment signups), while the tail is random. 24 hex chars stays under
/// NetHack's name cap (`PL_NSIZ` 32, i.e. 31 usable) and is collision-free in
/// practice. (Cost: bones/ghost names are opaque, not the username.)
pub fn nethack_playname(user_id: uuid::Uuid) -> String {
    let simple = user_id.simple().to_string(); // 32 lowercase hex
    format!("late{}", &simple[simple.len() - 24..])
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

    // Disable software flow control (XON/XOFF) on the pty. Otherwise a stray
    // Ctrl-S from the client is read as XOFF and the line discipline freezes the
    // game's output until an XON (Ctrl-Q) arrives, leaving the screen stuck and
    // glyphs garbled when output finally resumes. nethack has no use for
    // XON/XOFF, so Ctrl-S should pass through as an ordinary (ignored) key. We
    // set this before exec so the child inherits it; cbreak-mode curses like
    // nethack's tty window-port don't turn it back on.
    {
        use nix::sys::termios::{self, InputFlags, SetArg};
        if let Ok(mut tio) = termios::tcgetattr(&slave) {
            tio.input_flags
                .remove(InputFlags::IXON | InputFlags::IXOFF | InputFlags::IXANY);
            let _ = termios::tcsetattr(&slave, SetArg::TCSANOW, &tio);
        }
    }

    let mut cmd = Command::new(&cfg.bin);
    // Spawn with a cleared environment and an explicit allowlist. late-ssh runs
    // with production secrets in its own env (DB password, S3 keys, AI key,
    // LiveKit/tunnel/rebels secrets); the door child must never inherit them.
    // NetHack's shell ('!') and suspend ('^Z') escapes are compiled out in the
    // nethack-build stage (NOSHELL/NOSUSPEND), so there's no in-game path to a
    // shell as the service user; clearing the env is additional defense in depth.
    cmd.env_clear()
        .arg("-u")
        .arg(&cfg.playname)
        .env("TERM", &cfg.term)
        // HOME holds the per-player `.nethackrc`. We deliberately do NOT set
        // NETHACKDIR: the distro nethack package ships its data files and
        // playground (saves/bones) at its own compiled-in location, and
        // overriding NETHACKDIR to an empty dir makes nethack fail to chdir.
        .env("HOME", &cfg.data_dir)
        .env("LINES", cfg.rows.max(1).to_string())
        .env("COLUMNS", cfg.cols.max(1).to_string())
        .stdin(Stdio::from(
            slave
                .try_clone()
                .context("clone nethack pty slave for stdin")?,
        ))
        .stdout(Stdio::from(
            slave
                .try_clone()
                .context("clone nethack pty slave for stdout")?,
        ))
        .stderr(Stdio::from(
            slave
                .try_clone()
                .context("clone nethack pty slave for stderr")?,
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
    let reader_master = master
        .try_clone()
        .context("clone nethack pty master for reader")?;
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
                    reader_parser
                        .lock()
                        .expect("parser mutex")
                        .process(&buf[..n]);
                    if let Some(sig) = &repaint {
                        sig.wake();
                    }
                }
            }
        }
    });

    bridge_loop(cmd_rx, &master, &mut child).await;

    // Diagnostic: timestamps the moment the child is gone. Compare against the
    // `received input data` log for the save-confirm `y` to see how much of any
    // perceived freeze is nethack's own save/exit latency vs late.sh teardown.
    tracing::debug!("nethack child exited; returning to launcher");

    // Flip to Closed and wake the foreground the instant nethack exits, BEFORE
    // the best-effort cleanup below. `tick()` watches this status to return to
    // the launcher; if a slow `reader.join()` gated it (as it used to), the
    // screen froze on the last frame after `S` saved. Setting it here makes the
    // return reliable regardless of how the reader winds down.
    *status.lock().expect("status mutex") = ProxyStatus::Closed;
    if let Some(sig) = &cfg.repaint {
        sig.wake();
    }

    // Dropping the child kills nethack (kill_on_drop); the reader then sees EOF.
    let _ = child.kill().await;
    drop(master);

    // Deliberately do NOT join the reader here. When nethack saves it can hand
    // the save file to an external compressor that inherits the pty slave and
    // outlives the game by several seconds (worse on slow container storage).
    // nethack itself has already exited and `status` is Closed, so the launcher
    // must come back NOW; a blocking `reader.join()` would pin a runtime worker
    // on that lingering compressor and, on a CPU-limited host, starve the render
    // loop so the return to the launcher freezes for as long as it runs. The
    // detached reader thread exits on its own once the pty finally hits EOF.
    drop(reader);
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
    fn playname_is_account_derived_and_pty_safe() {
        let id = uuid::Uuid::from_u128(0x1234_5678_9abc_def0_1122_3344_5566_7788);
        let name = nethack_playname(id);
        assert!(name.starts_with("late"));
        // trailing 24 hex of the id
        assert!(name.ends_with(&id.simple().to_string()[8..]));
        assert!(name.chars().all(|c| c.is_ascii_alphanumeric()));
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
