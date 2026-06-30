use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
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

/// Per-session host for a local dopewars process. Owns a background task that
/// runs the curses client on a PTY and pumps its terminal output into a shared
/// `vt100::Parser`; the foreground blits that grid and feeds keystrokes back in.
///
/// This is the local-PTY twin of the rebels/nethack doors: same vt100 model and
/// `with_screen` blit, but the child runs in-process here instead of behind an
/// SSH channel. dopewars has no savegame and no save-lock, so teardown is a
/// plain SIGKILL (via `kill_on_drop`) with none of NetHack's SIGHUP-save dance.
pub struct DopewarsProcess {
    cmd_tx: mpsc::Sender<OutboundCommand>,
    task: JoinHandle<()>,
    parser: Arc<Mutex<vt100::Parser>>,
    status: Arc<Mutex<ProxyStatus>>,
}

pub struct ProcessConfig {
    /// Path to the dopewars binary (resolved via `PATH` if unqualified).
    pub bin: String,
    /// Immutable account id, used to name the per-session score file.
    pub user_id: uuid::Uuid,
    pub cols: u16,
    pub rows: u16,
    pub term: String,
    /// Render-loop wakeup. The reader pokes it on new child output so the
    /// embedded game repaints promptly. `None` on headless/test paths.
    pub repaint: Option<Arc<RenderSignal>>,
}

impl DopewarsProcess {
    pub fn spawn(cfg: ProcessConfig) -> Self {
        let (cmd_tx, cmd_rx) = mpsc::channel::<OutboundCommand>(256);
        let parser = Arc::new(Mutex::new(vt100::Parser::new(cfg.rows, cfg.cols, 0)));
        let status = Arc::new(Mutex::new(ProxyStatus::Starting));

        let task_parser = parser.clone();
        let task_status = status.clone();
        // Wake the render loop when the child exits so the foreground runs
        // `tick()`, sees `Closed`, and repaints the launcher instead of freezing
        // on the last game frame.
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
        // Abort the bridge task; the child is reaped via `kill_on_drop`.
        self.task.abort();
    }
}

/// Per-session high-score path. dopewars wants a writable score file; deriving
/// it from the account id keeps each session isolated. We deliberately do NOT
/// run a setgid binary (dopewars refuses a user `-f` under setgid), so this
/// service-user-writable path is honored. Lives under the scratch run dir, which
/// the wrapper creates on demand.
fn score_path(user_id: uuid::Uuid) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("late-dopewars-{}.sco", user_id.simple()))
}

async fn run_bridge(
    cfg: ProcessConfig,
    mut cmd_rx: mpsc::Receiver<OutboundCommand>,
    parser: Arc<Mutex<vt100::Parser>>,
    status: Arc<Mutex<ProxyStatus>>,
) -> Result<()> {
    use std::io::Write;
    use std::os::fd::AsRawFd;
    use std::process::Stdio;
    use std::{fs, io};

    use nix::libc;
    use nix::pty::{Winsize, openpty};
    use nix::unistd::setsid;
    use tokio::process::Command as TokioCommand;

    let winsize = Winsize {
        ws_row: cfg.rows.max(1),
        ws_col: cfg.cols.max(1),
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let pty = openpty(Some(&winsize), None).context("failed to allocate dopewars pty")?;
    let master = Arc::new(fs::File::from(pty.master));
    let slave = fs::File::from(pty.slave);
    let slave_fd = slave.as_raw_fd();

    // Disable software flow control (XON/XOFF) on the pty so a stray Ctrl-S from
    // the client doesn't freeze the game's output until Ctrl-Q. dopewars has no
    // use for XON/XOFF; the key should pass through as an ordinary (ignored) one.
    {
        use nix::sys::termios::{self, InputFlags, SetArg};
        if let Ok(mut tio) = termios::tcgetattr(&slave) {
            tio.input_flags
                .remove(InputFlags::IXON | InputFlags::IXOFF | InputFlags::IXANY);
            let _ = termios::tcsetattr(&slave, SetArg::TCSANOW, &tio);
        }
    }

    // A real terminfo entry is required for ncurses ACS line-drawing; fall back
    // to xterm-256color for empty/unknown TERMs (mirrors the nethack host).
    let term = if cfg.term.trim().is_empty() {
        "xterm-256color".to_string()
    } else {
        cfg.term.clone()
    };
    let score_file = score_path(cfg.user_id);

    let mut cmd = TokioCommand::new(&cfg.bin);
    // Single-player (`-n`), curses text client (`-t`), black-and-white (`-b`),
    // with a per-session score file (`-f`). `-b` is deliberate: dopewars' own
    // color scheme hard-codes a blue-on-blue window palette that assumes a black
    // terminal and renders nearly unreadable when embedded. Monochrome lets its
    // default colors map to `Color::Reset`, so the game inherits the late.sh
    // theme (same approach as the nethack/rebels doors) and stays legible.
    //
    // Spawn with a cleared environment plus an explicit allowlist so the child
    // sees only what curses needs: a TERM, a UTF-8 locale for the ncursesw
    // line-drawing, and the window size.
    cmd.env_clear()
        .arg("-t")
        .arg("-n")
        .arg("-b")
        .arg("-f")
        .arg(&score_file)
        .env("TERM", &term)
        .env("LANG", "C.UTF-8")
        .env("LC_ALL", "C.UTF-8")
        .env("LINES", cfg.rows.max(1).to_string())
        .env("COLUMNS", cfg.cols.max(1).to_string())
        .stdin(Stdio::from(
            slave
                .try_clone()
                .context("clone dopewars pty slave for stdin")?,
        ))
        .stdout(Stdio::from(
            slave
                .try_clone()
                .context("clone dopewars pty slave for stdout")?,
        ))
        .stderr(Stdio::from(
            slave
                .try_clone()
                .context("clone dopewars pty slave for stderr")?,
        ))
        .kill_on_drop(true);

    // Give the child its own session and make the PTY its controlling terminal,
    // so curses sizing and job control behave.
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
        .with_context(|| format!("failed to start dopewars ({})", cfg.bin))?;
    drop(slave);

    *status.lock().expect("status mutex") = ProxyStatus::Running;

    // Blocking reader: pump child output into the shared vt100 parser on its own
    // thread, waking the render loop on each chunk. The grid is what the
    // foreground blits.
    let reader_master = master
        .try_clone()
        .context("clone dopewars pty master for reader")?;
    let reader_parser = parser.clone();
    let reader_repaint = cfg.repaint.clone();
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
                    if let Some(sig) = &reader_repaint {
                        sig.wake();
                    }
                }
            }
        }
    });

    loop {
        tokio::select! {
            cmd = cmd_rx.recv() => match cmd {
                Some(OutboundCommand::Input(bytes)) => {
                    let mut sink: &fs::File = &master;
                    if sink.write_all(&bytes).is_err() {
                        break; // pty master write failed: child's tty is gone
                    }
                }
                Some(OutboundCommand::Resize { cols, rows }) => set_winsize(&master, cols, rows),
                None => break, // proxy dropped (left the screen)
            },
            _ = child.wait() => break, // dopewars exited (quit, end of game, crash)
        }
    }

    // Backstop SIGKILL (a no-op if the child already exited). No graceful save:
    // dopewars has no savegame, so a dropped run simply ends.
    let _ = child.kill().await;
    drop(master);
    // The reader exits on its own at EOF; don't block teardown joining it.
    drop(reader);
    let _ = std::fs::remove_file(score_path(cfg.user_id));
    Ok(())
}

/// Push a new window size to the PTY; the kernel signals SIGWINCH to the child,
/// and dopewars does a full `endwin()`+`newterm()` rebuild at the new size.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn score_path_is_account_scoped() {
        let a = score_path(uuid::Uuid::from_u128(1));
        let b = score_path(uuid::Uuid::from_u128(2));
        assert_ne!(a, b);
        assert!(a.to_string_lossy().ends_with(".sco"));
    }
}
